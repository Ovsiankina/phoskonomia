//! `phosk_insights` — the **dashboard read-model gateway** (ADR-002 L5/L6).
//!
//! This crate is the service that backs the three "current cycle" dashboard
//! reads. It does no new finance arithmetic of its own: it *composes* the two
//! lower bounded-context services into the serde-`Serialize` DTOs the HTTP edge
//! returns verbatim, with the JSON shape the React frontend consumes.
//!
//! - [`dashboard_totals`] → `GET /cycle/current/totals` — wraps
//!   [`phosk_planning::totals`] as [`TotalsDto`].
//! - [`spend_series`] → `GET /cycle/current/spend-series` — derives the
//!   `cumulative[]` / `pace[]` (and, on request, the prior cycle's
//!   `lastCycleCumulative[]`) from [`phosk_ledger::daily_spend`] as
//!   [`SpendSeriesDto`].
//! - [`top_shops`] → `GET /cycle/current/top-shops` — turns
//!   [`phosk_ledger::top_shops`] into [`TopShopsDto`], computing each shop's
//!   `share` and the `maxTotal` denominator.
//!
//! **Layering (ADR-010).** Every service fn takes `&dyn DatabaseAdapter` (the
//! PORT) and resolves the current cycle ([`Period::Month`]) for an `as_of` date
//! internally, so the HTTP edge passes only "today". It depends on the PORT
//! trait crate and the two feature services — never on a concrete adapter
//! (`phosk_db_memory` is a *dev*-dependency, tests only). A technology swap is a
//! new adapter `impl`, never a change here.
//!
//! **Money is exact centimes (ADR-010, centimes-everywhere).** Every money
//! field serializes as its lossless i64 centime count, via
//! [`phosk_model::money_centimes`] / [`phosk_model::opt_money_centimes`] (and
//! local centime-vec serializers for the series). CHF `f64` is render-only
//! (`Money::as_chf_f64`) and never appears on the wire. There is no
//! `unwrap`/`expect`/`panic!` in this code: every fallible step maps explicitly
//! to a [`PhoskError`].
//!
//! **Units (pinned to the frontend consumers).** `spentPct` is a `0–100`
//! integer; `savingsRate` is a `0–1` ratio; `vsLastCyclePct` is a signed integer
//! percent; every money field and array element is exact i64 centimes;
//! `todayIndex` is the 0-based index of `as_of`; `share` is a `0–1` proportion
//! and `maxTotal` the centime amount the bar widths divide by.
//!
//! [`Money`]: phosk_core::money::Money
//! [`Period::Month`]: phosk_core::cycle::Period::Month
//! [`PhoskError`]: phosk_core::error::PhoskError

pub mod analytics;
pub mod exports;
pub mod momentum;

use serde::{Serialize, Serializer};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::{CycleWindow, Period};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_planning::CycleTotals;

/// Serialize a `Vec<Money>` as a JSON array of exact i64 centimes — the
/// centime-vec analogue of [`phosk_model::money_centimes`], which only covers a
/// scalar `Money`. No CHF float ever enters the wire form (centimes-everywhere).
fn money_vec_centimes<S: Serializer>(v: &[Money], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(v.iter().map(|m| m.centimes()))
}

/// The headline cycle KPIs, `GET /cycle/current/totals`.
///
/// A thin serialization view over [`phosk_planning::CycleTotals`]: every money
/// field becomes exact i64 centimes; the three pre-computed unitless fields
/// (`savingsRate`, `spentPct`, `vsLastCyclePct`) pass straight through in the
/// units the frontend expects. Keys are the contract's camelCase.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotalsDto {
    /// Cycle budget ceiling, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
    /// Total spent so far this cycle, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub spent: Money,
    /// `budget − spent`, exact i64 centimes (negative if overspent).
    #[serde(with = "phosk_model::money_centimes")]
    pub remaining: Money,
    /// Sum of all category budget caps, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub allocated: Money,
    /// Savings target for the cycle, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub savings_target: Money,
    /// Savings realised so far (`max(0, budget − spent)`), exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub saved: Money,
    /// Projected savings at the current run-rate, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub savings_projected: Money,
    /// `saved / budget` as a `0–1` ratio (the frontend's `pct()` ×100s it).
    pub savings_rate: f64,
    /// Previous cycle's total spend, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub last_cycle_spent: Money,
    /// `round(100 · spent / budget)`, integer `0–100` (frontend divides by 100).
    pub spent_pct: i32,
    /// `round(100 · (spent − lastCycleSpent) / lastCycleSpent)`, signed percent.
    pub vs_last_cycle_pct: i32,
    /// `remaining / daysLeft`, exact i64 centimes (the daily spend to stay on budget).
    #[serde(with = "phosk_model::money_centimes")]
    pub per_day_to_stay_on_budget: Money,
}

impl From<CycleTotals> for TotalsDto {
    fn from(t: CycleTotals) -> Self {
        Self {
            budget: t.budget,
            spent: t.spent,
            remaining: t.remaining,
            allocated: t.allocated,
            savings_target: t.savings_target,
            saved: t.saved,
            savings_projected: t.savings_projected,
            savings_rate: t.savings_rate,
            last_cycle_spent: t.last_cycle_spent,
            spent_pct: t.spent_pct,
            vs_last_cycle_pct: t.vs_last_cycle_pct,
            per_day_to_stay_on_budget: t.per_day_to_stay_on_budget,
        }
    }
}

/// The spend-over-time series, `GET /cycle/current/spend-series`.
///
/// All arrays have length [`CycleWindow::len_days`] (the current cycle). `daily`
/// is the per-day spend; `cumulative` its running sum; `pace` the ideal straight
/// line to the full budget; `lastCycleCumulative` the prior cycle's running sum
/// (present only when `?compare=lastCycle`), aligned to this cycle's length.
/// `todayIndex` is the 0-based position of `as_of`. Every element is exact i64 centimes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendSeriesDto {
    /// Per-day spend, exact i64 centimes, length = days in cycle.
    #[serde(serialize_with = "money_vec_centimes")]
    pub daily: Vec<Money>,
    /// Cumulative spend through each day, exact i64 centimes.
    #[serde(serialize_with = "money_vec_centimes")]
    pub cumulative: Vec<Money>,
    /// Ideal cumulative if the budget is spent evenly, exact i64 centimes.
    #[serde(serialize_with = "money_vec_centimes")]
    pub pace: Vec<Money>,
    /// Prior cycle's cumulative, exact i64 centimes; `None` unless comparing.
    #[serde(serialize_with = "opt_money_vec_centimes")]
    pub last_cycle_cumulative: Option<Vec<Money>>,
    /// 0-based index of `as_of` within the cycle.
    pub today_index: usize,
}

/// Serialize an `Option<Vec<Money>>` as JSON `null` or an array of exact i64 centimes.
// The `&Option<…>` is dictated by serde's `serialize_with` contract (`fn(&T, S)`),
// where `T` is the field type `Option<Vec<Money>>`; `Option<&T>` is not an option.
#[allow(
    clippy::ref_option,
    reason = "signature fixed by serde's serialize_with contract"
)]
fn opt_money_vec_centimes<S: Serializer>(v: &Option<Vec<Money>>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(v) => money_vec_centimes(v, s),
        None => s.serialize_none(),
    }
}

/// One shop's slice of the cycle, an element of [`TopShopsDto::shops`].
#[derive(Debug, Clone, Serialize)]
pub struct ShopShareDto {
    /// Shop display name (ADR-008 identity).
    pub shop: String,
    /// Total spent at this shop this cycle, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
    /// `total / Σ(all shop totals)` as a `0–1` proportion.
    pub share: f64,
}

/// The cycle's top shops, `GET /cycle/current/top-shops`.
///
/// `shops` is ranked by `total` descending (ties on name ascending); `maxTotal`
/// is the largest shop total, the denominator the frontend divides bar widths by.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopShopsDto {
    /// The ranked shops with their `share`s.
    pub shops: Vec<ShopShareDto>,
    /// Largest shop total this cycle, exact i64 centimes (bar-width denominator).
    #[serde(with = "phosk_model::money_centimes")]
    pub max_total: Money,
}

/// Resolve the current cycle window for `as_of`: the calendar month it falls in.
fn current_cycle(as_of: chrono::NaiveDate) -> Result<CycleWindow, PhoskError> {
    Period::Month.resolve(as_of)
}

/// The headline KPIs for the cycle containing `as_of` (`GET /cycle/current/totals`).
///
/// Resolves [`Period::Month`] for `as_of`, delegates to [`phosk_planning::totals`],
/// and wraps the result as the CHF-number [`TotalsDto`].
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution or the planning service
/// (adapter reads, checked-arithmetic overflow).
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn dashboard_totals(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<TotalsDto, PhoskError> {
    let window = current_cycle(as_of)?;
    let totals = phosk_planning::totals(db, window).await?;
    tracing::debug!("composed dashboard totals DTO");
    Ok(TotalsDto::from(totals))
}

/// The spend-over-time series for the cycle containing `as_of`
/// (`GET /cycle/current/spend-series`).
///
/// `daily` comes from [`phosk_ledger::daily_spend`]; `cumulative` is its running
/// sum; `pace[i] = budget / (len − 1) · i` (the even-spend ideal, flat at
/// `budget` for a single-day cycle); `todayIndex = day_index − 1`. When
/// `compare_last_cycle` is set, `lastCycleCumulative` is the previous calendar
/// month's cumulative spend, length-aligned to this cycle (trailing extra days
/// dropped, a shorter prior cycle padded with its final value) so all arrays
/// share the contract's single length.
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution, the ledger/planning
/// reads, or checked-arithmetic overflow in the running sums / pace.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of, compare_last_cycle))]
pub async fn spend_series(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
    compare_last_cycle: bool,
) -> Result<SpendSeriesDto, PhoskError> {
    let window = current_cycle(as_of)?;
    let len = window.len_days() as usize;

    let daily = phosk_ledger::daily_spend(db, window).await?;
    let cumulative = running_sum(&daily)?;

    // pace[i] = budget / (len − 1) · i: the straight line from 0 on day 1 to the
    // full budget on the last day. A single-day cycle (len == 1) has no slope, so
    // pace is just the budget on that one day.
    let budget = db.budget_config().await?.monthly_budget;
    let pace = pace_series(budget, len)?;

    let last_cycle_cumulative = if compare_last_cycle {
        Some(last_cycle_cumulative(db, window, len).await?)
    } else {
        None
    };

    // todayIndex is the 0-based index of as_of; day_index is 1-based and >= 1.
    let today_index = (window.day_index() as usize).saturating_sub(1);

    tracing::debug!(len, today_index, "composed spend-series DTO");
    Ok(SpendSeriesDto {
        daily,
        cumulative,
        pace,
        last_cycle_cumulative,
        today_index,
    })
}

/// The cycle's top shops with shares, `GET /cycle/current/top-shops`.
///
/// Delegates to [`phosk_ledger::top_shops`] (ranked, truncated to `limit`), then
/// computes each `share = total / Σ(all shop totals)` and the `maxTotal`. The
/// share denominator is the sum over the **returned** shops; with a generous
/// `limit` that is the whole cycle, matching the frontend's reading of `share`.
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution, the ledger read, or the
/// checked sum of the shop totals.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of, limit))]
pub async fn top_shops(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
    limit: usize,
) -> Result<TopShopsDto, PhoskError> {
    let window = current_cycle(as_of)?;
    let ranked = phosk_ledger::top_shops(db, window, limit).await?;

    // maxTotal is the largest total; ranked is sorted descending, so it is the
    // first element (zero when there are no shops).
    let max_total = ranked.first().map_or(Money::ZERO, |s| s.total);

    // share denominator: the checked sum of every returned shop's total. Zero
    // when empty — guarded so the division never produces NaN.
    let grand_total = Money::sum(ranked.iter().map(|s| s.total))?;
    let denom = grand_total.centimes();

    let shops = ranked
        .into_iter()
        .map(|s| ShopShareDto {
            share: ratio(s.total.centimes(), denom),
            shop: s.shop,
            total: s.total,
        })
        .collect();

    Ok(TopShopsDto { shops, max_total })
}

/// Running (cumulative) sum of a money series; element `i` is the checked sum of
/// `daily[0..=i]`. Length matches the input; overflow surfaces a [`PhoskError`].
fn running_sum(daily: &[Money]) -> Result<Vec<Money>, PhoskError> {
    let mut cumulative = Vec::with_capacity(daily.len());
    let mut acc = Money::ZERO;
    for &m in daily {
        acc = acc.checked_add(m)?;
        cumulative.push(acc);
    }
    Ok(cumulative)
}

/// The budget-pace series: `pace[i] = budget / (len − 1) · i`, the ideal
/// cumulative if the budget is spent evenly across the cycle. A single-day cycle
/// (`len <= 1`) has the full budget on its one day. Integer-centime arithmetic;
/// no float money.
fn pace_series(budget: Money, len: usize) -> Result<Vec<Money>, PhoskError> {
    if len == 0 {
        return Ok(Vec::new());
    }
    if len == 1 {
        return Ok(vec![budget]);
    }
    // `len` is a cycle's day count (~28–366), far inside i64; the conversion is
    // fallible only in principle, and we surface that rather than cast-wrap (ADR §0).
    let span = i64::try_from(len - 1)
        .map_err(|_| PhoskError::Overflow(format!("cycle length {len} out of range")))?;
    let budget_c = budget.centimes();
    let mut pace = Vec::with_capacity(len);
    for i in 0..len {
        let i_c = i64::try_from(i)
            .map_err(|_| PhoskError::Overflow(format!("day index {i} out of range")))?;
        // budget_c · i / span, computed as i64 centimes; the multiply is checked
        // so an absurd budget can never wrap into a wrong-signed pace (ADR §0).
        let scaled = budget_c.checked_mul(i_c).ok_or_else(|| {
            PhoskError::Overflow(format!("pacing {budget} over {len} days at day {i}"))
        })?;
        pace.push(Money::from_centimes(scaled / span));
    }
    Ok(pace)
}

/// The previous calendar month's cumulative spend, aligned to `len` days.
///
/// The prior cycle is the [`Period::Month`] containing the day before
/// `window.start`. Its own daily series (its natural length) is summed
/// cumulatively, then conformed to `len`: a longer prior cycle is truncated to
/// its first `len` days, a shorter one padded with its final cumulative value so
/// the frontend's comparison line spans the whole current cycle.
async fn last_cycle_cumulative(
    db: &dyn DatabaseAdapter,
    window: CycleWindow,
    len: usize,
) -> Result<Vec<Money>, PhoskError> {
    // The day before this cycle begins; its calendar month is the prior cycle.
    // pred_opt guards the lower date edge explicitly (ADR §0 — never drop a None).
    let prev_day = window
        .start
        .pred_opt()
        .ok_or_else(|| PhoskError::InvalidDate(format!("no day before {}", window.start)))?;
    let prev = Period::Month.resolve(prev_day)?;

    let prev_daily = phosk_ledger::daily_spend(db, prev).await?;
    let prev_cum = running_sum(&prev_daily)?;

    Ok(conform_len(&prev_cum, len))
}

/// Conform a cumulative series to exactly `len` elements: truncate if longer, pad
/// with the final value (or [`Money::ZERO`] when empty) if shorter. Padding with
/// the last cumulative value keeps the comparison line flat past the prior
/// cycle's end rather than dropping back to zero.
fn conform_len(series: &[Money], len: usize) -> Vec<Money> {
    let mut out: Vec<Money> = series.iter().copied().take(len).collect();
    let pad = out.last().copied().unwrap_or(Money::ZERO);
    out.resize(len, pad);
    out
}

/// `numerator / denominator` as an `f64` `0–1` ratio, or `0.0` when the
/// denominator is `0`. A unitless proportion (a shop's `share`), never money.
// The cast yields a fraction the frontend renders as a proportion; an f64 loss of
// precision on these centime magnitudes is far below display resolution, and no
// money value is reconstructed from it (ADR §0 — float never feeds money math).
#[allow(
    clippy::cast_precision_loss,
    reason = "computing a unitless 0–1 ratio, not an amount; never reused as money"
)]
fn ratio(numerator: i64, denominator: i64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::NaiveDate;
    use phosk_db_memory::MemoryDb;
    use serde_json::{Value, json};

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

    // ── totals ────────────────────────────────────────────────────────────────

    /// The totals DTO serializes to the exact camelCase contract with exact i64
    /// centime money and the pinned seed values (June through `as_of` 2026-06-18).
    #[tokio::test]
    async fn totals_json_matches_contract_shape_and_seed() {
        let dto = dashboard_totals(&seeded(), today())
            .await
            .expect("totals ok");
        let v = to_json(&dto);

        // Money fields are exact i64 centimes (not CHF floats, not strings).
        assert_eq!(v["budget"], json!(420_000), "budget CHF 4200.00");
        assert_eq!(v["spent"], json!(316_180), "June spend-to-date CHF 3161.80");
        assert_eq!(v["remaining"], json!(103_820), "remaining CHF 1038.20");
        assert_eq!(v["allocated"], json!(426_000), "Σ caps CHF 4260.00");
        assert_eq!(
            v["savingsTarget"],
            json!(90_000),
            "savings target CHF 900.00"
        );
        assert_eq!(v["saved"], json!(103_820), "saved CHF 1038.20");
        assert_eq!(v["savingsProjected"], json!(0), "run-rate overshoots");
        assert_eq!(v["lastCycleSpent"], json!(378_770), "May total CHF 3787.70");
        assert_eq!(v["perDayToStayOnBudget"], json!(8651), "CHF 86.51/day");

        // Unitless fields pass through in their frontend units.
        assert_eq!(v["spentPct"], json!(75), "0–100 integer");
        assert_eq!(v["vsLastCyclePct"], json!(-17), "signed integer percent");
        let rate = v["savingsRate"].as_f64().expect("number");
        assert!(
            (rate - 1038.20 / 4200.0).abs() < 1e-9,
            "savingsRate 0–1 ratio, got {rate}"
        );
        assert!((0.0..=1.0).contains(&rate), "savingsRate stays in 0..=1");
    }

    /// Exactly the contract's camelCase keys are present, no `snake_case` leakage.
    #[tokio::test]
    async fn totals_json_keys_are_camel_case() {
        let dto = dashboard_totals(&seeded(), today())
            .await
            .expect("totals ok");
        let v = to_json(&dto);
        let obj = v.as_object().expect("totals is a JSON object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut expected = [
            "budget",
            "spent",
            "remaining",
            "allocated",
            "savingsTarget",
            "saved",
            "savingsProjected",
            "savingsRate",
            "lastCycleSpent",
            "spentPct",
            "vsLastCyclePct",
            "perDayToStayOnBudget",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected, "exact camelCase contract keys");
    }

    // ── spend-series ────────────────────────────────────────────────────────────

    /// Without `compare`, `lastCycleCumulative` is JSON `null`; all other arrays
    /// have length = days in cycle (30 for June), and `todayIndex` is 0-based 17.
    #[tokio::test]
    async fn spend_series_without_compare_omits_last_cycle() {
        let dto = spend_series(&seeded(), today(), false)
            .await
            .expect("series ok");
        let v = to_json(&dto);

        assert_eq!(v["daily"].as_array().expect("array").len(), 30);
        assert_eq!(v["cumulative"].as_array().expect("array").len(), 30);
        assert_eq!(v["pace"].as_array().expect("array").len(), 30);
        assert_eq!(v["todayIndex"], json!(17), "0-based index of day 18");
        assert_eq!(
            v["lastCycleCumulative"],
            Value::Null,
            "no comparison without ?compare=lastCycle"
        );
    }

    /// With `compare`, `lastCycleCumulative` is present and length-aligned to the
    /// current cycle (30), and its final value is the prior cycle's full spend.
    #[tokio::test]
    async fn spend_series_with_compare_includes_aligned_last_cycle() {
        let dto = spend_series(&seeded(), today(), true)
            .await
            .expect("series ok");
        let v = to_json(&dto);

        let last = v["lastCycleCumulative"]
            .as_array()
            .expect("last-cycle present when comparing");
        assert_eq!(last.len(), 30, "aligned to the current cycle length");
        // May (31 days) is longer than June (30); truncating to 30 days drops only
        // May 31, which carried no seeded spend, so the final value is May's full
        // cumulative total, CHF 3787.70.
        assert_eq!(
            last[29],
            json!(378_770),
            "prior-cycle line ends at May's total"
        );
        // Monotonic non-decreasing — it is a cumulative curve.
        let nums: Vec<i64> = last.iter().map(|x| x.as_i64().expect("centimes")).collect();
        assert!(
            nums.windows(2).all(|w| w[0] <= w[1]),
            "cumulative never decreases"
        );
    }

    /// `daily` is the seed's per-day spend as exact i64 centimes; `cumulative` is
    /// its running sum, ending at June's full-window total CHF 3222.45.
    #[tokio::test]
    async fn spend_series_daily_and_cumulative_match_the_seed() {
        let dto = spend_series(&seeded(), today(), false)
            .await
            .expect("series ok");
        let v = to_json(&dto);

        // Day 1 (index 0): Migros 58.75 + rent 1680.00 + insurance 318.00.
        assert_eq!(v["daily"][0], json!(205_675), "June 1 spend");
        // Day 16 (index 15): Galaxus 129.90 + Restaurant Linde 64.50.
        assert_eq!(v["daily"][15], json!(19_440), "June 16 spend");
        // Days 20–30 (indices 19..30) carry no seeded spend.
        assert_eq!(v["daily"][19], json!(0), "June 20 has no spend");

        // Cumulative: index 0 == daily[0]; the final index == the cycle total.
        assert_eq!(v["cumulative"][0], json!(205_675), "cumulative day 1");
        assert_eq!(
            v["cumulative"][29],
            json!(322_245),
            "cumulative ends at June full-window total"
        );
    }

    /// `pace[i] = budget / (len − 1) · i`: 0 on day 1, the full budget on the last
    /// day, monotonic in between.
    #[tokio::test]
    async fn spend_series_pace_is_even_spend_line() {
        let dto = spend_series(&seeded(), today(), false)
            .await
            .expect("series ok");
        let v = to_json(&dto);
        let pace = v["pace"].as_array().expect("array");

        assert_eq!(pace[0], json!(0), "pace starts at zero");
        // budget 420_000 c / 29 = 14_482 c (integer-centime) → CHF 144.82 on day 2.
        assert_eq!(pace[1], json!(14_482), "even daily step");
        // Day 30 (index 29): 420_000 · 29 / 29 = 420_000 c → CHF 4200.00.
        assert_eq!(pace[29], json!(420_000), "pace reaches the full budget");
    }

    /// `todayIndex` tracks `as_of`: day 1 → 0, last day → len − 1.
    #[tokio::test]
    async fn spend_series_today_index_tracks_as_of() {
        let first = spend_series(&seeded(), naive(2026, 6, 1), false)
            .await
            .expect("series ok");
        assert_eq!(first.today_index, 0, "first day of cycle → index 0");

        let last = spend_series(&seeded(), naive(2026, 6, 30), false)
            .await
            .expect("series ok");
        assert_eq!(last.today_index, 29, "last day of June → index 29");
    }

    // ── top-shops ────────────────────────────────────────────────────────────────

    /// The top-shops DTO serializes to the contract shape: a `shops` array of
    /// `{shop,total,share}` plus a `maxTotal` centime amount.
    #[tokio::test]
    async fn top_shops_json_matches_contract_shape() {
        let dto = top_shops(&seeded(), today(), 5).await.expect("shops ok");
        let v = to_json(&dto);

        let shops = v["shops"].as_array().expect("shops array");
        assert_eq!(shops.len(), 5, "limit of 5");

        // Highest spender is Landlord (rent CHF 1680.00); maxTotal matches it.
        assert_eq!(shops[0]["shop"], json!("Landlord"));
        assert_eq!(shops[0]["total"], json!(168_000));
        assert_eq!(v["maxTotal"], json!(168_000), "largest shop total");

        // The top shop's share == total/grandTotal; the first share is the largest.
        let first_share = shops[0]["share"].as_f64().expect("number");
        assert!((0.0..=1.0).contains(&first_share), "share is a 0–1 ratio");
    }

    /// `share` is `total / Σ(returned totals)`; with a generous limit that is the
    /// whole cycle, so the shares sum to 1.0.
    #[tokio::test]
    async fn top_shops_shares_sum_to_one_over_full_cycle() {
        let dto = top_shops(&seeded(), today(), 1000).await.expect("shops ok");
        let v = to_json(&dto);
        let shops = v["shops"].as_array().expect("array");
        assert_eq!(shops.len(), 20, "June has 20 distinct shops");

        let sum: f64 = shops
            .iter()
            .map(|s| s["share"].as_f64().expect("number"))
            .sum();
        assert!((sum - 1.0).abs() < 1e-9, "shares sum to 1, got {sum}");

        // Landlord's share = 1680.00 / 3222.45 (the full June total).
        let landlord = shops[0]["share"].as_f64().expect("number");
        assert!(
            (landlord - 1680.0 / 3222.45).abs() < 1e-6,
            "Landlord share = total/grandTotal"
        );
    }

    /// An empty cycle yields no shops, `maxTotal` zero, and no NaN shares — never
    /// a divide-by-zero on the empty grand total.
    #[tokio::test]
    async fn top_shops_empty_cycle_is_safe() {
        // April 2026 has no seeded transactions.
        let dto = top_shops(&seeded(), naive(2026, 4, 15), 10)
            .await
            .expect("shops ok");
        let v = to_json(&dto);
        assert!(v["shops"].as_array().expect("array").is_empty());
        assert_eq!(v["maxTotal"], json!(0), "no shops → zero maxTotal");
    }

    /// Every service is callable behind the `&dyn DatabaseAdapter` PORT handle
    /// (ADR-010) — the read-model never sees the concrete `MemoryDb` type.
    #[tokio::test]
    async fn services_work_through_the_port_trait_object() {
        let db = seeded();
        let port: &dyn DatabaseAdapter = &db;

        let totals = dashboard_totals(port, today()).await.expect("totals ok");
        assert_eq!(totals.spent.centimes(), 316_180);

        let series = spend_series(port, today(), true).await.expect("series ok");
        assert_eq!(series.daily.len(), 30);

        let shops = top_shops(port, today(), 3).await.expect("shops ok");
        assert_eq!(shops.shops.len(), 3);
    }
}
