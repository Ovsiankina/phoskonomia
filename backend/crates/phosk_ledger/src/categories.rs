//! `categories` — the ledger-side category breakdown of spend.
//!
//! The authoritative budget/envelope category model lives in `phosk_planning`
//! (`CategoryCap`, caps, alerts). This ledger module owns only the *observed*
//! per-category spend distribution derived from receipts — the filter option
//! list the Transactions page shows and the per-category roll-up the receipt
//! list groups on.

use std::collections::BTreeSet;
use std::collections::HashMap;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::Period;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One category's observed spend across a cycle window (ledger view).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySpendDto {
    /// Category name (identity for UI/AI).
    pub category: String,
    /// Total spent in this category in the window (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
    /// Number of receipts in this category in the window.
    pub txns: u32,
}

/// The observed per-category spend distribution for the current cycle, ranked
/// by total spend descending (ties broken on name ascending).
#[tracing::instrument(level = "debug", skip_all)]
pub async fn category_spend(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<Vec<CategorySpendDto>, PhoskError> {
    let window = Period::Month.resolve(as_of)?;
    let receipts = db.receipts_between(window.start, window.end).await?;

    let mut totals: HashMap<String, (Money, u32)> = HashMap::new();
    for r in receipts {
        let entry = totals.entry(r.category).or_insert((Money::ZERO, 0));
        entry.0 = entry.0.checked_add(r.amount)?;
        entry.1 = entry.1.saturating_add(1);
    }

    let mut ranked: Vec<CategorySpendDto> = totals
        .into_iter()
        .map(|(category, (total, txns))| CategorySpendDto {
            category,
            total,
            txns,
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| a.category.cmp(&b.category))
    });
    Ok(ranked)
}

/// The distinct category names present in the seed (filter-dropdown options),
/// sorted ascending.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn available_categories(db: &dyn DatabaseAdapter) -> Result<Vec<String>, PhoskError> {
    let names: BTreeSet<String> = db
        .all_receipts()
        .await?
        .into_iter()
        .map(|r| r.category)
        .collect();
    Ok(names.into_iter().collect())
}
