//! `shops` — the shop directory + per-shop spend roll-up.
//!
//! Complements the existing dashboard `top_shops` (cycle-bounded ranking, in
//! `lib.rs`) with the unfiltered shop directory the Transactions filter dropdown
//! and the shop inspector use. DTOs carry money as exact i64 centimes.

use std::collections::BTreeSet;
use std::collections::HashMap;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One shop's all-time spend + visit count (shop-directory row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopDto {
    /// Shop display name (identity for UI/AI).
    pub shop: String,
    /// All-time total spent at this shop (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
    /// Number of receipts from this shop.
    pub txns: u32,
}

/// The full shop directory across all receipts, ranked by total spend
/// descending (ties broken on name ascending).
#[tracing::instrument(level = "debug", skip_all)]
pub async fn list_shops(db: &dyn DatabaseAdapter) -> Result<Vec<ShopDto>, PhoskError> {
    let receipts = db.all_receipts().await?;

    // Group by shop name, summing exactly and counting visits.
    let mut totals: HashMap<String, (Money, u32)> = HashMap::new();
    for r in receipts {
        let entry = totals.entry(r.shop).or_insert((Money::ZERO, 0));
        entry.0 = entry.0.checked_add(r.amount)?;
        entry.1 = entry.1.saturating_add(1);
    }

    let mut ranked: Vec<ShopDto> = totals
        .into_iter()
        .map(|(shop, (total, txns))| ShopDto { shop, total, txns })
        .collect();
    // Total descending, ties on shop name ascending — a deterministic ranking.
    ranked.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.shop.cmp(&b.shop)));
    Ok(ranked)
}

/// The distinct shop names present in the seed (filter-dropdown options),
/// sorted ascending.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn available_shops(db: &dyn DatabaseAdapter) -> Result<Vec<String>, PhoskError> {
    let names: BTreeSet<String> = db
        .all_receipts()
        .await?
        .into_iter()
        .map(|r| r.shop)
        .collect();
    Ok(names.into_iter().collect())
}
