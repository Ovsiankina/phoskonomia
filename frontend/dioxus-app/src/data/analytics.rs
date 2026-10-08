//! Analytics read-model (F3).
//!
//! Backs the Analytics page: the multi-cycle spend-trend scope, category
//! momentum small-multiples, the weekday spending-rhythm heatmap and the computed
//! "read" insight. Item-signals/movers come from [`crate::data::signals`].
//! Mirrors React `GET /analytics/spend-history`, `/analytics/spend-history/stats`,
//! `/analytics/category-momentum`, `/analytics/rhythm/weekday`,
//! `/analytics/insights/movers`.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One cycle's point on the spend-trend scope (`SpendHistoryDto::points` element).
///
/// `spend` is the cycle total; `budget` its ceiling (the reference line); `rate`
/// the 0–1 savings rate (for the "savings" series); `over` flags an over-budget
/// cycle; `projected` marks the current in-progress cycle (drawn hollow). `m`/`yr`
/// are the axis labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendPointDto {
    /// Month label, e.g. `"JUN"`.
    pub m: String,
    /// Year suffix, e.g. `"26"`.
    pub yr: String,
    /// Cycle total spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub spend: Money,
    /// Cycle budget (reference line).
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CyclePointDto {
    /// Month label.
    pub m: String,
    /// Year suffix.
    pub yr: String,
    /// Spend for that cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub spend: Money,
    /// Budget for that cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
}

/// `GET /analytics/spend-history/stats` — the trend roll-ups.
///
/// Mirrors `phosk_insights::analytics::SpendStatsDto`: "on record" starts at
/// the first cycle with any spend; averages, `total_saved`, `peak` and `low`
/// cover the settled cycles on record only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendStatsDto {
    /// Cycles on record, the current one included (at least 1).
    pub months: u32,
    /// Average spend of the last `avg_cycles` settled cycles (zero if none).
    #[serde(with = "phosk_model::money_centimes")]
    pub avg: Money,
    /// How many settled cycles `avg` / `avg_rate` average (0–6).
    pub avg_cycles: u32,
    /// Average savings rate (saved / budget) over those cycles, 0–1.
    pub avg_rate: f64,
    /// Total saved over the settled cycles on record.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_saved: Money,
    /// This cycle so far.
    pub cur: CyclePointDto,
    /// Previous cycle.
    pub prev: CyclePointDto,
    /// Peak (highest-spend) settled cycle (`cur` if none).
    pub peak: CyclePointDto,
    /// Leanest (lowest-spend) settled cycle (`cur` if none).
    pub low: CyclePointDto,
    /// Signed percent of `cur` vs `avg` (0 without a settled cycle).
    pub cur_vs_avg_pct: i32,
    /// Signed percent vs the previous cycle.
    pub cur_vs_prev_pct: i32,
    /// This cycle projected to its end at the pace so far.
    #[serde(with = "phosk_model::money_centimes")]
    pub run_rate: Money,
    /// Signed percent of `run_rate` vs `avg` (0 without a settled cycle).
    pub run_rate_vs_avg_pct: i32,
}

/// One category's momentum card (`GET /analytics/category-momentum` element).
///
/// `now` is this cycle's spend; `series` the 12-cycle spark; `delta_pct` the
/// signed momentum vs the `baseline_cycles`-cycle average; `fixed` marks an
/// untunable channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentumDto {
    /// Category name.
    pub name: String,
    /// This cycle's spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub now: Money,
    /// Cap / budget for context.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
    /// 12-point spark.
    pub series: Vec<f64>,
    /// Signed momentum percent vs the baseline average.
    pub delta_pct: i32,
    /// Baseline (trailing-N cycle) average spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub prior_avg: Money,
    /// Settled cycles the baseline averages: the `momentum_baseline_cycles`
    /// setting, or fewer while less history exists (0 = no baseline yet).
    pub baseline_cycles: u32,
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

/// The computed soft-cap suggestion (`AnalyticsInsightDto::suggested_cap`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedCapDto {
    /// Signal the cap applies to.
    pub signal_id: String,
    /// Suggested cap amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Projected CHF saved by applying it.
    #[serde(with = "phosk_model::money_centimes")]
    pub projected_savings: Money,
}

/// `GET /analytics/insights/movers` — the computed "read" + a cap suggestion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsInsightDto {
    /// Who wrote `text`: `"COMPUTED"` (plain arithmetic, no model).
    pub source: String,
    /// The insight sentence.
    pub text: String,
    /// An actionable cap suggestion.
    pub suggested_cap: SuggestedCapDto,
}

/// The number of cycles the spend-trend scope draws (the React design's 12).
#[cfg(feature = "server-deps")]
const HISTORY_CYCLES: u32 = 12;

/// The 12-cycle spend-trend points (`GET /analytics/spend-history`).
///
/// REAL: composes `phosk_insights::analytics::spend_history` (12 cycles).
#[server]
pub async fn get_spend_history() -> Result<SpendHistoryDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let h = phosk_insights::analytics::spend_history(
            session.db(),
            crate::data::today(),
            HISTORY_CYCLES,
        )
        .await
        .map_err(crate::data::server_err)?;
        Ok(SpendHistoryDto {
            points: h.points.into_iter().map(map_point).collect(),
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The trend roll-ups (`GET /analytics/spend-history/stats`).
///
/// REAL: composes `phosk_insights::analytics::spend_stats` (12 cycles).
#[server]
pub async fn get_spend_stats() -> Result<SpendStatsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let s = phosk_insights::analytics::spend_stats(
            session.db(),
            crate::data::today(),
            HISTORY_CYCLES,
        )
        .await
        .map_err(crate::data::server_err)?;
        Ok(SpendStatsDto {
            months: s.months,
            avg: s.avg,
            avg_rate: s.avg_rate,
            total_saved: s.total_saved,
            cur: map_cycle(s.cur),
            prev: map_cycle(s.prev),
            peak: map_cycle(s.peak),
            low: map_cycle(s.low),
            cur_vs_avg_pct: s.cur_vs_avg_pct,
            cur_vs_prev_pct: s.cur_vs_prev_pct,
            avg_cycles: s.avg_cycles,
            run_rate: s.run_rate,
            run_rate_vs_avg_pct: s.run_rate_vs_avg_pct,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// Per-category momentum cards (`GET /analytics/category-momentum`).
///
/// REAL: composes `phosk_insights::analytics::category_momentum`.
#[server]
pub async fn get_category_momentum() -> Result<Vec<MomentumDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let cards =
            phosk_insights::analytics::category_momentum(session.db(), crate::data::today())
                .await
                .map_err(crate::data::server_err)?;
        Ok(cards
            .into_iter()
            .map(|c| MomentumDto {
                name: c.name,
                now: c.now,
                budget: c.budget,
                series: c.series,
                delta_pct: c.delta_pct,
                prior_avg: c.prior_avg,
                baseline_cycles: c.baseline_cycles,
                fixed: c.fixed,
            })
            .collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The weekday spending rhythm (`GET /analytics/rhythm/weekday`).
///
/// REAL: composes `phosk_insights::analytics::weekday_rhythm`.
#[server]
pub async fn get_rhythm() -> Result<RhythmDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let r = phosk_insights::analytics::weekday_rhythm(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(RhythmDto {
            weekday: r
                .weekday
                .iter()
                .map(|w| WeekdayDto {
                    d: w.d.clone(),
                    v: w.v,
                })
                .collect(),
            stats: RhythmStatsDto {
                max: r.stats.max,
                total: r.stats.total,
                avg: r.stats.avg,
                weekend_share: r.stats.weekend_share,
                peak: WeekdayDto {
                    d: r.stats.peak.d,
                    v: r.stats.peak.v,
                },
            },
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The computed read + cap suggestion (`GET /analytics/insights/movers`).
///
/// REAL: composes `phosk_insights::analytics::analytics_insight`.
#[server]
pub async fn get_analytics_insight() -> Result<AnalyticsInsightDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let i = phosk_insights::analytics::analytics_insight(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(AnalyticsInsightDto {
            source: i.source,
            text: i.text,
            suggested_cap: SuggestedCapDto {
                signal_id: i.suggested_cap.signal_id,
                amount: i.suggested_cap.amount,
                projected_savings: i.suggested_cap.projected_savings,
            },
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

// ── mappers (service DTO → wire DTO) ───────────────────────────────────────────

/// Map a `phosk_insights` spend-history point onto the wire [`SpendPointDto`].
#[cfg(feature = "server-deps")]
fn map_point(p: phosk_insights::analytics::SpendPointDto) -> SpendPointDto {
    SpendPointDto {
        m: p.m,
        yr: p.yr,
        spend: p.spend,
        budget: p.budget,
        rate: p.rate,
        over: p.over,
        projected: p.projected,
    }
}

/// Map a `phosk_insights` cycle reference onto the wire [`CyclePointDto`].
#[cfg(feature = "server-deps")]
fn map_cycle(c: phosk_insights::analytics::CyclePointDto) -> CyclePointDto {
    CyclePointDto {
        m: c.m,
        yr: c.yr,
        spend: c.spend,
        budget: c.budget,
    }
}
