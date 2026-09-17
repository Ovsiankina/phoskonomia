//! `line_items` — a receipt's parsed lines + the line-correction write path.
//!
//! Backs the receipt accordion (per-line name/qty/price/category/confidence,
//! the coral `< 0.7` flag, signal pills). DTOs mirror
//! `dioxus-app/src/data/transactions.rs` (`TxnLineDto`, `TxnLinesDto`).
//!
//! `line_total` is backend-derived (`round(qty * unit_price)`), never trusted
//! from input.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{CorrectionId, LineItemId};
use phosk_model::{CorrectionEvent, LineItem, Provenance};
use serde::{Deserialize, Serialize};

/// One parsed receipt line (`TxnLinesDto::lines` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnLineDto {
    /// Item name.
    pub name: String,
    /// Quantity.
    pub qty: f64,
    /// Unit price (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub unit_price: Money,
    /// Line total, backend-derived `round(qty * unit_price)` (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub line_total: Money,
    /// Line category.
    pub category: String,
    /// Tracked item-signal id, if any (empty = plain category tag).
    pub signal_id: String,
    /// OCR/LLM reading confidence, 0–1.
    pub confidence: f64,
}

/// `transaction_lines` payload — the parsed lines + signal feeds + low-conf count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnLinesDto {
    /// The parsed line items.
    pub lines: Vec<TxnLineDto>,
    /// Distinct tracked item-signal ids this receipt feeds.
    pub sigs: Vec<String>,
    /// Count of lines below the 0.7 confidence threshold.
    pub low_conf: u32,
}

/// One receipt's parsed lines, resolved by its seed `slug`.
///
/// Derives each `line_total` from `round(qty * unit_price)`, the distinct `sigs`
/// across the lines, and `low_conf` = count of lines with `confidence < 0.7`.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn transaction_lines(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<TxnLinesDto, PhoskError> {
    let receipt = db.receipt_by_slug(slug).await?;
    let lines = db.line_items(receipt.id).await?;
    build_lines_dto(db, lines).await
}

/// Project a receipt's persisted lines into the wire DTO: re-derive each line
/// total from `round(qty * unit_price)`, count the `< 0.7` lines, and resolve
/// each linked signal id to its stable slug for the distinct `sigs` feed.
async fn build_lines_dto(
    db: &dyn DatabaseAdapter,
    lines: Vec<LineItem>,
) -> Result<TxnLinesDto, PhoskError> {
    let mut dtos = Vec::with_capacity(lines.len());
    let mut sigs: Vec<String> = Vec::new();
    let mut low_conf: u32 = 0;

    for line in lines {
        if line.provenance.is_low_confidence() {
            low_conf = low_conf.saturating_add(1);
        }
        let signal_slug = match line.signal_id {
            Some(id) => {
                let signal = db.signal(id).await?;
                if !sigs.contains(&signal.slug) {
                    sigs.push(signal.slug.clone());
                }
                signal.slug
            }
            None => String::new(),
        };
        dtos.push(TxnLineDto {
            name: line.name,
            qty: line.qty,
            unit_price: line.unit_price,
            line_total: line_total(line.qty, line.unit_price)?,
            category: line.category,
            signal_id: signal_slug,
            confidence: line.provenance.confidence,
        });
    }

    Ok(TxnLinesDto {
        lines: dtos,
        sigs,
        low_conf,
    })
}

/// Backend-derived line total = `round(qty * unit_price_centimes)` as i64.
///
/// Pure helper; scaled integer-centime math, surfaces [`PhoskError::Overflow`].
#[allow(
    clippy::cast_precision_loss,
    reason = "centime magnitudes are far below f64's 2^53 exact-integer limit; the multiply is the only float step and its result is range-checked before re-entering Money"
)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the f64 is explicitly range-checked against i64 bounds before the `as` cast, so no truncation or sign loss can occur"
)]
pub fn line_total(qty: f64, unit_price: Money) -> Result<Money, PhoskError> {
    // Exact-centime product. The frontend never supplies the line total — it is
    // always re-derived here so a mis-keyed total cannot enter the ledger.
    let product = qty * (unit_price.centimes() as f64);
    if !product.is_finite() {
        return Err(PhoskError::Overflow(format!(
            "line total {qty} × {} is not finite",
            unit_price.centimes()
        )));
    }
    // Round half away from zero to the nearest centime (matches the contract's
    // 166.5 → 167 rule) and guard the i64 range before casting.
    let rounded = product.round();
    #[allow(
        clippy::cast_precision_loss,
        reason = "comparison bounds only; the i64 extremes round-trip through f64 closely enough to reject any product that would not fit"
    )]
    if rounded > i64::MAX as f64 || rounded < i64::MIN as f64 {
        return Err(PhoskError::Overflow(format!(
            "line total {qty} × {} overflows centime range",
            unit_price.centimes()
        )));
    }
    Ok(Money::from_centimes(rounded as i64))
}

/// Correct a single line field (records a `CorrectionEvent`, updates the line,
/// flips its provenance to `UserModified`). Write path.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn correct_line(
    db: &dyn DatabaseAdapter,
    line: LineItemId,
    field: &str,
    new_value: &str,
) -> Result<(), PhoskError> {
    let mut current = find_line(db, line).await?;
    let old_value = field_value(&current, field)?;

    apply_field(db, &mut current, field, new_value).await?;
    // Any user touch re-derives the (backend-owned) total and clears the
    // low-confidence flag: the line is now user-reviewed at full confidence.
    current.line_total = line_total(current.qty, current.unit_price)?;
    current.provenance = Provenance::user_modified();

    db.update_line_item(current).await?;
    db.record_correction(CorrectionEvent {
        id: CorrectionId::new(),
        entity_id: line.to_string(),
        field: field.to_owned(),
        old_value,
        new_value: new_value.to_owned(),
        at: chrono::Utc::now().date_naive(),
    })
    .await
}

/// Locate a line by its typed id across every receipt (the port exposes lines
/// per receipt, not by line id). [`PhoskError::NotFound`] if no receipt holds it.
async fn find_line(db: &dyn DatabaseAdapter, line: LineItemId) -> Result<LineItem, PhoskError> {
    for receipt in db.all_receipts().await? {
        if let Some(found) = db
            .line_items(receipt.id)
            .await?
            .into_iter()
            .find(|l| l.id == line)
        {
            return Ok(found);
        }
    }
    Err(PhoskError::NotFound(format!("line item {line}")))
}

/// The current stringified value of a correctable field (for the audit log's
/// `old_value`). Rejects unknown fields up front as [`PhoskError::Invalid`].
fn field_value(line: &LineItem, field: &str) -> Result<String, PhoskError> {
    match field {
        "name" => Ok(line.name.clone()),
        "category" => Ok(line.category.clone()),
        "qty" => Ok(line.qty.to_string()),
        "unit_price" => Ok(line.unit_price.centimes().to_string()),
        "signal_id" => Ok(line.signal_id.map_or_else(String::new, |id| id.to_string())),
        "confirmed" => Ok(line.provenance.confidence.to_string()),
        other => Err(PhoskError::Invalid(format!("unknown line field {other}"))),
    }
}

/// Apply one parsed field edit to the in-memory line. Numeric fields parse
/// strictly (non-numeric ⇒ [`PhoskError::Invalid`]); `signal_id` resolves the
/// slug to a typed id via the port; `confirmed` is a no-op value carrier (the
/// provenance flip in the caller does the work).
async fn apply_field(
    db: &dyn DatabaseAdapter,
    line: &mut LineItem,
    field: &str,
    new_value: &str,
) -> Result<(), PhoskError> {
    match field {
        "name" => new_value.clone_into(&mut line.name),
        "category" => new_value.clone_into(&mut line.category),
        "qty" => {
            line.qty = new_value
                .trim()
                .parse::<f64>()
                .map_err(|_| PhoskError::Invalid(format!("qty `{new_value}` is not a number")))?;
        }
        "unit_price" => {
            let centimes = new_value.trim().parse::<i64>().map_err(|_| {
                PhoskError::Invalid(format!("unit_price `{new_value}` is not a number"))
            })?;
            line.unit_price = Money::from_centimes(centimes);
        }
        "signal_id" => {
            if new_value.is_empty() {
                line.signal_id = None;
            } else {
                line.signal_id = Some(db.signal_by_slug(new_value).await?.id);
            }
        }
        "confirmed" => {}
        other => return Err(PhoskError::Invalid(format!("unknown line field {other}"))),
    }
    Ok(())
}
