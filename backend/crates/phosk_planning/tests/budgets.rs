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
//! RED tests for the `phosk_planning::budgets` service slice (F3).
//!
//! These pin the budgets/categories/allocation contract against the deterministic
//! Swiss seed (`phosk_db_memory::MemoryDb::seeded()`) at the spec clock
//! `as_of = 2026-06-18` (day 18 of the 30-day June cycle). Every body in
//! `phosk_planning::budgets` is `todo!()` today, so every test here MUST COMPILE
//! and then FAIL at runtime (the `todo!()` panics). No production logic lives in
//! the test crate.
//!
//! ## Ground truth (hand-computed from the seed)
//!
//! The budgets slice computes per-category spend from **receipts** (the
//! `seed_receipts_and_lines` table — mixed-case category names matching
//! `seed_category_caps`), NOT from the dashboard `Transaction` seed (uppercase
//! `GROCERIES`). The nine seeded June receipts all fall on/before `as_of`
//! `2026-06-18`, so the whole spend-to-date window `[2026-06-01, 2026-06-18]`
//! includes every one:
//!
//! | receipt | date   | category         | amount (c) |
//! |---------|--------|------------------|-----------:|
//! | t1      | 06-16  | Groceries        |      5_875 |
//! | t2      | 06-16  | Going out        |      6_450 |
//! | t3      | 06-13  | Groceries        |      4_230 |
//! | t4      | 06-13  | Shopping         |     12_990 |
//! | t5      | 06-11  | Coffee & snacks  |      1_280 |
//! | t6      | 06-09  | Groceries        |      2_990 |
//! | t7      | 06-07  | Transport        |      3_400 |
//! | t8      | 06-01  | Rent (fixed)     |    168_000 |
//! | t9      | 06-01  | Health insurance |     31_800 |
//!
//! Per-category spend-to-date (centimes), and `proj = spent * 30 / 18`
//! (integer-centime run-rate; day_index 18, N 30):
//!
//! | category         | cap (c) | spent (c) | items | proj = spent*30/18 (c) |
//! |------------------|--------:|----------:|------:|-----------------------:|
//! | Groceries        |  80_000 |    13_095 |     3 |   13_095*30/18 = 21_825 |
//! | Going out        |  40_000 |     6_450 |     1 |    6_450*30/18 = 10_750 |
//! | Coffee & snacks  |  12_000 |     1_280 |     1 |    1_280*30/18 =  2_133 |
//! | Transport        |  18_000 |     3_400 |     1 |    3_400*30/18 =  5_666 |
//! | Rent             | 168_000 |   168_000 |     1 |  168_000*30/18 = 280_000 |
//! | Health insurance |  31_800 |    31_800 |     1 |   31_800*30/18 =  53_000 |
//! | Shopping         |  50_000 |    12_990 |     1 |   12_990*30/18 =  21_650 |
//! | Subscriptions    |  26_000 |         0 |     0 |                       0 |
//!
//! Totals: Σcaps (allocated) = 80+40+12+18+168+31.8+50+26 (×1000) = 425_800;
//! Σspent = 13_095+6_450+1_280+3_400+168_000+31_800+12_990 = 237_015;
//! budget = 420_000;
//! Σproj = 21_825+10_750+2_133+5_666+280_000+53_000+21_650+0 = 395_024.
//!
//! `remaining` (totals) = budget − Σspent = 420_000 − 237_015 = 182_985.
//! `overAllocated` = max(0, allocated − budget) = max(0, 425_800 − 420_000) = 5_800.
//! `unallocated` = max(0, budget − allocated) = max(0, 420_000 − 425_800) = 0.
//! `envelopeCount` = 8.
//!
//! Per-category `remaining = cap − spent` (signed) and
//! `usedPct = round(100*spent/cap)`:
//!   Groceries 80_000−13_095=66_905, round(100*13_095/80_000)=round(16.36)=16.
//!   Rent 0 remaining, 100%. Subscriptions 26_000 remaining, 0%.
//!
//! `histAvg` (detail) = trailing-N=3 average of the prior-cycle spends in
//! `seed_budget_history` (excludes the current cycle). Groceries prior spends
//! (CHF) [720,690,810,740,760,880] → last 3 = [740,760,880] CHF =
//! [74_000,76_000,88_000] c, avg = 238_000/3 = 79_333 c (integer division).
//! `overCapAmount` (detail) = max(0, proj − cap); Groceries max(0,21_825−80_000)=0.

use chrono::NaiveDate;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;

use phosk_planning::budgets::{
    AllocationDto, BudgetTotalsDto, CategoryDetailDto, CategoryDto, allocation, budget_totals,
    categories, category_detail, category_transactions, set_cap,
};

// ── fixtures ────────────────────────────────────────────────────────────────

fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
}

/// The spec clock: 2026-06-18 (day 18 of the 30-day June cycle).
fn as_of() -> NaiveDate {
    naive(2026, 6, 18)
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

fn find<'a>(cats: &'a [CategoryDto], name: &str) -> &'a CategoryDto {
    cats.iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("category {name} present"))
}

// ── categories(): per-envelope figures ──────────────────────────────────────

#[tokio::test]
async fn categories_returns_all_eight_envelopes() {
    let cats = categories(&seeded(), as_of()).await.expect("categories ok");
    assert_eq!(cats.len(), 8, "eight seeded budget channels");
    for name in [
        "Groceries",
        "Going out",
        "Coffee & snacks",
        "Transport",
        "Rent",
        "Health insurance",
        "Shopping",
        "Subscriptions",
    ] {
        assert!(
            cats.iter().any(|c| c.name == name),
            "channel {name} present"
        );
    }
}

#[tokio::test]
async fn categories_carry_the_seeded_caps_as_budget() {
    let cats = categories(&seeded(), as_of()).await.expect("categories ok");
    assert_eq!(find(&cats, "Groceries").budget.centimes(), 80_000);
    assert_eq!(find(&cats, "Going out").budget.centimes(), 40_000);
    assert_eq!(find(&cats, "Coffee & snacks").budget.centimes(), 12_000);
    assert_eq!(find(&cats, "Transport").budget.centimes(), 18_000);
    assert_eq!(find(&cats, "Rent").budget.centimes(), 168_000);
    assert_eq!(find(&cats, "Health insurance").budget.centimes(), 31_800);
    assert_eq!(find(&cats, "Shopping").budget.centimes(), 50_000);
    assert_eq!(find(&cats, "Subscriptions").budget.centimes(), 26_000);
}

#[tokio::test]
async fn categories_spend_to_date_is_summed_from_receipts() {
    let cats = categories(&seeded(), as_of()).await.expect("categories ok");
    // Groceries = t1 5_875 + t3 4_230 + t6 2_990 = 13_095.
    assert_eq!(find(&cats, "Groceries").spent.centimes(), 13_095);
    assert_eq!(find(&cats, "Going out").spent.centimes(), 6_450);
    assert_eq!(find(&cats, "Coffee & snacks").spent.centimes(), 1_280);
    assert_eq!(find(&cats, "Transport").spent.centimes(), 3_400);
    assert_eq!(find(&cats, "Rent").spent.centimes(), 168_000);
    assert_eq!(find(&cats, "Health insurance").spent.centimes(), 31_800);
    assert_eq!(find(&cats, "Shopping").spent.centimes(), 12_990);
    // No receipt → empty envelope.
    assert_eq!(find(&cats, "Subscriptions").spent.centimes(), 0);
}

#[tokio::test]
async fn categories_item_counts_are_receipt_counts_this_cycle() {
    let cats = categories(&seeded(), as_of()).await.expect("categories ok");
    assert_eq!(find(&cats, "Groceries").items, 3, "t1/t3/t6");
    assert_eq!(find(&cats, "Going out").items, 1);
    assert_eq!(find(&cats, "Shopping").items, 1);
    assert_eq!(find(&cats, "Subscriptions").items, 0, "empty envelope");
}

#[tokio::test]
async fn categories_projection_is_run_rate_spent_times_n_over_d() {
    let cats = categories(&seeded(), as_of()).await.expect("categories ok");
    // proj = spent * 30 / 18 (integer-centime division).
    assert_eq!(find(&cats, "Groceries").proj.centimes(), 13_095 * 30 / 18);
    assert_eq!(find(&cats, "Groceries").proj.centimes(), 21_825);
    assert_eq!(find(&cats, "Going out").proj.centimes(), 6_450 * 30 / 18);
    assert_eq!(find(&cats, "Going out").proj.centimes(), 10_750);
    assert_eq!(
        find(&cats, "Coffee & snacks").proj.centimes(),
        1_280 * 30 / 18
    );
    assert_eq!(find(&cats, "Transport").proj.centimes(), 3_400 * 30 / 18);
    // Empty envelope projects to zero, never divides by anything bad.
    assert_eq!(find(&cats, "Subscriptions").proj.centimes(), 0);
}

#[tokio::test]
async fn categories_remaining_is_cap_minus_spent_signed() {
    let cats = categories(&seeded(), as_of()).await.expect("categories ok");
    assert_eq!(
        find(&cats, "Groceries").remaining.centimes(),
        80_000 - 13_095
    );
    assert_eq!(find(&cats, "Groceries").remaining.centimes(), 66_905);
    // Fixed full-cap charge → exactly zero remaining.
    assert_eq!(find(&cats, "Rent").remaining.centimes(), 0);
    // Untouched envelope → full cap remaining.
    assert_eq!(find(&cats, "Subscriptions").remaining.centimes(), 26_000);
}

#[tokio::test]
async fn categories_used_pct_is_rounded_percent_of_cap() {
    let cats = categories(&seeded(), as_of()).await.expect("categories ok");
    // round(100*13_095/80_000) = round(16.369) = 16.
    assert_eq!(find(&cats, "Groceries").used_pct, 16);
    // Fixed full charge → 100%.
    assert_eq!(find(&cats, "Rent").used_pct, 100);
    assert_eq!(find(&cats, "Health insurance").used_pct, 100);
    // Untouched → 0%.
    assert_eq!(find(&cats, "Subscriptions").used_pct, 0);
}

#[tokio::test]
async fn categories_fixed_flag_tracks_the_seeded_caps() {
    let cats = categories(&seeded(), as_of()).await.expect("categories ok");
    assert!(find(&cats, "Rent").fixed, "Rent is a standing charge");
    assert!(find(&cats, "Health insurance").fixed);
    assert!(!find(&cats, "Groceries").fixed, "Groceries is tunable");
    assert!(!find(&cats, "Going out").fixed);
}

// ── budget_totals(): the KPI band ───────────────────────────────────────────

#[tokio::test]
async fn budget_totals_budget_comes_from_config() {
    let t = budget_totals(&seeded(), as_of())
        .await
        .expect("budget totals ok");
    assert_eq!(t.budget.centimes(), 420_000, "monthly budget CHF 4200");
}

#[tokio::test]
async fn budget_totals_allocated_is_sum_of_caps() {
    let t = budget_totals(&seeded(), as_of())
        .await
        .expect("budget totals ok");
    // Σ caps = 80+40+12+18+168+31.8+50+26 (×1000) = 425_800.
    assert_eq!(t.allocated.centimes(), 425_800);
}

#[tokio::test]
async fn budget_totals_spent_is_sum_of_all_receipts_to_date() {
    let t = budget_totals(&seeded(), as_of())
        .await
        .expect("budget totals ok");
    // 13_095+6_450+1_280+3_400+168_000+31_800+12_990 = 237_015.
    assert_eq!(t.spent.centimes(), 237_015);
}

#[tokio::test]
async fn budget_totals_projected_is_sum_of_per_category_projections() {
    let t = budget_totals(&seeded(), as_of())
        .await
        .expect("budget totals ok");
    // Σ proj = 21_825+10_750+2_133+5_666+280_000+53_000+21_650+0 = 395_024.
    let expected = (13_095 * 30 / 18)
        + (6_450 * 30 / 18)
        + (1_280 * 30 / 18)
        + (3_400 * 30 / 18)
        + (168_000 * 30 / 18)
        + (31_800 * 30 / 18)
        + (12_990 * 30 / 18);
    assert_eq!(t.projected.centimes(), expected);
    assert_eq!(t.projected.centimes(), 395_024);
}

#[tokio::test]
async fn budget_totals_remaining_is_budget_minus_spent() {
    let t = budget_totals(&seeded(), as_of())
        .await
        .expect("budget totals ok");
    assert_eq!(t.remaining.centimes(), 420_000 - 237_015);
    assert_eq!(t.remaining.centimes(), 182_985);
}

#[tokio::test]
async fn budget_totals_over_allocated_when_caps_exceed_budget() {
    let t = budget_totals(&seeded(), as_of())
        .await
        .expect("budget totals ok");
    // max(0, 425_800 − 420_000) = 5_800.
    assert_eq!(t.over_allocated.centimes(), 5_800);
}

#[tokio::test]
async fn budget_totals_unallocated_is_zero_when_over_allocated() {
    let t = budget_totals(&seeded(), as_of())
        .await
        .expect("budget totals ok");
    // max(0, 420_000 − 425_800) = 0 (caps already exceed budget).
    assert_eq!(t.unallocated.centimes(), 0);
}

#[tokio::test]
async fn budget_totals_envelope_count_is_eight() {
    let t = budget_totals(&seeded(), as_of())
        .await
        .expect("budget totals ok");
    assert_eq!(t.envelope_count, 8);
}

#[tokio::test]
async fn budget_totals_round_trips_money_as_centimes_json() {
    let t = budget_totals(&seeded(), as_of())
        .await
        .expect("budget totals ok");
    let v = serde_json::to_value(&t).expect("serialize");
    // money_centimes ⇒ exact i64, NOT a CHF float.
    assert_eq!(v["budget"], serde_json::json!(420_000));
    assert_eq!(v["allocated"], serde_json::json!(425_800));
    assert_eq!(v["overAllocated"], serde_json::json!(5_800));
    assert_eq!(v["envelopeCount"], serde_json::json!(8));
    let back: BudgetTotalsDto = serde_json::from_value(v).expect("deserialize");
    assert_eq!(back, t, "DTO round-trips through camelCase centimes JSON");
}

// ── allocation(): segments + advice ─────────────────────────────────────────

#[tokio::test]
async fn allocation_has_one_segment_per_envelope() {
    let a = allocation(&seeded(), as_of()).await.expect("allocation ok");
    assert_eq!(a.segments.len(), 8, "one segment per cap");
}

#[tokio::test]
async fn allocation_segment_cap_is_the_envelope_cap() {
    let a = allocation(&seeded(), as_of()).await.expect("allocation ok");
    let groc = a
        .segments
        .iter()
        .find(|s| s.name == "Groceries")
        .expect("Groceries segment");
    assert_eq!(groc.cap.centimes(), 80_000);
    let rent = a
        .segments
        .iter()
        .find(|s| s.name == "Rent")
        .expect("Rent segment");
    assert_eq!(rent.cap.centimes(), 168_000);
    assert!(rent.fixed, "Rent segment is fixed/hatched");
}

#[tokio::test]
async fn allocation_segment_share_is_cap_over_sum_of_caps() {
    let a = allocation(&seeded(), as_of()).await.expect("allocation ok");
    let groc = a
        .segments
        .iter()
        .find(|s| s.name == "Groceries")
        .expect("Groceries segment");
    // share = cap / Σcaps = 80_000 / 425_800.
    let share = groc.share.expect("share present");
    assert!(
        (share - 80_000.0 / 425_800.0).abs() < 1e-9,
        "share {share} ≈ 80_000/425_800"
    );
}

#[tokio::test]
async fn allocation_carries_an_ai_advice_line() {
    let a = allocation(&seeded(), as_of()).await.expect("allocation ok");
    assert!(
        !a.ai_advice.model.is_empty(),
        "advice carries a model badge"
    );
    assert!(!a.ai_advice.text.is_empty(), "advice carries a sentence");
}

#[tokio::test]
async fn allocation_serializes_segments_with_centimes_caps() {
    let a = allocation(&seeded(), as_of()).await.expect("allocation ok");
    let v = serde_json::to_value(&a).expect("serialize");
    let seg0 = &v["segments"][0];
    assert!(
        seg0["cap"].is_i64(),
        "segment cap is exact centimes, not a CHF float"
    );
    let back: AllocationDto = serde_json::from_value(v).expect("deserialize");
    assert_eq!(back, a);
}

// ── category_detail(): inspector ────────────────────────────────────────────

#[tokio::test]
async fn category_detail_projected_spend_is_run_rate() {
    let d = category_detail(&seeded(), as_of(), "Groceries")
        .await
        .expect("detail ok");
    // proj = 13_095 * 30 / 18 = 21_825.
    assert_eq!(d.projected_spend.centimes(), 21_825);
}

#[tokio::test]
async fn category_detail_hist_avg_is_trailing_three_cycle_average() {
    let d = category_detail(&seeded(), as_of(), "Groceries")
        .await
        .expect("detail ok");
    // Groceries prior spends [720,690,810,740,760,880] CHF; trailing N=3 ⇒
    // [740,760,880] CHF = [74_000,76_000,88_000] c; avg = 238_000/3 = 79_333 c.
    assert_eq!(d.hist_avg.centimes(), (74_000 + 76_000 + 88_000) / 3);
    assert_eq!(d.hist_avg.centimes(), 79_333);
}

#[tokio::test]
async fn category_detail_over_cap_amount_is_zero_when_proj_under_cap() {
    let d = category_detail(&seeded(), as_of(), "Groceries")
        .await
        .expect("detail ok");
    // max(0, proj 21_825 − cap 80_000) = 0.
    assert_eq!(d.over_cap_amount.centimes(), 0);
}

#[tokio::test]
async fn category_detail_over_cap_amount_for_fixed_full_cap_charge() {
    // Rent: spent 168_000 == cap, proj = 168_000*30/18 = 280_000 > cap ⇒
    // overCap = max(0, 280_000 − 168_000) = 112_000.
    let d = category_detail(&seeded(), as_of(), "Rent")
        .await
        .expect("detail ok");
    assert_eq!(d.projected_spend.centimes(), 168_000 * 30 / 18);
    assert_eq!(d.over_cap_amount.centimes(), 280_000 - 168_000);
    assert_eq!(d.over_cap_amount.centimes(), 112_000);
}

#[tokio::test]
async fn category_detail_carries_a_guidance_line() {
    let d = category_detail(&seeded(), as_of(), "Going out")
        .await
        .expect("detail ok");
    assert!(!d.guidance.is_empty(), "AI guidance paragraph present");
}

#[tokio::test]
async fn category_detail_serializes_money_as_centimes() {
    let d = category_detail(&seeded(), as_of(), "Groceries")
        .await
        .expect("detail ok");
    let v = serde_json::to_value(&d).expect("serialize");
    assert_eq!(v["projectedSpend"], serde_json::json!(21_825));
    assert_eq!(v["histAvg"], serde_json::json!(79_333));
    assert_eq!(v["overCapAmount"], serde_json::json!(0));
    let back: CategoryDetailDto = serde_json::from_value(v).expect("deserialize");
    assert_eq!(back, d);
}

#[tokio::test]
async fn category_detail_unknown_name_is_not_found() {
    let err = category_detail(&seeded(), as_of(), "Nonexistent")
        .await
        .expect_err("unknown category is NotFound");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}

// ── category_transactions(): the recent-rows list ───────────────────────────

#[tokio::test]
async fn category_transactions_lists_this_cycle_receipts_newest_first() {
    let rows = category_transactions(&seeded(), as_of(), "Groceries")
        .await
        .expect("category txns ok");
    // Groceries has three June receipts: t1 (06-16), t3 (06-13), t6 (06-09).
    assert_eq!(rows.len(), 3, "three Groceries receipts this cycle");
    // Newest first.
    assert_eq!(rows[0].shop, "Migros", "t1 06-16 is newest");
    assert_eq!(rows[0].amount.centimes(), 5_875);
    assert_eq!(rows[1].shop, "Coop", "t3 06-13");
    assert_eq!(rows[1].amount.centimes(), 4_230);
    assert_eq!(rows[2].shop, "Denner", "t6 06-09 is oldest");
    assert_eq!(rows[2].amount.centimes(), 2_990);
}

#[tokio::test]
async fn category_transactions_empty_for_untouched_envelope() {
    let rows = category_transactions(&seeded(), as_of(), "Subscriptions")
        .await
        .expect("category txns ok");
    assert!(rows.is_empty(), "no receipts in Subscriptions this cycle");
}

#[tokio::test]
async fn category_transactions_single_receipt_category() {
    let rows = category_transactions(&seeded(), as_of(), "Shopping")
        .await
        .expect("category txns ok");
    assert_eq!(rows.len(), 1, "Shopping = just t4 Galaxus");
    assert_eq!(rows[0].shop, "Galaxus");
    assert_eq!(rows[0].amount.centimes(), 12_990);
}

#[tokio::test]
async fn category_transactions_unknown_name_is_not_found() {
    let err = category_transactions(&seeded(), as_of(), "Nonexistent")
        .await
        .expect_err("unknown category is NotFound");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}

// ── set_cap(): cap edits (absolute, clear/unlimited, NotFound) ───────────────

#[tokio::test]
async fn set_cap_absolute_changes_the_envelope_budget() {
    let db = seeded();
    set_cap(&db, "Going out", Some(Money::from_centimes(35_000)))
        .await
        .expect("set cap ok");
    // The change is observable through the port.
    let cap = db
        .category_cap_by_name("Going out")
        .await
        .expect("cap present");
    assert_eq!(cap.cap, Some(Money::from_centimes(35_000)));
    // And reflected in the recomputed envelope.
    let cats = categories(&db, as_of()).await.expect("categories ok");
    assert_eq!(find(&cats, "Going out").budget.centimes(), 35_000);
}

#[tokio::test]
async fn set_cap_none_makes_the_envelope_unlimited() {
    let db = seeded();
    set_cap(&db, "Shopping", None).await.expect("clear cap ok");
    let cap = db
        .category_cap_by_name("Shopping")
        .await
        .expect("cap present");
    assert_eq!(cap.cap, None, "None cap = unlimited envelope");
}

#[tokio::test]
async fn set_cap_lowers_total_allocated() {
    let db = seeded();
    // Drop Going out from 40_000 → 34_200 (a −5_800 delta); allocated falls to
    // exactly the budget, so overAllocated clears.
    set_cap(&db, "Going out", Some(Money::from_centimes(34_200)))
        .await
        .expect("set cap ok");
    let t = budget_totals(&db, as_of()).await.expect("budget totals ok");
    assert_eq!(t.allocated.centimes(), 420_000, "425_800 − 5_800");
    assert_eq!(t.over_allocated.centimes(), 0, "now exactly at budget");
    assert_eq!(t.unallocated.centimes(), 0);
}

#[tokio::test]
async fn set_cap_unknown_name_is_not_found() {
    let db = seeded();
    let err = set_cap(&db, "Nonexistent", Some(Money::from_centimes(1_000)))
        .await
        .expect_err("unknown category is NotFound");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}

// ── port-object-safety + serialization sanity ───────────────────────────────

#[tokio::test]
async fn services_work_through_the_port_trait_object() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;
    let cats = categories(port, as_of()).await.expect("categories ok");
    assert_eq!(cats.len(), 8);
}

#[tokio::test]
async fn category_dto_serializes_money_fields_as_centimes() {
    let cats = categories(&seeded(), as_of()).await.expect("categories ok");
    let groc = find(&cats, "Groceries").clone();
    let v = serde_json::to_value(&groc).expect("serialize");
    assert_eq!(v["budget"], serde_json::json!(80_000));
    assert_eq!(v["spent"], serde_json::json!(13_095));
    assert_eq!(v["proj"], serde_json::json!(21_825));
    assert_eq!(v["remaining"], serde_json::json!(66_905));
    assert_eq!(v["usedPct"], serde_json::json!(16));
    let back: CategoryDto = serde_json::from_value(v).expect("deserialize");
    assert_eq!(back, groc);
}
