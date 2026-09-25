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
//! Integration tests for `phosk_debts` (F3): institutional debts + personal IOUs.
//!
//! These drive every service through `&dyn DatabaseAdapter` against the
//! deterministic Swiss seed (`MemoryDb::seeded()`) at `as_of = 2026-06-18`, and
//! pin EXACT centime values + derived fields per the build-contract amortization
//! engine (§5.4), against the implemented services.
//!
//! Derived-field math (build-contract §5.4):
//! - `monthlyRate = apr/12`; `annualInterest = round(balance_centimes * apr)`.
//! - `monthsToPayoff`: amortize `balance` compounding at `monthlyRate`, paying
//!   `monthly`, until balance ≤ 0; cap 600; revolving (`monthly <= balance*rate`)
//!   ⇒ 600.
//! - `interestRemaining`: revolving ⇒ `annualInterest*5`; else `annualInterest*months/12`.
//! - `paidOffPct = (orig-balance)/orig`.
//! - spark: amortizing = `balance + (5-i)*monthly`; revolving (CARD|high) =
//!   `balance - (5-i)*(monthly/3)`; each `/100` floored.
//!
//! Seed slugs: debts `vw/card/loan/tax`, IOUs `i1..i4`.

use chrono::NaiveDate;

use phosk_adapter_db::DatabaseAdapter;
use phosk_db_memory::MemoryDb;
use phosk_debts::{debts, personal_ious};

/// The pinned read date for every test (within the May/June-2026 seed cycle).
fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid as_of date")
}

/// A seeded in-memory adapter, behind the PORT trait object the services take.
fn db() -> MemoryDb {
    MemoryDb::seeded().expect("seed the in-memory adapter")
}

/// Look up one debt DTO by id in a list.
fn find<'a>(list: &'a [debts::DebtDto], id: &str) -> &'a debts::DebtDto {
    list.iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("debt {id} present in seed"))
}

// ── debts::list_debts ─────────────────────────────────────────────────────────

#[tokio::test]
async fn list_debts_returns_all_four_seed_debts() {
    let db = db();
    let out = debts::list_debts(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("list_debts ok");
    assert_eq!(out.len(), 4, "the seed has four debts");
    let mut ids: Vec<&str> = out.iter().map(|d| d.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["card", "loan", "tax", "vw"]);
}

#[tokio::test]
async fn list_debts_carries_raw_seed_fields() {
    let db = db();
    let out = debts::list_debts(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("list_debts ok");

    let vw = find(&out, "vw");
    assert_eq!(vw.name, "VW lease");
    assert_eq!(vw.lender, "AMAG Leasing");
    assert_eq!(vw.kind, "LEASE");
    assert_eq!(vw.balance.centimes(), 1_820_000);
    assert_eq!(vw.orig.centimes(), 3_200_000);
    assert_eq!(vw.monthly.centimes(), 45_000);
    assert!((vw.apr - 0.039).abs() < 1e-9);
    assert_eq!(vw.day, 1);
    assert_eq!(vw.term, 48);
    assert_eq!(vw.src, "user");
    assert_eq!(vw.status, "ok");

    let tax = find(&out, "tax");
    assert_eq!(tax.kind, "TAX");
    assert!((tax.apr - 0.0).abs() < 1e-12);
    assert_eq!(tax.src, "llm", "tax is the LLM-inferred debt");
    assert_eq!(tax.status, "due");
}

#[tokio::test]
async fn list_debts_annual_interest_is_rounded_balance_times_apr() {
    let db = db();
    let out = debts::list_debts(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("list_debts ok");
    // round(balance_centimes * apr).
    assert_eq!(find(&out, "vw").annual_interest.centimes(), 70_980);
    assert_eq!(find(&out, "card").annual_interest.centimes(), 43_860);
    assert_eq!(find(&out, "loan").annual_interest.centimes(), 50_960);
    assert_eq!(find(&out, "tax").annual_interest.centimes(), 0);
}

#[tokio::test]
async fn list_debts_months_to_payoff_amortizes_with_cap() {
    let db = db();
    let out = debts::list_debts(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("list_debts ok");
    // Amortize compounding at apr/12 paying `monthly` until balance ≤ 0.
    assert_eq!(find(&out, "vw").months_to_payoff, 44);
    assert_eq!(
        find(&out, "card").months_to_payoff,
        27,
        "card monthly 15000 > monthly interest 3655 ⇒ it amortizes, not revolving"
    );
    assert_eq!(find(&out, "loan").months_to_payoff, 33);
    assert_eq!(
        find(&out, "tax").months_to_payoff,
        6,
        "0% APR ⇒ balance/monthly"
    );
}

#[tokio::test]
async fn list_debts_interest_remaining_non_revolving_formula() {
    let db = db();
    let out = debts::list_debts(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("list_debts ok");
    // Non-revolving: annualInterest * monthsToPayoff / 12 (integer centimes).
    assert_eq!(
        find(&out, "vw").interest_remaining.centimes(),
        70_980 * 44 / 12
    );
    assert_eq!(
        find(&out, "card").interest_remaining.centimes(),
        43_860 * 27 / 12
    );
    assert_eq!(
        find(&out, "loan").interest_remaining.centimes(),
        50_960 * 33 / 12
    );
    assert_eq!(find(&out, "tax").interest_remaining.centimes(), 0);
}

#[tokio::test]
async fn list_debts_paid_off_pct() {
    let db = db();
    let out = debts::list_debts(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("list_debts ok");
    // (orig - balance) / orig.
    assert!(
        (find(&out, "vw").paid_off_pct - (3_200_000.0 - 1_820_000.0) / 3_200_000.0).abs() < 1e-9
    );
    assert!(
        (find(&out, "card").paid_off_pct - 0.0).abs() < 1e-12,
        "card balance == orig"
    );
    assert!(
        (find(&out, "loan").paid_off_pct - (1_500_000.0 - 980_000.0) / 1_500_000.0).abs() < 1e-9
    );
    assert!((find(&out, "tax").paid_off_pct - 0.5).abs() < 1e-9);
}

#[tokio::test]
async fn list_debts_status_and_group_labels() {
    let db = db();
    let out = debts::list_debts(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("list_debts ok");
    assert_eq!(find(&out, "vw").status_label, "ON TRACK");
    assert_eq!(find(&out, "card").status_label, "HIGH INTEREST");
    assert_eq!(find(&out, "tax").status_label, "DUE SOON");

    assert_eq!(find(&out, "vw").group_label, "LEASES & LOANS");
    assert_eq!(find(&out, "loan").group_label, "LEASES & LOANS");
    assert_eq!(find(&out, "card").group_label, "REVOLVING CREDIT");
    assert_eq!(find(&out, "tax").group_label, "OBLIGATIONS");
}

#[tokio::test]
async fn list_debts_balance_spark_shapes() {
    let db = db();
    let out = debts::list_debts(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("list_debts ok");
    // Amortizing (declining): balance + (5-i)*monthly, /100 floored.
    assert_eq!(
        find(&out, "vw").hist,
        vec![20_450.0, 20_000.0, 19_550.0, 19_100.0, 18_650.0, 18_200.0]
    );
    assert_eq!(
        find(&out, "loan").hist,
        vec![11_400.0, 11_080.0, 10_760.0, 10_440.0, 10_120.0, 9_800.0]
    );
    assert_eq!(
        find(&out, "tax").hist,
        vec![3_850.0, 3_500.0, 3_150.0, 2_800.0, 2_450.0, 2_100.0]
    );
    // Revolving (CARD|high) rising into today: balance - (5-i)*(monthly/3), /100.
    assert_eq!(
        find(&out, "card").hist,
        vec![3_150.0, 3_200.0, 3_250.0, 3_300.0, 3_350.0, 3_400.0]
    );
    // Spark always has six points and ends at (≈) today's balance for revolving.
    for d in &out {
        assert_eq!(d.hist.len(), 6, "spark is always 6 points");
    }
}

#[tokio::test]
async fn list_debts_serializes_money_as_centimes_integer_and_type_key() {
    let db = db();
    let out = debts::list_debts(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("list_debts ok");
    let vw = find(&out, "vw").clone();
    let v = serde_json::to_value(&vw).expect("DebtDto serializes");
    // Money is exact i64 centimes on the wire (never a float CHF).
    assert_eq!(v["balance"], serde_json::json!(1_820_000));
    assert_eq!(v["annualInterest"], serde_json::json!(70_980));
    // camelCase keys + the `type` rename for `kind`.
    assert_eq!(v["type"], serde_json::json!("LEASE"));
    assert_eq!(v["paidOffPct"].is_number(), true);
    assert!(
        v.get("kind").is_none(),
        "kind serializes as `type`, not `kind`"
    );
}

// ── debts::debt_stats ─────────────────────────────────────────────────────────

#[tokio::test]
async fn debt_stats_totals_and_counts() {
    let db = db();
    let s = debts::debt_stats(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("debt_stats ok");
    assert_eq!(s.count, 4);
    assert_eq!(s.auto_count, 1, "only `tax` is LLM-inferred");
    assert_eq!(s.total_owed.centimes(), 3_350_000);
    assert_eq!(s.total_orig.centimes(), 5_460_000);
    assert_eq!(s.total_monthly.centimes(), 127_000);
    // Σ round(balance*apr): 70980 + 43860 + 50960 + 0.
    assert_eq!(s.total_interest_yr.centimes(), 165_800);
}

#[tokio::test]
async fn debt_stats_weighted_apr_and_paid_off_total() {
    let db = db();
    let s = debts::debt_stats(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("debt_stats ok");
    // Σ(balance*apr) / Σ balance.
    let expect = (1_820_000.0 * 0.039 + 340_000.0 * 0.129 + 980_000.0 * 0.052) / 3_350_000.0;
    assert!(
        (s.weighted_apr - expect).abs() < 1e-9,
        "weightedApr {} != {expect}",
        s.weighted_apr
    );
    // (totalOrig - totalOwed) / totalOrig.
    let paid = (5_460_000.0 - 3_350_000.0) / 5_460_000.0;
    assert!((s.paid_off_total_pct - paid).abs() < 1e-9);
}

#[tokio::test]
async fn debt_stats_strategy_targets() {
    let db = db();
    let s = debts::debt_stats(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("debt_stats ok");
    assert_eq!(s.avalanche_target, "card", "highest APR (0.129)");
    assert_eq!(s.snowball_target, "tax", "smallest balance (210000)");
}

#[tokio::test]
async fn debt_stats_horizon_and_label_present() {
    let db = db();
    let s = debts::debt_stats(&db as &dyn DatabaseAdapter, as_of())
        .await
        .expect("debt_stats ok");
    // Combined-payoff horizon is the longest single-debt horizon at current pace
    // (vw amortizes in 44 months — the last balance to clear).
    assert_eq!(
        s.horizon, 44,
        "months to clear the longest-running debt (vw)"
    );
    assert!(
        !s.debt_free_label.is_empty(),
        "a projected month label is present"
    );
}

// ── debts::trajectory ─────────────────────────────────────────────────────────

#[tokio::test]
async fn trajectory_has_history_today_and_projection() {
    let db = db();
    let t = debts::trajectory(&db as &dyn DatabaseAdapter, as_of(), "avalanche")
        .await
        .expect("trajectory ok");
    assert!(!t.points.is_empty(), "the curve has points");
    // m == 0 is today; combined balance there is Σ balances.
    let today = t
        .points
        .iter()
        .find(|p| p.m == 0)
        .expect("a today point (m == 0)");
    assert_eq!(today.total.centimes(), 3_350_000, "today = Σ open balances");
    // History points (negative m) precede projection points (positive m).
    assert!(t.points.iter().any(|p| p.m < 0), "has history");
    assert!(t.points.iter().any(|p| p.m > 0), "has projection");
    // Strictly increasing month offsets.
    for w in t.points.windows(2) {
        assert!(w[0].m < w[1].m, "points are ordered by month offset");
    }
    // The projection decays to zero at the debt-free horizon.
    let last = t.points.last().expect("a final point");
    assert_eq!(last.total.centimes(), 0, "curve ends at zero balance");
    assert!(!t.x_ticks.is_empty(), "x-axis ticks present");
    assert!(!t.debt_free_label.is_empty());
}

#[tokio::test]
async fn trajectory_strategy_changes_ordering_not_total_or_endpoints() {
    let db = db();
    let av = debts::trajectory(&db as &dyn DatabaseAdapter, as_of(), "avalanche")
        .await
        .expect("avalanche ok");
    let sn = debts::trajectory(&db as &dyn DatabaseAdapter, as_of(), "snowball")
        .await
        .expect("snowball ok");
    // Both strategies start at the same combined balance and end at zero.
    let av_today = av.points.iter().find(|p| p.m == 0).expect("av today");
    let sn_today = sn.points.iter().find(|p| p.m == 0).expect("sn today");
    assert_eq!(av_today.total.centimes(), sn_today.total.centimes());
    assert_eq!(av.points.last().expect("av last").total.centimes(), 0);
    assert_eq!(sn.points.last().expect("sn last").total.centimes(), 0);
}

// ── debts::debt_detail ────────────────────────────────────────────────────────

#[tokio::test]
async fn debt_detail_builds_decay_series() {
    let db = db();
    let d = debts::debt_detail(&db as &dyn DatabaseAdapter, "vw")
        .await
        .expect("debt_detail ok");
    let s = &d.decay_series;
    assert!(!s.hist.is_empty(), "history present");
    assert!(!s.forward.is_empty(), "forward projection present");
    // todayIndex points at the last historical (today) entry.
    assert_eq!(
        s.today_index,
        s.hist.len() - 1,
        "todayIndex = last hist index"
    );
    // hist ends at (≈) today's balance; forward starts there and decays to zero.
    assert_eq!(
        s.hist.last().expect("last hist").centimes(),
        1_820_000,
        "hist ends at today's balance"
    );
    assert_eq!(
        s.forward.last().expect("last forward").centimes(),
        0,
        "forward decays to zero balance"
    );
    assert!(!d.guidance.is_empty(), "AI guidance line present");
}

#[tokio::test]
async fn debt_detail_unknown_slug_is_not_found() {
    let db = db();
    let err = debts::debt_detail(&db as &dyn DatabaseAdapter, "does-not-exist")
        .await
        .expect_err("unknown slug ⇒ error");
    assert!(
        matches!(err, phosk_core::error::PhoskError::NotFound(_)),
        "unknown debt slug ⇒ NotFound, got {err:?}"
    );
}

// ── debts::debt_payments ──────────────────────────────────────────────────────

#[tokio::test]
async fn debt_payments_projects_seed_history() {
    let db = db();
    let pays = debts::debt_payments(&db as &dyn DatabaseAdapter, "vw")
        .await
        .expect("debt_payments ok");
    assert!(!pays.is_empty(), "vw has at least one recorded payment");
    // The seed records one payment per debt of `monthly`, balance_after =
    // balance + monthly (running back in time).
    let p = &pays[0];
    assert_eq!(p.amount.centimes(), 45_000, "the seed payment is `monthly`");
    assert_eq!(
        p.balance.centimes(),
        1_820_000 + 45_000,
        "balance after the (back-in-time) payment"
    );
}

#[tokio::test]
async fn debt_payments_unknown_slug_is_not_found() {
    let db = db();
    let err = debts::debt_payments(&db as &dyn DatabaseAdapter, "nope")
        .await
        .expect_err("unknown slug ⇒ error");
    assert!(
        matches!(err, phosk_core::error::PhoskError::NotFound(_)),
        "unknown debt slug ⇒ NotFound, got {err:?}"
    );
}

// ── personal_ious::list_personal_ious ─────────────────────────────────────────

#[tokio::test]
async fn list_personal_ious_returns_four_with_directions() {
    let db = db();
    let out = personal_ious::list_personal_ious(&db as &dyn DatabaseAdapter)
        .await
        .expect("list_personal_ious ok");
    assert_eq!(out.len(), 4);
    assert_eq!(out.iter().filter(|i| i.dir == "in").count(), 2);
    assert_eq!(out.iter().filter(|i| i.dir == "out").count(), 2);
}

#[tokio::test]
async fn list_personal_ious_carries_fields_and_repaid_pct() {
    let db = db();
    let out = personal_ious::list_personal_ious(&db as &dyn DatabaseAdapter)
        .await
        .expect("list_personal_ious ok");
    let by = |id: &str| {
        out.iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("iou {id} present"))
            .clone()
    };

    let i1 = by("i1");
    assert_eq!(i1.dir, "in");
    assert_eq!(i1.person, "Léa");
    assert_eq!(i1.amount.centimes(), 12_000);
    assert_eq!(i1.of.centimes(), 12_000);
    assert!((i1.repaid_pct - 0.0).abs() < 1e-12, "(of-amount)/of = 0");

    // i2: (9000-4500)/9000 = 0.5.
    let i2 = by("i2");
    assert_eq!(i2.amount.centimes(), 4_500);
    assert_eq!(i2.of.centimes(), 9_000);
    assert!((i2.repaid_pct - 0.5).abs() < 1e-9);

    // i4: (50000-20000)/50000 = 0.6.
    let i4 = by("i4");
    assert_eq!(i4.dir, "out");
    assert!((i4.repaid_pct - 0.6).abs() < 1e-9);
}

#[tokio::test]
async fn personal_iou_serializes_money_as_centimes() {
    let db = db();
    let out = personal_ious::list_personal_ious(&db as &dyn DatabaseAdapter)
        .await
        .expect("list_personal_ious ok");
    let i4 = out
        .iter()
        .find(|i| i.id == "i4")
        .expect("i4 present")
        .clone();
    let v = serde_json::to_value(&i4).expect("PersonalIouDto serializes");
    assert_eq!(v["amount"], serde_json::json!(20_000));
    assert_eq!(v["of"], serde_json::json!(50_000));
    assert_eq!(v["repaidPct"].is_number(), true, "camelCase repaidPct");
}

// ── personal_ious::iou_stats ──────────────────────────────────────────────────

#[tokio::test]
async fn iou_stats_net_position() {
    let db = db();
    let s = personal_ious::iou_stats(&db as &dyn DatabaseAdapter)
        .await
        .expect("iou_stats ok");
    assert_eq!(s.owed_to_you.centimes(), 16_500, "i1 12000 + i2 4500");
    assert_eq!(s.you_owe.centimes(), 26_000, "i3 6000 + i4 20000");
    assert_eq!(s.net.centimes(), -9_500, "owedToYou - youOwe");
    assert_eq!(s.count_in, 2);
    assert_eq!(s.count_out, 2);
}

#[tokio::test]
async fn iou_stats_serializes_net_as_signed_centimes() {
    let db = db();
    let s = personal_ious::iou_stats(&db as &dyn DatabaseAdapter)
        .await
        .expect("iou_stats ok");
    let v = serde_json::to_value(&s).expect("IouStatsDto serializes");
    assert_eq!(v["net"], serde_json::json!(-9_500));
    assert_eq!(v["owedToYou"], serde_json::json!(16_500));
    assert_eq!(v["youOwe"], serde_json::json!(26_000));
}
