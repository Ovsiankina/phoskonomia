//! `phosk_ledger` — the spending-record bounded context (transactions ·
//! categories · signals), exposing a **service** over the `DatabaseAdapter`
//! PORT (ADR-010 gateway).
//!
//! This file holds the dashboard's two spend-aggregation reads:
//!
//! - [`top_shops`] — the cycle's shops ranked by total spend (descending).
//! - [`daily_spend`] — the per-day spend totals across the cycle window.
//!
//! The rest of the context lives in the feature modules declared below —
//! including the write side ([`transactions::create_transaction`] for manual
//! entry, [`transactions::edit_transaction`] /
//! [`transactions::delete_transaction`] for correcting and removing a recorded
//! spend, the category create/rename/delete-if-empty in [`categories`], and
//! the CSV bank-export import in [`import`]).
//!
//! **Layering (ADR-010).** The service takes `&dyn DatabaseAdapter` and depends
//! only on the PORT trait crate (`phosk_adapter_db`) plus the domain/foundation
//! crates. It never imports a concrete adapter (`phosk_db_memory` is a *dev*-
//! dependency, used only by the tests). A technology swap is a new adapter
//! `impl`, never a change here.
//!
//! **Money & errors (ADR §0).** All amounts stay as exact [`Money`] (i64
//! centimes); summation is *checked* and surfaces a [`PhoskError::Overflow`]
//! rather than wrapping or panicking. CHF-number conversion is the HTTP edge's
//! job, not the service's. There is no `unwrap`/`expect`/`panic!` in this code:
//! every fallible step maps explicitly to a [`PhoskError`].
//!
//! [`Money`]: phosk_core::money::Money
//! [`PhoskError::Overflow`]: phosk_core::error::PhoskError::Overflow

use std::collections::HashMap;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::CycleWindow;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;

// ── Feature modules (build-contract §5.1: transactions · lines · categories ·
//    shops · signals). Each owns its DTOs + service fns; the existing dashboard
//    spend-aggregation reads (`top_shops`, `daily_spend`) stay in this file.
pub mod categories;
pub mod import;
pub mod line_items;
pub mod shops;
pub mod signals;
pub mod transactions;

/// One shop's total spend across a cycle window: the shop's display name
/// (its identity, ADR-008) and the exact [`Money`] summed for it.
///
/// Produced by [`top_shops`], sorted by `total` descending. The HTTP edge turns
/// `total` into a CHF number and derives the `share` (`total / maxTotal`) the
/// frontend renders; neither is computed here (ADR-010).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopTotal {
    /// The shop's display name, e.g. `"Migros"` (ADR-008 identity).
    pub shop: String,
    /// The exact total spent at this shop in the window, in [`Money`] centimes.
    pub total: Money,
}

/// The cycle's shops ranked by total spend, highest first, truncated to `limit`.
///
/// Every transaction in `window` (inclusive `[start, end]`) is grouped by shop
/// `name` and its amounts summed exactly. The result is sorted by `total`
/// descending; ties break on the shop name ascending so the ordering is
/// **deterministic** (the frontend's bar chart and the tests depend on a stable
/// order). A `limit` of `0` yields an empty `Vec`; a `limit` larger than the
/// number of distinct shops simply returns them all.
///
/// # Errors
/// - Propagates any [`PhoskError`] from the adapter's range query.
/// - Returns [`PhoskError::Overflow`] if a shop's running total overflows the
///   i64 centime range (checked arithmetic, never wrapping; ADR §0).
#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(from = %window.start, to = %window.end, limit)
)]
pub async fn top_shops(
    db: &dyn DatabaseAdapter,
    window: CycleWindow,
    limit: usize,
) -> Result<Vec<ShopTotal>, PhoskError> {
    let txns = db.transactions_between(window.start, window.end).await?;
    tracing::debug!(count = txns.len(), "loaded transactions for top-shops");

    // Group amounts by shop name, summing as we go with checked arithmetic so an
    // overflow becomes a real PhoskError rather than a silent wrap (ADR §0).
    let mut totals: HashMap<String, Money> = HashMap::new();
    for tx in txns {
        let entry = totals.entry(tx.shop).or_insert(Money::ZERO);
        *entry = entry.checked_add(tx.amount)?;
    }

    // Sort by total descending, breaking ties on shop name ascending for a
    // stable, reproducible ranking.
    let mut ranked: Vec<ShopTotal> = totals
        .into_iter()
        .map(|(shop, total)| ShopTotal { shop, total })
        .collect();
    ranked.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.shop.cmp(&b.shop)));
    ranked.truncate(limit);

    tracing::debug!(shops = ranked.len(), "ranked top shops");
    Ok(ranked)
}

/// The per-day spend totals across the cycle window.
///
/// Returns a `Vec<Money>` of length [`CycleWindow::len_days`], indexed by the
/// **0-based day offset from `window.start`** (index 0 = `window.start`). Each
/// bucket is the exact sum of that day's transaction amounts; days with no
/// spend are [`Money::ZERO`]. Transactions outside `[start, end]` cannot appear
/// — the adapter's range query already bounds them, and any stray date is
/// ignored defensively.
///
/// This is the canonical `daily[]` series; the HTTP edge derives `cumulative[]`
/// / `pace[]` from it and converts to CHF numbers (ADR-010).
///
/// # Errors
/// - Propagates any [`PhoskError`] from the adapter's range query.
/// - Returns [`PhoskError::Overflow`] if a day's running total overflows the
///   i64 centime range (checked arithmetic, never wrapping; ADR §0).
#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(from = %window.start, to = %window.end, days = window.len_days())
)]
pub async fn daily_spend(
    db: &dyn DatabaseAdapter,
    window: CycleWindow,
) -> Result<Vec<Money>, PhoskError> {
    let txns = db.transactions_between(window.start, window.end).await?;
    tracing::debug!(count = txns.len(), "loaded transactions for daily-spend");

    let len = window.len_days() as usize;
    let mut buckets = vec![Money::ZERO; len];

    for tx in txns {
        // 0-based offset from the window start. `transactions_between` already
        // bounds the dates to `[start, end]`, so this is in `0..len`; a stray
        // out-of-range date is skipped defensively rather than panicking or
        // corrupting a neighbouring bucket — a date the DB should never have
        // returned is not a domain value to carry (ADR §0).
        let offset = (tx.date - window.start).num_days();
        let Ok(idx) = usize::try_from(offset) else {
            tracing::trace!(date = %tx.date, "skipping transaction before window start");
            continue;
        };
        if let Some(bucket) = buckets.get_mut(idx) {
            *bucket = bucket.checked_add(tx.amount)?;
        } else {
            tracing::trace!(date = %tx.date, "skipping transaction after window end");
        }
    }

    tracing::debug!(buckets = buckets.len(), "built daily spend series");
    Ok(buckets)
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::NaiveDate;
    use phosk_core::cycle::Period;
    use phosk_db_memory::MemoryDb;

    fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    /// The June 2026 cycle window resolved at the spec's "today" (2026-06-18).
    /// `Period::Month` gives the full calendar month `[2026-06-01, 2026-06-30]`,
    /// 30 days long, which is what the dashboard's "current cycle" uses.
    fn june_window() -> CycleWindow {
        Period::Month
            .resolve(naive(2026, 6, 18))
            .expect("June resolves")
    }

    fn may_window() -> CycleWindow {
        Period::Month
            .resolve(naive(2026, 5, 18))
            .expect("May resolves")
    }

    fn seeded() -> MemoryDb {
        MemoryDb::seeded().expect("seed is valid")
    }

    // ── top_shops ────────────────────────────────────────────────────────────

    /// The June ranking by total spend, descending, is exact and deterministic.
    /// Computed directly from the seed line-items (the canonical source):
    /// Landlord 1680.00 > Krankenkasse 318.00 > Coop 212.80 > Migros 174.35 > …
    #[tokio::test]
    async fn top_shops_june_ranked_by_total_descending() {
        let db = seeded();
        let shops = top_shops(&db, june_window(), 5)
            .await
            .expect("top_shops ok");

        let expected = [
            ("Landlord", 168_000_i64),
            ("Krankenkasse", 31_800),
            ("Coop", 21_280),
            ("Migros", 17_435),
            ("Galaxus", 12_990),
        ];
        assert_eq!(shops.len(), 5, "limit of 5 returns five shops");
        for (got, (name, cents)) in shops.iter().zip(expected) {
            assert_eq!(got.shop, name, "shop name in rank order");
            assert_eq!(got.total.centimes(), cents, "exact total for {name}");
        }
    }

    /// Totals are non-increasing down the ranking — the core ordering invariant.
    #[tokio::test]
    async fn top_shops_is_sorted_non_increasing() {
        let db = seeded();
        let shops = top_shops(&db, june_window(), 100)
            .await
            .expect("top_shops ok");
        assert!(
            shops.windows(2).all(|w| w[0].total >= w[1].total),
            "each total is >= the next"
        );
    }

    /// June has 20 distinct shops; a `limit` larger than that returns them all
    /// (no padding, no error).
    #[tokio::test]
    async fn top_shops_limit_beyond_count_returns_all_distinct_shops() {
        let db = seeded();
        let shops = top_shops(&db, june_window(), 1000)
            .await
            .expect("top_shops ok");
        assert_eq!(shops.len(), 20, "June has 20 distinct shops");
        // The summed totals equal the cycle total (CHF 3222.45 from the seed).
        let summed = Money::sum(shops.iter().map(|s| s.total)).expect("no overflow");
        assert_eq!(summed.centimes(), 322_245);
    }

    /// A `limit` of 0 is a valid request for "no shops".
    #[tokio::test]
    async fn top_shops_limit_zero_is_empty() {
        let db = seeded();
        let shops = top_shops(&db, june_window(), 0)
            .await
            .expect("top_shops ok");
        assert!(shops.is_empty());
    }

    /// Repeat shops within the window are aggregated, not listed separately:
    /// Migros appears 3× in June (58.75 + 61.75 + 53.85) → CHF 174.35.
    #[tokio::test]
    async fn top_shops_aggregates_repeat_visits() {
        let db = seeded();
        let shops = top_shops(&db, june_window(), 100)
            .await
            .expect("top_shops ok");
        let migros = shops
            .iter()
            .find(|s| s.shop == "Migros")
            .expect("Migros present");
        assert_eq!(migros.total.centimes(), 17_435, "3 Migros visits summed");
        assert_eq!(
            shops.iter().filter(|s| s.shop == "Migros").count(),
            1,
            "Migros appears once, aggregated"
        );
    }

    /// An empty window (no seeded data) yields no shops, not an error.
    #[tokio::test]
    async fn top_shops_empty_window_is_empty() {
        let db = seeded();
        let april = Period::Month
            .resolve(naive(2026, 4, 15))
            .expect("April resolves");
        let shops = top_shops(&db, april, 10).await.expect("top_shops ok");
        assert!(shops.is_empty(), "no seeded data in April");
    }

    /// Ties on total break on shop name ascending, so the order is reproducible.
    /// Built from a tiny custom DB with two equal totals to assert alphabetical
    /// order deterministically.
    #[tokio::test]
    async fn top_shops_breaks_ties_on_name_ascending() {
        use phosk_model::{BudgetConfig, Transaction};

        let txns = vec![
            Transaction {
                date: naive(2026, 6, 2),
                shop: "Zebra".to_owned(),
                category: "GROCERIES".to_owned(),
                amount: Money::from_centimes(5000),
            },
            Transaction {
                date: naive(2026, 6, 3),
                shop: "Alpha".to_owned(),
                category: "GROCERIES".to_owned(),
                amount: Money::from_centimes(5000),
            },
        ];
        let db = MemoryDb::new(
            txns,
            Vec::new(),
            BudgetConfig {
                monthly_budget: Money::ZERO,
                savings_target: Money::ZERO,
            },
        );
        let shops = top_shops(&db, june_window(), 10)
            .await
            .expect("top_shops ok");
        assert_eq!(
            shops[0].shop, "Alpha",
            "equal totals sort by name ascending"
        );
        assert_eq!(shops[1].shop, "Zebra");
    }

    // ── daily_spend ────────────────────────────────────────────────────────────

    /// The series length equals the window's day count (30 for June 2026).
    #[tokio::test]
    async fn daily_spend_length_is_days_in_cycle() {
        let db = seeded();
        let series = daily_spend(&db, june_window())
            .await
            .expect("daily_spend ok");
        assert_eq!(series.len(), 30, "June 2026 spans 30 days");
    }

    /// The full June daily series, indexed by day offset from June 1, matches the
    /// seed's per-day spend exactly (days 20–30 have no spend → CHF 0.00).
    #[tokio::test]
    async fn daily_spend_june_buckets_are_exact() {
        let db = seeded();
        let series = daily_spend(&db, june_window())
            .await
            .expect("daily_spend ok");

        // (0-based day offset, centimes) for every non-zero day; computed from the
        // seed line-items.
        let expected: [(usize, i64); 19] = [
            (0, 205_675),
            (1, 7_950),
            (2, 1_850),
            (3, 1_280),
            (4, 10_395),
            (5, 2_860),
            (6, 6_750),
            (7, 10_920),
            (8, 7_430),
            (9, 4_800),
            (10, 6_175),
            (11, 5_795),
            (12, 3_640),
            (13, 8_520),
            (14, 2_840),
            (15, 19_440),
            (16, 7_900),
            (17, 1_960),
            (18, 6_065),
        ];
        for (idx, cents) in expected {
            assert_eq!(
                series[idx].centimes(),
                cents,
                "day offset {idx} (June {}) total",
                idx + 1
            );
        }
        // Days 20..=30 (offsets 19..30) have no seeded spend.
        for (idx, bucket) in series.iter().enumerate().skip(19) {
            assert_eq!(*bucket, Money::ZERO, "day offset {idx} has no spend");
        }
    }

    /// The buckets sum to the cycle total — the daily series and the line-item
    /// total are consistent (CHF 3222.45 for June).
    #[tokio::test]
    async fn daily_spend_buckets_sum_to_cycle_total() {
        let db = seeded();
        let series = daily_spend(&db, june_window())
            .await
            .expect("daily_spend ok");
        let total = Money::sum(series).expect("no overflow");
        assert_eq!(total.centimes(), 322_245, "June series sums to CHF 3222.45");
    }

    /// Index 0 is the window start (June 1), carrying that day's three fixed
    /// line-items (Migros 58.75 + rent 1680.00 + insurance 318.00 = CHF 2056.75).
    #[tokio::test]
    async fn daily_spend_index_zero_is_window_start() {
        let db = seeded();
        let series = daily_spend(&db, june_window())
            .await
            .expect("daily_spend ok");
        assert_eq!(
            series[0].centimes(),
            205_675,
            "index 0 = June 1 = CHF 2056.75"
        );
    }

    /// The May cycle (31-day window) also produces a full-length series summing
    /// to the seed's May total (CHF 3787.70) — last-cycle comparison is available.
    #[tokio::test]
    async fn daily_spend_may_full_window_sums_to_may_total() {
        let db = seeded();
        let series = daily_spend(&db, may_window())
            .await
            .expect("daily_spend ok");
        assert_eq!(series.len(), 31, "May 2026 spans 31 days");
        let total = Money::sum(series).expect("no overflow");
        assert_eq!(total.centimes(), 378_770, "May series sums to CHF 3787.70");
    }

    /// An empty window yields an all-zero series of the right length, not an error.
    #[tokio::test]
    async fn daily_spend_empty_window_is_all_zero() {
        let db = seeded();
        let april = Period::Month
            .resolve(naive(2026, 4, 15))
            .expect("April resolves");
        let series = daily_spend(&db, april).await.expect("daily_spend ok");
        assert_eq!(series.len(), 30, "April spans 30 days");
        assert!(
            series.iter().all(|m| *m == Money::ZERO),
            "no spend → every bucket is zero"
        );
    }

    /// The service is callable behind the `&dyn DatabaseAdapter` PORT handle
    /// (ADR-010) — it never sees the concrete `MemoryDb` type.
    #[tokio::test]
    async fn service_works_through_the_port_trait_object() {
        let db = seeded();
        let port: &dyn DatabaseAdapter = &db;
        let shops = top_shops(port, june_window(), 3)
            .await
            .expect("top_shops ok");
        assert_eq!(shops.len(), 3);
        let series = daily_spend(port, june_window())
            .await
            .expect("daily_spend ok");
        assert_eq!(series.len(), 30);
    }
}
