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
/// 0–1 (lines < 0.7 render coral + a confirm/correct affordance); `signal_id`
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
/// (`line_index`). The `expected_*` values are what the page displayed for that
/// line. If the stored line differs in any of them, the request is refused
/// (409): it would otherwise edit the wrong line or write over a newer
/// correction.
///
/// Each edit is raw editor text, sent only for a field the user changed (`None`
/// = leave it alone; see [`TxnLineCorrection::from_draft`]). The server writes a
/// field only if its value differs from the stored one, and validates exactly
/// those values before writing anything (one audited `correct_line` call per
/// field). A request that changes nothing CONFIRMS the reading: the line
/// becomes user-reviewed and its low-confidence flag clears.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnLineCorrection {
    /// Receipt id (slug) the line belongs to.
    pub receipt_id: String,
    /// Zero-based position of the line in the receipt's line list.
    pub line_index: u32,
    /// The item name the page displayed (stale-read guard).
    pub expected_name: String,
    /// The category the page displayed (stale-read guard).
    pub expected_category: String,
    /// The quantity the page displayed (stale-read guard).
    pub expected_qty: f64,
    /// The unit price the page displayed (stale-read guard), exact centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub expected_unit_price: Money,
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

impl TxnLineCorrection {
    /// A request that changes nothing on the line the page shows as `seen` at
    /// position `index` of receipt `receipt_id`, i.e. a CONFIRM of that reading.
    #[must_use]
    pub fn confirm(receipt_id: &str, index: usize, seen: &TxnLineDto) -> Self {
        Self {
            receipt_id: receipt_id.to_owned(),
            // A position past u32 cannot exist; the server answers it with
            // "no longer exists".
            line_index: u32::try_from(index).unwrap_or(u32::MAX),
            expected_name: seen.name.clone(),
            expected_category: seen.category.clone(),
            expected_qty: seen.qty,
            expected_unit_price: seen.unit_price,
            name: None,
            category: None,
            qty: None,
            unit_price: None,
        }
    }

    /// The request for an editor that opened on `seen` and now holds `draft`.
    /// Only the fields whose text the user changed are sent, so an untouched
    /// field can never write back a value that is no longer current.
    #[must_use]
    pub fn from_draft(
        receipt_id: &str,
        index: usize,
        seen: &TxnLineDto,
        draft: &TxnLineDraft,
    ) -> Self {
        let shown = TxnLineDraft::of(seen);
        let edited = |now: &String, was: &String| (now != was).then(|| now.clone());
        Self {
            name: edited(&draft.name, &shown.name),
            category: edited(&draft.category, &shown.category),
            qty: edited(&draft.qty, &shown.qty),
            unit_price: edited(&draft.unit_price, &shown.unit_price),
            ..Self::confirm(receipt_id, index, seen)
        }
    }
}

/// The line editor's four text fields.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TxnLineDraft {
    /// Item name as typed.
    pub name: String,
    /// Category as typed.
    pub category: String,
    /// Quantity as typed.
    pub qty: String,
    /// Unit price in CHF as typed.
    pub unit_price: String,
}

impl TxnLineDraft {
    /// The text the editor shows for `line` before any edit.
    #[must_use]
    pub fn of(line: &TxnLineDto) -> Self {
        Self {
            name: line.name.clone(),
            category: line.category.clone(),
            qty: line.qty.to_string(),
            unit_price: crate::data::chf2(line.unit_price),
        }
    }
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

/// Whether a failed correction means the page's copy of the line is out of
/// date (the line changed or is gone), so the open line lists must refetch.
#[must_use]
pub fn correction_needs_reload(err: &ServerFnError) -> bool {
    matches!(
        err,
        ServerFnError::ServerError {
            code: 404 | 409,
            ..
        }
    )
}

/// The logic behind [`correct_transaction_line`], driven through the database
/// PORT so tests can run it against a fresh in-memory store.
///
/// Order matters. The addressed line is resolved (404), then checked against
/// every value the page displayed (409). Next, the values that differ from the
/// stored line are validated (400, all-or-nothing). Only then is anything
/// written.
#[cfg(feature = "server-deps")]
pub(crate) async fn correct_transaction_line_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    fix: TxnLineCorrection,
) -> Result<(), ServerFnError> {
    use phosk_ledger::line_items::correct_line;

    let receipt = db
        .receipt_by_slug(&fix.receipt_id)
        .await
        .map_err(line_fix::failed)?;
    let lines = db.line_items(receipt.id).await.map_err(line_fix::failed)?;
    let current = usize::try_from(fix.line_index)
        .ok()
        .and_then(|i| lines.get(i))
        .ok_or_else(line_fix::gone)?;
    if !line_fix::shows(current, &fix) {
        return Err(line_fix::stale());
    }

    let changes = line_fix::changes(&fix, current).map_err(line_fix::refused)?;
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

    /// How far apart (relative) two quantities may be and still count as the
    /// same displayed value. A float that crossed the wire twice can land an
    /// ulp or two off, because the web client's JSON float parsing is not
    /// exact. That must not read as a newer value.
    const QTY_TOLERANCE: f64 = 4.0 * f64::EPSILON;

    /// Whether `line` still carries every value the page displayed for it.
    pub(super) fn shows(line: &LineItem, fix: &super::TxnLineCorrection) -> bool {
        line.name == fix.expected_name
            && line.category == fix.expected_category
            && same_qty(line.qty, fix.expected_qty)
            && line.unit_price == fix.expected_unit_price
    }

    fn same_qty(stored: f64, seen: f64) -> bool {
        stored.to_bits() == seen.to_bits()
            || (stored - seen).abs() <= stored.abs().max(seen.abs()) * QTY_TOLERANCE
    }

    /// The `(field, value)` pairs `correct_line` must write, in the text form it
    /// parses. Only values that differ from the stored `line` are included, and
    /// only those are validated. A value equal to the stored one is left alone
    /// even if it would fail today's caps (an imported line may predate them).
    /// The first problem refuses the whole request.
    pub(super) fn changes(
        fix: &super::TxnLineCorrection,
        line: &LineItem,
    ) -> Result<Vec<(&'static str, String)>, PhoskError> {
        let mut out = Vec::new();
        if let Some(raw) = fix.name.as_deref().filter(|r| !same_label(r, &line.name)) {
            out.push(("name", label(raw, "Item name", MAX_NAME_CHARS)?));
        }
        if let Some(raw) = fix
            .category
            .as_deref()
            .filter(|r| !same_label(r, &line.category))
        {
            out.push(("category", label(raw, "Category", MAX_CATEGORY_CHARS)?));
        }
        if let Some(raw) = fix.qty.as_deref() {
            let qty = parse_qty(raw)?;
            // Typed text parses exactly, so an unchanged value has the very
            // same bits as the stored one.
            if qty.to_bits() != line.qty.to_bits() {
                out.push(("qty", check_qty(qty)?.to_string()));
            }
        }
        if let Some(raw) = fix.unit_price.as_deref() {
            let centimes = parse_chf_centimes(raw)?;
            if centimes != line.unit_price.centimes() {
                out.push(("unit_price", check_unit_price(centimes)?.to_string()));
            }
        }
        Ok(out)
    }

    /// Whether typed label text is the stored label (as-is or trimmed).
    fn same_label(raw: &str, stored: &str) -> bool {
        raw == stored || raw.trim() == stored
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
    /// sign, exponent, `inf` or `NaN`). The bounds are [`check_qty`]'s job.
    fn parse_qty(raw: &str) -> Result<f64, PhoskError> {
        const SHAPE: &str = "Quantity must be a number such as 2 or 0.5.";
        let text = raw.trim().replace(',', ".");
        let (whole, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
        if !is_digits(whole) || !is_digits(frac) || whole.len() + frac.len() == 0 {
            return Err(invalid(SHAPE));
        }
        text.parse().map_err(|_| invalid(SHAPE))
    }

    /// A new quantity must be greater than zero and at most [`MAX_QTY`].
    fn check_qty(qty: f64) -> Result<f64, PhoskError> {
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
    /// exponents and amounts past `i64`. The cap is [`check_unit_price`]'s job.
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
        // Digits only by now, so a failed parse can only be an i64 overflow.
        let francs: i64 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| unit_price_too_large())?
        };
        let cents: i64 = match frac.len() {
            0 => 0,
            1 => frac.parse::<i64>().map_err(|_| invalid(SHAPE))? * 10,
            _ => frac.parse().map_err(|_| invalid(SHAPE))?,
        };
        francs
            .checked_mul(100)
            .and_then(|c| c.checked_add(cents))
            .ok_or_else(unit_price_too_large)
    }

    /// A new unit price must be at most [`MAX_UNIT_PRICE_CENTIMES`].
    fn check_unit_price(centimes: i64) -> Result<i64, PhoskError> {
        if centimes > MAX_UNIT_PRICE_CENTIMES {
            return Err(unit_price_too_large());
        }
        Ok(centimes)
    }

    fn unit_price_too_large() -> PhoskError {
        invalid("Unit price is too large (at most CHF 1'000'000).")
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

    /// A refused request (400). Only [`changes`] feeds this, and its `Invalid`
    /// messages are written for the user, so they pass through.
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
        reply(404, "This receipt line no longer exists.")
    }

    /// The line at that position no longer carries what the page showed (409).
    pub(super) fn stale() -> ServerFnError {
        reply(
            409,
            "This line changed since it was loaded. Reload it and try again.",
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

    /// The line the page displays at `slug[index]` (the same read and mapping
    /// `get_transaction_lines` uses).
    async fn shown_line(db: &MemoryDb, slug: &str, index: usize) -> TxnLineDto {
        let lines = phosk_ledger::line_items::transaction_lines(db, slug)
            .await
            .expect("receipt lines");
        lines
            .lines
            .into_iter()
            .nth(index)
            .map(map_line)
            .expect("the page shows this line")
    }

    /// A request for what the page shows at `slug[index]` that changes nothing
    /// yet (a CONFIRM).
    async fn request(db: &MemoryDb, slug: &str, index: usize) -> TxnLineCorrection {
        TxnLineCorrection::confirm(slug, index, &shown_line(db, slug, index).await)
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
        assert_eq!(before.name, BREAD, "precondition");
        assert!(before.provenance.is_low_confidence(), "precondition");

        let fix = TxnLineCorrection {
            name: Some("  Sourdough loaf  ".to_owned()),
            ..request(&db, "t1", 2).await
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
            ..request(&db, "t1", 1).await
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
            ..request(&db, "t1", 3).await
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
            ..request(&db, "t1", 1).await
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
        // Exactly what the editor pre-fills for this line, sent back verbatim.
        let fix = TxnLineCorrection {
            name: Some(BREAD.to_owned()),
            category: Some("Groceries".to_owned()),
            qty: Some("1".to_owned()),
            unit_price: Some("4.31".to_owned()),
            ..request(&db, "t1", 2).await
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
        correct_transaction_line_with(&db, request(&db, "t1", 2).await)
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
            ..request(&db, "t1", 2).await
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("category change is saved");

        let after = stored_line(&db, "t1", 2).await;
        assert_eq!(after.category, "Bakery");
        assert_eq!(after.name, BREAD);
        assert_eq!(after.unit_price.centimes(), 431);
    }

    /// `base` with one edit field set to `value`.
    fn with_edit(mut fix: TxnLineCorrection, field: &str, value: &str) -> TxnLineCorrection {
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
        // The seed is the same in every fresh store, so one request fits all.
        let bread = request(&seeded(), "t1", 2).await;
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
        .map(|(case, field, value)| (case, with_edit(bread.clone(), field, value)))
        .collect();
        // A valid field next to an invalid one: nothing may be written.
        let mixed = with_edit(with_edit(bread.clone(), "name", "Sourdough"), "qty", "0");
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
            assert!(!correction_needs_reload(&err), "{case}: no reload needed");
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
        let bread = shown_line(&db, "t1", 2).await;
        let fix = TxnLineCorrection::confirm("no-such-receipt", 0, &bread);
        let err = correct_transaction_line_with(&db, fix)
            .await
            .expect_err("unknown receipt");
        let (code, message) = refusal(&err);
        assert_eq!(code, 404);
        assert!(!message.contains("no-such-receipt"), "{message:?}");
        assert!(!message.contains("not found:"), "{message:?}");
        assert!(correction_needs_reload(&err), "the page must refetch");
    }

    #[tokio::test]
    async fn line_index_past_the_end_is_a_404() {
        let db = seeded();
        let bread = shown_line(&db, "t1", 2).await;
        let err = correct_transaction_line_with(&db, TxnLineCorrection::confirm("t1", 99, &bread))
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
            ..TxnLineCorrection::confirm("t1", 1, &shown_line(&db, "t1", 2).await)
        };
        let err = correct_transaction_line_with(&db, fix)
            .await
            .expect_err("stale read");
        assert_eq!(refusal(&err).0, 409);
        assert!(correction_needs_reload(&err), "the page must refetch");
        assert_eq!(stored_line(&db, "t1", 1).await, before);
    }

    /// Review finding F2: one view corrects the price while another view still
    /// shows the old line. A save from the stale view must not revert it.
    #[tokio::test]
    async fn a_stale_view_cannot_revert_a_newer_correction() {
        let db = seeded();
        // The accordion opened the bread line at 4.31.
        let stale = request(&db, "t1", 2).await;

        // The receipt screen corrects the unit price to 4.50.
        let newer = TxnLineCorrection {
            unit_price: Some("4.50".to_owned()),
            ..request(&db, "t1", 2).await
        };
        correct_transaction_line_with(&db, newer)
            .await
            .expect("the newer correction is saved");
        assert_eq!(stored_line(&db, "t1", 2).await.unit_price.centimes(), 450);

        // The accordion saves: once resending its stale price, once sending
        // only an edited name, once as a bare CONFIRM.
        let resend = TxnLineCorrection {
            name: Some("Sourdough".to_owned()),
            unit_price: Some("4.31".to_owned()),
            ..stale.clone()
        };
        let name_only = TxnLineCorrection {
            name: Some("Sourdough".to_owned()),
            ..stale.clone()
        };
        for (case, fix) in [
            ("resend", resend),
            ("name only", name_only),
            ("confirm", stale),
        ] {
            let err = correct_transaction_line_with(&db, fix)
                .await
                .expect_err(case);
            assert_eq!(refusal(&err).0, 409, "{case}: refused as stale");
            let after = stored_line(&db, "t1", 2).await;
            assert_eq!(after.unit_price.centimes(), 450, "{case}: price kept");
            assert_eq!(after.name, BREAD, "{case}: nothing written");
        }
    }

    #[tokio::test]
    async fn every_displayed_value_is_guarded() {
        let db = seeded();
        let before = stored_line(&db, "t1", 2).await;
        let shown = request(&db, "t1", 2).await;
        let cases = [
            (
                "name",
                TxnLineCorrection {
                    expected_name: "Bread".to_owned(),
                    ..shown.clone()
                },
            ),
            (
                "category",
                TxnLineCorrection {
                    expected_category: "Bakery".to_owned(),
                    ..shown.clone()
                },
            ),
            (
                "qty",
                TxnLineCorrection {
                    expected_qty: 2.0,
                    ..shown.clone()
                },
            ),
            (
                "unit price",
                TxnLineCorrection {
                    expected_unit_price: Money::from_centimes(450),
                    ..shown
                },
            ),
        ];
        for (case, fix) in cases {
            let fix = TxnLineCorrection {
                category: Some("Bakery".to_owned()),
                ..fix
            };
            let err = correct_transaction_line_with(&db, fix)
                .await
                .expect_err(case);
            assert_eq!(refusal(&err).0, 409, "{case}: refused as stale");
            assert_eq!(stored_line(&db, "t1", 2).await, before, "{case}: untouched");
        }
    }

    #[tokio::test]
    async fn a_qty_a_few_ulps_off_after_the_wire_still_matches() {
        let db = seeded();
        let shown = request(&db, "t1", 1).await;
        // A JSON float parser that is not exact may land one ulp off 1.2.
        let fix = TxnLineCorrection {
            expected_qty: f64::from_bits(shown.expected_qty.to_bits() + 1),
            ..shown
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("still the displayed line");
        let after = stored_line(&db, "t1", 1).await;
        assert_eq!(after.provenance.source, Source::UserModified);
        assert_eq!(after.qty.to_bits(), 1.2_f64.to_bits(), "qty not rewritten");
    }

    #[tokio::test]
    async fn untouched_values_outside_the_new_caps_do_not_block_a_save() {
        let db = seeded();
        // An imported line whose values predate today's validation caps.
        let long_category = "c".repeat(61);
        let mut imported = stored_line(&db, "t1", 2).await;
        imported.category.clone_from(&long_category);
        imported.qty = 150_000.0;
        db.update_line_item(imported)
            .await
            .expect("store the imported line");

        // The editor sends only the edited name.
        let fix = TxnLineCorrection {
            name: Some("Sourdough".to_owned()),
            ..request(&db, "t1", 2).await
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("the name is saved");

        // A client that resends the untouched values verbatim is fine too.
        let fix = TxnLineCorrection {
            name: Some("Rye".to_owned()),
            category: Some(long_category.clone()),
            qty: Some("150000".to_owned()),
            unit_price: Some("4.31".to_owned()),
            ..request(&db, "t1", 2).await
        };
        correct_transaction_line_with(&db, fix)
            .await
            .expect("unchanged values are not re-validated");

        let after = stored_line(&db, "t1", 2).await;
        assert_eq!(after.name, "Rye");
        assert_eq!(after.category, long_category, "category untouched");
        assert_eq!(
            after.qty.to_bits(),
            150_000.0_f64.to_bits(),
            "qty untouched"
        );
    }

    /// A displayed line whose unit price shows Swiss grouping.
    fn grouped_line_dto() -> TxnLineDto {
        TxnLineDto {
            name: BREAD.to_owned(),
            qty: 1.2,
            unit_price: Money::from_centimes(123_431),
            line_total: Money::from_centimes(148_117),
            category: "Groceries".to_owned(),
            signal_id: String::new(),
            confidence: 0.58,
        }
    }

    #[test]
    fn the_editor_sends_only_the_fields_the_user_changed() {
        let seen = grouped_line_dto();
        let untouched = TxnLineDraft::of(&seen);
        assert_eq!(untouched.qty, "1.2");
        assert_eq!(untouched.unit_price, "1\u{2019}234.31");

        let fix = TxnLineCorrection::from_draft("t1", 2, &seen, &untouched);
        assert_eq!(
            fix,
            TxnLineCorrection::confirm("t1", 2, &seen),
            "an untouched editor sends a bare CONFIRM"
        );

        let draft = TxnLineDraft {
            unit_price: "1234.50".to_owned(),
            ..untouched
        };
        let fix = TxnLineCorrection::from_draft("t1", 2, &seen, &draft);
        assert_eq!(fix.unit_price.as_deref(), Some("1234.50"));
        assert_eq!((fix.name, fix.category, fix.qty), (None, None, None));
        assert_eq!(fix.expected_unit_price, seen.unit_price);
        assert_eq!(fix.expected_qty.to_bits(), seen.qty.to_bits());
        assert_eq!(fix.expected_name, seen.name);
        assert_eq!(fix.expected_category, seen.category);
    }

    #[test]
    fn a_confirm_request_carries_the_displayed_values() {
        let seen = grouped_line_dto();
        let fix = TxnLineCorrection::confirm("t1", 2, &seen);
        assert_eq!(fix.receipt_id, "t1");
        assert_eq!(fix.line_index, 2);
        assert_eq!(fix.expected_unit_price.centimes(), 123_431);
        assert_eq!(
            TxnLineCorrection::confirm("t1", usize::MAX, &seen).line_index,
            u32::MAX,
            "an impossible position stays impossible"
        );
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
        assert!(!correction_needs_reload(&transport));

        let failed = ServerFnError::ServerError {
            message: "The correction could not be saved. Try again.".to_owned(),
            code: 500,
            details: None,
        };
        assert!(!correction_needs_reload(&failed));
    }
}
