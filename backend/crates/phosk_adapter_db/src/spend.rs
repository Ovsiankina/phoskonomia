//! Item-level category spend: the one rule every per-category figure uses.
//!
//! A receipt is categorised **per line** (item-level categorisation is the
//! product's core promise): a Migros receipt with a toothpaste line booked
//! under Health counts that line under Health and the rest under Groceries.
//! Only a receipt with no lines at all (a total-only entry) counts under its
//! receipt-level category.
//!
//! When the lines do not add up to the receipt amount (rounding, a discount
//! the reader did not itemise), the difference stays with the receipt-level
//! category, so the parts of a receipt always sum to exactly its amount and
//! the per-category figures always add up to the cycle total.
//!
//! Budgets, alerts, momentum, the budget export, the dashboard insight and the
//! chat context all go through [`receipt_parts`]; none of them sums
//! `Receipt::amount` by `Receipt::category` on its own.
//!
//! This is pure arithmetic over the port's own reads (no adapter type), so it
//! lives next to the port where every feature crate can reach it.

use std::collections::HashMap;

use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::ReceiptId;
use phosk_model::{CategoryCap, LineItem, Receipt};

use crate::DatabaseAdapter;

/// The share of one receipt that falls into one category.
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryPart<'a> {
    /// The receipt this share belongs to.
    pub receipt: &'a Receipt,
    /// The category the share is booked under.
    pub category: String,
    /// The share, exact centimes (Σ of a receipt's parts = its amount).
    pub amount: Money,
}

impl CategoryPart<'_> {
    /// `true` when this share is a fixed/standing charge: its receipt is
    /// flagged fixed, or it is booked under a category whose cap is fixed.
    /// A fixed share is never projected (see
    /// [`CycleWindow::project_spend`](phosk_core::cycle::CycleWindow::project_spend)).
    #[must_use]
    pub fn is_fixed(&self, caps: &[CategoryCap]) -> bool {
        self.receipt.fixed || caps.iter().any(|c| c.fixed && c.name == self.category)
    }
}

/// Split one receipt into its per-category shares, given its lines.
///
/// - no lines → one share: the whole amount under the receipt's category;
/// - lines → `line_total` summed per line category (in the order the
///   categories first appear), plus any difference between the receipt
///   amount and Σ lines under the receipt's category.
///
/// A category whose share comes to exactly zero is dropped, unless it is the
/// only one (a zero receipt still "exists" in its category).
///
/// # Errors
/// [`PhoskError::Overflow`] if a sum leaves the `i64` centime range.
pub fn split_receipt<'a>(
    receipt: &'a Receipt,
    lines: &[LineItem],
) -> Result<Vec<CategoryPart<'a>>, PhoskError> {
    let mut parts: Vec<CategoryPart<'a>> = Vec::new();
    let mut add = |category: &str, amount: Money| -> Result<(), PhoskError> {
        if let Some(p) = parts.iter_mut().find(|p| p.category == category) {
            p.amount = p.amount.checked_add(amount)?;
        } else {
            parts.push(CategoryPart {
                receipt,
                category: category.to_owned(),
                amount,
            });
        }
        Ok(())
    };
    let itemised = Money::sum(lines.iter().map(|l| l.line_total))?;
    for line in lines {
        add(&line.category, line.line_total)?;
    }
    let rest = receipt.amount.checked_sub(itemised)?;
    if lines.is_empty() || rest != Money::ZERO {
        add(&receipt.category, rest)?;
    }
    if parts.len() > 1 {
        parts.retain(|p| p.amount != Money::ZERO);
    }
    if parts.is_empty() {
        // Every line nets to zero against the receipt: keep it visible where
        // the receipt itself is filed.
        parts.push(CategoryPart {
            receipt,
            category: receipt.category.clone(),
            amount: Money::ZERO,
        });
    }
    Ok(parts)
}

/// Split every receipt into its per-category shares, reading all their lines
/// from `db` in one [`DatabaseAdapter::line_items_for`] call (see
/// [`split_receipt`]). Parts come in receipt order.
///
/// # Errors
/// Propagates the adapter's [`PhoskError`]; [`PhoskError::Overflow`] on a
/// centime overflow.
pub async fn receipt_parts<'a>(
    db: &dyn DatabaseAdapter,
    receipts: &'a [Receipt],
) -> Result<Vec<CategoryPart<'a>>, PhoskError> {
    let ids: Vec<ReceiptId> = receipts.iter().map(|r| r.id).collect();
    let mut by_receipt: HashMap<ReceiptId, Vec<LineItem>> = HashMap::new();
    for line in db.line_items_for(&ids).await? {
        by_receipt.entry(line.receipt_id).or_default().push(line);
    }
    let mut out = Vec::with_capacity(receipts.len());
    for r in receipts {
        let lines = by_receipt.get(&r.id).map_or(&[][..], Vec::as_slice);
        out.extend(split_receipt(r, lines)?);
    }
    Ok(out)
}

/// Σ of the parts booked under `category`.
///
/// # Errors
/// [`PhoskError::Overflow`] if the sum leaves the `i64` centime range.
pub fn spent_in(parts: &[CategoryPart<'_>], category: &str) -> Result<Money, PhoskError> {
    Money::sum(
        parts
            .iter()
            .filter(|p| p.category == category)
            .map(|p| p.amount),
    )
}

/// Σ of the item-level spend under `category` across `receipts`: the
/// one-shot form of [`receipt_parts`] + [`spent_in`].
///
/// # Errors
/// As [`receipt_parts`].
pub async fn category_spent(
    db: &dyn DatabaseAdapter,
    receipts: &[Receipt],
    category: &str,
) -> Result<Money, PhoskError> {
    spent_in(&receipt_parts(db, receipts).await?, category)
}

/// The `(fixed, variable)` totals of `parts` (see [`CategoryPart::is_fixed`]),
/// ready for
/// [`CycleWindow::project_spend`](phosk_core::cycle::CycleWindow::project_spend).
///
/// # Errors
/// [`PhoskError::Overflow`] if a sum leaves the `i64` centime range.
pub fn fixed_and_variable<'p, 'a: 'p>(
    parts: impl IntoIterator<Item = &'p CategoryPart<'a>>,
    caps: &[CategoryCap],
) -> Result<(Money, Money), PhoskError> {
    let (mut fixed, mut variable) = (Money::ZERO, Money::ZERO);
    for p in parts {
        if p.is_fixed(caps) {
            fixed = fixed.checked_add(p.amount)?;
        } else {
            variable = variable.checked_add(p.amount)?;
        }
    }
    Ok((fixed, variable))
}

/// The per-category totals of `parts`, in the order categories first appear.
///
/// # Errors
/// [`PhoskError::Overflow`] if a sum leaves the `i64` centime range.
pub fn totals_by_category(parts: &[CategoryPart<'_>]) -> Result<Vec<(String, Money)>, PhoskError> {
    let mut totals: Vec<(String, Money)> = Vec::new();
    for p in parts {
        match totals.iter_mut().find(|(n, _)| *n == p.category) {
            Some((_, t)) => *t = t.checked_add(p.amount)?,
            None => totals.push((p.category.clone(), p.amount)),
        }
    }
    Ok(totals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use phosk_id::{CategoryId, LineItemId, ReceiptId};
    use phosk_model::Provenance;

    fn receipt(category: &str, centimes: i64) -> Receipt {
        Receipt {
            id: ReceiptId::new(),
            slug: "r".to_owned(),
            shop: "Migros".to_owned(),
            date: NaiveDate::from_ymd_opt(2026, 10, 1).expect("valid date"),
            category: category.to_owned(),
            amount: Money::from_centimes(centimes),
            fixed: false,
            provenance: Provenance::user_entered(),
            source_kind: "PHOTO".to_owned(),
            ocr_engine: String::new(),
            ocr_regions: 0,
        }
    }

    fn line(r: &Receipt, category: &str, centimes: i64) -> LineItem {
        LineItem {
            id: LineItemId::new(),
            receipt_id: r.id,
            name: "x".to_owned(),
            qty: 1.0,
            unit_price: Money::from_centimes(centimes),
            line_total: Money::from_centimes(centimes),
            category: category.to_owned(),
            signal_id: None,
            provenance: Provenance::user_entered(),
        }
    }

    fn cap(name: &str, fixed: bool) -> CategoryCap {
        CategoryCap {
            id: CategoryId::new(),
            slug: name.to_lowercase(),
            name: name.to_owned(),
            cap: Some(Money::from_centimes(10_000)),
            fixed,
            glyph: String::new(),
            note: String::new(),
            provenance: Provenance::user_entered(),
        }
    }

    fn shares(parts: &[CategoryPart<'_>]) -> Vec<(String, i64)> {
        parts
            .iter()
            .map(|p| (p.category.clone(), p.amount.centimes()))
            .collect()
    }

    #[test]
    fn a_total_only_receipt_counts_under_its_own_category() {
        let r = receipt("Groceries", 4_250);
        let parts = split_receipt(&r, &[]).expect("split");
        assert_eq!(shares(&parts), vec![("Groceries".to_owned(), 4_250)]);
    }

    #[test]
    fn lines_count_under_their_own_category() {
        let r = receipt("Groceries", 4_250);
        let lines = [
            line(&r, "Groceries", 3_000),
            line(&r, "Health", 450),
            line(&r, "Groceries", 800),
        ];
        let parts = split_receipt(&r, &lines).expect("split");
        assert_eq!(
            shares(&parts),
            vec![("Groceries".to_owned(), 3_800), ("Health".to_owned(), 450)]
        );
    }

    #[test]
    fn a_receipt_whose_lines_are_all_elsewhere_leaves_its_own_category_empty() {
        let r = receipt("Groceries", 450);
        let parts = split_receipt(&r, &[line(&r, "Health", 450)]).expect("split");
        assert_eq!(shares(&parts), vec![("Health".to_owned(), 450)]);
    }

    #[test]
    fn an_unitemised_difference_stays_with_the_receipt_category() {
        // CHF 10.00 receipt, lines add up to 10.50: a 0.50 discount nobody
        // itemised. The parts still sum to the receipt amount.
        let r = receipt("Groceries", 1_000);
        let lines = [line(&r, "Health", 300), line(&r, "Groceries", 750)];
        let parts = split_receipt(&r, &lines).expect("split");
        assert_eq!(
            shares(&parts),
            vec![("Health".to_owned(), 300), ("Groceries".to_owned(), 700)]
        );
        let total = Money::sum(parts.iter().map(|p| p.amount)).expect("sum");
        assert_eq!(total, r.amount);
        assert_eq!(spent_in(&parts, "Health").expect("sum").centimes(), 300);
        assert_eq!(spent_in(&parts, "Transport").expect("sum"), Money::ZERO);
    }

    #[test]
    fn a_zero_receipt_stays_in_its_category() {
        let r = receipt("Groceries", 0);
        let parts = split_receipt(&r, &[]).expect("split");
        assert_eq!(shares(&parts), vec![("Groceries".to_owned(), 0)]);
    }

    #[test]
    fn fixed_shares_follow_the_receipt_flag_or_a_fixed_cap() {
        let mut rent = receipt("Rent", 168_000);
        rent.fixed = true;
        let food = receipt("Groceries", 3_000);
        let health = receipt("Health insurance", 31_800);
        let mut parts = split_receipt(&rent, &[]).expect("split");
        parts.extend(split_receipt(&food, &[]).expect("split"));
        parts.extend(split_receipt(&health, &[]).expect("split"));
        let caps = [cap("Groceries", false), cap("Health insurance", true)];
        let (fixed, variable) = fixed_and_variable(&parts, &caps).expect("sum");
        assert_eq!(fixed, Money::from_centimes(168_000 + 31_800));
        assert_eq!(variable, Money::from_centimes(3_000));
        assert_eq!(
            totals_by_category(&parts).expect("sum"),
            vec![
                ("Rent".to_owned(), Money::from_centimes(168_000)),
                ("Groceries".to_owned(), Money::from_centimes(3_000)),
                ("Health insurance".to_owned(), Money::from_centimes(31_800)),
            ]
        );
    }
}
