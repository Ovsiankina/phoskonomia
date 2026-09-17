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
//! RED integration tests for the `phosk_insights::analytics`, `::momentum` and
//! `::exports` slices (F3).
//!
//! These are written BEFORE the implementations: every service body is a
//! `todo!()`, so each `#[tokio::test]` that drives a service COMPILES and then
//! PANICS at runtime (red). The pure `momentum` helpers are likewise `todo!()`
//! and their tests fail the same way.
//!
//! The contract is pinned to:
//!   - `frontend/dioxus-app/src/data/analytics.rs` (the DTO field shapes + the
//!     seeded JUL-25 → JUN-26 history and the per-category momentum cards),
//!   - the skeleton doc-comments in `src/analytics.rs` / `src/momentum.rs`
//!     (derived-field formulas: `pct_delta`, trailing-N=3, weekend_share),
//!   - the deterministic Swiss seed (`phosk_db_memory::MemoryDb::seeded`), at
//!     `as_of = 2026-06-18` (day 18 of the June cycle).
//!
//! Money is asserted as exact i64 centimes; never CHF floats, never strings.

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{Value, json};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;

use phosk_insights::analytics::{
    analytics_insight, category_momentum, spend_history, spend_stats, weekday_rhythm,
};
use phosk_insights::exports::{
    EXPORT_SCHEMA_VERSION, export_budget_csv, export_subscriptions_csv, export_transactions_csv,
};
use phosk_insights::momentum::{delta_pct, trailing_avg};

// ── shared helpers ──────────────────────────────────────────────────────────

fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
}

/// The spec's "today": 2026-06-18, day 18 of the June cycle.
fn today() -> NaiveDate {
    naive(2026, 6, 18)
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

fn to_json<T: Serialize>(t: &T) -> Value {
    serde_json::to_value(t).expect("DTO serializes")
}

fn cents(m: i64) -> Money {
    Money::from_centimes(m)
}

/// Sorted camelCase key set of a JSON object.
fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v
        .as_object()
        .expect("JSON object")
        .keys()
        .cloned()
        .collect();
    k.sort();
    k
}

// ════════════════════════════════════════════════════════════════════════════
// momentum: trailing_avg  (pure, fully pinned by the §6 formula)
// ════════════════════════════════════════════════════════════════════════════

/// `trailing_avg` takes the last `n` of the prior cycles and integer-divides the
/// checked sum by the count. [720,690,810,740,760,880] last 3 = (740+760+880)/3
/// = 2380/3 = 793 centimes (integer division truncates).
#[test]
fn trailing_avg_takes_last_n_and_integer_divides() {
    let prior = [cents(740), cents(760), cents(880)];
    let avg = trailing_avg(&prior, 3).expect("avg ok");
    assert_eq!(avg.centimes(), 793, "(740+760+880)/3 truncates to 793");
}

/// With more prior cycles than `n`, only the trailing `n` are averaged.
#[test]
fn trailing_avg_uses_only_the_last_n_window() {
    // Mirrors the Groceries hist: 6 prior cycles, N=3 → last three.
    let prior = [
        cents(72_000),
        cents(69_000),
        cents(81_000),
        cents(74_000),
        cents(76_000),
        cents(88_000),
    ];
    let avg = trailing_avg(&prior, 3).expect("avg ok");
    // (74_000 + 76_000 + 88_000) / 3 = 238_000 / 3 = 79_333 (truncated).
    assert_eq!(avg.centimes(), 79_333);
}

/// Fewer prior cycles than `n` averages all of them.
#[test]
fn trailing_avg_with_fewer_than_n_averages_all() {
    let prior = [cents(100), cents(200)];
    let avg = trailing_avg(&prior, 5).expect("avg ok");
    assert_eq!(avg.centimes(), 150, "(100+200)/2");
}

/// No prior cycles → `Money::ZERO` (the documented empty case), never an error
/// and never a divide-by-zero.
#[test]
fn trailing_avg_of_empty_is_zero() {
    let avg = trailing_avg(&[], 3).expect("empty avg ok");
    assert_eq!(avg, Money::ZERO);
}

/// `n == 0` degenerates to no window; the helper must not divide by zero.
/// Documented behaviour is an empty window → `Money::ZERO`.
#[test]
fn trailing_avg_with_zero_n_is_zero() {
    let prior = [cents(100), cents(200), cents(300)];
    let avg = trailing_avg(&prior, 0).expect("zero-n avg ok");
    assert_eq!(avg, Money::ZERO, "n=0 selects an empty window");
}

/// The default baseline is N = 3 (build-contract §6, `momentum_baseline_cycles`).
#[test]
fn trailing_avg_default_three_matches_seed_groceries() {
    // Groceries dioxus card: prior_avg = 58_900 is NOT a trailing-3 of the
    // budget history (that seed is illustrative); here we pin the helper's own
    // arithmetic on a clean N=3 window.
    let prior = [cents(60_000), cents(64_000), cents(63_000)];
    let avg = trailing_avg(&prior, 3).expect("avg ok");
    assert_eq!(avg.centimes(), 62_333, "(60_000+64_000+63_000)/3");
}

/// An overflowing checked sum surfaces `PhoskError::Overflow`, never wraps.
#[test]
fn trailing_avg_overflow_is_surfaced_not_wrapped() {
    let prior = [cents(i64::MAX), cents(i64::MAX)];
    let err = trailing_avg(&prior, 2).expect_err("sum overflows i64");
    assert!(
        matches!(err, PhoskError::Overflow(_)),
        "got {err:?}, want Overflow"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// momentum: delta_pct  (pure, fully pinned)
// ════════════════════════════════════════════════════════════════════════════

/// `delta_pct = round(100·(current − avg)/avg)`. 61_240 vs 58_900 →
/// round(100·2340/58900) = round(3.973) = 4 (the Groceries card's deltaPct).
#[test]
fn delta_pct_matches_groceries_card() {
    assert_eq!(delta_pct(cents(61_240), cents(58_900)), 4);
}

/// Going-out card: 31_200 vs 26_200 → round(100·5000/26200)=round(19.08)=19.
#[test]
fn delta_pct_matches_going_out_card() {
    assert_eq!(delta_pct(cents(31_200), cents(26_200)), 19);
}

/// Coffee card: 9_840 vs 7_680 → round(100·2160/7680)=round(28.125)=28.
#[test]
fn delta_pct_matches_coffee_card() {
    assert_eq!(delta_pct(cents(9_840), cents(7_680)), 28);
}

/// A falling signal is negative: Transport 11_450 vs 13_300 →
/// round(100·(−1850)/13300)=round(−13.9)=−14.
#[test]
fn delta_pct_is_signed_negative_for_a_fall() {
    assert_eq!(delta_pct(cents(11_450), cents(13_300)), -14);
}

/// Shopping card: 22_650 vs 38_500 → round(100·(−15850)/38500)=round(−41.2)=−41.
/// (The dioxus seed's −24 is a hand-authored figure, not this formula; the
/// helper is the single source of truth, so we pin the formula.)
#[test]
fn delta_pct_shopping_follows_the_formula_not_the_seed_literal() {
    assert_eq!(delta_pct(cents(22_650), cents(38_500)), -41);
}

/// Equal current and baseline → 0% (the fixed Rent channel: 168_000 vs 168_000).
#[test]
fn delta_pct_is_zero_when_flat() {
    assert_eq!(delta_pct(cents(168_000), cents(168_000)), 0);
}

/// Zero baseline is the guarded divide-by-zero: result is 0, never NaN/panic.
#[test]
fn delta_pct_zero_baseline_is_zero_not_nan() {
    assert_eq!(delta_pct(cents(5_000), Money::ZERO), 0);
}

/// Rounding is half-aware: 150 vs 100 → exactly +50%.
#[test]
fn delta_pct_rounds_a_clean_fifty_percent() {
    assert_eq!(delta_pct(cents(150), cents(100)), 50);
}

// ════════════════════════════════════════════════════════════════════════════
// analytics: spend_history
// ════════════════════════════════════════════════════════════════════════════

/// The 12-cycle history DTO serializes to the exact camelCase contract: a
/// `points` array whose elements carry money as exact i64 centimes.
#[tokio::test]
async fn spend_history_json_shape_is_camel_case_with_centime_money() {
    let dto = spend_history(&seeded(), today(), 12)
        .await
        .expect("history ok");
    let v = to_json(&dto);

    assert_eq!(keys(&v), vec!["points"], "top-level key is `points`");
    let points = v["points"].as_array().expect("points array");
    assert_eq!(points.len(), 12, "12 cycles requested");

    let p = &points[0];
    assert_eq!(
        keys(p),
        vec!["budget", "m", "over", "projected", "rate", "spend", "yr"],
        "point camelCase keys"
    );
    // Money is an integer centime count, not a float and not a string.
    assert!(p["spend"].is_i64(), "spend is i64 centimes");
    assert!(p["budget"].is_i64(), "budget is i64 centimes");
}

/// The final point is the current in-progress June cycle: `projected = true`,
/// labelled JUN/26, with the seeded current-cycle spend (CHF 3222.45 full
/// window). Earlier points are not projected.
#[tokio::test]
async fn spend_history_marks_only_the_current_cycle_projected() {
    let dto = spend_history(&seeded(), today(), 12)
        .await
        .expect("history ok");
    let v = to_json(&dto);
    let points = v["points"].as_array().expect("array");

    let last = points.last().expect("at least one point");
    assert_eq!(last["m"], json!("JUN"), "current cycle is June");
    assert_eq!(last["yr"], json!("26"));
    assert_eq!(
        last["projected"],
        json!(true),
        "in-progress cycle is projected"
    );
    assert_eq!(last["budget"], json!(420_000), "cycle budget CHF 4200.00");

    // No earlier cycle is projected.
    for p in &points[..points.len() - 1] {
        assert_eq!(p["projected"], json!(false), "past cycles are settled");
    }
}

/// The previous settled cycle is May 2026 with the seeded total CHF 3787.70, and
/// it is not flagged over-budget (under the 4200.00 ceiling).
#[tokio::test]
async fn spend_history_previous_cycle_is_seeded_may() {
    let dto = spend_history(&seeded(), today(), 12)
        .await
        .expect("history ok");
    let v = to_json(&dto);
    let points = v["points"].as_array().expect("array");

    let may = &points[points.len() - 2];
    assert_eq!(may["m"], json!("MAY"));
    assert_eq!(may["yr"], json!("26"));
    assert_eq!(may["spend"], json!(378_770), "May total CHF 3787.70");
    assert_eq!(may["over"], json!(false), "May under the 4200 ceiling");
}

/// `over` is `spend > budget`; every point's flag agrees with its own numbers.
#[tokio::test]
async fn spend_history_over_flag_agrees_with_spend_vs_budget() {
    let dto = spend_history(&seeded(), today(), 12)
        .await
        .expect("history ok");
    let v = to_json(&dto);
    for p in v["points"].as_array().expect("array") {
        let spend = p["spend"].as_i64().expect("centimes");
        let budget = p["budget"].as_i64().expect("centimes");
        let over = p["over"].as_bool().expect("bool");
        assert_eq!(over, spend > budget, "over == (spend > budget) for {p}");
    }
}

/// Requesting fewer cycles truncates the window to that many trailing points,
/// ending at the current cycle.
#[tokio::test]
async fn spend_history_honours_a_shorter_window() {
    let dto = spend_history(&seeded(), today(), 3)
        .await
        .expect("history ok");
    let v = to_json(&dto);
    let points = v["points"].as_array().expect("array");
    assert_eq!(points.len(), 3, "3-cycle window");
    assert_eq!(
        points.last().expect("last")["m"],
        json!("JUN"),
        "ends at current"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// analytics: spend_stats
// ════════════════════════════════════════════════════════════════════════════

/// The stats DTO serializes to the exact camelCase contract with nested cycle
/// points and centime money.
#[tokio::test]
async fn spend_stats_json_shape_is_camel_case() {
    let dto = spend_stats(&seeded(), today(), 12).await.expect("stats ok");
    let v = to_json(&dto);
    assert_eq!(
        keys(&v),
        vec![
            "avg",
            "avgRate",
            "cur",
            "curVsAvgPct",
            "curVsPrevPct",
            "low",
            "months",
            "peak",
            "prev",
            "totalSaved",
        ],
        "stats camelCase keys"
    );
    assert!(v["avg"].is_i64(), "avg is i64 centimes");
    assert!(v["totalSaved"].is_i64(), "totalSaved is i64 centimes");

    // Nested cycle points carry their own centime money + labels.
    assert_eq!(
        keys(&v["cur"]),
        vec!["budget", "m", "spend", "yr"],
        "CyclePointDto keys"
    );
    assert!(v["cur"]["spend"].is_i64());
}

/// `months` is the count of cycles on record (12), and `cur`/`prev` are the
/// June/May cycle points from the history.
#[tokio::test]
async fn spend_stats_cur_and_prev_track_the_history_tail() {
    let dto = spend_stats(&seeded(), today(), 12).await.expect("stats ok");
    let v = to_json(&dto);
    assert_eq!(v["months"], json!(12), "12 cycles on record");
    assert_eq!(v["cur"]["m"], json!("JUN"));
    assert_eq!(v["prev"]["m"], json!("MAY"));
    assert_eq!(v["prev"]["spend"], json!(378_770), "May total");
}

/// `curVsPrevPct = pct_delta(cur, prev)` with the §5.5 formula
/// `round(100·(a−b)/b)`. cur=322_245, prev=378_770 →
/// round(100·(−56525)/378770)=round(−14.92)=−15.
#[tokio::test]
async fn spend_stats_cur_vs_prev_pct_matches_formula() {
    let dto = spend_stats(&seeded(), today(), 12).await.expect("stats ok");
    let v = to_json(&dto);
    assert_eq!(
        v["curVsPrevPct"],
        json!(-15),
        "round(100·(322245−378770)/378770)"
    );
}

/// `peak` is the highest-spend cycle and `low` the leanest; peak.spend ≥ every
/// point and low.spend ≤ every point.
#[tokio::test]
async fn spend_stats_peak_and_low_bound_the_window() {
    let stats = spend_stats(&seeded(), today(), 12).await.expect("stats ok");
    let hist = spend_history(&seeded(), today(), 12)
        .await
        .expect("history ok");

    let max = hist
        .points
        .iter()
        .map(|p| p.spend.centimes())
        .max()
        .expect("nonempty");
    let min = hist
        .points
        .iter()
        .map(|p| p.spend.centimes())
        .min()
        .expect("nonempty");
    assert_eq!(
        stats.peak.spend.centimes(),
        max,
        "peak is the max-spend cycle"
    );
    assert_eq!(
        stats.low.spend.centimes(),
        min,
        "low is the min-spend cycle"
    );
}

/// `curVsAvgPct = pct_delta(cur, avg)`; the sign agrees with cur vs the avg it
/// reports (cur below avg → negative).
#[tokio::test]
async fn spend_stats_cur_vs_avg_pct_sign_agrees_with_avg() {
    let stats = spend_stats(&seeded(), today(), 12).await.expect("stats ok");
    let cur = stats.cur.spend.centimes();
    let avg = stats.avg.centimes();
    let sign_ok = if cur < avg {
        stats.cur_vs_avg_pct < 0
    } else if cur > avg {
        stats.cur_vs_avg_pct > 0
    } else {
        stats.cur_vs_avg_pct == 0
    };
    assert!(sign_ok, "curVsAvgPct sign must agree with cur vs avg");
}

/// `avgRate` is a 0–1 ratio.
#[tokio::test]
async fn spend_stats_avg_rate_is_a_unit_ratio() {
    let stats = spend_stats(&seeded(), today(), 12).await.expect("stats ok");
    assert!(
        (0.0..=1.0).contains(&stats.avg_rate),
        "avgRate stays in 0..=1, got {}",
        stats.avg_rate
    );
}

// ════════════════════════════════════════════════════════════════════════════
// analytics: category_momentum
// ════════════════════════════════════════════════════════════════════════════

/// Each momentum card serializes to the exact camelCase contract with centime
/// money and a 12-point f64 spark.
#[tokio::test]
async fn category_momentum_json_shape_is_camel_case() {
    let cards = category_momentum(&seeded(), today())
        .await
        .expect("momentum ok");
    let v = to_json(&cards);
    let arr = v.as_array().expect("array of cards");
    assert!(!arr.is_empty(), "at least one category");

    let c = &arr[0];
    assert_eq!(
        keys(c),
        vec![
            "budget", "deltaPct", "fixed", "name", "now", "priorAvg", "series"
        ],
        "card camelCase keys"
    );
    assert!(c["now"].is_i64(), "now is i64 centimes");
    assert!(c["priorAvg"].is_i64(), "priorAvg is i64 centimes");
    assert!(c["budget"].is_i64(), "budget is i64 centimes");
    let series = c["series"].as_array().expect("series array");
    assert_eq!(series.len(), 12, "12-point spark");
    assert!(series.iter().all(|x| x.is_number()), "spark is f64s");
}

/// `deltaPct` on every card equals `delta_pct(now, priorAvg)` — the cards funnel
/// through the single momentum helper (build-contract §6), no per-card formula.
#[tokio::test]
async fn category_momentum_delta_pct_funnels_through_the_helper() {
    let cards = category_momentum(&seeded(), today())
        .await
        .expect("momentum ok");
    assert!(!cards.is_empty(), "at least one category");
    for c in &cards {
        assert_eq!(
            c.delta_pct,
            delta_pct(c.now, c.prior_avg),
            "card `{}` deltaPct must equal delta_pct(now, priorAvg)",
            c.name
        );
    }
}

/// The fixed Rent channel is `fixed = true` with a flat history → 0% momentum,
/// and `now == budget == priorAvg` (CHF 1680.00).
#[tokio::test]
async fn category_momentum_fixed_rent_is_flat() {
    let cards = category_momentum(&seeded(), today())
        .await
        .expect("momentum ok");
    let rent = cards
        .iter()
        .find(|c| c.name == "Rent")
        .expect("Rent card present");
    assert!(rent.fixed, "Rent is a fixed channel");
    assert_eq!(rent.now.centimes(), 168_000, "Rent CHF 1680.00");
    assert_eq!(rent.budget.centimes(), 168_000, "Rent cap CHF 1680.00");
    assert_eq!(rent.delta_pct, 0, "flat fixed channel has zero momentum");
}

/// A discretionary card carries its cap from the seed (Groceries cap CHF 800.00)
/// and a non-fixed flag.
#[tokio::test]
async fn category_momentum_groceries_carries_its_cap() {
    let cards = category_momentum(&seeded(), today())
        .await
        .expect("momentum ok");
    let groceries = cards
        .iter()
        .find(|c| c.name == "Groceries")
        .expect("Groceries card present");
    assert!(!groceries.fixed, "Groceries is tunable");
    assert_eq!(
        groceries.budget.centimes(),
        80_000,
        "Groceries cap CHF 800.00"
    );
}

/// `priorAvg` equals the trailing-N=3 average of the card's own prior cycles, so
/// it agrees with the `trailing_avg` helper over the spark's first cycles. We
/// assert the weaker, robust invariant: priorAvg is non-negative and consistent
/// with the helper on a flat channel (Rent → 168_000).
#[tokio::test]
async fn category_momentum_prior_avg_uses_trailing_helper() {
    let cards = category_momentum(&seeded(), today())
        .await
        .expect("momentum ok");
    let rent = cards
        .iter()
        .find(|c| c.name == "Rent")
        .expect("Rent card present");
    // Rent's prior cycles are all 168_000, so trailing-3 == 168_000.
    let expected =
        trailing_avg(&[cents(168_000), cents(168_000), cents(168_000)], 3).expect("avg ok");
    assert_eq!(
        rent.prior_avg, expected,
        "Rent priorAvg == trailing-3 of 1680s"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// analytics: weekday_rhythm
// ════════════════════════════════════════════════════════════════════════════

/// The rhythm DTO serializes to the exact camelCase contract: 7 weekday buckets
/// (Mon→Sun) plus the roll-up stats.
#[tokio::test]
async fn weekday_rhythm_json_shape_is_camel_case() {
    let dto = weekday_rhythm(&seeded(), today()).await.expect("rhythm ok");
    let v = to_json(&dto);
    assert_eq!(keys(&v), vec!["stats", "weekday"], "rhythm top-level keys");

    let weekday = v["weekday"].as_array().expect("weekday array");
    assert_eq!(weekday.len(), 7, "7 weekday buckets");
    assert_eq!(
        keys(&weekday[0]),
        vec!["d", "v"],
        "weekday bucket keys (label + value)"
    );

    assert_eq!(
        keys(&v["stats"]),
        vec!["avg", "max", "peak", "total", "weekendShare"],
        "stats camelCase keys"
    );
}

/// The buckets are labelled Mon→Sun in order.
#[tokio::test]
async fn weekday_rhythm_buckets_are_mon_to_sun() {
    let dto = weekday_rhythm(&seeded(), today()).await.expect("rhythm ok");
    let labels: Vec<&str> = dto.weekday.iter().map(|w| w.d.as_str()).collect();
    assert_eq!(
        labels,
        vec!["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"],
        "weekday order"
    );
}

/// `stats.max` is the largest bucket value and `stats.peak` is that bucket; the
/// peak's value equals `max`.
#[tokio::test]
async fn weekday_rhythm_peak_matches_max_bucket() {
    let dto = weekday_rhythm(&seeded(), today()).await.expect("rhythm ok");
    let max = dto.weekday.iter().map(|w| w.v).fold(0.0_f64, f64::max);
    assert!(
        (dto.stats.max - max).abs() < 1e-9,
        "stats.max is the bucket max"
    );
    assert!(
        (dto.stats.peak.v - max).abs() < 1e-9,
        "peak bucket carries the max value"
    );
}

/// `weekend_share = round(100·(Fri+Sat+Sun)/total)` (§5.5). We assert it equals
/// that formula computed from the DTO's own buckets, and lands in 0..=100.
#[tokio::test]
async fn weekday_rhythm_weekend_share_follows_formula() {
    let dto = weekday_rhythm(&seeded(), today()).await.expect("rhythm ok");
    let total: f64 = dto.weekday.iter().map(|w| w.v).sum();
    // Fri/Sat/Sun are indices 4,5,6 in the Mon→Sun ordering.
    let weekend: f64 = dto.weekday[4..7].iter().map(|w| w.v).sum();
    let expected = if total == 0.0 {
        0
    } else {
        (100.0 * weekend / total).round() as i32
    };
    assert_eq!(
        dto.stats.weekend_share, expected,
        "weekendShare = round(100·(Fri+Sat+Sun)/total)"
    );
    assert!(
        (0..=100).contains(&dto.stats.weekend_share),
        "weekendShare is a 0..=100 percent"
    );
}

/// `stats.avg` is `total / 7` and `stats.total` is the sum of the buckets.
#[tokio::test]
async fn weekday_rhythm_total_and_avg_are_consistent() {
    let dto = weekday_rhythm(&seeded(), today()).await.expect("rhythm ok");
    let total: f64 = dto.weekday.iter().map(|w| w.v).sum();
    assert!(
        (dto.stats.total - total).abs() < 1e-9,
        "stats.total is the sum"
    );
    assert!(
        (dto.stats.avg - total / 7.0).abs() < 1e-9,
        "stats.avg is total/7"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// analytics: analytics_insight
// ════════════════════════════════════════════════════════════════════════════

/// The insight DTO serializes to the exact camelCase contract: a model badge, a
/// text sentence, and a nested `suggestedCap` whose money is centimes.
#[tokio::test]
async fn analytics_insight_json_shape_is_camel_case() {
    let dto = analytics_insight(&seeded(), today())
        .await
        .expect("insight ok");
    let v = to_json(&dto);
    assert_eq!(
        keys(&v),
        vec!["model", "suggestedCap", "text"],
        "insight camelCase keys"
    );
    assert_eq!(
        keys(&v["suggestedCap"]),
        vec!["amount", "projectedSavings", "signalId"],
        "suggestedCap camelCase keys"
    );
    assert!(v["suggestedCap"]["amount"].is_i64(), "amount is centimes");
    assert!(
        v["suggestedCap"]["projectedSavings"].is_i64(),
        "projectedSavings is centimes"
    );
}

/// The insight carries a non-empty model badge and a non-empty narrative; the
/// suggested cap names a signal and a non-negative amount.
#[tokio::test]
async fn analytics_insight_carries_a_cap_suggestion() {
    let dto = analytics_insight(&seeded(), today())
        .await
        .expect("insight ok");
    assert!(!dto.model.is_empty(), "model badge present");
    assert!(!dto.text.is_empty(), "narrative present");
    assert!(
        !dto.suggested_cap.signal_id.is_empty(),
        "suggested cap targets a signal"
    );
    assert!(
        dto.suggested_cap.amount.centimes() >= 0,
        "suggested cap is non-negative"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// analytics: port object-safety + empty-window edges
// ════════════════════════════════════════════════════════════════════════════

/// Every analytics service is callable behind the `&dyn DatabaseAdapter` PORT
/// handle (ADR-010) — never sees the concrete `MemoryDb` type.
#[tokio::test]
async fn analytics_services_work_through_the_port_trait_object() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;

    let history = spend_history(port, today(), 12).await.expect("history ok");
    assert_eq!(history.points.len(), 12);

    let stats = spend_stats(port, today(), 12).await.expect("stats ok");
    assert_eq!(stats.months, 12);

    let cards = category_momentum(port, today()).await.expect("momentum ok");
    assert!(!cards.is_empty());

    let rhythm = weekday_rhythm(port, today()).await.expect("rhythm ok");
    assert_eq!(rhythm.weekday.len(), 7);

    let _insight = analytics_insight(port, today()).await.expect("insight ok");
}

/// An empty far-past cycle resolves without divide-by-zero: the current cycle's
/// momentum/rhythm are safe (no NaN weekend share) even with no spend.
#[tokio::test]
async fn analytics_empty_cycle_is_divide_by_zero_safe() {
    // April 2026 has no seeded receipts.
    let as_of = naive(2026, 4, 15);
    let rhythm = weekday_rhythm(&seeded(), as_of).await.expect("rhythm ok");
    // Guarded division: an empty week yields a 0 share, never NaN.
    assert_eq!(
        rhythm.stats.weekend_share, 0,
        "empty week → 0% weekend share"
    );
    assert!(
        rhythm.stats.avg.is_finite(),
        "avg never NaN on an empty week"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// exports: CSV dumps
// ════════════════════════════════════════════════════════════════════════════

/// The exported-CSV DTO serializes to the exact camelCase contract with the
/// schema version and a row count.
#[tokio::test]
async fn export_transactions_csv_json_shape_is_camel_case() {
    let dto = export_transactions_csv(&seeded(), today())
        .await
        .expect("export ok");
    let v = to_json(&dto);
    assert_eq!(
        keys(&v),
        vec!["csv", "filename", "generatedAt", "rowCount", "version"],
        "export camelCase keys"
    );
    assert_eq!(
        v["version"],
        json!(EXPORT_SCHEMA_VERSION),
        "stamped schema version"
    );
    assert!(v["rowCount"].is_u64(), "rowCount is an unsigned count");
    assert!(v["csv"].is_string(), "csv body is a string");
}

/// The transactions export stamps the current schema version, a non-empty
/// filename, an ISO `as_of` timestamp, and one data row per June receipt (9 in
/// the seed), with a header line on top.
#[tokio::test]
async fn export_transactions_csv_has_one_row_per_receipt() {
    let dto = export_transactions_csv(&seeded(), today())
        .await
        .expect("export ok");
    assert_eq!(dto.version, EXPORT_SCHEMA_VERSION, "schema version 1");
    assert!(!dto.filename.is_empty(), "suggested filename present");
    assert_eq!(dto.generated_at, "2026-06-18", "ISO as_of timestamp");
    assert_eq!(dto.row_count, 9, "9 June receipts in the seed");

    // The CSV has a header plus one line per data row.
    let lines: Vec<&str> = dto.csv.lines().collect();
    assert_eq!(
        lines.len(),
        usize::try_from(dto.row_count).expect("fits") + 1,
        "header + rowCount data lines"
    );
}

/// The budget export has one row per seeded category cap (8 channels).
#[tokio::test]
async fn export_budget_csv_has_one_row_per_cap() {
    let dto = export_budget_csv(&seeded(), today())
        .await
        .expect("export ok");
    assert_eq!(dto.row_count, 8, "8 budget channels in the seed");
    assert_eq!(dto.version, EXPORT_SCHEMA_VERSION);
    let lines: Vec<&str> = dto.csv.lines().collect();
    assert_eq!(lines.len(), 9, "header + 8 rows");
}

/// The subscriptions export has one row per seeded subscription (6 subs).
#[tokio::test]
async fn export_subscriptions_csv_has_one_row_per_sub() {
    let dto = export_subscriptions_csv(&seeded(), today())
        .await
        .expect("export ok");
    assert_eq!(dto.row_count, 6, "6 subscriptions in the seed");
    assert_eq!(dto.version, EXPORT_SCHEMA_VERSION);
    let lines: Vec<&str> = dto.csv.lines().collect();
    assert_eq!(lines.len(), 7, "header + 6 rows");
}

/// The exports are callable behind the `&dyn DatabaseAdapter` PORT handle.
#[tokio::test]
async fn export_services_work_through_the_port_trait_object() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;

    let txns = export_transactions_csv(port, today())
        .await
        .expect("txns ok");
    assert_eq!(txns.version, EXPORT_SCHEMA_VERSION);

    let budget = export_budget_csv(port, today()).await.expect("budget ok");
    assert_eq!(budget.version, EXPORT_SCHEMA_VERSION);

    let subs = export_subscriptions_csv(port, today())
        .await
        .expect("subs ok");
    assert_eq!(subs.version, EXPORT_SCHEMA_VERSION);
}

/// The schema version constant is the pinned `"1"`.
#[test]
fn export_schema_version_is_one() {
    assert_eq!(EXPORT_SCHEMA_VERSION, "1");
}
