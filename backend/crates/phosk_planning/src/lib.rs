//! `phosk_planning` — the cycle-budgeting bounded context, exposing a
//! **service** over the `DatabaseAdapter` PORT (ADR-010 gateway).
//!
//! This slice answers the dashboard's headline KPI read,
//! `GET /cycle/current/totals`: how the current cycle's spend sits against the
//! budget, how savings are tracking, the run-rate projection, and the
//! comparison against the previous cycle. The single entry point is
//! [`totals`], returning a [`CycleTotals`].
//!
//! **Layering (ADR-010).** The service takes `&dyn DatabaseAdapter` and depends
//! only on the PORT trait crate (`phosk_adapter_db`) plus the domain/foundation
//! crates. It never imports a concrete adapter (`phosk_db_memory` is a *dev*-
//! dependency, used only by the tests). A technology swap is a new adapter
//! `impl`, never a change here.
//!
//! **Money & errors (ADR §0).** Every CHF amount stays exact [`Money`] (i64
//! centimes); summation and differencing are *checked* and surface a
//! [`PhoskError::Overflow`] rather than wrapping or panicking. There is no
//! `unwrap`/`expect`/`panic!` in this code: every fallible step and every
//! `Option`/`Result` maps explicitly to a [`PhoskError`]. The CHF-`f64`
//! conversion the frontend renders is the HTTP edge's job, not the service's —
//! the one exception is [`CycleTotals::savings_rate`], a pure unitless `0–1`
//! ratio (a fraction, never an amount) that the edge passes straight through.
//!
//! ## Formulas (documented where the frontend contract is loose)
//!
//! Let `B` = budget, `S` = spent this cycle, `L` = last cycle's spent,
//! `T` = savings target, `d` = `day_index` (1-based position of `as_of`),
//! `N` = `len_days`, `r` = `days_left`. All money math is checked.
//!
//! - `remaining      = B − S`
//! - `allocated      = Σ category caps` (unlimited — `cap == None` — excluded)
//! - `saved          = max(0, B − S)`
//! - `projectedSpend = S · N / d`  (run-rate; `d ≥ 1` inside a resolved cycle)
//! - `savingsProjected = max(0, B − projectedSpend)`
//! - `savingsRate    = saved / B`            as a `0–1` ratio
//! - `spentPct       = round(100 · S / B)`   as an integer `0–100`
//! - `vsLastCyclePct = round(100 · (S − L) / L)` as a signed integer percent;
//!   `0` when `L == 0`, since there is no base to compare to
//! - `perDayToStayOnBudget = remaining / r` by integer-centime division;
//!   `0` when no days are left — the cycle is over
//!
//! [`Money`]: phosk_core::money::Money
//! [`PhoskError::Overflow`]: phosk_core::error::PhoskError::Overflow

pub mod alerts;
pub mod budgets;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::{CycleWindow, Period};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;

/// The headline KPIs for one cycle, the basis for `GET /cycle/current/totals`.
///
/// Money fields are exact [`Money`] (i64 centimes); the HTTP edge converts them
/// to CHF JSON numbers (ADR-010). The three non-money fields are already in the
/// units the frontend consumes: [`savings_rate`](Self::savings_rate) is a `0–1`
/// ratio, [`spent_pct`](Self::spent_pct) an integer `0–100`, and
/// [`vs_last_cycle_pct`](Self::vs_last_cycle_pct) a signed integer percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CycleTotals {
    /// The cycle's spend ceiling, `B` (from [`BudgetConfig::monthly_budget`]).
    ///
    /// [`BudgetConfig::monthly_budget`]: phosk_model::BudgetConfig::monthly_budget
    pub budget: Money,
    /// Total spent so far this cycle, `S`.
    pub spent: Money,
    /// `B − S`: budget left for the rest of the cycle (negative if overspent).
    pub remaining: Money,
    /// Sum of every category's budget cap, `allocated`. Unlimited categories
    /// (`cap == None`) contribute nothing.
    pub allocated: Money,
    /// The savings target for the cycle, `T` (from
    /// [`BudgetConfig::savings_target`]).
    ///
    /// [`BudgetConfig::savings_target`]: phosk_model::BudgetConfig::savings_target
    pub savings_target: Money,
    /// `max(0, B − S)`: savings realised so far (clamped at zero — an overspend
    /// is not negative savings here, it is simply zero saved).
    pub saved: Money,
    /// `max(0, B − projectedSpend)`: savings expected by cycle end if the
    /// current run-rate holds (clamped at zero).
    pub savings_projected: Money,
    /// `saved / B` as a `0–1` ratio. Unitless; the edge passes it straight to
    /// the frontend's `pct()` (which multiplies by 100).
    pub savings_rate: f64,
    /// Last cycle's total spend, `L` — the previous calendar month's spend.
    pub last_cycle_spent: Money,
    /// `round(100 · S / B)` as an integer `0–100` (`0` when `B == 0`).
    pub spent_pct: i32,
    /// `round(100 · (S − L) / L)` as a signed integer percent (`0` when
    /// `L == 0`).
    pub vs_last_cycle_pct: i32,
    /// `remaining / days_left`: the daily spend that keeps the cycle on budget
    /// (integer-centime division; [`Money::ZERO`] once the cycle is over).
    pub per_day_to_stay_on_budget: Money,
}

/// Compute the cycle KPIs for `window` from the data behind the PORT.
///
/// `spent` is the exact sum of `window`'s transactions; `last_cycle_spent` is
/// the sum of the **previous calendar month** (the [`Period::Month`] resolved
/// for the day before `window.start`). `budget`/`savings_target` come from
/// [`DatabaseAdapter::budget_config`], and `allocated` from the category caps.
/// All derived figures follow the documented formulas (see the crate docs).
///
/// # Errors
/// - Propagates any [`PhoskError`] from the adapter's reads.
/// - Returns [`PhoskError::Overflow`] if any checked sum/difference overflows
///   the i64 centime range (never wrapping; ADR §0).
/// - Returns [`PhoskError::InvalidDate`] if the previous month cannot be
///   resolved (e.g. a date before the representable range).
#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(from = %window.start, to = %window.end, as_of = %window.as_of)
)]
pub async fn totals(
    db: &dyn DatabaseAdapter,
    window: CycleWindow,
) -> Result<CycleTotals, PhoskError> {
    let config = db.budget_config().await?;
    let budget = config.monthly_budget;
    let savings_target = config.savings_target;

    // `spent` is spend *to date* — the inclusive `[start, as_of]` slice, not the
    // whole `[start, end]` window. The dashboard's KPIs (remaining, run-rate
    // projection, per-day-to-stay-on-budget) only cohere if `spent` is what has
    // actually been spent so far; the full-window picture lives in the separate
    // spend-series. For a cycle whose `as_of == end` the two coincide.
    let spent = sum_window(db, window.start, window.as_of).await?;
    tracing::debug!(spent = %spent, "summed current-cycle spend to date");

    let last_cycle_spent = last_cycle_spend(db, window).await?;
    tracing::debug!(last_cycle_spent = %last_cycle_spent, "summed last-cycle spend");

    let allocated = allocated_caps(db).await?;

    // remaining = B − S (may go negative when overspent).
    let remaining = budget.checked_sub(spent)?;
    // saved = max(0, B − S): an overspend is zero saved, not negative savings.
    let saved = remaining.max(Money::ZERO);

    // projectedSpend = S · N / d, the run-rate extrapolation to cycle end. The
    // window guarantees start <= as_of, so day_index >= 1 and the division is
    // safe; we still guard the zero defensively rather than risk a divide.
    let day_index = i64::from(window.day_index());
    let len_days = i64::from(window.len_days());
    let projected_spend = if day_index == 0 {
        spent
    } else {
        let scaled = spent.centimes().checked_mul(len_days).ok_or_else(|| {
            PhoskError::Overflow(format!("projecting {spent} over {len_days} days"))
        })?;
        Money::from_centimes(scaled / day_index)
    };
    // savingsProjected = max(0, B − projectedSpend).
    let savings_projected = budget.checked_sub(projected_spend)?.max(Money::ZERO);

    // savingsRate = saved / B, a unitless 0–1 ratio (0 when there is no budget).
    let savings_rate = ratio(saved.centimes(), budget.centimes());

    // spentPct = round(100 · S / B), integer 0–100 (0 when there is no budget).
    let spent_pct = pct_round(spent.centimes(), budget.centimes());

    // vsLastCyclePct = round(100 · (S − L) / L), signed (0 with no base).
    let delta = spent.checked_sub(last_cycle_spent)?;
    let vs_last_cycle_pct = pct_round(delta.centimes(), last_cycle_spent.centimes());

    // perDayToStayOnBudget = remaining / days_left (0 once the cycle is over).
    let days_left = i64::from(window.days_left());
    let per_day_to_stay_on_budget = if days_left == 0 {
        Money::ZERO
    } else {
        Money::from_centimes(remaining.centimes() / days_left)
    };

    let result = CycleTotals {
        budget,
        spent,
        remaining,
        allocated,
        savings_target,
        saved,
        savings_projected,
        savings_rate,
        last_cycle_spent,
        spent_pct,
        vs_last_cycle_pct,
        per_day_to_stay_on_budget,
    };
    tracing::debug!(
        spent_pct,
        vs_last_cycle_pct,
        savings_rate,
        "computed cycle totals"
    );
    Ok(result)
}

/// Exact sum of every transaction amount in the inclusive `[from, to]` window.
#[tracing::instrument(level = "trace", skip_all, fields(from = %from, to = %to))]
async fn sum_window(
    db: &dyn DatabaseAdapter,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<Money, PhoskError> {
    let txns = db.transactions_between(from, to).await?;
    Money::sum(txns.into_iter().map(|tx| tx.amount))
}

/// Spend of the cycle before `window`: the [`Period::Month`] that contains the
/// day immediately before `window.start`.
#[tracing::instrument(level = "trace", skip_all, fields(start = %window.start))]
async fn last_cycle_spend(
    db: &dyn DatabaseAdapter,
    window: CycleWindow,
) -> Result<Money, PhoskError> {
    // The day before this cycle begins; the previous calendar month is the cycle
    // it falls in. `pred_opt` guards the lower edge of the date range explicitly
    // rather than risking an underflow (ADR §0 — never silently drop a `None`).
    let prev_day = window
        .start
        .pred_opt()
        .ok_or_else(|| PhoskError::InvalidDate(format!("no day before {}", window.start)))?;
    let prev = Period::Month.resolve(prev_day)?;
    sum_window(db, prev.start, prev.end).await
}

/// Sum of every category's budget cap; unlimited categories (`cap == None`)
/// contribute nothing.
#[tracing::instrument(level = "trace", skip_all)]
async fn allocated_caps(db: &dyn DatabaseAdapter) -> Result<Money, PhoskError> {
    let categories = db.categories().await?;
    Money::sum(categories.into_iter().filter_map(|c| c.cap))
}

/// `round(100 · numerator / denominator)` as an `i32`, or `0` when the
/// denominator is `0` (no base to take a percentage of). Rounds half away from
/// zero. Centime magnitudes are far inside `i32` range for any real cycle, so
/// the `as` casts are exact here; the result is a small percentage, and a
/// saturating `f64 → i32` cast can never produce a wrong-signed percent.
// The casts compute a unitless *percentage* (a small number), never money: an
// f64 loss of precision on the inputs cannot change a rounded integer percent,
// and the `as i32` saturates rather than wraps (ADR §0 — no silent corruption).
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "computing a small integer percentage, not an amount; saturating cast"
)]
fn pct_round(numerator: i64, denominator: i64) -> i32 {
    if denominator == 0 {
        return 0;
    }
    // Work in f64 only to round a *percentage* (a small unitless number), never
    // to hold money — the inputs are already exact integer centimes.
    let pct = 100.0 * numerator as f64 / denominator as f64;
    pct.round() as i32
}

/// `numerator / denominator` as an `f64` ratio, or `0.0` when the denominator is
/// `0`. A pure unitless fraction for the savings dial, never a money value.
// The cast yields a fraction the HTTP edge renders as a percent; an f64 loss of
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

    fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    /// The June 2026 cycle resolved at the spec's "today" (2026-06-18): the full
    /// calendar month `[2026-06-01, 2026-06-30]`, 30 days, `as_of` on day 18.
    fn june_window() -> CycleWindow {
        Period::Month
            .resolve(naive(2026, 6, 18))
            .expect("June resolves")
    }

    fn seeded() -> MemoryDb {
        MemoryDb::seeded().expect("seed is valid")
    }

    // ── Ground-truth constants, hand-computed from the seed line-items ────────
    //
    // The seed's canonical figures (see phosk_db_memory's own tests):
    //   budget          = CHF 4200.00  = 420_000 c
    //   savingsTarget   = CHF  900.00  =  90_000 c
    //   allocated       = Σ caps 800+350+1680+520+280+190+240+200 = CHF 4260 = 426_000 c
    //   lastCycle (May) = CHF 3787.70  = 378_770 c   (FULL May line-items)
    //
    // `spent` is spend-to-date `[start, as_of]`, NOT the full window. The June
    // line-items total CHF 3222.45 (322_245 c) across the whole month, but
    // through `as_of` 2026-06-18 they total CHF 3161.80 (316_180 c) — June 19's
    // Migros 53.85 + SBB 6.80 (6_065 c) fall after `as_of` and are excluded.
    //
    // June window at 2026-06-18: day_index = 18, len_days = 30, days_left = 12.
    //   spent          = 316_180 c (June through day 18)
    //   remaining      = 420_000 − 316_180 = 103_820 c
    //   saved          = max(0, 103_820)    = 103_820 c
    //   projectedSpend = 316_180 · 30 / 18 = 9_485_400 / 18 = 527_000 c
    //   savingsProj    = max(0, 420_000 − 527_000) = 0 c
    //   savingsRate    = 103_820 / 420_000 = 0.247_19
    //   spentPct       = round(100 · 316_180 / 420_000) = round(75.281) = 75
    //   vsLastCyclePct = round(100 · (316_180 − 378_770) / 378_770)
    //                  = round(100 · −62_590 / 378_770) = round(−16.524) = −17
    //   perDay         = 103_820 / 12 = 8_651 c (integer-centime division)
    const BUDGET: i64 = 420_000;
    const SAVINGS_TARGET: i64 = 90_000;
    const ALLOCATED: i64 = 426_000;
    const JUNE_SPENT: i64 = 316_180;
    const MAY_SPENT: i64 = 378_770;

    async fn june_totals() -> CycleTotals {
        totals(&seeded(), june_window()).await.expect("totals ok")
    }

    #[tokio::test]
    async fn budget_and_target_come_from_config() {
        let t = june_totals().await;
        assert_eq!(t.budget.centimes(), BUDGET, "budget is CHF 4200.00");
        assert_eq!(
            t.savings_target.centimes(),
            SAVINGS_TARGET,
            "savings target is CHF 900.00"
        );
    }

    #[tokio::test]
    async fn spent_is_june_spend_to_date() {
        let t = june_totals().await;
        // Spend-to-date through as_of 2026-06-18, June 19 excluded.
        assert_eq!(
            t.spent.centimes(),
            JUNE_SPENT,
            "June spend-to-date is CHF 3161.80"
        );
    }

    #[tokio::test]
    async fn last_cycle_spent_is_the_may_total() {
        let t = june_totals().await;
        assert_eq!(
            t.last_cycle_spent.centimes(),
            MAY_SPENT,
            "last cycle (May) spend is CHF 3787.70"
        );
    }

    #[tokio::test]
    async fn remaining_is_budget_minus_spent() {
        let t = june_totals().await;
        assert_eq!(
            t.remaining.centimes(),
            BUDGET - JUNE_SPENT,
            "remaining is CHF 1038.20"
        );
        assert_eq!(t.remaining.centimes(), 103_820);
    }

    #[tokio::test]
    async fn allocated_is_the_sum_of_category_caps() {
        let t = june_totals().await;
        assert_eq!(
            t.allocated.centimes(),
            ALLOCATED,
            "allocated is Σ of the eight caps = CHF 4260.00"
        );
    }

    #[tokio::test]
    async fn saved_is_clamped_remaining() {
        let t = june_totals().await;
        // Under budget in June → saved == remaining == 103_820 c.
        assert_eq!(t.saved.centimes(), 103_820);
    }

    #[tokio::test]
    async fn savings_projected_is_zero_at_overspending_run_rate() {
        let t = june_totals().await;
        // projectedSpend (537_075 c) exceeds budget, so projected savings clamp
        // to zero rather than going negative.
        assert_eq!(
            t.savings_projected,
            Money::ZERO,
            "run-rate overshoots budget → projected savings is zero"
        );
    }

    #[tokio::test]
    async fn savings_rate_is_saved_over_budget() {
        let t = june_totals().await;
        let expected = 103_820.0 / 420_000.0;
        assert!(
            (t.savings_rate - expected).abs() < 1e-9,
            "savings_rate {} ≈ {expected}",
            t.savings_rate
        );
        assert!(
            (0.0..=1.0).contains(&t.savings_rate),
            "savings_rate stays in 0..=1"
        );
    }

    #[tokio::test]
    async fn spent_pct_is_rounded_percent_zero_to_hundred() {
        let t = june_totals().await;
        // round(100 · 316_180 / 420_000) = round(75.281) = 75.
        assert_eq!(t.spent_pct, 75);
    }

    #[tokio::test]
    async fn vs_last_cycle_pct_is_signed_rounded_percent() {
        let t = june_totals().await;
        // round(100 · (316_180 − 378_770) / 378_770) = round(−16.524) = −17.
        assert_eq!(
            t.vs_last_cycle_pct, -17,
            "June-to-date is ~17% below full May → −17"
        );
    }

    #[tokio::test]
    async fn per_day_to_stay_on_budget_is_remaining_over_days_left() {
        let t = june_totals().await;
        // 103_820 c / 12 days left = 8_651 c (integer-centime division).
        assert_eq!(
            t.per_day_to_stay_on_budget.centimes(),
            8_651,
            "per-day-to-stay-on-budget is CHF 86.51"
        );
    }

    /// Every KPI at once, the full pinned dashboard contract for the June seed.
    #[tokio::test]
    async fn full_june_contract_snapshot() {
        let t = june_totals().await;
        assert_eq!(t.budget.centimes(), 420_000);
        assert_eq!(t.spent.centimes(), 316_180);
        assert_eq!(t.remaining.centimes(), 103_820);
        assert_eq!(t.allocated.centimes(), 426_000);
        assert_eq!(t.savings_target.centimes(), 90_000);
        assert_eq!(t.saved.centimes(), 103_820);
        assert_eq!(t.savings_projected.centimes(), 0);
        assert_eq!(t.last_cycle_spent.centimes(), 378_770);
        assert_eq!(t.spent_pct, 75);
        assert_eq!(t.vs_last_cycle_pct, -17);
        assert_eq!(t.per_day_to_stay_on_budget.centimes(), 8_651);
    }

    /// At the first day of a cycle the run-rate extrapolation uses `day_index 1`
    /// and `days_left = N − 1`; nothing divides by zero and projected savings is
    /// based on a single day's spend.
    #[tokio::test]
    async fn first_day_of_cycle_projects_from_one_day() {
        let db = seeded();
        // 2026-06-01: day_index 1, days_left 29, spend-to-date = June 1's 2056.75
        // (the whole window's later days are not yet spent at as_of == start).
        let window = Period::Month.resolve(naive(2026, 6, 1)).expect("resolves");
        let t = totals(&db, window).await.expect("totals ok");
        assert_eq!(t.spent.centimes(), 205_675, "June 1 spend-to-date");
        // projectedSpend = 205_675 · 30 / 1 = 6_170_250 c → way over budget →
        // projected savings clamps to zero.
        assert_eq!(t.savings_projected, Money::ZERO);
        // perDay = remaining / 29 = (420_000 − 205_675)/29 = 214_325/29 = 7_390 c.
        assert_eq!(t.per_day_to_stay_on_budget.centimes(), 214_325 / 29);
    }

    /// On the last day of the cycle `days_left == 0`; per-day-to-stay-on-budget
    /// is reported as zero rather than dividing by zero (the cycle is over).
    #[tokio::test]
    async fn last_day_of_cycle_has_zero_per_day() {
        let db = seeded();
        let window = Period::Month.resolve(naive(2026, 6, 30)).expect("resolves");
        let t = totals(&db, window).await.expect("totals ok");
        assert_eq!(window.days_left(), 0);
        assert_eq!(t.per_day_to_stay_on_budget, Money::ZERO);
    }

    /// A cycle with no prior month of data (May 2026 is the earliest seed) yields
    /// `lastCycleSpent == 0` and, with no base, `vsLastCyclePct == 0` — never a
    /// divide-by-zero.
    #[tokio::test]
    async fn no_previous_cycle_data_gives_zero_comparison() {
        let db = seeded();
        // May 2026's previous month is April, which has no seeded transactions.
        let window = Period::Month.resolve(naive(2026, 5, 18)).expect("resolves");
        let t = totals(&db, window).await.expect("totals ok");
        assert_eq!(t.last_cycle_spent, Money::ZERO, "April has no data");
        assert_eq!(t.vs_last_cycle_pct, 0, "no base → 0%, not a divide-by-zero");
        // May spend-to-date through day 18 = CHF 3171.00 (317_100 c); the full
        // month is CHF 3787.70 but May 19–31 fall after this `as_of`.
        assert_eq!(
            t.spent.centimes(),
            317_100,
            "May spend-to-date is CHF 3171.00"
        );
    }

    /// Unlimited categories (`cap == None`) are excluded from `allocated`, and a
    /// zero budget yields `0` percent / `0` ratio rather than a divide-by-zero.
    #[tokio::test]
    async fn unlimited_caps_excluded_and_zero_budget_is_safe() {
        use phosk_model::{BudgetConfig, Category, Transaction};

        let txns = vec![Transaction {
            date: naive(2026, 6, 2),
            shop: "Migros".to_owned(),
            category: "GROCERIES".to_owned(),
            amount: Money::from_centimes(5_000),
        }];
        let categories = vec![
            Category {
                name: "GROCERIES".to_owned(),
                cap: Some(Money::from_centimes(10_000)),
            },
            Category {
                name: "MISC".to_owned(),
                cap: None, // unlimited → excluded from allocated
            },
        ];
        let db = MemoryDb::new(
            txns,
            categories,
            BudgetConfig {
                monthly_budget: Money::ZERO,
                savings_target: Money::ZERO,
            },
        );
        let t = totals(&db, june_window()).await.expect("totals ok");
        assert_eq!(
            t.allocated.centimes(),
            10_000,
            "only the capped category counts toward allocated"
        );
        assert_eq!(t.spent_pct, 0, "zero budget → 0% spent, not NaN");
        assert!(
            (t.savings_rate - 0.0).abs() < f64::EPSILON,
            "zero budget → 0.0 savings rate, not NaN"
        );
    }

    /// The service is callable behind the `&dyn DatabaseAdapter` PORT handle
    /// (ADR-010) — it never sees the concrete `MemoryDb` type.
    #[tokio::test]
    async fn service_works_through_the_port_trait_object() {
        let db = seeded();
        let port: &dyn DatabaseAdapter = &db;
        let t = totals(port, june_window()).await.expect("totals ok");
        assert_eq!(t.spent.centimes(), JUNE_SPENT);
    }
}
