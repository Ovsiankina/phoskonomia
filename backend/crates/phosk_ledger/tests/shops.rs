#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors the
    // dashboard integration test).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::bool_assert_comparison,
    clippy::needless_collect,
    clippy::comparison_chain,
    clippy::redundant_closure_for_method_calls,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_possible_truncation
)]
//! RED tests for `phosk_ledger::shops` — the shop directory + per-shop spend
//! roll-up derived from the receipt seed (`t1..t9`).
//!
//! These assert EXACT centime values computed directly from the deterministic
//! Swiss seed's receipts (`phosk_db_memory::seed::seed_receipts_and_lines`):
//!
//! | slug | shop             | amount (cents) |
//! |------|------------------|----------------|
//! | t1   | Migros           |          5_875 |
//! | t2   | Restaurant Linde |          6_450 |
//! | t3   | Coop             |          4_230 |
//! | t4   | Galaxus          |         12_990 |
//! | t5   | Migros           |          1_280 |
//! | t6   | Denner           |          2_990 |
//! | t7   | SBB              |          3_400 |
//! | t8   | Landlord         |        168_000 |
//! | t9   | Helsana          |         31_800 |
//!
//! Aggregated by shop (all-time, across `all_receipts()`):
//!   Landlord 168_000(1) · Helsana 31_800(1) · Galaxus 12_990(1) ·
//!   Migros 7_155(2) · Restaurant Linde 6_450(1) · Coop 4_230(1) ·
//!   SBB 3_400(1) · Denner 2_990(1). Eight distinct shops; total 237_015.
//!
//! The bodies are `todo!()`, so every test here MUST compile and then FAIL at
//! runtime (RED phase). No production logic lives here.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_ledger::shops::{ShopDto, available_shops, list_shops};

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// The eight distinct shop names across the receipt seed.
const SHOPS: [&str; 8] = [
    "Coop",
    "Denner",
    "Galaxus",
    "Helsana",
    "Landlord",
    "Migros",
    "Restaurant Linde",
    "SBB",
];

/// The full directory ranked by total spend descending, ties on name ascending.
/// `(shop, total_cents, txns)`.
const RANKED: [(&str, i64, u32); 8] = [
    ("Landlord", 168_000, 1),
    ("Helsana", 31_800, 1),
    ("Galaxus", 12_990, 1),
    ("Migros", 7_155, 2),
    ("Restaurant Linde", 6_450, 1),
    ("Coop", 4_230, 1),
    ("SBB", 3_400, 1),
    ("Denner", 2_990, 1),
];

// ── list_shops ─────────────────────────────────────────────────────────────

/// Eight distinct shops are derived from the nine seed receipts (Migros twice).
#[tokio::test]
async fn list_shops_has_eight_distinct_rows() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    assert_eq!(shops.len(), 8, "nine receipts collapse to eight shops");
}

/// The full directory is exact in name, total (centimes), and txn count, in the
/// agreed rank order (total desc, name asc on ties).
#[tokio::test]
async fn list_shops_full_directory_is_exact() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    assert_eq!(shops.len(), RANKED.len(), "row count matches the directory");
    for (got, (name, cents, txns)) in shops.iter().zip(RANKED) {
        assert_eq!(got.shop, name, "shop name in rank order");
        assert_eq!(got.total.centimes(), cents, "exact total for {name}");
        assert_eq!(got.txns, txns, "receipt count for {name}");
    }
}

/// Migros appears on two receipts (t1 58.75 + t5 12.80) and is aggregated into
/// one row of CHF 71.55 with txns = 2 — not listed twice.
#[tokio::test]
async fn list_shops_aggregates_repeat_shop() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    let migros: Vec<&ShopDto> = shops.iter().filter(|s| s.shop == "Migros").collect();
    assert_eq!(migros.len(), 1, "Migros appears once, aggregated");
    assert_eq!(migros[0].total.centimes(), 7_155, "5_875 + 1_280");
    assert_eq!(migros[0].txns, 2, "two Migros receipts");
}

/// Ranked by total spend descending — the core ordering invariant.
#[tokio::test]
async fn list_shops_is_sorted_non_increasing() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    assert!(
        shops.windows(2).all(|w| w[0].total >= w[1].total),
        "each total is >= the next"
    );
}

/// The directory leads with the largest standing charge (Landlord 1680.00).
#[tokio::test]
async fn list_shops_leads_with_landlord() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    let first = shops.first().expect("at least one shop");
    assert_eq!(first.shop, "Landlord");
    assert_eq!(first.total.centimes(), 168_000);
    assert_eq!(first.txns, 1);
}

/// Every shop's total is the exact sum of its receipts; the directory totals sum
/// to the all-receipts total (CHF 2370.15 across the nine receipts).
#[tokio::test]
async fn list_shops_totals_sum_to_all_receipts_total() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    let summed = Money::sum(shops.iter().map(|s| s.total)).expect("no overflow");
    assert_eq!(summed.centimes(), 237_015, "9 receipts sum to CHF 2370.15");
    let total_txns: u32 = shops.iter().map(|s| s.txns).sum();
    assert_eq!(total_txns, 9, "txn counts sum to the nine receipts");
}

/// An empty database yields an empty directory, not an error.
#[tokio::test]
async fn list_shops_empty_db_is_empty() {
    use phosk_model::BudgetConfig;
    let db = MemoryDb::new(
        Vec::new(),
        Vec::new(),
        BudgetConfig {
            monthly_budget: Money::ZERO,
            savings_target: Money::ZERO,
        },
    );
    let shops = list_shops(&db).await.expect("list_shops ok");
    assert!(shops.is_empty(), "no receipts → no shops");
}

/// `ShopDto` serialises in the camelCase wire shape with money as an exact i64
/// centime number (never a CHF float) — the locked money-on-the-wire rule.
#[tokio::test]
async fn list_shops_serialises_money_as_exact_centimes() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    let migros = shops
        .iter()
        .find(|s| s.shop == "Migros")
        .expect("Migros present");
    let json = serde_json::to_value(migros).expect("serialises");
    assert_eq!(json["shop"], "Migros", "shop key present");
    assert_eq!(json["total"], 7_155, "total is exact i64 centimes, not CHF");
    assert_eq!(json["txns"], 2, "txns key present");
    assert!(json.get("totalChf").is_none(), "no CHF float on the wire");
    assert!(
        json["total"].is_i64() || json["total"].is_u64(),
        "total is integral"
    );
}

// ── available_shops ──────────────────────────────────────────────────────────

/// The distinct shop names, sorted ascending, for the filter dropdown.
#[tokio::test]
async fn available_shops_is_distinct_and_sorted_ascending() {
    let db = seeded();
    let names = available_shops(&db).await.expect("available_shops ok");
    assert_eq!(names, SHOPS.to_vec(), "distinct shop names, A→Z");
}

/// No duplicates even though Migros backs two receipts.
#[tokio::test]
async fn available_shops_has_no_duplicates() {
    let db = seeded();
    let names = available_shops(&db).await.expect("available_shops ok");
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "no duplicate shop names");
    assert_eq!(names.len(), 8, "eight distinct shops");
}

/// The names are strictly ascending (sorted, no equal neighbours).
#[tokio::test]
async fn available_shops_is_strictly_ascending() {
    let db = seeded();
    let names = available_shops(&db).await.expect("available_shops ok");
    assert!(
        names.windows(2).all(|w| w[0] < w[1]),
        "names strictly ascending"
    );
}

/// An empty database yields no shop options, not an error.
#[tokio::test]
async fn available_shops_empty_db_is_empty() {
    use phosk_model::BudgetConfig;
    let db = MemoryDb::new(
        Vec::new(),
        Vec::new(),
        BudgetConfig {
            monthly_budget: Money::ZERO,
            savings_target: Money::ZERO,
        },
    );
    let names = available_shops(&db).await.expect("available_shops ok");
    assert!(names.is_empty(), "no receipts → no shop options");
}

/// `list_shops` and `available_shops` agree on the shop set.
#[tokio::test]
async fn directory_and_options_agree_on_the_shop_set() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    let names = available_shops(&db).await.expect("available_shops ok");
    let mut from_dir: Vec<String> = shops.iter().map(|s| s.shop.clone()).collect();
    from_dir.sort();
    assert_eq!(
        from_dir, names,
        "the directory and the options list the same shops"
    );
}

// ── port object-safety ───────────────────────────────────────────────────────

/// Both services are callable behind the `&dyn DatabaseAdapter` PORT handle
/// (ADR-010) — they never see the concrete `MemoryDb` type.
#[tokio::test]
async fn services_work_through_the_port_trait_object() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;
    let shops = list_shops(port).await.expect("list_shops ok");
    assert_eq!(shops.len(), 8);
    let names = available_shops(port).await.expect("available_shops ok");
    assert_eq!(names.len(), 8);
}
