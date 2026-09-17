//! Transactions read-model (F3).
//!
//! Backs the Transactions page (filterable dated list, accordion receipt lines,
//! full receipt screen with OCR reference) and the Dashboard "Recent" tape.
//! Mirrors React `GET /transactions` (with filter params), `/transactions/{id}`,
//! `/transactions/{id}/lines`.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One receipt row in the list (`TransactionListDto::transactions` element).
///
/// `date` is the day label the page groups on; `item_count` / `low_conf_count`
/// drive the meta marks; `signal_ids` are the tracked item-signals this receipt
/// feeds; `fixed` marks a standing charge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDto {
    /// Stable id.
    pub id: String,
    /// Day label, e.g. `"16 JUN"` (the list groups by this).
    pub date: String,
    /// Shop name.
    pub shop: String,
    /// Primary category.
    pub category: String,
    /// Receipt total.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Number of line items.
    pub item_count: u32,
    /// Count of low-confidence lines (drives the ⚠ mark).
    pub low_conf_count: u32,
    /// Tracked item-signal ids this receipt feeds (⌁ marks).
    pub signal_ids: Vec<String>,
    /// `true` for a fixed/standing charge.
    pub fixed: bool,
}

/// The list summary band (`TransactionListDto::summary`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnSummaryDto {
    /// Number of entries in the current filter.
    pub entry_count: u32,
    /// Total CHF across the filtered entries.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_amount: Money,
    /// Human period label, e.g. `"JUN 2026"`.
    pub period_label: String,
}

/// `GET /transactions` — the filtered list + summary + filter option lists.
///
/// `transactions` is already filtered/sorted server-side per the params; the
/// page just groups by `date`. `available_shops`/`available_categories` populate
/// the filter dropdowns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionListDto {
    /// The filtered receipts.
    pub transactions: Vec<TransactionDto>,
    /// Roll-up for the page header.
    pub summary: TxnSummaryDto,
    /// Shop options for the filter dropdown.
    pub available_shops: Vec<String>,
    /// Category options for the filter dropdown.
    pub available_categories: Vec<String>,
}

/// One parsed receipt line (`TxnLinesDto::lines` element).
///
/// `line_total` is backend-derived (not recomputed client-side); `confidence`
/// 0–1 (lines < 0.7 get the amber `--warn` flag + CONFIRM / CORRECT); `signal_id`
/// links a tracked item-signal pill (else the line shows its `category`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnLineDto {
    /// Item name.
    pub name: String,
    /// Quantity.
    pub qty: f64,
    /// Unit price.
    #[serde(with = "phosk_model::money_centimes")]
    pub unit_price: Money,
    /// Line total (backend-derived).
    #[serde(with = "phosk_model::money_centimes")]
    pub line_total: Money,
    /// Line category.
    pub category: String,
    /// Tracked item-signal id, if any (empty = plain category tag).
    pub signal_id: String,
    /// OCR/LLM reading confidence, 0–1.
    pub confidence: f64,
}

/// `GET /transactions/{id}/lines` — the parsed lines + signal feeds + low-conf count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnLinesDto {
    /// The parsed line items.
    pub lines: Vec<TxnLineDto>,
    /// Tracked item-signal ids this receipt feeds.
    pub sigs: Vec<String>,
    /// Count of lines below the 0.7 confidence threshold.
    pub low_conf: u32,
}

/// The receipt photo source descriptor (`TxnDetailDto::source`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnSourceDto {
    /// Source kind, e.g. `"PHOTO"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// OCR engine label, e.g. `"PADDLEOCR"`.
    pub ocr_engine: String,
}

/// `GET /transactions/{id}` — receipt detail for the full-screen receipt view.
///
/// `avg_confidence` drives the reading-confidence bar; `ocr_regions` is the
/// count of detected boxes (the receipt screen also falls back to line count).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnDetailDto {
    /// Stable id (echoed).
    pub id: String,
    /// Average reading confidence across lines, 0–1.
    pub avg_confidence: f64,
    /// Photo/OCR source descriptor.
    pub source: TxnSourceDto,
    /// Detected OCR region count.
    pub ocr_regions: u32,
}

/// Transaction filters for `GET /transactions` (all optional, empty = "all").
///
/// `period` is `day|week|month|quarter|year` (empty = ALL); `shop`/`category`
/// narrow the set; `sort` is `date|amount|shop`; `q` is free-text search.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnFilter {
    /// Horizon: `""|day|week|month|quarter|year`.
    pub period: String,
    /// Shop filter (empty = all).
    pub shop: String,
    /// Category filter (empty = all).
    pub category: String,
    /// Sort key: `date|amount|shop`.
    pub sort: String,
    /// Free-text query.
    pub q: String,
}

/// The filtered transaction list (`GET /transactions`).
///
/// REAL: composes `phosk_ledger::transactions::list_transactions` — it resolves
/// the `period` window, applies the shop/category/q filters, sorts by `sort`, and
/// derives each row's item/low-conf/signal marks from its lines.
#[server]
pub async fn list_transactions(filter: TxnFilter) -> Result<TransactionListDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let svc_filter = phosk_ledger::transactions::TxnFilter {
            period: filter.period,
            shop: filter.shop,
            category: filter.category,
            sort: filter.sort,
            q: filter.q,
        };
        let list = phosk_ledger::transactions::list_transactions(
            session.db(),
            crate::data::today(),
            svc_filter,
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(TransactionListDto {
            transactions: list.transactions.into_iter().map(map_txn).collect(),
            summary: TxnSummaryDto {
                entry_count: list.summary.entry_count,
                total_amount: list.summary.total_amount,
                period_label: list.summary.period_label,
            },
            available_shops: list.available_shops,
            available_categories: list.available_categories,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = filter;
        Err(ServerFnError::new("server-only"))
    }
}

/// One receipt's parsed lines (`GET /transactions/{id}/lines`).
///
/// REAL: composes `phosk_ledger::line_items::transaction_lines` for the receipt
/// slug.
#[server]
pub async fn get_transaction_lines(id: String) -> Result<TxnLinesDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let l = phosk_ledger::line_items::transaction_lines(session.db(), &id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(TxnLinesDto {
            lines: l.lines.into_iter().map(map_line).collect(),
            sigs: l.sigs,
            low_conf: l.low_conf,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// One receipt's detail (`GET /transactions/{id}`).
///
/// REAL: composes `phosk_ledger::transactions::transaction_detail` for the slug.
#[server]
pub async fn get_transaction(id: String) -> Result<TxnDetailDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let d = phosk_ledger::transactions::transaction_detail(session.db(), &id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(TxnDetailDto {
            id: d.id,
            avg_confidence: d.avg_confidence,
            source: TxnSourceDto {
                kind: d.source.kind,
                ocr_engine: d.source.ocr_engine,
            },
            ocr_regions: d.ocr_regions,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

// ── mappers (service DTO → wire DTO) ───────────────────────────────────────────

/// Map a `phosk_ledger` receipt row onto the wire [`TransactionDto`].
#[cfg(feature = "server-deps")]
fn map_txn(t: phosk_ledger::transactions::TransactionDto) -> TransactionDto {
    TransactionDto {
        id: t.id,
        date: t.date,
        shop: t.shop,
        category: t.category,
        amount: t.amount,
        item_count: t.item_count,
        low_conf_count: t.low_conf_count,
        signal_ids: t.signal_ids,
        fixed: t.fixed,
    }
}

/// Map a `phosk_ledger` parsed line onto the wire [`TxnLineDto`].
#[cfg(feature = "server-deps")]
fn map_line(l: phosk_ledger::line_items::TxnLineDto) -> TxnLineDto {
    TxnLineDto {
        name: l.name,
        qty: l.qty,
        unit_price: l.unit_price,
        line_total: l.line_total,
        category: l.category,
        signal_id: l.signal_id,
        confidence: l.confidence,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// T31 · review / correct one receipt line (write path)
// ════════════════════════════════════════════════════════════════════════════

/// A reviewed receipt line as the user left it in the editor (the
/// [`correct_transaction_line`] input).
///
/// The line is addressed by its receipt's stable id (`receipt_id`, the slug the
/// list returns) and its zero-based position in [`get_transaction_lines`] order
/// (`line_index`). `expected_name` is the name the page displayed: if the line at
/// that position no longer carries it, the request is refused rather than
/// editing the wrong line.
///
/// Each value is raw editor text (`None` = not part of this request). The server
/// validates all of them before writing anything, then writes only the fields
/// whose value really changed (one audited `correct_line` call each). A request
/// that changes nothing CONFIRMS the reading: the line becomes user-reviewed and
/// its low-confidence flag clears.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnLineCorrection {
    /// Receipt id (slug) the line belongs to.
    pub receipt_id: String,
    /// Zero-based position of the line in the receipt's line list.
    pub line_index: u32,
    /// The line name the page displayed (stale-read guard).
    pub expected_name: String,
    /// Corrected item name.
    pub name: Option<String>,
    /// Corrected category.
    pub category: Option<String>,
    /// Corrected quantity as typed (`"2"`, `"0.5"`, `"0,5"`).
    pub qty: Option<String>,
    /// Corrected unit price in CHF as typed (`"4.20"`, `"CHF 1'234.50"`).
    /// Parsed to exact centimes on the server, never through a float.
    pub unit_price: Option<String>,
}

/// Save a reviewed or corrected receipt line.
///
/// REAL: validates the request, then composes
/// `phosk_ledger::line_items::correct_line` once per changed field (each call
/// re-derives the line total, stamps `UserModified` provenance and appends a
/// correction event). Failures carry a short message that is safe to show as-is
/// (see [`correction_error_text`]).
#[server]
pub async fn correct_transaction_line(fix: TxnLineCorrection) -> Result<(), ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        correct_transaction_line_with(session.db(), fix).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = fix;
        Err(ServerFnError::new("server-only"))
    }
}

/// The text to show for a failed correction: the server's own message (always
/// user-safe for [`correct_transaction_line`]), or a generic line when the
/// server could not be reached.
#[must_use]
pub fn correction_error_text(err: &ServerFnError) -> String {
    match err {
        ServerFnError::ServerError { message, .. } => message.clone(),
        _ => "Could not reach the server. Try again.".to_owned(),
    }
}

/// The logic behind [`correct_transaction_line`], driven through the database
/// PORT so tests can run it against a fresh in-memory store.
///
/// Order matters: every value is validated first (all-or-nothing), then the
/// addressed line is resolved and checked against `expected_name`, and only then
/// is anything written.
#[cfg(feature = "server-deps")]
pub(crate) async fn correct_transaction_line_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    fix: TxnLineCorrection,
) -> Result<(), ServerFnError> {
    use phosk_ledger::line_items::correct_line;

    let edits = line_fix::LineEdits::parse(&fix).map_err(line_fix::refused)?;

    let receipt = db
        .receipt_by_slug(&fix.receipt_id)
        .await
        .map_err(line_fix::failed)?;
    let lines = db.line_items(receipt.id).await.map_err(line_fix::failed)?;
    let current = usize::try_from(fix.line_index)
        .ok()
        .and_then(|i| lines.get(i))
        .ok_or_else(line_fix::gone)?;
    if current.name != fix.expected_name {
        return Err(line_fix::stale());
    }

    let changes = edits.changes_from(current);
    if changes.is_empty() {
        // Nothing differs: the user accepted the reading as-is.
        return correct_line(db, current.id, "confirmed", "true")
            .await
            .map_err(line_fix::failed);
    }
    for (field, value) in changes {
        correct_line(db, current.id, field, &value)
            .await
            .map_err(line_fix::failed)?;
    }
    Ok(())
}

/// Validation, CHF parsing and error mapping for [`correct_transaction_line`].
///
/// Validation mirrors the receipt intake rules in `phosk_pipeline_receipt`
/// (quantity finite and > 0, unit price >= 0, trimmed labels) plus length and
/// magnitude caps. Every message here is written for the user; store errors are
/// mapped by kind and never echo their internal detail.
#[cfg(feature = "server-deps")]
mod line_fix {
    use dioxus::prelude::ServerFnError;
    use phosk_core::error::PhoskError;
    use phosk_model::LineItem;

    /// Longest accepted item name, in characters.
    const MAX_NAME_CHARS: usize = 120;
    /// Longest accepted category, in characters.
    const MAX_CATEGORY_CHARS: usize = 60;
    /// Largest accepted quantity (pieces or weighed units).
    const MAX_QTY: f64 = 100_000.0;
    /// Largest accepted unit price: CHF 1'000'000, in centimes.
    const MAX_UNIT_PRICE_CENTIMES: i64 = 100_000_000;

    /// The validated, normalised values of one request.
    pub(super) struct LineEdits {
        name: Option<String>,
        category: Option<String>,
        qty: Option<f64>,
        unit_price_centimes: Option<i64>,
    }

    impl LineEdits {
        /// Validate every present value; the first problem refuses the lot.
        pub(super) fn parse(fix: &super::TxnLineCorrection) -> Result<Self, PhoskError> {
            Ok(Self {
                name: fix
                    .name
                    .as_deref()
                    .map(|v| label(v, "Item name", MAX_NAME_CHARS))
                    .transpose()?,
                category: fix
                    .category
                    .as_deref()
                    .map(|v| label(v, "Category", MAX_CATEGORY_CHARS))
                    .transpose()?,
                qty: fix.qty.as_deref().map(parse_qty).transpose()?,
                unit_price_centimes: fix
                    .unit_price
                    .as_deref()
                    .map(parse_chf_centimes)
                    .transpose()?,
            })
        }

        /// The `(field, value)` pairs `correct_line` must write: only values that
        /// differ from the stored line, in the text form `correct_line` parses.
        pub(super) fn changes_from(&self, line: &LineItem) -> Vec<(&'static str, String)> {
            let mut out = Vec::new();
            if let Some(name) = self.name.as_ref().filter(|n| **n != line.name) {
                out.push(("name", name.clone()));
            }
            if let Some(cat) = self.category.as_ref().filter(|c| **c != line.category) {
                out.push(("category", cat.clone()));
            }
            // Bit-exact on purpose: the editor pre-fills the stored qty in its
            // shortest round-trip form, so an untouched field parses back to the
            // very same bits and is not re-written.
            if let Some(qty) = self.qty.filter(|q| q.to_bits() != line.qty.to_bits()) {
                out.push(("qty", qty.to_string()));
            }
            if let Some(cents) = self
                .unit_price_centimes
                .filter(|c| *c != line.unit_price.centimes())
            {
                out.push(("unit_price", cents.to_string()));
            }
            out
        }
    }

    /// A trimmed, non-empty, bounded, control-character-free label.
    fn label(raw: &str, what: &str, max_chars: usize) -> Result<String, PhoskError> {
        let value = raw.trim();
        if value.is_empty() {
            return Err(invalid(format!("{what} cannot be empty.")));
        }
        if value.chars().count() > max_chars {
            return Err(invalid(format!(
                "{what} is too long (at most {max_chars} characters)."
            )));
        }
        if value.chars().any(char::is_control) {
            return Err(invalid(format!(
                "{what} contains characters that are not allowed."
            )));
        }
        Ok(value.to_owned())
    }

    /// A typed quantity: plain digits with an optional `.`/`,` decimal part (no
    /// sign, exponent, `inf` or `NaN`), greater than zero and at most
    /// [`MAX_QTY`].
    fn parse_qty(raw: &str) -> Result<f64, PhoskError> {
        const SHAPE: &str = "Quantity must be a number such as 2 or 0.5.";
        let text = raw.trim().replace(',', ".");
        let (whole, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
        if !is_digits(whole) || !is_digits(frac) || whole.len() + frac.len() == 0 {
            return Err(invalid(SHAPE));
        }
        let qty: f64 = text.parse().map_err(|_| invalid(SHAPE))?;
        if qty <= 0.0 {
            return Err(invalid("Quantity must be greater than zero."));
        }
        if !qty.is_finite() || qty > MAX_QTY {
            return Err(invalid("Quantity is too large (at most 100000)."));
        }
        Ok(qty)
    }

    /// A typed CHF amount → exact centimes, without ever going through a float.
    ///
    /// Accepts an optional `CHF` prefix, Swiss grouping (`'`, `’`, spaces) and a
    /// `.` or `,` decimal separator with at most two decimals. Refuses signs,
    /// exponents and anything above [`MAX_UNIT_PRICE_CENTIMES`].
    ///
    /// Private on purpose: a shared CHF parser is being added to `phosk_core`
    /// separately; this should be folded into it once that lands.
    fn parse_chf_centimes(raw: &str) -> Result<i64, PhoskError> {
        const SHAPE: &str = "Unit price must be an amount in CHF such as 4.20.";
        let mut text = raw.trim();
        if let Some(rest) = text
            .get(..3)
            .filter(|p| p.eq_ignore_ascii_case("CHF"))
            .and_then(|_| text.get(3..))
        {
            text = rest.trim_start();
        }
        if text.starts_with(['-', '\u{2212}']) {
            return Err(invalid("Unit price cannot be negative."));
        }
        let text: String = text
            .chars()
            .filter(|c| !matches!(c, '\'' | '\u{2019}' | ' ' | '\u{a0}' | '\u{202f}'))
            .map(|c| if c == ',' { '.' } else { c })
            .collect();
        let (whole, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
        if !is_digits(whole) || !is_digits(frac) || whole.len() + frac.len() == 0 {
            return Err(invalid(SHAPE));
        }
        if frac.len() > 2 {
            return Err(invalid("Unit price can have at most 2 decimals."));
        }
        let too_large = || invalid("Unit price is too large (at most CHF 1'000'000).");
        // Digits only by now, so a failed parse can only be an i64 overflow.
        let francs: i64 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| too_large())?
        };
        let cents: i64 = match frac.len() {
            0 => 0,
            1 => frac.parse::<i64>().map_err(|_| invalid(SHAPE))? * 10,
            _ => frac.parse().map_err(|_| invalid(SHAPE))?,
        };
        let centimes = francs
            .checked_mul(100)
            .and_then(|c| c.checked_add(cents))
            .ok_or_else(too_large)?;
        if centimes > MAX_UNIT_PRICE_CENTIMES {
            return Err(too_large());
        }
        Ok(centimes)
    }

    /// `true` when every byte is an ASCII digit (vacuously for `""`).
    fn is_digits(s: &str) -> bool {
        s.bytes().all(|b| b.is_ascii_digit())
    }

    fn invalid(message: impl Into<String>) -> PhoskError {
        PhoskError::Invalid(message.into())
    }

    fn reply(code: u16, message: &str) -> ServerFnError {
        ServerFnError::ServerError {
            message: message.to_owned(),
            code,
            details: None,
        }
    }

    /// A refused request (400). Only [`LineEdits::parse`] feeds this, and its
    /// `Invalid` messages are written for the user, so they pass through.
    pub(super) fn refused(err: PhoskError) -> ServerFnError {
        match err {
            PhoskError::Invalid(message) => reply(400, &message),
            other => failed(other),
        }
    }

    /// A store or service failure, mapped by kind. The carried detail may name
    /// internals (ids, storage errors), so it is never forwarded.
    pub(super) fn failed(err: PhoskError) -> ServerFnError {
        match err {
            PhoskError::NotFound(_) => gone(),
            PhoskError::Invalid(_) | PhoskError::InvalidDate(_) | PhoskError::Overflow(_) => {
                reply(500, "The correction could not be saved. Try again.")
            }
        }
    }

    /// The addressed receipt or line does not exist (404).
    pub(super) fn gone() -> ServerFnError {
        reply(
            404,
            "This receipt line no longer exists. Refresh and try again.",
        )
    }

    /// The line at that position is not the one the page showed (409).
    pub(super) fn stale() -> ServerFnError {
        reply(
            409,
            "This receipt changed since it was loaded. Refresh and try again.",
        )
    }
}

#[cfg(all(test, feature = "server-deps"))]
mod correct_transaction_line_tests {
    use super::*;
    use phosk_adapter_db::DatabaseAdapter;
    use phosk_db_memory::MemoryDb;
    use phosk_model::{LineItem, Source};

    /// A FRESH seeded store per test (never the process-global stack).
    fn seeded() -> MemoryDb {
        MemoryDb::seeded().expect("the seed builds")
    }

    /// The stored line at `index` on receipt `slug`.
    async fn stored_line(db: &MemoryDb, slug: &str, index: usize) -> LineItem {
        let receipt = db.receipt_by_slug(slug).await.expect("seeded receipt");
        let mut lines = db.line_items(receipt.id).await.expect("receipt lines");
        assert!(index < lines.len(), "seeded line {index} exists on {slug}");
        lines.swap_remove(index)
    }

    /// A request addressing `slug[index]` that changes nothing yet.
    fn request(slug: &str, index: u32, expected_name: &str) -> TxnLineCorrection {
        TxnLineCorrection {
            receipt_id: slug.to_owned(),
            line_index: index,
            expected_name: expected_name.to_owned(),
            ..TxnLineCorrection::default()
        }
    }

    /// The (status code, message) a refused request carries.
    fn refusal(err: &ServerFnError) -> (u16, String) {
        match err {
            ServerFnError::ServerError { code, message, .. } => (*code, message.clone()),
            other => panic!("expected a ServerError, got {other:?}"),
        }
    }

    // Seed facts used below (receipt "t1"):
    //   [1] "Bananas"          qty 1.2 × 320 = 384, conf 0.91
    //   [2] "Bread (unclear)"  qty 1   × 431 = 431, conf 0.58 (the low-confidence line)
    //   [3] "Mixed basket"     qty 1   × 4500,      conf 0.95
    const BREAD: &str = "Bread (unclear)";

    #[tokio::test]
    async fn corrected_name_is_saved_trimmed_and_stamped_user_modified() {
        let db = seeded();
        let before = stored_line(&db, "t1", 2).await;
        assert!(before.provenance.is_low_confidence(), "precondition");

        let fix = TxnLineCorrection {
            name: Some("  Sourdough loaf  ".to_owned()),
            ..request("t1", 2, BREAD)
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("a valid name is saved");

        let after = stored_line(&db, "t1", 2).await;
        assert_eq!(after.id, before.id, "the same line was edited");
        assert_eq!(after.name, "Sourdough loaf", "stored trimmed");
        assert_eq!(after.provenance.source, Source::UserModified);
        assert!(
            !after.provenance.is_low_confidence(),
            "a user-reviewed line is no longer flagged"
        );
    }

    #[tokio::test]
    async fn unit_price_text_is_parsed_to_exact_centimes_and_total_rederived() {
        let db = seeded();
        let fix = TxnLineCorrection {
            unit_price: Some("CHF 5.00".to_owned()),
            ..request("t1", 1, "Bananas")
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("a valid price is saved");

        let after = stored_line(&db, "t1", 1).await;
        assert_eq!(after.unit_price.centimes(), 500);
        assert_eq!(
            after.line_total.centimes(),
            600,
            "1.2 × 500, backend-derived"
        );
        assert_eq!(after.provenance.source, Source::UserModified);
    }

    #[tokio::test]
    async fn unit_price_accepts_swiss_grouping_and_decimal_comma() {
        let db = seeded();
        let fix = TxnLineCorrection {
            unit_price: Some("1\u{2019}234,5".to_owned()),
            ..request("t1", 3, "Mixed basket")
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("a Swiss-formatted price is accepted");

        let after = stored_line(&db, "t1", 3).await;
        assert_eq!(after.unit_price.centimes(), 123_450);
        assert_eq!(after.line_total.centimes(), 123_450);
    }

    #[tokio::test]
    async fn qty_text_is_parsed_and_total_rederived() {
        let db = seeded();
        let fix = TxnLineCorrection {
            qty: Some(" 2,5 ".to_owned()),
            ..request("t1", 1, "Bananas")
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("a valid qty is saved");

        let after = stored_line(&db, "t1", 1).await;
        assert!((after.qty - 2.5).abs() < f64::EPSILON, "qty is 2.5");
        assert_eq!(after.line_total.centimes(), 800, "2.5 × 320");
    }

    #[tokio::test]
    async fn saving_the_values_unchanged_confirms_the_reading() {
        let db = seeded();
        let before = stored_line(&db, "t1", 2).await;
        // Exactly what the editor pre-fills for this line.
        let fix = TxnLineCorrection {
            name: Some(BREAD.to_owned()),
            category: Some("Groceries".to_owned()),
            qty: Some("1".to_owned()),
            unit_price: Some("4.31".to_owned()),
            ..request("t1", 2, BREAD)
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("an unchanged save is a confirmation");

        let after = stored_line(&db, "t1", 2).await;
        assert_eq!(after.name, before.name);
        assert_eq!(after.category, before.category);
        assert_eq!(after.unit_price, before.unit_price);
        assert_eq!(after.line_total, before.line_total);
        assert_eq!(after.provenance.source, Source::UserModified);

        let lines = phosk_ledger::line_items::transaction_lines(&db, "t1")
            .await
            .expect("t1 lines");
        assert_eq!(lines.low_conf, 0, "the receipt has no flagged line left");
    }

    #[tokio::test]
    async fn a_bare_confirm_request_clears_the_flag() {
        let db = seeded();
        correct_transaction_line_with(&db, request("t1", 2, BREAD))
            .await
            .expect("CONFIRM is accepted");

        let after = stored_line(&db, "t1", 2).await;
        assert_eq!(after.name, BREAD, "nothing but the review state changed");
        assert_eq!(after.provenance.source, Source::UserModified);
        assert!(!after.provenance.is_low_confidence());
    }

    #[tokio::test]
    async fn only_the_changed_field_is_written() {
        let db = seeded();
        let fix = TxnLineCorrection {
            name: Some(BREAD.to_owned()),
            category: Some("Bakery".to_owned()),
            unit_price: Some("4.31".to_owned()),
            ..request("t1", 2, BREAD)
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("category change is saved");

        let after = stored_line(&db, "t1", 2).await;
        assert_eq!(after.category, "Bakery");
        assert_eq!(after.name, BREAD);
        assert_eq!(after.unit_price.centimes(), 431);
    }

    /// `slug[2]` (the bread line) with one field set to `value`.
    fn bread_with(field: &str, value: &str) -> TxnLineCorrection {
        let mut fix = request("t1", 2, BREAD);
        let value = Some(value.to_owned());
        match field {
            "name" => fix.name = value,
            "category" => fix.category = value,
            "qty" => fix.qty = value,
            "unit_price" => fix.unit_price = value,
            other => panic!("unknown test field {other}"),
        }
        fix
    }

    #[tokio::test]
    async fn invalid_values_are_refused_before_anything_is_written() {
        let long_name = "x".repeat(121);
        let mut cases: Vec<(&str, TxnLineCorrection)> = [
            ("blank name", "name", "   "),
            ("overlong name", "name", long_name.as_str()),
            ("control char", "name", "a\u{7}b"),
            ("blank category", "category", ""),
            ("zero qty", "qty", "0"),
            ("negative qty", "qty", "-1"),
            ("NaN qty", "qty", "NaN"),
            ("inf qty", "qty", "inf"),
            ("exponent qty", "qty", "1e3"),
            ("word qty", "qty", "two"),
            ("two separators qty", "qty", "1,2.5"),
            ("huge qty", "qty", "100001"),
            ("negative price", "unit_price", "-4.00"),
            ("minus-sign price", "unit_price", "\u{2212}4.00"),
            ("sub-centime price", "unit_price", "4.315"),
            ("word price", "unit_price", "free"),
            ("empty price", "unit_price", ""),
            ("two dots", "unit_price", "1.2.3"),
            ("i64 overflow", "unit_price", "99999999999999999999"),
            ("over the cap", "unit_price", "1000000.01"),
        ]
        .into_iter()
        .map(|(case, field, value)| (case, bread_with(field, value)))
        .collect();
        // A valid field next to an invalid one: nothing may be written.
        let mixed = TxnLineCorrection {
            qty: Some("0".to_owned()),
            ..bread_with("name", "Sourdough")
        };
        cases.push(("valid name + invalid qty", mixed));

        for (case, fix) in cases {
            let db = seeded();
            let before = stored_line(&db, "t1", 2).await;
            let err = correct_transaction_line_with(&db, fix)
                .await
                .expect_err(case);
            let (code, message) = refusal(&err);
            assert_eq!(code, 400, "{case}: a caller error");
            assert!(!message.is_empty(), "{case}: says what is wrong");
            assert!(
                !message.contains("invalid input:"),
                "{case}: no internal error prefix in {message:?}"
            );
            assert_eq!(
                stored_line(&db, "t1", 2).await,
                before,
                "{case}: the line is untouched"
            );
        }
    }

    #[tokio::test]
    async fn unknown_receipt_is_a_404_without_internal_detail() {
        let db = seeded();
        let err = correct_transaction_line_with(&db, request("no-such-receipt", 0, BREAD))
            .await
            .expect_err("unknown receipt");
        let (code, message) = refusal(&err);
        assert_eq!(code, 404);
        assert!(!message.contains("no-such-receipt"), "{message:?}");
        assert!(!message.contains("not found:"), "{message:?}");
    }

    #[tokio::test]
    async fn line_index_past_the_end_is_a_404() {
        let db = seeded();
        let err = correct_transaction_line_with(&db, request("t1", 99, BREAD))
            .await
            .expect_err("no such line");
        assert_eq!(refusal(&err).0, 404);
    }

    #[tokio::test]
    async fn a_stale_read_is_refused_with_409_and_nothing_written() {
        let db = seeded();
        let before = stored_line(&db, "t1", 1).await;
        // The page believes line 1 is the bread line; it is not.
        let fix = TxnLineCorrection {
            name: Some("Sourdough".to_owned()),
            ..request("t1", 1, BREAD)
        };
        let err = correct_transaction_line_with(&db, fix)
            .await
            .expect_err("stale read");
        assert_eq!(refusal(&err).0, 409);
        assert_eq!(stored_line(&db, "t1", 1).await, before);
    }

    #[test]
    fn error_text_is_the_server_message_or_a_generic_line() {
        let refused = ServerFnError::ServerError {
            message: "Quantity must be greater than zero.".to_owned(),
            code: 400,
            details: None,
        };
        assert_eq!(
            correction_error_text(&refused),
            "Quantity must be greater than zero."
        );

        let transport = ServerFnError::StreamError("socket closed".to_owned());
        let text = correction_error_text(&transport);
        assert!(!text.is_empty());
        assert!(!text.contains("socket"), "no transport detail: {text:?}");
    }
}
