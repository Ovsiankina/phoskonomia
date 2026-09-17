//! Analytics read-model (feature F3).
//!
//! Backs the Analytics page: the multi-cycle spend-trend scope, the per-category
//! momentum small-multiples, the weekday spending-rhythm heatmap, and the GEMMA4
//! "read" insight + soft-cap suggestion. The DTOs mirror
//! `frontend/dioxus-app/src/data/analytics.rs` field-for-field (camelCase keys,
//! money as exact i64 centimes via [`phosk_model::money_centimes`]).
//!
//! Every service fn takes `&dyn DatabaseAdapter` (the PORT) + an `as_of`
//! `NaiveDate`, resolves cycle windows internally, and returns
//! `Result<_, PhoskError>`. The derived-field formulas they implement are
//! documented per fn (build-contract §5.5).
//!
//! Momentum / `deltaPct` / `priorAvg` go through [`crate::momentum`]
//! ([`trailing_avg`](crate::momentum::trailing_avg) +
//! [`delta_pct`](crate::momentum::delta_pct)); the trailing-N is the user's
//! `momentum_baseline_cycles` (default 3).

use chrono::Datelike;
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::Period;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;

use crate::momentum::{delta_pct, trailing_avg};

/// One cycle's point on the spend-trend scope (`SpendHistoryDto::points` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendPointDto {
    /// Month label, e.g. `"JUN"`.
    pub m: String,
    /// Year suffix, e.g. `"26"`.
    pub yr: String,
    /// Cycle total spend, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub spend: Money,
    /// Cycle budget (reference line), exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
    /// Savings rate, 0–1.
    pub rate: f64,
    /// `true` if spend exceeded budget.
    pub over: bool,
    /// `true` for the current in-progress (projected) cycle.
    pub projected: bool,
}

/// `GET /analytics/spend-history` — the 12-cycle trend points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendHistoryDto {
    /// Per-cycle points, oldest → current.
    pub points: Vec<SpendPointDto>,
}

/// A peak/lean cycle reference (`SpendStatsDto::peak`/`low`/`cur`/`prev`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CyclePointDto {
    /// Month label.
    pub m: String,
    /// Year suffix.
    pub yr: String,
    /// Spend for that cycle, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub spend: Money,
    /// Budget for that cycle, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
}

/// `GET /analytics/spend-history/stats` — the trend roll-ups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendStatsDto {
    /// Cycles on record.
    pub months: u32,
    /// 6-month average spend, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub avg: Money,
    /// Average savings rate, 0–1.
    pub avg_rate: f64,
    /// Total saved over the window, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_saved: Money,
    /// This cycle (run-rate).
    pub cur: CyclePointDto,
    /// Previous cycle.
    pub prev: CyclePointDto,
    /// Peak (highest-spend) cycle.
    pub peak: CyclePointDto,
    /// Leanest (lowest-spend) cycle.
    pub low: CyclePointDto,
    /// Signed percent vs 6-mo avg.
    pub cur_vs_avg_pct: i32,
    /// Signed percent vs the previous cycle.
    pub cur_vs_prev_pct: i32,
}

/// One category's momentum card (`GET /analytics/category-momentum` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentumDto {
    /// Category name.
    pub name: String,
    /// This cycle's spend, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub now: Money,
    /// Cap / budget for context, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
    /// 12-point spark.
    pub series: Vec<f64>,
    /// Signed momentum percent vs the trailing-N (default 3) cycle average.
    pub delta_pct: i32,
    /// Trailing-N (default 3) cycle average spend, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub prior_avg: Money,
    /// `true` for a fixed channel.
    pub fixed: bool,
}

/// One weekday bucket (`RhythmDto::weekday` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekdayDto {
    /// Weekday label, e.g. `"MON"`.
    pub d: String,
    /// Average discretionary spend for that weekday (CHF as a chart number).
    pub v: f64,
}

/// The rhythm roll-ups (`RhythmDto::stats`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RhythmStatsDto {
    /// Max weekday value (heatmap denominator).
    pub max: f64,
    /// Total across the week.
    pub total: f64,
    /// Daily average.
    pub avg: f64,
    /// Percent of spend landing Fri–Sun.
    pub weekend_share: i32,
    /// The peak weekday.
    pub peak: WeekdayDto,
}

/// `GET /analytics/rhythm/weekday` — the weekday spend heatmap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RhythmDto {
    /// The 7 weekday buckets, Mon → Sun.
    pub weekday: Vec<WeekdayDto>,
    /// Roll-up stats.
    pub stats: RhythmStatsDto,
}

/// The GEMMA4-suggested soft cap (`AnalyticsInsightDto::suggested_cap`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedCapDto {
    /// Signal the cap applies to.
    pub signal_id: String,
    /// Suggested cap amount, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Projected saved by applying it, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub projected_savings: Money,
}

/// `GET /analytics/insights/movers` — the GEMMA4 "read" + a cap suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsInsightDto {
    /// Model badge.
    pub model: String,
    /// The insight sentence.
    pub text: String,
    /// An actionable cap suggestion.
    pub suggested_cap: SuggestedCapDto,
}

/// The 12-cycle spend/savings history (`GET /analytics/spend-history`).
///
/// Resolves the trailing `cycles` cycle windows ending at the cycle containing
/// `as_of`; for each, sums receipts in-window for `spend`, reads its `budget`,
/// derives `rate` (`saved/budget`), `over` (`spend > budget`), and flags the
/// current in-progress cycle as `projected`.
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution, adapter reads, or
/// checked-arithmetic overflow.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of, cycles))]
pub async fn spend_history(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
    cycles: u32,
) -> Result<SpendHistoryDto, PhoskError> {
    let starts = cycle_starts(as_of, cycles)?;
    let budget = db.budget_config().await?.monthly_budget;

    let mut points = Vec::with_capacity(starts.len());
    let last_idx = starts.len().saturating_sub(1);
    for (i, &start) in starts.iter().enumerate() {
        let window = Period::Month.resolve(start)?;
        let spend = sum_window(db, window.start, window.end).await?;
        // saved = max(0, budget − spend); rate = saved / budget (0..1).
        let saved = budget.checked_sub(spend)?.max(Money::ZERO);
        let rate = ratio(saved.centimes(), budget.centimes());
        points.push(SpendPointDto {
            m: month_label(start.month()).to_owned(),
            yr: year_suffix(start.year()),
            spend,
            budget,
            rate,
            over: spend.centimes() > budget.centimes(),
            // Only the final (current, in-progress) cycle is projected.
            projected: i == last_idx,
        });
    }
    Ok(SpendHistoryDto { points })
}

/// The spend-trend roll-ups (`GET /analytics/spend-history/stats`).
///
/// `avg` = mean spend over the trailing 6 cycles; `cur`/`prev`/`peak`/`low` from
/// the history points; `curVsAvgPct = pct_delta(cur, avg)`,
/// `curVsPrevPct = pct_delta(cur, prev)` with
/// `pct_delta(a,b) = round(100·(a−b)/b)` (0 when `b == 0`).
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution, adapter reads, or
/// checked-arithmetic overflow.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of, cycles))]
pub async fn spend_stats(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
    cycles: u32,
) -> Result<SpendStatsDto, PhoskError> {
    let history = spend_history(db, as_of, cycles).await?;
    let points = &history.points;
    if points.is_empty() {
        return Err(PhoskError::Invalid(
            "spend_stats requires at least one cycle".to_owned(),
        ));
    }

    let cur = cycle_point(points.last().ok_or_else(no_point)?);
    let prev = points
        .get(points.len().wrapping_sub(2))
        .map_or_else(|| cur.clone(), cycle_point);

    // peak = highest-spend cycle, low = leanest. Ties resolve to the first seen.
    let peak = cycle_point(
        points
            .iter()
            .max_by_key(|p| p.spend.centimes())
            .ok_or_else(no_point)?,
    );
    let low = cycle_point(
        points
            .iter()
            .min_by_key(|p| p.spend.centimes())
            .ok_or_else(no_point)?,
    );

    // avg = mean spend over the trailing 6 cycles (or all if fewer).
    let tail_start = points.len().saturating_sub(6);
    let tail = &points[tail_start..];
    let tail_count = i64::try_from(tail.len())
        .map_err(|_| PhoskError::Overflow("cycle count out of range".to_owned()))?;
    let avg_sum = Money::sum(tail.iter().map(|p| p.spend))?;
    let avg = Money::from_centimes(avg_sum.centimes() / tail_count.max(1));

    // avgRate = mean savings rate over the same trailing window (0..1).
    let avg_rate = mean(&tail.iter().map(|p| p.rate).collect::<Vec<_>>());

    // totalSaved = Σ max(0, budget − spend) over the whole window.
    let mut total_saved = Money::ZERO;
    for p in points {
        let saved = p.budget.checked_sub(p.spend)?.max(Money::ZERO);
        total_saved = total_saved.checked_add(saved)?;
    }

    let cur_vs_avg_pct = pct_delta(cur.spend, avg);
    let cur_vs_prev_pct = pct_delta(cur.spend, prev.spend);

    Ok(SpendStatsDto {
        months: u32::try_from(points.len())
            .map_err(|_| PhoskError::Overflow("cycle count out of range".to_owned()))?,
        avg,
        avg_rate,
        total_saved,
        cur,
        prev,
        peak,
        low,
        cur_vs_avg_pct,
        cur_vs_prev_pct,
    })
}

/// Per-category momentum cards (`GET /analytics/category-momentum`).
///
/// For each category: `now` = current-cycle spend, `series` = 12-cycle spark,
/// `prior_avg` = [`trailing_avg`](crate::momentum::trailing_avg) over the prior
/// cycles (N from `momentum_baseline_cycles`, default 3), `delta_pct` from
/// [`delta_pct`](crate::momentum::delta_pct) over `now` and `prior_avg`,
/// `fixed` from the category cap.
///
/// # Errors
/// Propagates any [`PhoskError`] from adapter reads or checked-arithmetic overflow.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn category_momentum(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<Vec<MomentumDto>, PhoskError> {
    let n = momentum_baseline_cycles(db).await?;
    let caps = db.category_caps().await?;

    // Current-cycle spend per category, from this cycle's receipts.
    let window = Period::Month.resolve(as_of)?;
    let receipts = db.receipts_between(window.start, window.end).await?;

    let mut cards = Vec::with_capacity(caps.len());
    for cap in caps {
        // `now` = Σ this cycle's receipts in this category.
        let now = Money::sum(
            receipts
                .iter()
                .filter(|r| r.category == cap.name)
                .map(|r| r.amount),
        )?;

        // Prior-cycle spends (oldest → newest) drive the trailing-N average and
        // the 12-point spark.
        let history = db.budget_history(&cap.name).await?;
        let prior: Vec<Money> = history.iter().map(|h| h.spent).collect();
        let prior_avg = trailing_avg(&prior, n)?;
        let delta = delta_pct(now, prior_avg);

        // 12-point spark: the prior cycles plus `now`, conformed to 12 points
        // (CHF as a chart number — render-only, never recomputed into money).
        let series = spark_series(&prior, now);

        // `budget` for context = the cap (0 when unlimited).
        let budget = cap.cap.unwrap_or(Money::ZERO);

        cards.push(MomentumDto {
            name: cap.name,
            now,
            budget,
            series,
            delta_pct: delta,
            prior_avg,
            fixed: cap.fixed,
        });
    }
    Ok(cards)
}

/// The weekday spending rhythm (`GET /analytics/rhythm/weekday`).
///
/// Buckets discretionary spend into the 7 weekdays (Mon → Sun);
/// `stats.weekend_share = round(100·(Fri+Sat+Sun)/total)`; `max`/`total`/`avg`/`peak`
/// from the buckets.
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution or adapter reads.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn weekday_rhythm(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<RhythmDto, PhoskError> {
    let window = Period::Month.resolve(as_of)?;
    let receipts = db.receipts_between(window.start, window.end).await?;

    // Discretionary spend (non-fixed receipts) bucketed Mon→Sun, in CHF (a
    // render-only chart number, never recomputed into money).
    let labels = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];
    let mut buckets = [0.0_f64; 7];
    for r in &receipts {
        if r.fixed {
            continue;
        }
        let idx = r.date.weekday().num_days_from_monday() as usize;
        if let Some(slot) = buckets.get_mut(idx) {
            *slot += r.amount.as_chf_f64();
        }
    }

    let weekday: Vec<WeekdayDto> = labels
        .iter()
        .zip(buckets.iter())
        .map(|(d, &v)| WeekdayDto {
            d: (*d).to_owned(),
            v,
        })
        .collect();

    let total: f64 = buckets.iter().sum();
    let max = buckets.iter().copied().fold(0.0_f64, f64::max);
    let avg = total / 7.0;
    // weekend_share = round(100·(Fri+Sat+Sun)/total); guarded against an empty week.
    let weekend = buckets[4] + buckets[5] + buckets[6];
    let weekend_share = if total == 0.0 {
        0
    } else {
        round_pct(100.0 * weekend / total)
    };
    // peak = the bucket carrying the max value (first such bucket).
    let peak = weekday
        .iter()
        .find(|w| (w.v - max).abs() < f64::EPSILON)
        .cloned()
        .unwrap_or_else(|| WeekdayDto {
            d: labels[0].to_owned(),
            v: max,
        });

    Ok(RhythmDto {
        weekday,
        stats: RhythmStatsDto {
            max,
            total,
            avg,
            weekend_share,
            peak,
        },
    })
}

/// The GEMMA4 "read" + soft-cap suggestion (`GET /analytics/insights/movers`).
///
/// Composes the narrative insight (seed/canned for now) with an actionable
/// [`SuggestedCapDto`] over the top-rising signal.
///
/// # Errors
/// Propagates any [`PhoskError`] from adapter reads.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn analytics_insight(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<AnalyticsInsightDto, PhoskError> {
    // The top-rising momentum card drives the cap suggestion: cap at the trailing
    // baseline (priorAvg) and project the over-run (now − baseline) as the saving.
    let cards = category_momentum(db, as_of).await?;
    let top = cards
        .iter()
        .filter(|c| !c.fixed)
        .max_by_key(|c| c.delta_pct)
        .or_else(|| cards.first());

    let (signal_id, amount, projected_savings, name) = match top {
        Some(c) => {
            let cap = if c.prior_avg.centimes() > 0 {
                c.prior_avg
            } else {
                c.now
            };
            let savings = c.now.checked_sub(cap)?.max(Money::ZERO);
            (slugify(&c.name), cap, savings, c.name.clone())
        }
        None => (
            "spend".to_owned(),
            Money::ZERO,
            Money::ZERO,
            "spend".to_owned(),
        ),
    };

    Ok(AnalyticsInsightDto {
        model: "GEMMA4".to_owned(),
        text: format!(
            "{name} is your fastest-rising channel this cycle — a soft cap at its recent baseline would protect your savings."
        ),
        suggested_cap: SuggestedCapDto {
            signal_id,
            amount,
            projected_savings,
        },
    })
}

// ── internal helpers ──────────────────────────────────────────────────────────

/// The cycle starts (oldest → newest) of the trailing `cycles` calendar months
/// ending at the cycle containing `as_of`.
fn cycle_starts(
    as_of: chrono::NaiveDate,
    cycles: u32,
) -> Result<Vec<chrono::NaiveDate>, PhoskError> {
    let current = Period::Month.resolve(as_of)?.start;
    let mut starts = Vec::with_capacity(cycles as usize);
    // Absolute month index of the current cycle (year·12 + month-1), in i32.
    let month0 = i32::try_from(current.month())
        .map_err(|_| PhoskError::Overflow("month out of range".to_owned()))?
        - 1;
    let current_idx = current
        .year()
        .checked_mul(12)
        .and_then(|y| y.checked_add(month0))
        .ok_or_else(|| PhoskError::Overflow("cycle index out of range".to_owned()))?;
    for back in (0..cycles).rev() {
        // Step `back` whole calendar months before the current cycle start.
        let offset = i32::try_from(back)
            .map_err(|_| PhoskError::Overflow(format!("cycle offset {back} out of range")))?;
        let total = current_idx - offset;
        let year = total.div_euclid(12);
        let month = u32::try_from(total.rem_euclid(12) + 1)
            .map_err(|_| PhoskError::Overflow("month out of range".to_owned()))?;
        let start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .ok_or_else(|| PhoskError::InvalidDate(format!("cycle start {year}-{month:02}")))?;
        starts.push(start);
    }
    Ok(starts)
}

/// Exact sum of every transaction amount in the inclusive `[from, to]` window.
async fn sum_window(
    db: &dyn DatabaseAdapter,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<Money, PhoskError> {
    let txns = db.transactions_between(from, to).await?;
    Money::sum(txns.into_iter().map(|tx| tx.amount))
}

/// The trailing-N momentum baseline (`momentum_baseline_cycles`), default 3.
///
/// Read straight from the preference store; a missing key or a non-numeric value
/// falls back to the documented default of 3 (build-contract §6) rather than
/// erroring.
async fn momentum_baseline_cycles(db: &dyn DatabaseAdapter) -> Result<u32, PhoskError> {
    match db.preference("momentum_baseline_cycles").await {
        Ok(pref) => Ok(pref.value.trim().parse::<u32>().unwrap_or(3)),
        Err(PhoskError::NotFound(_)) => Ok(3),
        Err(e) => Err(e),
    }
}

/// Project one history point onto the leaner [`CyclePointDto`].
fn cycle_point(p: &SpendPointDto) -> CyclePointDto {
    CyclePointDto {
        m: p.m.clone(),
        yr: p.yr.clone(),
        spend: p.spend,
        budget: p.budget,
    }
}

/// `pct_delta(a, b) = round(100·(a−b)/b)`; 0 when `b == 0` (build-contract §5.5).
fn pct_delta(a: Money, b: Money) -> i32 {
    // Reuse the single momentum formula: round(100·(a−b)/b) is exactly delta_pct.
    delta_pct(a, b)
}

/// A consistent "no point" error for the unreachable empty-slice branches (the
/// callers guard non-emptiness first; this keeps them panic-free).
fn no_point() -> PhoskError {
    PhoskError::Invalid("empty spend history".to_owned())
}

/// The three-letter uppercase month label for a 1-based month number.
fn month_label(month: u32) -> &'static str {
    const LABELS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    LABELS
        .get((month as usize).wrapping_sub(1))
        .copied()
        .unwrap_or("???")
}

/// Two-digit year suffix, e.g. 2026 → `"26"`.
fn year_suffix(year: i32) -> String {
    format!("{:02}", year.rem_euclid(100))
}

/// A 12-point spark of CHF chart numbers: the prior-cycle spends followed by the
/// current spend, conformed to exactly 12 points (left-padded with the earliest
/// value, or zero when empty). Render-only — never recomputed into money.
fn spark_series(prior: &[Money], now: Money) -> Vec<f64> {
    let mut raw: Vec<f64> = prior.iter().map(|m| m.as_chf_f64()).collect();
    raw.push(now.as_chf_f64());
    conform_12(&raw)
}

/// Conform a series of f64 chart points to exactly 12 elements: keep the trailing
/// 12 if longer; left-pad with the first value (or 0.0 when empty) if shorter.
fn conform_12(series: &[f64]) -> Vec<f64> {
    const N: usize = 12;
    if series.len() >= N {
        return series[series.len() - N..].to_vec();
    }
    let pad = series.first().copied().unwrap_or(0.0);
    let mut out = vec![pad; N - series.len()];
    out.extend_from_slice(series);
    out
}

/// `numerator / denominator` as an f64 0–1 ratio, or 0.0 when the denominator is
/// 0. A unitless proportion (savings rate), never money.
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

/// Arithmetic mean of an f64 slice, or `0.0` when empty (a unitless ratio such as
/// the average savings rate; never money).
#[allow(
    clippy::cast_precision_loss,
    reason = "dividing by a small element count to average unitless ratios"
)]
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// `round(value)` of an f64 percentage to a saturating `i32` (half away from
/// zero). The input is a small unitless percent, never money.
#[allow(
    clippy::cast_possible_truncation,
    reason = "rounding a small integer percentage; saturating cast"
)]
const fn round_pct(value: f64) -> i32 {
    value.round() as i32
}

/// Lowercase, hyphenated slug of a category name (the suggestion's `signalId`).
fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}
