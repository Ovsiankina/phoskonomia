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
//! RED tests for `phosk_ledger::categories` — the ledger-side per-category spend
//! distribution (`category_spend`) and the filter-dropdown option list
//! (`available_categories`).
//!
//! These pin EXACT centime totals + receipt counts against the deterministic
//! Swiss seed at `as_of = 2026-06-18` (June cycle `[2026-06-01, 2026-06-30]`).
//! The service bodies are `todo!()`, so every test here COMPILES then panics at
//! runtime — this is the red phase.
//!
//! Source of truth for expected values (the June receipts `t1..t9`, see
//! `phosk_db_memory::seed::seed_receipts_and_lines`):
//!
//! | slug | shop            | category          | amount (cents) |
//! |------|-----------------|-------------------|----------------|
//! | t1   | Migros          | Groceries         |  5_875         |
//! | t2   | Restaurant Linde| Going out         |  6_450         |
//! | t3   | Coop            | Groceries         |  4_230         |
//! | t4   | Galaxus         | Shopping          | 12_990         |
//! | t5   | Migros          | Coffee & snacks   |  1_280         |
//! | t6   | Denner          | Groceries         |  2_990         |
//! | t7   | SBB             | Transport         |  3_400         |
//! | t8   | Landlord        | Rent              |168_000         |
//! | t9   | Helsana         | Health insurance  | 31_800         |
//!
//! Per-category roll-up (centimes / receipt count), ranked by total descending,
//! ties broken on name ascending:
//!   Rent             168_000 / 1
//!   Health insurance  31_800 / 1
//!   Groceries         13_095 / 3   (5_875 + 4_230 + 2_990)
//!   Shopping          12_990 / 1
//!   Going out          6_450 / 1
//!   Transport          3_400 / 1
//!   Coffee & snacks    1_280 / 1
//!   ─────────────────────────────
//!   total            237_015 / 9   (7 distinct categories)

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_ledger::categories::{CategorySpendDto, available_categories, category_spend};

/// The seeded demo clock: day 18 of the June 2026 cycle.
fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid as_of date")
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// Look up one category's roll-up by name, or fail the test with a clear message.
fn find<'a>(rows: &'a [CategorySpendDto], name: &str) -> &'a CategorySpendDto {
    rows.iter()
        .find(|r| r.category == name)
        .unwrap_or_else(|| panic!("category {name} present in roll-up"))
}

// ── category_spend ─────────────────────────────────────────────────────────────

/// The June cycle has exactly seven distinct categories.
#[tokio::test]
async fn category_spend_has_seven_distinct_categories() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    assert_eq!(rows.len(), 7, "seven distinct categories in the June cycle");
}

/// The full ranking — category, total (centimes), receipt count — is exact and
/// ordered by total descending (ties on name ascending). This is the spine test.
#[tokio::test]
async fn category_spend_full_ranking_is_exact() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");

    // (category, total_centimes, txns) in the expected rank order.
    let expected = [
        ("Rent", 168_000_i64, 1_u32),
        ("Health insurance", 31_800, 1),
        ("Groceries", 13_095, 3),
        ("Shopping", 12_990, 1),
        ("Going out", 6_450, 1),
        ("Transport", 3_400, 1),
        ("Coffee & snacks", 1_280, 1),
    ];
    assert_eq!(
        rows.len(),
        expected.len(),
        "row count matches expected ranking"
    );
    for (got, (name, cents, txns)) in rows.iter().zip(expected) {
        assert_eq!(got.category, name, "category name in rank order");
        assert_eq!(got.total.centimes(), cents, "exact total for {name}");
        assert_eq!(got.txns, txns, "receipt count for {name}");
    }
}

/// Totals are non-increasing down the ranking — the core ordering invariant.
#[tokio::test]
async fn category_spend_is_sorted_non_increasing() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    assert!(
        rows.windows(2).all(|w| w[0].total >= w[1].total),
        "each total is >= the next"
    );
}

/// Groceries spans three receipts (t1 5_875 + t3 4_230 + t6 2_990); they are
/// aggregated into ONE row, not listed separately, with a count of 3.
#[tokio::test]
async fn category_spend_aggregates_repeat_category_receipts() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    assert_eq!(
        rows.iter().filter(|r| r.category == "Groceries").count(),
        1,
        "Groceries appears once, aggregated"
    );
    let groceries = find(&rows, "Groceries");
    assert_eq!(
        groceries.total.centimes(),
        13_095,
        "3 grocery receipts summed"
    );
    assert_eq!(groceries.txns, 3, "three grocery receipts counted");
}

/// A single-receipt category (Coffee & snacks: only t5) carries txns == 1 and the
/// receipt's exact amount.
#[tokio::test]
async fn category_spend_single_receipt_category_is_exact() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    let coffee = find(&rows, "Coffee & snacks");
    assert_eq!(coffee.total.centimes(), 1_280, "lone t5 amount");
    assert_eq!(coffee.txns, 1, "one Coffee & snacks receipt");
}

/// The per-category totals sum to the June receipt total (CHF 2370.15), and the
/// counts sum to the nine seeded receipts — the roll-up is exhaustive and lossless.
#[tokio::test]
async fn category_spend_totals_and_counts_are_exhaustive() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");

    let summed = Money::sum(rows.iter().map(|r| r.total)).expect("no overflow");
    assert_eq!(
        summed.centimes(),
        237_015,
        "category totals sum to CHF 2370.15"
    );

    let receipts: u32 = rows.iter().map(|r| r.txns).sum();
    assert_eq!(receipts, 9, "counts sum to the nine June receipts");
}

/// The fixed standing charges (Rent, Health insurance) are present in the spend
/// roll-up just like variable categories — the ledger view does not exclude them.
#[tokio::test]
async fn category_spend_includes_fixed_charges() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    assert_eq!(find(&rows, "Rent").total.centimes(), 168_000, "Rent total");
    assert_eq!(
        find(&rows, "Health insurance").total.centimes(),
        31_800,
        "Health insurance total"
    );
}

/// An `as_of` whose cycle has no receipts (April 2026) yields an empty roll-up,
/// not an error.
#[tokio::test]
async fn category_spend_empty_cycle_is_empty() {
    let db = seeded();
    let april = NaiveDate::from_ymd_opt(2026, 4, 15).expect("valid date");
    let rows = category_spend(&db, april).await.expect("category_spend ok");
    assert!(
        rows.is_empty(),
        "no seeded receipts in April → empty roll-up"
    );
}

/// The May cycle holds no seeded *receipts* (receipts are all dated June), so its
/// roll-up is empty — confirming `category_spend` is scoped to the resolved cycle
/// window, not the whole receipt table.
#[tokio::test]
async fn category_spend_is_scoped_to_the_resolved_cycle() {
    let db = seeded();
    let may = NaiveDate::from_ymd_opt(2026, 5, 18).expect("valid date");
    let rows = category_spend(&db, may).await.expect("category_spend ok");
    assert!(
        rows.is_empty(),
        "seeded receipts are June-only → May cycle roll-up is empty"
    );
}

/// The DTO serializes to the camelCase wire form with money as exact i64 centimes
/// (`total` is a raw integer, NOT a CHF float).
#[tokio::test]
async fn category_spend_dto_serializes_centimes_camel_case() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    let rent = find(&rows, "Rent");
    let json = serde_json::to_value(rent).expect("serialize");
    assert_eq!(json["category"], serde_json::json!("Rent"));
    assert_eq!(
        json["total"],
        serde_json::json!(168_000),
        "exact centimes, not CHF"
    );
    assert_eq!(json["txns"], serde_json::json!(1));
}

/// The service is callable behind the `&dyn DatabaseAdapter` PORT handle (ADR-010)
/// — it never sees the concrete `MemoryDb` type.
#[tokio::test]
async fn category_spend_works_through_the_port_trait_object() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;
    let rows = category_spend(port, as_of())
        .await
        .expect("category_spend ok");
    assert_eq!(rows.len(), 7);
}

// ── available_categories ───────────────────────────────────────────────────────

/// The distinct category names across all receipts, sorted ascending — the exact
/// filter-dropdown option list.
#[tokio::test]
async fn available_categories_are_distinct_and_sorted() {
    let db = seeded();
    let cats = available_categories(&db)
        .await
        .expect("available_categories ok");
    let expected = vec![
        "Coffee & snacks".to_owned(),
        "Going out".to_owned(),
        "Groceries".to_owned(),
        "Health insurance".to_owned(),
        "Rent".to_owned(),
        "Shopping".to_owned(),
        "Transport".to_owned(),
    ];
    assert_eq!(cats, expected, "ascending, de-duplicated category names");
}

/// There are exactly seven distinct categories (no duplicates from the three
/// Groceries receipts).
#[tokio::test]
async fn available_categories_has_seven_entries() {
    let db = seeded();
    let cats = available_categories(&db)
        .await
        .expect("available_categories ok");
    assert_eq!(
        cats.len(),
        7,
        "seven distinct categories across all receipts"
    );
}

/// The list is strictly ascending (sorted, no dupes) — a structural invariant the
/// filter UI relies on.
#[tokio::test]
async fn available_categories_is_strictly_ascending() {
    let db = seeded();
    let cats = available_categories(&db)
        .await
        .expect("available_categories ok");
    assert!(
        cats.windows(2).all(|w| w[0] < w[1]),
        "strictly ascending: sorted and de-duplicated"
    );
}

/// `available_categories` spans the WHOLE receipt table (it is the unfiltered
/// option list), so it is independent of any cycle window — its set of names is a
/// superset of (here equal to) the current cycle's `category_spend` names.
#[tokio::test]
async fn available_categories_covers_category_spend_names() {
    let db = seeded();
    let cats = available_categories(&db)
        .await
        .expect("available_categories ok");
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    for row in &rows {
        assert!(
            cats.contains(&row.category),
            "{} from the cycle roll-up is a known filter option",
            row.category
        );
    }
}
