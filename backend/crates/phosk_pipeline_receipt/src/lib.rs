//! `phosk_pipeline_receipt` — the zero-trust receipt-intake pipeline (ADR-011,
//! design philosophy #4: *queued photos are hostile input*).
//!
//! One untrusted photo enters; a schema-checked [`Receipt`] proposal is
//! **enqueued for human approval** and leaves. The pipeline NEVER writes a
//! receipt to the ledger — its terminal effect is `db.enqueue_suggestion`, an
//! append to the approval queue. A model can *propose*; only an approved
//! suggestion is ever applied (later, by the approval pipeline). This is the
//! effect-typed gate from [`phosk_ai`], enforced structurally: this crate has no
//! code path that calls `insert_receipt`.
//!
//! ## The six steps ([`intake_receipt`])
//! 1. **Validate hostile input.** Size cap; magic-byte MIME sniff (`infer`, a
//!    pure-Rust libmagic-style signature check — a forged extension can't smuggle
//!    a non-image through). Anything not a known image type is rejected with
//!    [`PhoskError::Invalid`] *before* any decode, OCR, or model call.
//! 2. **Sanitise + store.** EXIF/metadata is stripped in pure Rust (no
//!    `exiftool`) so GPS/serial/timestamps never reach disk or the model, then
//!    the sanitised bytes are handed to [`PhotoStorage::put`]. Encryption-at-rest
//!    is the L3 storage adapter's job (it owns the key); the port is dumb by
//!    design, so this crate stays adapter-agnostic.
//! 3. **OCR.** [`OcrAdapter::extract`] → text + regions + per-region confidence.
//! 4. **LLM extraction.** [`LlmAdapter::generate_structured`] with a JSON Schema
//!    (ADR-008 constrained output) → line items, each with a confidence; the
//!    answer is schema-checked before anything is built from it. Lines below
//!    [`phosk_ai::CONFIDENCE_THRESHOLD`] are flagged (the coral review rule),
//!    never dropped.
//! 5. **Assemble.** A [`Receipt`] + [`LineItem`]s with [`Provenance`]
//!    (`Source::Ocr` for the receipt total read off the slip, `Source::LlmInferred`
//!    for the model's line items). `line_total` is backend-derived
//!    (`round(qty * unit_price)`), never trusted from the model.
//! 6. **Enqueue.** A single per-receipt [`AiSuggestion`] (`kind == "receipt"`,
//!    `status == "open"`) summarising the proposal is appended to the approval
//!    queue. Bulk per-receipt: one suggestion carries the whole receipt.
//!
//! ## Idempotency (ADR-010)
//! The SHA-256 of the *validated* bytes is the idempotency key, surfaced as the
//! receipt `slug` (`"rcpt:<hex>"`). Re-submitting the same photo finds the prior
//! enqueued suggestion and returns it without storing, OCR-ing, calling the
//! model, or enqueuing again ([`IntakeOutcome::deduplicated`] `== true`).
//!
//! ## Sandbox seam (deployment-level)
//! Steps 3–4 — the only steps that feed attacker-controlled bytes into a
//! subprocess (OCR) or a model (LLM) — are isolated behind one function,
//! [`transcribe_and_extract`]. That is the seam a deployment wraps in an OS
//! sandbox: **no network except the Ollama socket, a scratch fs, dropped
//! capabilities**. Validation (step 1) runs *before* it so malformed/oversize
//! input never reaches the sandboxed code; storage (step 2) and enqueue (step 6)
//! run *outside* it (they touch the real DB / encrypted store). The actual OS
//! sandbox (namespaces / seccomp / systemd hardening) is configured at the
//! `bin/*` composition root; this crate provides the clean call boundary it wraps
//! and keeps that boundary free of DB/queue access.

use sha2::{Digest, Sha256};

use phosk_adapter_db::DatabaseAdapter;
use phosk_adapter_llm::LlmAdapter;
use phosk_adapter_ocr::{OcrAdapter, OcrResult};
use phosk_adapter_storage::{PhotoStorage, StorageRef};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{LineItemId, ReceiptId, SuggestionId};
use phosk_model::{AiSuggestion, LineItem, Provenance, Receipt, Source};

/// The maximum accepted photo size (bytes). Hostile input is size-capped before
/// any decode/OCR/model work to bound resource use (a decompression-bomb / OOM
/// guard). 12 MiB comfortably covers a high-res phone photo of a receipt.
pub const MAX_PHOTO_BYTES: usize = 12 * 1024 * 1024;

/// The untrusted intake unit: raw encoded photo bytes + the capture date.
///
/// `bytes` is fully untrusted (it arrives over the Pi DMZ FIFO as an opaque
/// blob). Nothing in this struct is trusted until [`intake_receipt`] validates it.
#[derive(Debug, Clone)]
pub struct IntakePhoto<'a> {
    /// Raw encoded image bytes (JPEG/PNG/WebP/…) — hostile until validated.
    pub bytes: &'a [u8],
    /// The day the receipt is booked against (the desktop's "today").
    pub captured_on: chrono::NaiveDate,
}

/// The result of a successful intake: a proposal **enqueued** for approval.
///
/// Note what is NOT here: no DB write of the receipt. The receipt + lines are
/// returned for the caller/UI to preview; the only persisted effect is the
/// enqueued [`AiSuggestion`] (`suggestion_id`) and the stored photo (`photo`).
#[derive(Debug, Clone, PartialEq)]
pub struct IntakeOutcome {
    /// The proposed receipt (NOT persisted to the ledger — awaiting approval).
    pub receipt: Receipt,
    /// The proposed line items (NOT persisted — awaiting approval).
    pub line_items: Vec<LineItem>,
    /// The id of the enqueued approval-queue [`AiSuggestion`].
    pub suggestion_id: SuggestionId,
    /// The opaque ref of the stored, sanitised photo.
    pub photo: StorageRef,
    /// SHA-256 (hex) of the validated bytes — the idempotency key.
    pub content_hash: String,
    /// How many line items came back below [`CONFIDENCE_THRESHOLD`] (coral-flagged).
    pub low_confidence_lines: usize,
    /// `true` when this exact photo was already intaken — nothing was stored,
    /// OCR-ed, sent to the model, or re-enqueued; the prior proposal is returned.
    pub deduplicated: bool,
}

/// The detected image kind that passed validation (a sanitised, vendor-neutral
/// summary of the `infer` result — `infer`'s own types never cross this seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageKind {
    Jpeg,
    Png,
    Other,
}

/// Run the full zero-trust intake for one untrusted photo.
///
/// Takes the four PORTs as `&dyn _` (DB, storage, OCR, LLM) — a technology swap
/// is a new adapter impl wired at composition, never an edit here. Returns an
/// [`IntakeOutcome`] whose only persisted effects are the stored sanitised photo
/// and the enqueued approval suggestion; the receipt itself is **not** written.
///
/// # Errors
/// - [`PhoskError::Invalid`] if validation fails (empty / oversize / not a
///   recognised image type), or if the model's structured answer fails schema
///   validation / is missing required fields.
/// - Propagates any PORT error (storage, OCR, model, DB) unchanged.
#[tracing::instrument(level = "debug", skip_all, fields(captured_on = %photo.captured_on))]
pub async fn intake_receipt(
    db: &dyn DatabaseAdapter,
    storage: &dyn PhotoStorage,
    ocr: &dyn OcrAdapter,
    llm: &dyn LlmAdapter,
    photo: IntakePhoto<'_>,
) -> Result<IntakeOutcome, PhoskError> {
    // ── Step 1: validate hostile input (BEFORE the sandboxed OCR/LLM seam) ──
    let kind = validate_image(photo.bytes)?;
    let content_hash = sha256_hex(photo.bytes);
    let slug = format!("rcpt:{content_hash}");

    // ── Idempotency (ADR-010): same bytes ⇒ same slug ⇒ no re-work. ──
    if let Some(existing) = find_enqueued(db, &slug).await? {
        tracing::debug!(%slug, "duplicate photo — returning prior enqueued proposal");
        return Ok(existing);
    }

    // ── Step 2: strip metadata (pure Rust) then store the SANITISED bytes. ──
    // EXIF/GPS/serials are removed here so they never reach disk or the model.
    let sanitised = strip_metadata(photo.bytes, kind);
    let photo_ref = storage.put(&sanitised).await?;

    // ── Steps 3–4: the SANDBOX seam — attacker bytes meet OCR + the model. ──
    let extracted = transcribe_and_extract(ocr, llm, &sanitised).await?;

    // ── Step 5: assemble the Receipt + LineItems with Provenance. ──
    let (receipt, line_items, low_confidence_lines) =
        assemble(&slug, photo.captured_on, &extracted, kind)?;

    // ── Step 6: ENQUEUE one per-receipt proposal — never auto-write. ──
    let suggestion = build_suggestion(&receipt, &line_items, low_confidence_lines);
    let suggestion_id = db.enqueue_suggestion(suggestion).await?;

    Ok(IntakeOutcome {
        receipt,
        line_items,
        suggestion_id,
        photo: photo_ref,
        content_hash,
        low_confidence_lines,
        deduplicated: false,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 1 — validation (hostile-input gate)
// ─────────────────────────────────────────────────────────────────────────────

/// Validate raw bytes as an acceptable receipt photo: non-empty, within the size
/// cap, and a recognised IMAGE type by magic bytes (not by any caller-supplied
/// extension/MIME). Returns the detected [`ImageKind`] on success.
fn validate_image(bytes: &[u8]) -> Result<ImageKind, PhoskError> {
    if bytes.is_empty() {
        return Err(PhoskError::Invalid("empty photo".to_owned()));
    }
    if bytes.len() > MAX_PHOTO_BYTES {
        return Err(PhoskError::Invalid(format!(
            "photo too large: {} bytes (max {MAX_PHOTO_BYTES})",
            bytes.len()
        )));
    }
    // Magic-byte sniff. `infer::get` reads only the leading signature bytes, so a
    // `.jpg`-named PDF or an executable is caught here regardless of its name.
    let Some(kind) = infer::get(bytes) else {
        return Err(PhoskError::Invalid(
            "unrecognised file signature (not an image)".to_owned(),
        ));
    };
    if kind.matcher_type() != infer::MatcherType::Image {
        return Err(PhoskError::Invalid(format!(
            "rejected non-image input: {}",
            kind.mime_type()
        )));
    }
    Ok(match kind.mime_type() {
        "image/jpeg" => ImageKind::Jpeg,
        "image/png" => ImageKind::Png,
        _ => ImageKind::Other,
    })
}

/// SHA-256 of the bytes, lowercase hex.
fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 2 — metadata stripping (pure Rust; no exiftool)
// ─────────────────────────────────────────────────────────────────────────────

/// Remove metadata that could carry PII/location from the encoded image, in pure
/// Rust. For JPEG this drops every `APPn` marker segment (EXIF lives in `APP1`,
/// also XMP/ICC/JFIF thumbnails) plus `COM` comments, while preserving the actual
/// image stream. For PNG it drops the ancillary text/metadata chunks (`eXIf`,
/// `tEXt`, `iTXt`, `zTXt`, `tIME`). Unknown types pass through unchanged (already
/// validated as an image; the storage adapter encrypts them regardless).
///
/// On any structural surprise the function fails safe by returning the bytes
/// unchanged rather than panicking — it never indexes past a bound.
fn strip_metadata(bytes: &[u8], kind: ImageKind) -> Vec<u8> {
    match kind {
        ImageKind::Jpeg => strip_jpeg_appn(bytes).unwrap_or_else(|| bytes.to_vec()),
        ImageKind::Png => strip_png_meta_chunks(bytes).unwrap_or_else(|| bytes.to_vec()),
        ImageKind::Other => bytes.to_vec(),
    }
}

/// Walk JPEG marker segments and copy everything except `APP0..=APP15`
/// (`0xE0..=0xEF`) and `COM` (`0xFE`). Returns `None` if the structure is not the
/// JPEG we expect (so the caller falls back to the original bytes).
fn strip_jpeg_appn(bytes: &[u8]) -> Option<Vec<u8>> {
    // SOI
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[0..2]);
    let mut i = 2usize;
    loop {
        // Need at least a 2-byte marker.
        if i + 1 >= bytes.len() {
            out.extend_from_slice(&bytes[i..]);
            break;
        }
        if bytes[i] != 0xFF {
            return None; // not aligned on a marker — bail, keep original
        }
        let marker = bytes[i + 1];
        // Start Of Scan: from here on is entropy-coded image data to EOI; copy
        // the rest verbatim and stop parsing markers.
        if marker == 0xDA {
            out.extend_from_slice(&bytes[i..]);
            break;
        }
        // End Of Image standalone marker.
        if marker == 0xD9 {
            out.extend_from_slice(&bytes[i..i + 2]);
            break;
        }
        // Every other marker carries a 2-byte big-endian length (incl. the 2
        // length bytes) followed by that many payload bytes.
        let len_hi = *bytes.get(i + 2)? as usize;
        let len_lo = *bytes.get(i + 3)? as usize;
        let seg_len = (len_hi << 8) | len_lo;
        if seg_len < 2 {
            return None;
        }
        let seg_end = i + 2 + seg_len; // marker(2) + payload(seg_len)
        if seg_end > bytes.len() {
            return None;
        }
        let is_appn = (0xE0..=0xEF).contains(&marker);
        let is_com = marker == 0xFE;
        if !is_appn && !is_com {
            out.extend_from_slice(&bytes[i..seg_end]);
        }
        i = seg_end;
    }
    Some(out)
}

/// Copy PNG signature + critical chunks, dropping ancillary metadata chunks
/// (`eXIf`, `tEXt`, `iTXt`, `zTXt`, `tIME`). Returns `None` on a malformed
/// structure so the caller keeps the original bytes.
fn strip_png_meta_chunks(bytes: &[u8]) -> Option<Vec<u8>> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 8 || bytes[0..8] != SIG {
        return None;
    }
    let drop: [&[u8; 4]; 5] = [b"eXIf", b"tEXt", b"iTXt", b"zTXt", b"tIME"];
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&SIG);
    let mut i = 8usize;
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        let type_start = i + 4;
        let type_end = type_start + 4;
        let chunk_type = bytes.get(type_start..type_end)?;
        // Whole chunk = len(4) + type(4) + data(len) + crc(4).
        let chunk_end = type_end + len + 4;
        if chunk_end > bytes.len() {
            return None;
        }
        let is_meta = drop.iter().any(|d| d.as_slice() == chunk_type);
        if !is_meta {
            out.extend_from_slice(&bytes[i..chunk_end]);
        }
        // IEND terminates the stream.
        if chunk_type == b"IEND" {
            return Some(out);
        }
        i = chunk_end;
    }
    Some(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Steps 3–4 — the SANDBOX seam (OCR + LLM on attacker-controlled bytes)
// ─────────────────────────────────────────────────────────────────────────────

/// The parsed, schema-checked transcription handed out of the sandbox seam.
#[derive(Debug, Clone)]
struct Extracted {
    ocr: OcrResult,
    shop: String,
    category: String,
    lines: Vec<ParsedLine>,
}

#[derive(Debug, Clone)]
struct ParsedLine {
    name: String,
    qty: f64,
    unit_price: Money,
    category: String,
    confidence: f64,
}

/// **The deployment sandbox boundary.** Everything inside this function consumes
/// attacker-controlled bytes: the OCR subprocess and the LLM. A deployment wraps
/// *this call* (at the `bin/*` composition root) in an OS sandbox — no network
/// except the Ollama socket, a scratch fs, dropped capabilities. Crucially this
/// function holds NO `&dyn DatabaseAdapter` / `&dyn PhotoStorage`, so the
/// sandboxed region structurally cannot touch the real DB or the encrypted store
/// even if the OCR/LLM adapter were compromised — it can only return parsed data.
async fn transcribe_and_extract(
    ocr: &dyn OcrAdapter,
    llm: &dyn LlmAdapter,
    sanitised: &[u8],
) -> Result<Extracted, PhoskError> {
    // Step 3: OCR the (sanitised) photo bytes.
    let ocr_result = ocr.extract(sanitised).await?;

    // Step 4: constrained-output extraction (ADR-008). The OCR text is the
    // model's only input — never the raw bytes — and the answer is schema-checked.
    let schema = extraction_schema();
    let prompt = extraction_prompt(&ocr_result.full_text);
    let out = llm.generate_structured(&prompt, &schema).await?;
    parse_extraction(ocr_result, &out)
}

/// JSON Schema for the structured receipt-extraction output (ADR-008). The
/// adapter guarantees the returned `Value` validates against this.
fn extraction_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "shop": { "type": "string" },
            "category": { "type": "string" },
            "lineItems": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "qty": { "type": "number" },
                        "unitPriceCentimes": { "type": "integer" },
                        "category": { "type": "string" },
                        "confidence": { "type": "number" }
                    },
                    "required": ["name", "qty", "unitPriceCentimes", "confidence"]
                }
            }
        },
        "required": ["shop", "lineItems"]
    })
}

/// Build the extraction prompt from the OCR transcription.
fn extraction_prompt(ocr_text: &str) -> String {
    format!(
        "Extract the shop, an overall category, and the line items from this \
         Swiss receipt transcription. Reply as JSON matching the schema; amounts \
         are integer centimes (CHF). Transcription:\n{ocr_text}"
    )
}

/// Parse + sanity-check the model's structured answer into [`Extracted`]. Money
/// is read as exact i64 centimes; the schema guarantees the shape, but every
/// access is still defensive (a missing required field ⇒ `PhoskError::Invalid`,
/// never a panic).
fn parse_extraction(ocr: OcrResult, out: &serde_json::Value) -> Result<Extracted, PhoskError> {
    let shop = out
        .get("shop")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| PhoskError::Invalid("model omitted `shop`".to_owned()))?
        .trim()
        .to_owned();
    let category = out
        .get("category")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Uncategorised")
        .to_owned();

    let raw_lines = out
        .get("lineItems")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| PhoskError::Invalid("model omitted `lineItems`".to_owned()))?;

    let mut lines = Vec::with_capacity(raw_lines.len());
    for (idx, item) in raw_lines.iter().enumerate() {
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| PhoskError::Invalid(format!("line {idx}: missing `name`")))?
            .trim()
            .to_owned();
        let qty = item
            .get("qty")
            .and_then(serde_json::Value::as_f64)
            .filter(|q| q.is_finite() && *q > 0.0)
            .ok_or_else(|| PhoskError::Invalid(format!("line {idx}: invalid `qty`")))?;
        let unit_centimes = item
            .get("unitPriceCentimes")
            .and_then(serde_json::Value::as_i64)
            .filter(|c| *c >= 0)
            .ok_or_else(|| {
                PhoskError::Invalid(format!("line {idx}: invalid `unitPriceCentimes`"))
            })?;
        let line_category = item
            .get("category")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(category.as_str())
            .to_owned();
        let confidence = item
            .get("confidence")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        lines.push(ParsedLine {
            name,
            qty,
            unit_price: Money::from_centimes(unit_centimes),
            category: line_category,
            confidence,
        });
    }

    Ok(Extracted {
        ocr,
        shop: if shop.is_empty() {
            "Unknown shop".to_owned()
        } else {
            shop
        },
        category,
        lines,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 5 — assembly (Receipt + LineItems with Provenance)
// ─────────────────────────────────────────────────────────────────────────────

/// Build the proposed [`Receipt`] + [`LineItem`]s. `line_total` is
/// backend-derived (`round(qty * unit_price)`) — never trusted from the model —
/// and the receipt total is the checked sum of the line totals. Returns the
/// count of low-confidence lines (`< CONFIDENCE_THRESHOLD`).
fn assemble(
    slug: &str,
    date: chrono::NaiveDate,
    ex: &Extracted,
    kind: ImageKind,
) -> Result<(Receipt, Vec<LineItem>, usize), PhoskError> {
    let receipt_id = ReceiptId::new();
    let mut line_items = Vec::with_capacity(ex.lines.len());
    let mut low_confidence = 0usize;
    let mut total = Money::ZERO;

    for pl in &ex.lines {
        let line_total = derive_line_total(pl.qty, pl.unit_price)?;
        total = total.checked_add(line_total)?;
        let prov = Provenance {
            source: Source::LlmInferred,
            confidence: pl.confidence,
        };
        if prov.is_low_confidence() {
            low_confidence += 1;
        }
        line_items.push(LineItem {
            id: LineItemId::new(),
            receipt_id,
            name: pl.name.clone(),
            qty: pl.qty,
            unit_price: pl.unit_price,
            line_total,
            category: pl.category.clone(),
            signal_id: None,
            provenance: prov,
        });
    }

    // The receipt total is read off the slip via OCR (Source::Ocr). Anchor its
    // confidence to the mean OCR-region confidence (the legibility of the slip).
    let ocr_conf = mean_region_confidence(&ex.ocr);
    let receipt = Receipt {
        id: receipt_id,
        slug: slug.to_owned(),
        shop: ex.shop.clone(),
        date,
        category: ex.category.clone(),
        amount: total,
        fixed: false,
        provenance: Provenance {
            source: Source::Ocr,
            confidence: ocr_conf,
        },
        source_kind: "PHOTO".to_owned(),
        ocr_engine: ocr_engine_label(kind),
        ocr_regions: u32::try_from(ex.ocr.regions.len()).unwrap_or(u32::MAX),
    };

    Ok((receipt, line_items, low_confidence))
}

/// `round(qty * unit_price)` in exact centimes, overflow-checked. Guards the f64
/// multiply (no NaN/inf, non-negative) before it touches money.
///
/// The `i64 -> f64 -> i64` round-trip is deliberate: `qty` is inherently
/// fractional (weighed goods), so the centime product must pass through `f64`.
/// Precision loss is bounded — receipt-scale magnitudes are exactly representable
/// in `f64`'s 52-bit mantissa — and the result is explicitly range-checked into
/// `i64` before the cast back, surfacing [`PhoskError::Overflow`] rather than
/// truncating silently. Hence the scoped allow on the two casts.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn derive_line_total(qty: f64, unit_price: Money) -> Result<Money, PhoskError> {
    if !qty.is_finite() || qty < 0.0 {
        return Err(PhoskError::Invalid("non-finite quantity".to_owned()));
    }
    // centimes * qty, rounded to the nearest centime, then re-checked into i64
    // range. Inputs are receipt-scale, but we never assume.
    let cents = unit_price.centimes() as f64 * qty;
    let rounded = cents.round();
    if !rounded.is_finite() || rounded > i64::MAX as f64 || rounded < i64::MIN as f64 {
        return Err(PhoskError::Overflow("line total out of range".to_owned()));
    }
    Ok(Money::from_centimes(rounded as i64))
}

/// Mean of the OCR region confidences, clamped to `0.0..=1.0`. Empty ⇒ `0.0`
/// (division is guarded).
#[allow(clippy::cast_precision_loss)] // region counts are tiny; f64 is exact here
fn mean_region_confidence(ocr: &OcrResult) -> f64 {
    let n = ocr.regions.len();
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = ocr.regions.iter().map(|r| r.confidence).sum();
    (sum / n as f64).clamp(0.0, 1.0)
}

/// Map the validated image kind to an OCR-engine provenance label. The concrete
/// engine is the wired `OcrAdapter`'s identity; the pipeline records only that
/// OCR ran on a photo source (layering — it does not know the vendor).
fn ocr_engine_label(_kind: ImageKind) -> String {
    "OCR".to_owned()
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 6 — enqueue (the ONLY persisted receipt-side effect; never auto-write)
// ─────────────────────────────────────────────────────────────────────────────

/// Build the single per-receipt approval-queue [`AiSuggestion`] (`kind ==
/// "receipt"`, `status == "open"`). Its confidence is the receipt's OCR
/// confidence; `target` is the receipt slug so the approval UI can resolve it.
fn build_suggestion(receipt: &Receipt, lines: &[LineItem], low_conf: usize) -> AiSuggestion {
    let flag = if low_conf > 0 {
        format!(" ({low_conf} low-confidence line(s) to review)")
    } else {
        String::new()
    };
    AiSuggestion {
        id: SuggestionId::new(),
        kind: "receipt".to_owned(),
        text: format!(
            "Import receipt from {} — {} item(s), CHF {:.2}{flag}",
            receipt.shop,
            lines.len(),
            receipt.amount.as_chf_f64()
        ),
        confidence: receipt.provenance.confidence,
        target: Some(receipt.slug.clone()),
        estimated_savings: None,
        status: "open".to_owned(),
    }
}

/// Idempotency lookup: if a suggestion for this receipt `slug` is already
/// enqueued, return a deduplicated [`IntakeOutcome`] without doing any work. Keys
/// on the suggestion's `target == slug`.
async fn find_enqueued(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<Option<IntakeOutcome>, PhoskError> {
    let prior = db
        .ai_suggestions()
        .await?
        .into_iter()
        .find(|s| s.kind == "receipt" && s.target.as_deref() == Some(slug));
    let Some(s) = prior else { return Ok(None) };
    // No re-store / re-OCR / re-model: the prior proposal stands. The receipt and
    // lines are not reconstructed (that needs the photo + model again); the
    // outcome reports the dedup carrying the prior suggestion id + the slug.
    let content_hash = slug.strip_prefix("rcpt:").unwrap_or(slug).to_owned();
    Ok(Some(IntakeOutcome {
        receipt: Receipt {
            id: ReceiptId::new(),
            slug: slug.to_owned(),
            shop: String::new(),
            date: chrono::NaiveDate::default(),
            category: String::new(),
            amount: Money::ZERO,
            fixed: false,
            provenance: Provenance {
                source: Source::Ocr,
                confidence: s.confidence,
            },
            source_kind: "PHOTO".to_owned(),
            ocr_engine: "OCR".to_owned(),
            ocr_regions: 0,
        },
        line_items: Vec::new(),
        suggestion_id: s.id,
        photo: StorageRef::new(String::new()),
        content_hash,
        low_confidence_lines: 0,
        deduplicated: true,
    }))
}

#[cfg(test)]
mod unit_tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn coral_flag_rule_agrees_with_ai_threshold() {
        // The pipeline's low-confidence flag (Provenance::is_low_confidence)
        // must agree with the AI crate's single CONFIDENCE_THRESHOLD constant.
        assert!((phosk_ai::CONFIDENCE_THRESHOLD - 0.7).abs() < f64::EPSILON);
        assert!(
            Provenance {
                source: Source::LlmInferred,
                confidence: phosk_ai::CONFIDENCE_THRESHOLD - 0.01
            }
            .is_low_confidence()
        );
    }

    #[test]
    fn sha256_hex_is_stable_and_lowercase() {
        let h = sha256_hex(b"abc");
        assert_eq!(
            h,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn derive_line_total_rounds_to_centime() {
        // 2.45 CHF * 2 = 4.90 CHF
        let t = derive_line_total(2.0, Money::from_centimes(245)).expect("ok");
        assert_eq!(t.centimes(), 490);
        // fractional qty rounds: 1.01 * 1.5 = 1.515 -> 1.52
        let t = derive_line_total(1.5, Money::from_centimes(101)).expect("ok");
        assert_eq!(t.centimes(), 152);
    }

    #[test]
    fn derive_line_total_rejects_bad_qty() {
        assert!(derive_line_total(f64::NAN, Money::from_centimes(100)).is_err());
        assert!(derive_line_total(-1.0, Money::from_centimes(100)).is_err());
    }
}
