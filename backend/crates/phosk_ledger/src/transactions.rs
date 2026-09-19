//! `transactions` — the dated receipt list + receipt detail slice.
//!
//! Backs the Transactions page list (filterable, dated, grouped) and the
//! full-screen receipt detail. DTOs mirror `dioxus-app/src/data/transactions.rs`
//! field-for-field (camelCase keys, money as exact i64 centimes).
//!
//! The services compute the derivations (`itemCount`, `lowConfCount`,
//! `signalIds`, summary roll-ups, filter/sort) from receipt data.
//!
//! [`create_transaction`] is the manual-entry write path: a human-typed spend
//! becomes a [`Receipt`] plus its [`LineItem`]s, stamped [`Source::UserEntered`]
//! at full confidence. Every money value is backend-derived
//! (`round(qty × unit_price)`, summed exactly); a total the caller states is
//! only ever used to *cross-check*, never to overrule the itemisation.
//!
//! [`LineItem`]: phosk_model::LineItem
//! [`Source::UserEntered`]: phosk_model::Source

use std::collections::BTreeSet;

use chrono::{Datelike, NaiveDate};
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::Period;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{LineItemId, ReceiptId};
use phosk_model::{LineItem, Provenance, Receipt};
use serde::{Deserialize, Serialize};

use crate::line_items::line_total;

/// One receipt row in the list (`TransactionListDto::transactions` element).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDto {
    /// Stable id (the seed slug, e.g. `"t1"`).
    pub id: String,
    /// Day label, e.g. `"16 JUN"`.
    pub date: String,
    /// Shop name.
    pub shop: String,
    /// Primary category.
    pub category: String,
    /// Receipt total (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Number of line items.
    pub item_count: u32,
    /// Count of low-confidence (< 0.7) lines.
    pub low_conf_count: u32,
    /// Tracked item-signal ids this receipt feeds.
    pub signal_ids: Vec<String>,
    /// `true` for a fixed/standing charge.
    pub fixed: bool,
}

/// The list summary band (`TransactionListDto::summary`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnSummaryDto {
    /// Number of entries in the current filter.
    pub entry_count: u32,
    /// Total across the filtered entries (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub total_amount: Money,
    /// Human period label, e.g. `"JUN 2026"`.
    pub period_label: String,
}

/// `list_transactions` payload — filtered list + summary + filter option lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// The receipt photo source descriptor (`TxnDetailDto::source`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnSourceDto {
    /// Source kind, e.g. `"PHOTO"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// OCR engine label, e.g. `"PADDLEOCR"`.
    pub ocr_engine: String,
}

/// `transaction_detail` payload — receipt detail for the full-screen view.
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

/// Transaction filters (all optional, empty string = "all").
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
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

/// The filtered, sorted transaction list + summary + filter options.
///
/// Resolves `filter.period` to a [`CycleWindow`] (via `Period`), bounds the
/// receipt range, applies `shop`/`category`/`q`, sorts by `sort`, and derives
/// each row's `item_count` / `low_conf_count` / `signal_ids` from its lines.
///
/// [`CycleWindow`]: phosk_core::cycle::CycleWindow
#[tracing::instrument(level = "debug", skip_all)]
pub async fn list_transactions(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
    filter: TxnFilter,
) -> Result<TransactionListDto, PhoskError> {
    let window = resolve_period(&filter.period).resolve(as_of)?;
    let all = db.receipts_between(window.start, window.end).await?;

    // The filter-option lists span the whole resolved window, BEFORE the
    // shop/category/q narrowing, so the dropdowns stay complete.
    let available_shops = distinct(all.iter().map(|r| r.shop.clone()));
    let available_categories = distinct(all.iter().map(|r| r.category.clone()));

    let q = filter.q.to_lowercase();
    let mut matched: Vec<Receipt> = all
        .into_iter()
        .filter(|r| filter.shop.is_empty() || r.shop == filter.shop)
        .filter(|r| filter.category.is_empty() || r.category == filter.category)
        .filter(|r| {
            q.is_empty()
                || r.shop.to_lowercase().contains(&q)
                || r.category.to_lowercase().contains(&q)
        })
        .collect();

    sort_receipts(&mut matched, &filter.sort);

    let total_amount = Money::sum(matched.iter().map(|r| r.amount))?;
    let entry_count = u32::try_from(matched.len()).unwrap_or(u32::MAX);

    let mut transactions = Vec::with_capacity(matched.len());
    for r in matched {
        let lines = db.line_items(r.id).await?;
        let item_count = u32::try_from(lines.len()).unwrap_or(u32::MAX);
        let low_conf_count = u32::try_from(
            lines
                .iter()
                .filter(|l| l.provenance.is_low_confidence())
                .count(),
        )
        .unwrap_or(u32::MAX);
        let mut signal_ids: Vec<String> = Vec::new();
        for line in &lines {
            if let Some(id) = line.signal_id {
                let slug = db.signal(id).await?.slug;
                if !signal_ids.contains(&slug) {
                    signal_ids.push(slug);
                }
            }
        }
        transactions.push(TransactionDto {
            id: r.slug,
            date: date_label(r.date),
            shop: r.shop,
            category: r.category,
            amount: r.amount,
            item_count,
            low_conf_count,
            signal_ids,
            fixed: r.fixed,
        });
    }

    Ok(TransactionListDto {
        transactions,
        summary: TxnSummaryDto {
            entry_count,
            total_amount,
            period_label: period_label(window.start),
        },
        available_shops,
        available_categories,
    })
}

/// Map a filter horizon string to a [`Period`]; empty/unknown ⇒ the month cycle.
fn resolve_period(period: &str) -> Period {
    match period {
        "day" => Period::Day,
        "week" => Period::Week,
        "quarter" => Period::Quarter,
        "year" => Period::Year,
        _ => Period::Month,
    }
}

/// Sort the matched rows in place by the requested key. `amount` is descending,
/// `shop` ascending; the default `date` is newest-first (ties on shop ascending
/// for a stable order).
fn sort_receipts(rows: &mut [Receipt], sort: &str) {
    match sort {
        "amount" => rows.sort_by(|a, b| b.amount.cmp(&a.amount).then_with(|| a.shop.cmp(&b.shop))),
        "shop" => rows.sort_by(|a, b| a.shop.cmp(&b.shop).then_with(|| b.date.cmp(&a.date))),
        _ => rows.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.shop.cmp(&b.shop))),
    }
}

/// Distinct, ascending values (filter-dropdown options).
fn distinct<I: IntoIterator<Item = String>>(items: I) -> Vec<String> {
    items
        .into_iter()
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect()
}

/// `"16 JUN"` day label (presentation carried on the DTO per the wire contract).
fn date_label(date: chrono::NaiveDate) -> String {
    format!("{:02} {}", date.day(), month_abbrev(date.month()))
}

/// `"JUN 2026"` cycle label from the window start.
fn period_label(start: chrono::NaiveDate) -> String {
    format!("{} {}", month_abbrev(start.month()), start.year())
}

/// Uppercase three-letter month abbreviation.
const fn month_abbrev(month: u32) -> &'static str {
    match month {
        1 => "JAN",
        2 => "FEB",
        3 => "MAR",
        4 => "APR",
        5 => "MAY",
        6 => "JUN",
        7 => "JUL",
        8 => "AUG",
        9 => "SEP",
        10 => "OCT",
        11 => "NOV",
        _ => "DEC",
    }
}

// ── Manual entry (write path) ────────────────────────────────────────────────

/// One typed-in line of a manual entry.
///
/// `line_total` is deliberately absent: the backend derives it from
/// `qty × unit_price`, so a mis-keyed total cannot enter the ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewLineInput {
    /// Item name (required, trimmed).
    pub name: String,
    /// Quantity — must be a finite number greater than zero.
    pub qty: f64,
    /// Per-unit price in exact centimes; must not be negative.
    #[serde(with = "phosk_model::money_centimes")]
    pub unit_price: Money,
    /// Category for this line; empty inherits the receipt's category.
    pub category: String,
    /// Slug of a tracked item-[`Signal`] to attach, or empty for none.
    ///
    /// [`Signal`]: phosk_model::Signal
    pub signal_id: String,
}

/// A manually entered transaction: a dated spend at a shop, optionally itemised.
///
/// Either `lines` is non-empty (the total is derived from it) or `amount` is
/// set (a total-only entry) — an entry with neither is not a spend record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTransaction {
    /// Shop display name (required, trimmed).
    pub shop: String,
    /// Calendar date of the spend.
    pub date: NaiveDate,
    /// Primary category name (required, trimmed).
    pub category: String,
    /// `true` for a standing/fixed charge.
    pub fixed: bool,
    /// The receipt total, in exact centimes. Required when there are no lines;
    /// with lines it is optional and, if given, must equal their sum.
    #[serde(with = "phosk_model::opt_money_centimes")]
    pub amount: Option<Money>,
    /// The itemisation, possibly empty.
    pub lines: Vec<NewLineInput>,
}

/// What a successful [`create_transaction`] produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedTxnDto {
    /// The new receipt's stable slug — the `id` every read DTO keys on.
    pub id: String,
    /// The stored (backend-derived) total, exact centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Number of stored line items.
    pub item_count: u32,
}

/// Create a transaction from manual entry.
///
/// Validates the input, derives every money value, stamps
/// [`Provenance::user_entered`] on the receipt and each line, and persists both
/// through `insert_receipt` (which also maintains the dashboard projection, so
/// the new spend counts towards the cycle aggregates immediately).
///
/// The receipt is marked `source_kind = "MANUAL"` with no OCR metadata, and
/// gets a fresh `manual:<uuid>` slug — a manual entry is never a re-import, so
/// each call creates its own record rather than replacing an existing one.
///
/// # Errors
/// - [`PhoskError::Invalid`] for a blank shop/category/line name, a quantity
///   that is not finite and positive, a negative unit price or total, an entry
///   with neither lines nor a total, or a stated total that contradicts the
///   itemisation.
/// - [`PhoskError::NotFound`] if a line names a signal slug that does not exist.
/// - [`PhoskError::Overflow`] if the derived totals leave the centime range.
#[tracing::instrument(level = "debug", skip_all, fields(date = %input.date, lines = input.lines.len()))]
pub async fn create_transaction(
    db: &dyn DatabaseAdapter,
    input: NewTransaction,
) -> Result<CreatedTxnDto, PhoskError> {
    let shop = required(&input.shop, "shop")?;
    let category = required(&input.category, "category")?;

    let receipt_id = ReceiptId::new();
    let mut lines = Vec::with_capacity(input.lines.len());
    for raw in input.lines {
        lines.push(build_line(db, receipt_id, &category, raw).await?);
    }

    let amount = resolve_amount(input.amount, &lines)?;
    if amount.centimes() < 0 {
        return Err(PhoskError::Invalid(format!(
            "a transaction total must not be negative (got {} centimes)",
            amount.centimes()
        )));
    }

    let slug = format!("manual:{receipt_id}");
    let item_count = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    db.insert_receipt(
        Receipt {
            id: receipt_id,
            slug: slug.clone(),
            shop,
            date: input.date,
            category,
            amount,
            fixed: input.fixed,
            provenance: Provenance::user_entered(),
            source_kind: "MANUAL".to_owned(),
            ocr_engine: String::new(),
            ocr_regions: 0,
        },
        lines,
    )
    .await?;
    tracing::debug!(%slug, item_count, "created a manual transaction");

    Ok(CreatedTxnDto {
        id: slug,
        amount,
        item_count,
    })
}

/// A required free-text field, trimmed; blank is [`PhoskError::Invalid`].
fn required(value: &str, field: &str) -> Result<String, PhoskError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(PhoskError::Invalid(format!("{field} is required")));
    }
    Ok(trimmed.to_owned())
}

/// Validate one typed-in line and turn it into a storable [`LineItem`]: the
/// total is derived, the category falls back to the receipt's, and the signal
/// slug (if any) is resolved to a typed id through the port.
async fn build_line(
    db: &dyn DatabaseAdapter,
    receipt_id: ReceiptId,
    receipt_category: &str,
    raw: NewLineInput,
) -> Result<LineItem, PhoskError> {
    let name = required(&raw.name, "line name")?;
    if !raw.qty.is_finite() || raw.qty <= 0.0 {
        return Err(PhoskError::Invalid(format!(
            "line `{name}`: qty must be a finite number greater than zero"
        )));
    }
    if raw.unit_price.centimes() < 0 {
        return Err(PhoskError::Invalid(format!(
            "line `{name}`: unit price must not be negative"
        )));
    }
    let signal_id = if raw.signal_id.trim().is_empty() {
        None
    } else {
        Some(db.signal_by_slug(raw.signal_id.trim()).await?.id)
    };
    let category = match raw.category.trim() {
        "" => receipt_category.to_owned(),
        given => given.to_owned(),
    };

    Ok(LineItem {
        id: LineItemId::new(),
        receipt_id,
        name,
        qty: raw.qty,
        unit_price: raw.unit_price,
        line_total: line_total(raw.qty, raw.unit_price)?,
        category,
        signal_id,
        // Typed in by a human: authoritative, never review-flagged.
        provenance: Provenance::user_entered(),
    })
}

/// The receipt total. With lines it is the exact sum of their derived totals; a
/// `stated` total is then only a cross-check (a disagreement is the caller's
/// bug and is refused, never silently overruled). Without lines the stated
/// total is the record.
fn resolve_amount(stated: Option<Money>, lines: &[LineItem]) -> Result<Money, PhoskError> {
    if lines.is_empty() {
        return stated.ok_or_else(|| {
            PhoskError::Invalid(
                "a transaction needs either line items or a total amount".to_owned(),
            )
        });
    }
    let derived = Money::sum(lines.iter().map(|l| l.line_total))?;
    if let Some(stated) = stated
        && stated != derived
    {
        return Err(PhoskError::Invalid(format!(
            "stated total {} does not match the line items ({} centimes)",
            stated.centimes(),
            derived.centimes()
        )));
    }
    Ok(derived)
}

/// One receipt's detail (avg confidence, OCR source/regions). Resolves the
/// receipt by its seed `slug`.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn transaction_detail(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<TxnDetailDto, PhoskError> {
    let receipt = db.receipt_by_slug(slug).await?;
    let lines = db.line_items(receipt.id).await?;

    // Mean of the line confidences; a receipt with no lines reports 0.0 (no
    // division by zero — the empty mean is defined as zero here).
    let avg_confidence = if lines.is_empty() {
        0.0
    } else {
        let sum: f64 = lines.iter().map(|l| l.provenance.confidence).sum();
        #[allow(
            clippy::cast_precision_loss,
            reason = "a receipt's line count is tiny (single digits), far below f64's exact-integer limit"
        )]
        let count = lines.len() as f64;
        sum / count
    };

    Ok(TxnDetailDto {
        id: receipt.slug,
        avg_confidence,
        source: TxnSourceDto {
            kind: receipt.source_kind,
            ocr_engine: receipt.ocr_engine,
        },
        ocr_regions: receipt.ocr_regions,
    })
}
