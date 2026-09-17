//! Integration tests for the zero-trust receipt-intake pipeline, driven entirely
//! by the PORT fakes (`FakeOcr`, `FakeLlm`, `InMemoryStorage`, `MemoryDb`) — no
//! live model, OCR service, or disk. They assert the security invariants:
//!
//! - validation rejects non-image + oversize + empty input *before* any work;
//! - EXIF/metadata is stripped from the bytes *before* they reach storage;
//! - a valid photo yields a Receipt ENQUEUED for approval, NOT written to the
//!   ledger;
//! - low-confidence (`< 0.7`) line items are flagged, never dropped;
//! - re-submitting the same bytes is idempotent (no second enqueue / store).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(
    clippy::float_cmp,
    clippy::doc_markdown,
    clippy::unnecessary_literal_bound,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::missing_const_for_fn,
    clippy::trivially_copy_pass_by_ref
)]

use chrono::NaiveDate;
use serde_json::json;

use phosk_adapter_db::DatabaseAdapter;
use phosk_adapter_llm::{FakeLlm, LlmAdapter};
use phosk_adapter_ocr::{FakeOcr, OcrAdapter, OcrRegion, OcrResult};
use phosk_adapter_storage::{InMemoryStorage, PhotoStorage, StorageRef};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_model::BudgetConfig;
use phosk_pipeline_receipt::{IntakePhoto, MAX_PHOTO_BYTES, intake_receipt};

// ── helpers ──────────────────────────────────────────────────────────────────

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid date")
}

/// An empty MemoryDb (no seeded receipts/suggestions) so absolute-count asserts
/// (e.g. "exactly one enqueued", "no ledger receipt") are deterministic.
fn empty_db() -> MemoryDb {
    MemoryDb::new(
        Vec::new(),
        Vec::new(),
        BudgetConfig {
            monthly_budget: Money::from_centimes(420_000),
            savings_target: Money::from_centimes(90_000),
        },
    )
}

/// A minimal but signature-valid JPEG carrying an APP1 (EXIF) segment with a
/// recognisable secret payload, then an SOS + a couple of scan bytes + EOI. Not a
/// decodable image, but structurally a JPEG to `infer` and our marker walker.
fn jpeg_with_exif(secret: &[u8]) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8]; // SOI
    // APP1 (EXIF). length = 2 (len field) + "Exif\0\0" (6) + payload.
    let app1_payload = {
        let mut p = b"Exif\x00\x00".to_vec();
        p.extend_from_slice(secret);
        p
    };
    let app1_len = (app1_payload.len() + 2) as u16;
    v.push(0xFF);
    v.push(0xE1);
    v.extend_from_slice(&app1_len.to_be_bytes());
    v.extend_from_slice(&app1_payload);
    // A COM comment segment carrying another secret (also must be stripped).
    let com_payload = secret;
    let com_len = (com_payload.len() + 2) as u16;
    v.push(0xFF);
    v.push(0xFE);
    v.extend_from_slice(&com_len.to_be_bytes());
    v.extend_from_slice(com_payload);
    // SOS marker, then "entropy data", then EOI.
    v.push(0xFF);
    v.push(0xDA);
    // SOS has a header length too, but our walker copies everything from SOS on
    // verbatim, so we just append a small header + scan body + EOI.
    v.extend_from_slice(&[0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]); // SOS header
    v.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // "scan data"
    v.push(0xFF);
    v.push(0xD9); // EOI
    v
}

/// A minimal signature-valid PNG: 8-byte sig, IHDR, an `eXIf` + `tEXt` metadata
/// chunk (carrying a secret), then IEND. Not decodable, but structurally a PNG.
fn png_with_meta(secret: &[u8]) -> Vec<u8> {
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(&(data.len() as u32).to_be_bytes());
        c.extend_from_slice(kind);
        c.extend_from_slice(data);
        c.extend_from_slice(&[0, 0, 0, 0]); // fake CRC — our walker ignores it
        c
    }
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    v.extend_from_slice(&chunk(b"IHDR", &[0; 13]));
    v.extend_from_slice(&chunk(b"eXIf", secret));
    v.extend_from_slice(&chunk(b"tEXt", secret));
    v.extend_from_slice(&chunk(b"IDAT", &[0x78, 0x9c, 0x00])); // fake image data
    v.extend_from_slice(&chunk(b"IEND", &[]));
    v
}

/// A FakeLlm scripted to return a structured Migros receipt for ANY prompt, with
/// two confident lines and one low-confidence (0.4) line.
fn receipt_llm() -> FakeLlm {
    // The fake matches on EXACT prompt; the pipeline interpolates the OCR text, so
    // we can't predict the prompt. Instead we rely on the fake's *unscripted*
    // generate_structured synth — but that yields placeholder values, not our
    // confidences. So we drive a custom adapter below instead.
    FakeLlm::new()
}

/// A custom LLM adapter returning a fixed structured receipt (the FakeLlm's
/// exact-match scripting can't see the interpolated OCR text in the prompt).
struct ScriptedReceiptLlm {
    out: serde_json::Value,
}

#[async_trait::async_trait]
impl LlmAdapter for ScriptedReceiptLlm {
    fn model(&self) -> &str {
        "scripted-test"
    }
    async fn health(&self) -> Result<bool, PhoskError> {
        Ok(true)
    }
    async fn complete(&self, prompt: &str) -> Result<String, PhoskError> {
        if prompt.is_empty() {
            return Err(PhoskError::Invalid("empty".to_owned()));
        }
        Ok(String::new())
    }
    async fn generate_structured(
        &self,
        prompt: &str,
        _schema: &serde_json::Value,
    ) -> Result<serde_json::Value, PhoskError> {
        if prompt.is_empty() {
            return Err(PhoskError::Invalid("empty".to_owned()));
        }
        Ok(self.out.clone())
    }
}

fn migros_receipt_json() -> serde_json::Value {
    json!({
        "shop": "Migros Genève",
        "category": "Groceries",
        "lineItems": [
            {"name": "Bananes Bio", "qty": 1.0, "unitPriceCentimes": 245, "confidence": 0.95},
            {"name": "Pain complet", "qty": 1.0, "unitPriceCentimes": 320, "confidence": 0.91},
            // low-confidence line — must be flagged, not dropped
            {"name": "Illegible item", "qty": 2.0, "unitPriceCentimes": 150, "confidence": 0.40}
        ]
    })
}

fn scripted_llm() -> ScriptedReceiptLlm {
    ScriptedReceiptLlm {
        out: migros_receipt_json(),
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rejects_non_image_input() {
    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr = FakeOcr::new();
    let llm = scripted_llm();
    // A PDF magic header — definitely not an image.
    let pdf = b"%PDF-1.7\n%hostile payload".to_vec();
    let err = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &pdf,
            captured_on: today(),
        },
    )
    .await
    .expect_err("non-image must be rejected");
    assert!(matches!(err, PhoskError::Invalid(_)), "got {err:?}");
    // Nothing was stored and nothing was enqueued.
    assert!(storage.is_empty().expect("is_empty"));
    assert!(db.ai_suggestions().await.expect("suggestions").is_empty());
}

#[tokio::test]
async fn rejects_empty_and_oversize_input() {
    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr = FakeOcr::new();
    let llm = scripted_llm();

    // empty
    let err = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &[],
            captured_on: today(),
        },
    )
    .await
    .expect_err("empty must be rejected");
    assert!(matches!(err, PhoskError::Invalid(_)));

    // oversize: a valid PNG header but past the cap.
    let mut huge = png_with_meta(b"x");
    huge.resize(MAX_PHOTO_BYTES + 1, 0);
    let err = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &huge,
            captured_on: today(),
        },
    )
    .await
    .expect_err("oversize must be rejected");
    assert!(matches!(err, PhoskError::Invalid(_)));
    assert!(storage.is_empty().expect("is_empty"));
}

#[tokio::test]
async fn exif_is_stripped_before_storage() {
    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr = FakeOcr::new();
    let llm = scripted_llm();

    let secret = b"GPS:46.2044N,6.1432E;SERIAL:ABC123";
    let photo = jpeg_with_exif(secret);
    // sanity: the secret IS in the raw input
    assert!(
        photo.windows(secret.len()).any(|w| w == secret),
        "secret should be present pre-strip"
    );

    let outcome = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &photo,
            captured_on: today(),
        },
    )
    .await
    .expect("valid jpeg should intake");

    // Pull the stored bytes back out and prove the secret is GONE.
    let stored = storage.get(&outcome.photo).await.expect("get stored photo");
    assert!(
        !stored.windows(secret.len()).any(|w| w == secret),
        "EXIF/COM secret must be stripped before storage"
    );
    // The JPEG framing survived (SOI..EOI).
    assert_eq!(&stored[0..2], &[0xFF, 0xD8]);
    assert_eq!(&stored[stored.len() - 2..], &[0xFF, 0xD9]);
}

#[tokio::test]
async fn png_metadata_is_stripped_before_storage() {
    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr = FakeOcr::new();
    let llm = scripted_llm();

    let secret = b"creator=PhoneModelX;location=Geneva";
    let photo = png_with_meta(secret);
    assert!(photo.windows(secret.len()).any(|w| w == secret));

    let outcome = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &photo,
            captured_on: today(),
        },
    )
    .await
    .expect("valid png should intake");

    let stored = storage.get(&outcome.photo).await.expect("get stored png");
    assert!(
        !stored.windows(secret.len()).any(|w| w == secret),
        "PNG eXIf/tEXt secret must be stripped before storage"
    );
    // Signature + IEND survive.
    assert_eq!(&stored[0..4], &[0x89, b'P', b'N', b'G']);
    assert!(stored.windows(4).any(|w| w == b"IEND"));
}

#[tokio::test]
async fn valid_photo_enqueues_receipt_and_never_writes_ledger() {
    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr = FakeOcr::new();
    let llm = scripted_llm();

    let before_receipts = db.all_receipts().await.expect("receipts").len();
    let before_suggestions = db.ai_suggestions().await.expect("suggestions").len();

    let photo = jpeg_with_exif(b"meta");
    let outcome = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &photo,
            captured_on: today(),
        },
    )
    .await
    .expect("intake ok");

    // A proposal was ENQUEUED (suggestion count grew by exactly one).
    let suggestions = db.ai_suggestions().await.expect("suggestions");
    assert_eq!(suggestions.len(), before_suggestions + 1);
    let enq = suggestions
        .iter()
        .find(|s| s.id == outcome.suggestion_id)
        .expect("enqueued suggestion present");
    assert_eq!(enq.kind, "receipt");
    assert_eq!(enq.status, "open"); // awaiting approval
    assert_eq!(enq.target.as_deref(), Some(outcome.receipt.slug.as_str()));

    // The ledger was NOT touched — no receipt was persisted.
    assert_eq!(
        db.all_receipts().await.expect("receipts").len(),
        before_receipts,
        "intake must NOT auto-write a receipt to the ledger"
    );

    // The proposed receipt totals the line items exactly (centimes).
    // 245 + 320 + (2 * 150) = 865
    assert_eq!(outcome.receipt.amount.centimes(), 865);
    assert_eq!(outcome.line_items.len(), 3);
    assert_eq!(outcome.receipt.shop, "Migros Genève");
    assert_eq!(outcome.receipt.source_kind, "PHOTO");
}

#[tokio::test]
async fn low_confidence_lines_are_flagged_not_dropped() {
    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr = FakeOcr::new();
    let llm = scripted_llm();

    let outcome = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &jpeg_with_exif(b"m"),
            captured_on: today(),
        },
    )
    .await
    .expect("intake ok");

    // All three lines are kept (none dropped); exactly one is low-confidence.
    assert_eq!(outcome.line_items.len(), 3);
    assert_eq!(outcome.low_confidence_lines, 1);
    let flagged: Vec<_> = outcome
        .line_items
        .iter()
        .filter(|l| l.provenance.is_low_confidence())
        .collect();
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0].name, "Illegible item");
    // The enqueued suggestion text advertises the review need.
    let s = db
        .ai_suggestions()
        .await
        .expect("suggestions")
        .into_iter()
        .find(|s| s.id == outcome.suggestion_id)
        .expect("present");
    assert!(s.text.contains("low-confidence"), "got: {}", s.text);
}

#[tokio::test]
async fn resubmitting_same_bytes_is_idempotent() {
    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr = FakeOcr::new();
    let llm = scripted_llm();

    let photo = jpeg_with_exif(b"idem");
    let first = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &photo,
            captured_on: today(),
        },
    )
    .await
    .expect("first intake");
    assert!(!first.deduplicated);

    let suggestions_after_first = db.ai_suggestions().await.expect("s").len();
    let stored_after_first = storage.len().expect("len");

    // Re-submit the identical bytes.
    let second = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &photo,
            captured_on: today(),
        },
    )
    .await
    .expect("second intake");

    // Deduplicated: same suggestion id, no new enqueue, no new stored blob.
    assert!(second.deduplicated);
    assert_eq!(second.suggestion_id, first.suggestion_id);
    assert_eq!(second.content_hash, first.content_hash);
    assert_eq!(
        db.ai_suggestions().await.expect("s").len(),
        suggestions_after_first,
        "re-submit must NOT enqueue a second proposal"
    );
    assert_eq!(
        storage.len().expect("len"),
        stored_after_first,
        "re-submit must NOT store the photo again"
    );
}

#[tokio::test]
async fn fake_ocr_and_fake_llm_compose_through_ports() {
    // Sanity that the canonical fakes (FakeOcr canned Migros slip + FakeLlm) wire
    // through the pipeline as &dyn ports. FakeLlm's unscripted structured synth
    // yields schema-shaped placeholders, so we assert it produced *some* enqueue.
    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr: &dyn OcrAdapter = &FakeOcr::new();
    let llm: &dyn LlmAdapter = &receipt_llm();

    // FakeLlm.generate_structured synthesises an object with the schema's keys;
    // `lineItems` becomes an array with one placeholder object → one line.
    let res = intake_receipt(
        &db,
        &storage,
        ocr,
        llm,
        IntakePhoto {
            bytes: &jpeg_with_exif(b"z"),
            captured_on: today(),
        },
    )
    .await;
    // Either it parses the synthesised placeholder (Ok) or rejects a degenerate
    // qty (Err Invalid) — both are acceptable port-composition outcomes; what
    // matters is no panic and no ledger write.
    match res {
        Ok(o) => {
            assert!(!o.deduplicated);
            assert_eq!(db.all_receipts().await.expect("r").len(), 0);
        }
        Err(e) => assert!(matches!(e, PhoskError::Invalid(_))),
    }
}

#[tokio::test]
async fn ocr_region_confidence_drives_receipt_provenance() {
    // A custom OCR adapter with known region confidences → receipt provenance is
    // their mean.
    struct TwoRegionOcr;
    #[async_trait::async_trait]
    impl OcrAdapter for TwoRegionOcr {
        async fn extract(&self, image: &[u8]) -> Result<OcrResult, PhoskError> {
            if image.is_empty() {
                return Err(PhoskError::Invalid("empty".to_owned()));
            }
            Ok(OcrResult {
                full_text: "MIGROS\nTOTAL 8.65".to_owned(),
                regions: vec![
                    OcrRegion {
                        text: "MIGROS".to_owned(),
                        confidence: 0.90,
                        bbox: (0, 0, 10, 10),
                    },
                    OcrRegion {
                        text: "TOTAL 8.65".to_owned(),
                        confidence: 0.80,
                        bbox: (0, 10, 10, 10),
                    },
                ],
            })
        }
    }

    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr = TwoRegionOcr;
    let llm = scripted_llm();

    let outcome = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &jpeg_with_exif(b"q"),
            captured_on: today(),
        },
    )
    .await
    .expect("intake ok");

    assert_eq!(outcome.receipt.ocr_regions, 2);
    // mean(0.90, 0.80) = 0.85
    assert!((outcome.receipt.provenance.confidence - 0.85).abs() < 1e-9);
}

#[tokio::test]
async fn storage_ref_is_opaque_and_resolvable() {
    let db = empty_db();
    let storage = InMemoryStorage::new();
    let ocr = FakeOcr::new();
    let llm = scripted_llm();
    let outcome = intake_receipt(
        &db,
        &storage,
        &ocr,
        &llm,
        IntakePhoto {
            bytes: &jpeg_with_exif(b"r"),
            captured_on: today(),
        },
    )
    .await
    .expect("intake ok");
    // The returned ref resolves to the stored sanitised bytes.
    let _: &StorageRef = &outcome.photo;
    let bytes = storage.get(&outcome.photo).await.expect("resolves");
    assert!(!bytes.is_empty());
}
