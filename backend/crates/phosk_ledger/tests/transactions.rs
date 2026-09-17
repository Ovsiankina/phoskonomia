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
//! RED tests for `phosk_ledger::{transactions, line_items, categories, shops}`.
//!
//! These pin the EXACT derived values produced by the deterministic Swiss seed
//! in `phosk_db_memory` (the canonical source — its receipt line-items, NOT the
//! dioxus wire seed, are ground truth for the backend). Every body under test is
//! `todo!()`, so each test must COMPILE and then FAIL at runtime (RED phase).
//!
//! Money is asserted in exact i64 centimes; `as_of` is the seed clock 2026-06-18
//! (June cycle `[2026-06-01, 2026-06-30]`). Slug reads use the seed slugs
//! (`"t1"`, `"t8"`, …). Tests may `expect("msg")`; never bare `unwrap()`.
//!
//! ── Seed ground truth (phosk_db_memory::seed::seed_receipts_and_lines) ────────
//! 9 receipts t1..t9, all dated in June 2026:
//!   t1 Migros            Groceries          5875  6/16  photo  (4 lines)
//!   t2 Restaurant Linde  Going out          6450  6/16  photo  (2 lines)
//!   t3 Coop              Groceries          4230  6/13  photo  (2 lines)
//!   t4 Galaxus           Shopping          12990  6/13  photo  (0 lines)
//!   t5 Migros            Coffee & snacks    1280  6/11  photo  (2 lines)
//!   t6 Denner            Groceries          2990  6/9   photo  (0 lines)
//!   t7 SBB               Transport          3400  6/7   photo  (0 lines)
//!   t8 Landlord          Rent             168000  6/1   fixed  (0 lines)
//!   t9 Helsana           Health insurance  31800  6/1   fixed  (0 lines)
//! Total = 237015 centimes.
//!
//! t1 lines: oat-milk 1.0×560=560 (.88), bananas 1.2×320=384 (.91),
//!           bread 1.0×431=431 (.58 LOW), mixed 1.0×4500=4500 (.95)
//!   → low_conf 1, avg confidence (0.88+0.91+0.58+0.95)/4 = 0.83
//! t2 lines: IPA 2.0×720=1440 (.90), main 1.0×5010=5010 (.93) → low_conf 0, avg 0.915
//! t3 lines: gruyère 0.4×6600=2640 (.92), veg 1.0×1590=1590 (.89) → low_conf 0, avg 0.905
//! t5 lines: flat white 1.0×560=560 (.87), pain 2.0×360=720 (.84) → low_conf 0, avg 0.855

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_ledger::categories::{available_categories, category_spend};
use phosk_ledger::line_items::{correct_line, line_total, transaction_lines};
use phosk_ledger::shops::{available_shops, list_shops};
use phosk_ledger::transactions::{TxnFilter, list_transactions, transaction_detail};

/// The seed clock: the spec's "today".
fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid as_of")
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// Approximate-equality for confidence f64s (avoids brittle bit-exact float cmp).
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
}

// ══ list_transactions ═══════════════════════════════════════════════════════════

/// No filter (June cycle): every seeded receipt is returned, summed exactly.
#[tokio::test]
async fn list_transactions_unfiltered_returns_all_nine() {
    let db = seeded();
    let dto = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    assert_eq!(dto.transactions.len(), 9, "all nine June receipts");
    assert_eq!(dto.summary.entry_count, 9, "entry_count = nine");
    assert_eq!(
        dto.summary.total_amount.centimes(),
        237_015,
        "June receipt total is 237015 centimes"
    );
}

/// The summary period label reflects the resolved cycle.
#[tokio::test]
async fn list_transactions_summary_period_label_is_june() {
    let db = seeded();
    let dto = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    assert_eq!(
        dto.summary.period_label, "JUN 2026",
        "period label names the June cycle"
    );
}

/// Each row carries its derived per-row marks (item_count / low_conf_count /
/// fixed) computed from its lines — pinned for t1.
#[tokio::test]
async fn list_transactions_row_marks_for_t1_are_derived() {
    let db = seeded();
    let dto = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    let t1 = dto
        .transactions
        .iter()
        .find(|t| t.id == "t1")
        .expect("t1 present");
    assert_eq!(t1.shop, "Migros");
    assert_eq!(t1.category, "Groceries");
    assert_eq!(t1.amount.centimes(), 5_875);
    assert_eq!(t1.item_count, 4, "t1 has four seeded lines");
    assert_eq!(t1.low_conf_count, 1, "t1 has one < 0.7 line (bread .58)");
    assert!(!t1.fixed, "t1 is not a standing charge");
}

/// Fixed receipts (t8 rent) carry `fixed = true` and zero line-derived marks.
#[tokio::test]
async fn list_transactions_fixed_row_for_t8() {
    let db = seeded();
    let dto = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    let t8 = dto
        .transactions
        .iter()
        .find(|t| t.id == "t8")
        .expect("t8 present");
    assert!(t8.fixed, "rent is a fixed charge");
    assert_eq!(t8.amount.centimes(), 168_000);
    assert_eq!(t8.item_count, 0, "no itemised lines on the rent receipt");
    assert_eq!(t8.low_conf_count, 0);
}

/// Filtering by shop narrows to that shop only and re-rolls the summary total.
/// Migros appears on t1 (5875) and t5 (1280) → 2 rows, total 7155.
#[tokio::test]
async fn list_transactions_filter_by_shop_migros() {
    let db = seeded();
    let filter = TxnFilter {
        shop: "Migros".to_owned(),
        ..TxnFilter::default()
    };
    let dto = list_transactions(&db, as_of(), filter)
        .await
        .expect("list ok");
    assert_eq!(dto.transactions.len(), 2, "two Migros receipts");
    assert!(
        dto.transactions.iter().all(|t| t.shop == "Migros"),
        "every row is Migros"
    );
    assert_eq!(dto.summary.entry_count, 2);
    assert_eq!(
        dto.summary.total_amount.centimes(),
        7_155,
        "5875 + 1280 = 7155"
    );
}

/// Filtering by category narrows to that category. Groceries = t1+t3+t6.
#[tokio::test]
async fn list_transactions_filter_by_category_groceries() {
    let db = seeded();
    let filter = TxnFilter {
        category: "Groceries".to_owned(),
        ..TxnFilter::default()
    };
    let dto = list_transactions(&db, as_of(), filter)
        .await
        .expect("list ok");
    assert_eq!(dto.transactions.len(), 3, "three Groceries receipts");
    assert!(dto.transactions.iter().all(|t| t.category == "Groceries"));
    assert_eq!(
        dto.summary.total_amount.centimes(),
        13_095,
        "5875 + 4230 + 2990 = 13095"
    );
}

/// Free-text `q` matches shop/category substrings, case-insensitively.
#[tokio::test]
async fn list_transactions_free_text_query_matches_shop() {
    let db = seeded();
    let filter = TxnFilter {
        q: "coop".to_owned(),
        ..TxnFilter::default()
    };
    let dto = list_transactions(&db, as_of(), filter)
        .await
        .expect("list ok");
    assert_eq!(dto.transactions.len(), 1, "only Coop matches 'coop'");
    assert_eq!(dto.transactions[0].id, "t3");
}

/// `sort = "amount"` orders rows by amount descending.
#[tokio::test]
async fn list_transactions_sort_by_amount_descending() {
    let db = seeded();
    let filter = TxnFilter {
        sort: "amount".to_owned(),
        ..TxnFilter::default()
    };
    let dto = list_transactions(&db, as_of(), filter)
        .await
        .expect("list ok");
    assert!(
        dto.transactions
            .windows(2)
            .all(|w| w[0].amount >= w[1].amount),
        "each amount is >= the next"
    );
    assert_eq!(dto.transactions[0].id, "t8", "rent (168000) is the largest");
}

/// `sort = "shop"` orders rows by shop name ascending.
#[tokio::test]
async fn list_transactions_sort_by_shop_ascending() {
    let db = seeded();
    let filter = TxnFilter {
        sort: "shop".to_owned(),
        ..TxnFilter::default()
    };
    let dto = list_transactions(&db, as_of(), filter)
        .await
        .expect("list ok");
    assert!(
        dto.transactions.windows(2).all(|w| w[0].shop <= w[1].shop),
        "shop names are non-decreasing"
    );
    assert_eq!(dto.transactions[0].shop, "Coop", "Coop sorts first");
}

/// The filter-option lists are present and complete (sorted ascending,
/// distinct). Eight shops, seven categories across the nine receipts.
#[tokio::test]
async fn list_transactions_exposes_filter_options() {
    let db = seeded();
    let dto = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    assert_eq!(
        dto.available_shops,
        vec![
            "Coop",
            "Denner",
            "Galaxus",
            "Helsana",
            "Landlord",
            "Migros",
            "Restaurant Linde",
            "SBB",
        ],
        "eight distinct shops, ascending"
    );
    assert_eq!(
        dto.available_categories,
        vec![
            "Coffee & snacks",
            "Going out",
            "Groceries",
            "Health insurance",
            "Rent",
            "Shopping",
            "Transport",
        ],
        "seven distinct categories, ascending"
    );
}

/// A filter that matches nothing yields an empty list + a zero summary, not an
/// error.
#[tokio::test]
async fn list_transactions_no_match_is_empty_zero() {
    let db = seeded();
    let filter = TxnFilter {
        shop: "Nonexistent".to_owned(),
        ..TxnFilter::default()
    };
    let dto = list_transactions(&db, as_of(), filter)
        .await
        .expect("list ok");
    assert!(dto.transactions.is_empty(), "no rows");
    assert_eq!(dto.summary.entry_count, 0);
    assert_eq!(dto.summary.total_amount, Money::ZERO);
}

/// The list DTO round-trips to camelCase JSON with money as exact i64 centimes
/// (the wire contract).
#[tokio::test]
async fn list_transactions_serializes_camel_case_centimes() {
    let db = seeded();
    let dto = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    let json = serde_json::to_value(&dto).expect("serialize");
    assert!(
        json.get("availableShops").is_some(),
        "camelCase key present"
    );
    assert!(json.get("availableCategories").is_some());
    let summary = json.get("summary").expect("summary object");
    assert_eq!(
        summary
            .get("totalAmount")
            .and_then(serde_json::Value::as_i64),
        Some(237_015),
        "totalAmount serializes as exact i64 centimes"
    );
    assert_eq!(
        summary
            .get("entryCount")
            .and_then(serde_json::Value::as_u64),
        Some(9)
    );
}

/// The service is callable behind the `&dyn DatabaseAdapter` PORT (ADR-010).
#[tokio::test]
async fn list_transactions_through_the_port() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;
    let dto = list_transactions(port, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    assert_eq!(dto.transactions.len(), 9);
}

// ══ transaction_detail ═══════════════════════════════════════════════════════════

/// t1's detail: mean line confidence, PADDLEOCR photo source, region count.
#[tokio::test]
async fn transaction_detail_t1_avg_confidence_and_source() {
    let db = seeded();
    let dto = transaction_detail(&db, "t1").await.expect("detail ok");
    assert_eq!(dto.id, "t1");
    close(dto.avg_confidence, 0.83); // (0.88+0.91+0.58+0.95)/4
    assert_eq!(dto.source.kind, "PHOTO");
    assert_eq!(dto.source.ocr_engine, "PADDLEOCR");
    assert_eq!(
        dto.ocr_regions, 12,
        "seed sets 12 OCR regions on photo receipts"
    );
}

/// t2's average confidence is the mean of its two lines (0.90, 0.93).
#[tokio::test]
async fn transaction_detail_t2_avg_confidence() {
    let db = seeded();
    let dto = transaction_detail(&db, "t2").await.expect("detail ok");
    close(dto.avg_confidence, 0.915);
}

/// A fixed/manual receipt (t8 rent) reports a MANUAL source with no OCR engine
/// and zero regions.
#[tokio::test]
async fn transaction_detail_fixed_receipt_is_manual_source() {
    let db = seeded();
    let dto = transaction_detail(&db, "t8").await.expect("detail ok");
    assert_eq!(dto.source.kind, "MANUAL");
    assert_eq!(dto.source.ocr_engine, "", "manual source has no OCR engine");
    assert_eq!(dto.ocr_regions, 0);
}

/// An unknown slug is a `NotFound`, not a panic.
#[tokio::test]
async fn transaction_detail_unknown_slug_is_not_found() {
    let db = seeded();
    let err = transaction_detail(&db, "does-not-exist")
        .await
        .expect_err("unknown slug is an error");
    assert_eq!(err.code(), "not_found", "unknown receipt → NotFound");
}

/// The detail DTO serializes with camelCase keys (`avgConfidence`, `ocrRegions`).
#[tokio::test]
async fn transaction_detail_serializes_camel_case() {
    let db = seeded();
    let dto = transaction_detail(&db, "t1").await.expect("detail ok");
    let json = serde_json::to_value(&dto).expect("serialize");
    assert!(json.get("avgConfidence").is_some());
    assert!(json.get("ocrRegions").is_some());
    let source = json.get("source").expect("source object");
    assert_eq!(
        source.get("type").and_then(serde_json::Value::as_str),
        Some("PHOTO"),
        "source.type uses the renamed `type` key"
    );
}

// ══ transaction_lines (line_items) ═══════════════════════════════════════════════

/// t1's lines: four parsed lines, the distinct signal feeds, one low-conf line.
#[tokio::test]
async fn transaction_lines_t1_shape_is_exact() {
    let db = seeded();
    let dto = transaction_lines(&db, "t1").await.expect("lines ok");
    assert_eq!(dto.lines.len(), 4, "t1 has four lines");
    assert_eq!(dto.low_conf, 1, "the bread line (.58) is low-confidence");

    let bread = dto
        .lines
        .iter()
        .find(|l| l.name == "Bread (unclear)")
        .expect("bread line present");
    close(bread.confidence, 0.58);
    assert_eq!(bread.unit_price.centimes(), 431);
    assert_eq!(bread.line_total.centimes(), 431, "1.0 × 431 = 431");
    assert_eq!(bread.category, "Groceries");
}

/// Each line's `line_total` equals the backend-derived `round(qty * unit_price)`.
/// Bananas: 1.2 × 320 = 384 exactly.
#[tokio::test]
async fn transaction_lines_line_total_is_derived() {
    let db = seeded();
    let dto = transaction_lines(&db, "t1").await.expect("lines ok");
    let bananas = dto
        .lines
        .iter()
        .find(|l| l.name == "Bananas")
        .expect("bananas line present");
    assert_eq!(bananas.qty, 1.2);
    assert_eq!(bananas.unit_price.centimes(), 320);
    assert_eq!(bananas.line_total.centimes(), 384, "round(1.2 × 320) = 384");
}

/// A receipt with no itemised lines (t4 Galaxus) yields an empty line list,
/// empty sigs, zero low-conf — not an error.
#[tokio::test]
async fn transaction_lines_no_lines_is_empty() {
    let db = seeded();
    let dto = transaction_lines(&db, "t4").await.expect("lines ok");
    assert!(dto.lines.is_empty(), "t4 has no itemised lines");
    assert!(dto.sigs.is_empty());
    assert_eq!(dto.low_conf, 0);
}

/// t5's two lines are both high-confidence → low_conf 0.
#[tokio::test]
async fn transaction_lines_t5_all_high_confidence() {
    let db = seeded();
    let dto = transaction_lines(&db, "t5").await.expect("lines ok");
    assert_eq!(dto.lines.len(), 2);
    assert_eq!(dto.low_conf, 0, "both t5 lines are >= 0.7");
}

/// An unknown slug is a `NotFound`.
#[tokio::test]
async fn transaction_lines_unknown_slug_is_not_found() {
    let db = seeded();
    let err = transaction_lines(&db, "nope")
        .await
        .expect_err("unknown slug is an error");
    assert_eq!(err.code(), "not_found");
}

/// Line DTOs serialize camelCase with money as exact centimes (`unitPrice`,
/// `lineTotal`).
#[tokio::test]
async fn transaction_lines_serializes_camel_case_centimes() {
    let db = seeded();
    let dto = transaction_lines(&db, "t1").await.expect("lines ok");
    let json = serde_json::to_value(&dto).expect("serialize");
    let first = json
        .get("lines")
        .and_then(|l| l.get(0))
        .expect("first line");
    assert!(first.get("unitPrice").is_some(), "camelCase unitPrice");
    assert!(first.get("lineTotal").is_some(), "camelCase lineTotal");
    assert!(
        first
            .get("unitPrice")
            .and_then(serde_json::Value::as_i64)
            .is_some(),
        "unitPrice serializes as i64 centimes"
    );
}

// ── line_total (pure helper) ─────────────────────────────────────────────────────

/// Exact whole-quantity multiply.
#[tokio::test]
async fn line_total_whole_quantity() {
    let got = line_total(3.0, Money::from_centimes(195)).expect("line_total ok");
    assert_eq!(got.centimes(), 585, "3 × 195 = 585");
}

/// Fractional quantity rounds to the nearest centime: 1.2 × 320 = 384.
#[tokio::test]
async fn line_total_fractional_quantity_rounds() {
    let got = line_total(1.2, Money::from_centimes(320)).expect("line_total ok");
    assert_eq!(got.centimes(), 384);
}

/// A weighed-good fraction rounds: 0.4 × 6600 = 2640.
#[tokio::test]
async fn line_total_weighed_good() {
    let got = line_total(0.4, Money::from_centimes(6_600)).expect("line_total ok");
    assert_eq!(got.centimes(), 2_640);
}

/// Rounding is to the nearest centime (half-up at .5): 0.5 × 333 = 166.5 → 167.
#[tokio::test]
async fn line_total_rounds_half_up() {
    let got = line_total(0.5, Money::from_centimes(333)).expect("line_total ok");
    assert_eq!(got.centimes(), 167, "round(166.5) = 167");
}

/// Zero quantity is a zero total, never an error.
#[tokio::test]
async fn line_total_zero_quantity_is_zero() {
    let got = line_total(0.0, Money::from_centimes(999)).expect("line_total ok");
    assert_eq!(got, Money::ZERO);
}

/// An overflowing product surfaces `Overflow`, never wraps or panics.
#[tokio::test]
async fn line_total_overflow_is_reported() {
    let err =
        line_total(1e18, Money::from_centimes(i64::MAX)).expect_err("overflow must be reported");
    assert_eq!(err.code(), "overflow", "huge product → Overflow");
}

// ── correct_line (write path) ─────────────────────────────────────────────────────

/// Correcting a line resolves, applies, and logs (skeleton). Use a real seed
/// line id so the signature is exercised exactly.
#[tokio::test]
async fn correct_line_category_succeeds() {
    let db = seeded();
    // Resolve t1's first line id through the port.
    let receipt = db.receipt_by_slug("t1").await.expect("t1 present");
    let lines = db.line_items(receipt.id).await.expect("lines present");
    let line = lines.first().expect("at least one line");
    correct_line(&db, line.id, "category", "Coffee & snacks")
        .await
        .expect("correction applied");
}

// ══ categories ═══════════════════════════════════════════════════════════════════

/// June category spend, ranked by total descending. Computed from the receipts:
/// Rent 168000 > Health insurance 31800 > Groceries 13095 > Shopping 12990 >
/// Going out 6450 > Transport 3400 > Coffee & snacks 1280.
#[tokio::test]
async fn category_spend_june_ranked_descending() {
    let db = seeded();
    let cats = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    let expected: [(&str, i64, u32); 7] = [
        ("Rent", 168_000, 1),
        ("Health insurance", 31_800, 1),
        ("Groceries", 13_095, 3),
        ("Shopping", 12_990, 1),
        ("Going out", 6_450, 1),
        ("Transport", 3_400, 1),
        ("Coffee & snacks", 1_280, 1),
    ];
    assert_eq!(cats.len(), 7, "seven distinct June categories");
    for (got, (name, cents, txns)) in cats.iter().zip(expected) {
        assert_eq!(got.category, name, "rank order category");
        assert_eq!(got.total.centimes(), cents, "exact total for {name}");
        assert_eq!(got.txns, txns, "receipt count for {name}");
    }
}

/// The Groceries roll-up aggregates t1+t3+t6 into one row.
#[tokio::test]
async fn category_spend_groceries_aggregates_three_receipts() {
    let db = seeded();
    let cats = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    let groceries = cats
        .iter()
        .find(|c| c.category == "Groceries")
        .expect("Groceries present");
    assert_eq!(groceries.total.centimes(), 13_095);
    assert_eq!(groceries.txns, 3, "three Groceries receipts aggregated");
    assert_eq!(
        cats.iter().filter(|c| c.category == "Groceries").count(),
        1,
        "Groceries appears once"
    );
}

/// Totals are non-increasing down the ranking.
#[tokio::test]
async fn category_spend_is_sorted_non_increasing() {
    let db = seeded();
    let cats = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    assert!(
        cats.windows(2).all(|w| w[0].total >= w[1].total),
        "each total >= the next"
    );
}

/// An empty cycle (April — no seeded receipts) yields an empty breakdown.
#[tokio::test]
async fn category_spend_empty_cycle_is_empty() {
    let db = seeded();
    let april = NaiveDate::from_ymd_opt(2026, 4, 15).expect("valid date");
    let cats = category_spend(&db, april).await.expect("category_spend ok");
    assert!(cats.is_empty(), "no spend in April");
}

/// `available_categories` are the distinct receipt category names, ascending.
#[tokio::test]
async fn available_categories_distinct_sorted() {
    let db = seeded();
    let cats = available_categories(&db).await.expect("categories ok");
    assert_eq!(
        cats,
        vec![
            "Coffee & snacks",
            "Going out",
            "Groceries",
            "Health insurance",
            "Rent",
            "Shopping",
            "Transport",
        ]
    );
}

/// Category-spend DTO serializes camelCase with money as exact centimes.
#[tokio::test]
async fn category_spend_serializes_centimes() {
    let db = seeded();
    let cats = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    let json = serde_json::to_value(&cats[0]).expect("serialize");
    assert_eq!(
        json.get("total").and_then(serde_json::Value::as_i64),
        Some(168_000),
        "total serializes as i64 centimes"
    );
    assert!(json.get("txns").is_some());
}

// ══ shops ════════════════════════════════════════════════════════════════════════

/// The all-time shop directory, ranked by total descending. From the receipts:
/// Landlord 168000 > Helsana 31800 > Galaxus 12990 > Migros 7155 >
/// Restaurant Linde 6450 > Coop 4230 > SBB 3400 > Denner 2990.
#[tokio::test]
async fn list_shops_ranked_descending() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    let expected: [(&str, i64, u32); 8] = [
        ("Landlord", 168_000, 1),
        ("Helsana", 31_800, 1),
        ("Galaxus", 12_990, 1),
        ("Migros", 7_155, 2),
        ("Restaurant Linde", 6_450, 1),
        ("Coop", 4_230, 1),
        ("SBB", 3_400, 1),
        ("Denner", 2_990, 1),
    ];
    assert_eq!(shops.len(), 8, "eight distinct shops");
    for (got, (name, cents, txns)) in shops.iter().zip(expected) {
        assert_eq!(got.shop, name, "rank order shop");
        assert_eq!(got.total.centimes(), cents, "exact total for {name}");
        assert_eq!(got.txns, txns, "visit count for {name}");
    }
}

/// Migros (t1 + t5) aggregates into one row of 7155 over two visits.
#[tokio::test]
async fn list_shops_aggregates_repeat_visits() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    let migros = shops
        .iter()
        .find(|s| s.shop == "Migros")
        .expect("Migros present");
    assert_eq!(migros.total.centimes(), 7_155, "5875 + 1280");
    assert_eq!(migros.txns, 2);
    assert_eq!(
        shops.iter().filter(|s| s.shop == "Migros").count(),
        1,
        "Migros appears once, aggregated"
    );
}

/// Shop totals are non-increasing down the ranking.
#[tokio::test]
async fn list_shops_is_sorted_non_increasing() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    assert!(
        shops.windows(2).all(|w| w[0].total >= w[1].total),
        "each total >= the next"
    );
}

/// `available_shops` are the distinct receipt shop names, ascending.
#[tokio::test]
async fn available_shops_distinct_sorted() {
    let db = seeded();
    let shops = available_shops(&db).await.expect("shops ok");
    assert_eq!(
        shops,
        vec![
            "Coop",
            "Denner",
            "Galaxus",
            "Helsana",
            "Landlord",
            "Migros",
            "Restaurant Linde",
            "SBB",
        ]
    );
}

/// Shop DTO serializes camelCase with money as exact centimes.
#[tokio::test]
async fn list_shops_serializes_centimes() {
    let db = seeded();
    let shops = list_shops(&db).await.expect("list_shops ok");
    let json = serde_json::to_value(&shops[0]).expect("serialize");
    assert_eq!(
        json.get("total").and_then(serde_json::Value::as_i64),
        Some(168_000)
    );
    assert!(json.get("txns").is_some());
}
