//! The cycle/period engine (ADR §0 foundation — "used by almost every read
//! endpoint"). Pure date math: given a [`Period`] kind and a reference date, it
//! resolves the concrete `[start, end]` window that date falls in, and exposes
//! its length / position / remaining days.
//!
//! Presentation (the `"JUN 2026"` / `"19 JUN"` labels the frontend renders) is
//! deliberately *not* here — that is formatting and belongs at the HTTP edge
//! (ADR-010). This module deals only in dates and counts.
//!
//! [`Period`] is a closed enum (ADR-009): a new cadence is a deliberate code
//! change, and every `match` over it is total.

use chrono::{Datelike, Duration, Months, NaiveDate};

use crate::error::PhoskError;
use crate::money::Money;

/// A budgeting/reporting cadence. The dashboard's "current cycle" is
/// [`Period::Month`]; the transactions horizon filter uses the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// A single day.
    Day,
    /// The ISO week (Monday–Sunday) containing the reference date.
    Week,
    /// The calendar month.
    Month,
    /// The calendar quarter (Jan–Mar, Apr–Jun, Jul–Sep, Oct–Dec).
    Quarter,
    /// The calendar year.
    Year,
    /// The trailing `n` days ending on (and including) the reference date.
    Custom(u32),
}

/// A resolved cycle window: the inclusive `[start, end]` span the reference date
/// falls in, plus that reference date (`as_of`). By construction
/// `start <= as_of <= end`, so the count accessors never underflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleWindow {
    /// First day of the window (inclusive).
    pub start: NaiveDate,
    /// Last day of the window (inclusive).
    pub end: NaiveDate,
    /// The reference date the window was resolved for.
    pub as_of: NaiveDate,
}

impl Period {
    /// Resolve the window of this period containing `as_of`.
    ///
    /// Errors only on inputs that cannot describe a real window:
    /// [`Period::Custom`] with `n == 0`, or arithmetic that overflows the
    /// representable date range — never silently.
    #[tracing::instrument(level = "trace", skip_all, fields(period = ?self, as_of = %as_of))]
    pub fn resolve(self, as_of: NaiveDate) -> Result<CycleWindow, PhoskError> {
        let (start, end) = match self {
            Self::Day => (as_of, as_of),
            Self::Week => {
                let start =
                    as_of - Duration::days(i64::from(as_of.weekday().num_days_from_monday()));
                let end = start + Duration::days(6);
                (start, end)
            }
            Self::Month => {
                let start = first_of_month(as_of.year(), as_of.month())?;
                (start, last_of_month(start)?)
            }
            Self::Quarter => {
                let first_month = (as_of.month() - 1) / 3 * 3 + 1; // 1,4,7,10
                let start = first_of_month(as_of.year(), first_month)?;
                let last_month_start = first_of_month(as_of.year(), first_month + 2)?;
                (start, last_of_month(last_month_start)?)
            }
            Self::Year => (ymd(as_of.year(), 1, 1)?, ymd(as_of.year(), 12, 31)?),
            Self::Custom(n) => {
                if n == 0 {
                    return Err(PhoskError::Invalid(
                        "custom period length must be >= 1".into(),
                    ));
                }
                let start = as_of - Duration::days(i64::from(n - 1));
                (start, as_of)
            }
        };
        tracing::trace!(%start, %end, "resolved cycle window");
        Ok(CycleWindow { start, end, as_of })
    }
}

#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "start <= as_of <= end by construction, so every day-count is non-negative and at most ~366 — far inside u32, no sign loss or truncation possible"
)]
impl CycleWindow {
    /// Total number of days in the window (inclusive of both ends).
    pub fn len_days(&self) -> u32 {
        // start <= end by construction → non-negative, fits u32 comfortably.
        ((self.end - self.start).num_days() + 1).max(0) as u32
    }

    /// 1-based position of `as_of` within the window (day 1 = `start`).
    pub fn day_index(&self) -> u32 {
        ((self.as_of - self.start).num_days() + 1).max(0) as u32
    }

    /// Whole days remaining after `as_of` up to and including `end`.
    pub fn days_left(&self) -> u32 {
        (self.end - self.as_of).num_days().max(0) as u32
    }
}

impl CycleWindow {
    /// Linear run-rate projection of spend-to-date `spent` to the window's
    /// end: `spent · len_days / day_index`, in exact (truncated) centimes.
    ///
    /// Only for spend that accrues over the cycle. A fixed/standing charge is
    /// paid once per cycle and must not be scaled; use [`Self::project_spend`]
    /// for a figure that mixes both.
    ///
    /// # Errors
    /// [`PhoskError::Overflow`] if the scaled amount leaves the `i64` centime
    /// range.
    pub fn project_linear(&self, spent: Money) -> Result<Money, PhoskError> {
        let day_index = i64::from(self.day_index().max(1));
        let len_days = i64::from(self.len_days());
        let scaled = spent.centimes().checked_mul(len_days).ok_or_else(|| {
            PhoskError::Overflow(format!("projecting {spent} over {len_days} days"))
        })?;
        Ok(Money::from_centimes(scaled / day_index))
    }

    /// The end-of-window projection of a spend-to-date split into its `fixed`
    /// part (standing charges: rent paid on day 1 is the whole month's rent,
    /// so it is carried over unchanged) and its `variable` part (projected
    /// linearly, see [`Self::project_linear`]): `fixed + variable · N / d`.
    ///
    /// The one projection rule every "on pace for" / "projected" figure uses.
    ///
    /// # Errors
    /// [`PhoskError::Overflow`] if a step leaves the `i64` centime range.
    pub fn project_spend(&self, fixed: Money, variable: Money) -> Result<Money, PhoskError> {
        fixed.checked_add(self.project_linear(variable)?)
    }
}

/// First day of a calendar month, or [`PhoskError::InvalidDate`] if the
/// year/month pair cannot exist.
fn first_of_month(year: i32, month: u32) -> Result<NaiveDate, PhoskError> {
    ymd(year, month, 1)
}

/// Last day of the month that `first` (a 1st-of-month) begins.
fn last_of_month(first: NaiveDate) -> Result<NaiveDate, PhoskError> {
    // First of next month, minus one day. `checked_add_months` handles the
    // December → January year roll without manual wrapping.
    let first_of_next = first
        .checked_add_months(Months::new(1))
        .ok_or_else(|| PhoskError::InvalidDate(format!("month after {first} is out of range")))?;
    first_of_next.pred_opt().ok_or_else(|| {
        PhoskError::InvalidDate(format!("day before {first_of_next} is out of range"))
    })
}

/// `NaiveDate::from_ymd_opt` with the `None` mapped to a real error (never a
/// panic; ADR — no `unwrap`).
fn ymd(year: i32, month: u32, day: u32) -> Result<NaiveDate, PhoskError> {
    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| PhoskError::InvalidDate(format!("{year:04}-{month:02}-{day:02}")))
}

#[cfg(test)]
mod tests {
    use chrono::Weekday;

    use super::*;

    /// Build a date in tests, surfacing a clear message rather than `unwrap`.
    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("test date must be valid")
    }

    fn resolve(p: Period, date: NaiveDate) -> CycleWindow {
        p.resolve(date)
            .expect("period must resolve for a valid date")
    }

    // ── Month (the dashboard cycle) ──────────────────────────────────────
    #[test]
    fn month_june_2026() {
        let w = resolve(Period::Month, d(2026, 6, 19));
        assert_eq!(w.start, d(2026, 6, 1));
        assert_eq!(w.end, d(2026, 6, 30));
        assert_eq!(w.len_days(), 30);
        assert_eq!(w.day_index(), 19);
        assert_eq!(w.days_left(), 11);
    }

    #[test]
    fn month_february_leap_year_has_29_days() {
        let w = resolve(Period::Month, d(2024, 2, 15));
        assert_eq!(w.end, d(2024, 2, 29));
        assert_eq!(w.len_days(), 29);
    }

    #[test]
    fn month_february_non_leap_year_has_28_days() {
        let w = resolve(Period::Month, d(2026, 2, 15));
        assert_eq!(w.end, d(2026, 2, 28));
        assert_eq!(w.len_days(), 28);
    }

    #[test]
    fn month_december_does_not_overflow_the_year() {
        let w = resolve(Period::Month, d(2026, 12, 31));
        assert_eq!(w.start, d(2026, 12, 1));
        assert_eq!(w.end, d(2026, 12, 31));
        assert_eq!(w.len_days(), 31);
        assert_eq!(w.day_index(), 31);
        assert_eq!(w.days_left(), 0);
    }

    #[test]
    fn month_first_day_has_full_window_left() {
        let w = resolve(Period::Month, d(2026, 6, 1));
        assert_eq!(w.day_index(), 1);
        assert_eq!(w.days_left(), 29);
    }

    // ── Projection ───────────────────────────────────────────────────────
    #[test]
    fn linear_projection_scales_to_the_window_end() {
        // Day 10 of 30: ×3, truncated to the centime.
        let w = resolve(Period::Month, d(2026, 6, 10));
        let p = w.project_linear(Money::from_centimes(10_001)).expect("fits");
        assert_eq!(p, Money::from_centimes(30_003));
        let day1 = resolve(Period::Month, d(2026, 6, 1));
        let p = day1.project_linear(Money::from_centimes(100)).expect("fits");
        assert_eq!(p, Money::from_centimes(3_000));
    }

    #[test]
    fn a_fixed_charge_is_never_projected() {
        // Rent CHF 1680 paid on day 1, CHF 30 of groceries by day 1: the rent
        // stays 1680, only the groceries run at ×30.
        let w = resolve(Period::Month, d(2026, 6, 1));
        let p = w
            .project_spend(Money::from_centimes(168_000), Money::from_centimes(3_000))
            .expect("fits");
        assert_eq!(p, Money::from_centimes(168_000 + 90_000));
        // Nothing variable → the fixed total, whatever the day.
        let mid = resolve(Period::Month, d(2026, 6, 18));
        let p = mid
            .project_spend(Money::from_centimes(168_000), Money::ZERO)
            .expect("fits");
        assert_eq!(p, Money::from_centimes(168_000));
    }

    #[test]
    fn projection_overflow_is_an_error() {
        let w = resolve(Period::Month, d(2026, 6, 1));
        assert!(matches!(
            w.project_linear(Money::from_centimes(i64::MAX)),
            Err(PhoskError::Overflow(_))
        ));
    }

    // ── Day ──────────────────────────────────────────────────────────────
    #[test]
    fn day_is_a_single_day_window() {
        let w = resolve(Period::Day, d(2026, 6, 19));
        assert_eq!(w.start, w.end);
        assert_eq!(w.len_days(), 1);
        assert_eq!(w.day_index(), 1);
        assert_eq!(w.days_left(), 0);
    }

    // ── Week (ISO Monday–Sunday) ───────────────────────────────────────────
    #[test]
    fn week_spans_monday_to_sunday_containing_the_date() {
        let w = resolve(Period::Week, d(2026, 6, 19));
        assert_eq!(w.len_days(), 7);
        assert_eq!(w.start.weekday(), Weekday::Mon);
        assert_eq!(w.end.weekday(), Weekday::Sun);
        assert!(w.start <= w.as_of && w.as_of <= w.end);
    }

    #[test]
    fn week_on_a_monday_starts_that_day() {
        // Find a Monday deterministically, then assert it is the window start.
        let monday = {
            let mut day = d(2026, 6, 1);
            while day.weekday() != Weekday::Mon {
                day = day.succ_opt().expect("date in range");
            }
            day
        };
        let w = resolve(Period::Week, monday);
        assert_eq!(w.start, monday);
        assert_eq!(w.day_index(), 1);
        assert_eq!(w.days_left(), 6);
    }

    // ── Quarter ────────────────────────────────────────────────────────────
    #[test]
    fn quarter_q2_for_june() {
        let w = resolve(Period::Quarter, d(2026, 6, 19));
        assert_eq!(w.start, d(2026, 4, 1));
        assert_eq!(w.end, d(2026, 6, 30));
        assert_eq!(w.len_days(), 30 + 31 + 30); // Apr+May+Jun
    }

    #[test]
    fn quarter_q1_and_q4_boundaries() {
        let q1 = resolve(Period::Quarter, d(2026, 1, 1));
        assert_eq!(q1.start, d(2026, 1, 1));
        assert_eq!(q1.end, d(2026, 3, 31));
        let q4 = resolve(Period::Quarter, d(2026, 11, 30));
        assert_eq!(q4.start, d(2026, 10, 1));
        assert_eq!(q4.end, d(2026, 12, 31));
    }

    // ── Year ───────────────────────────────────────────────────────────────
    #[test]
    fn year_spans_jan_1_to_dec_31() {
        let w = resolve(Period::Year, d(2026, 6, 19));
        assert_eq!(w.start, d(2026, 1, 1));
        assert_eq!(w.end, d(2026, 12, 31));
        assert_eq!(w.len_days(), 365);
    }

    #[test]
    fn year_leap_has_366_days() {
        let w = resolve(Period::Year, d(2024, 6, 19));
        assert_eq!(w.len_days(), 366);
    }

    // ── Custom ─────────────────────────────────────────────────────────────
    #[test]
    fn custom_is_trailing_n_days_ending_today() {
        let w = resolve(Period::Custom(7), d(2026, 6, 19));
        assert_eq!(w.start, d(2026, 6, 13));
        assert_eq!(w.end, d(2026, 6, 19));
        assert_eq!(w.len_days(), 7);
        assert_eq!(w.day_index(), 7);
        assert_eq!(w.days_left(), 0);
    }

    #[test]
    fn custom_one_day() {
        let w = resolve(Period::Custom(1), d(2026, 6, 19));
        assert_eq!(w.start, w.end);
        assert_eq!(w.len_days(), 1);
    }

    #[test]
    fn custom_zero_is_rejected_not_panicked() {
        let err = Period::Custom(0).resolve(d(2026, 6, 19)).unwrap_err();
        assert_eq!(err.http_status(), 400);
    }

    // ── Invariant ──────────────────────────────────────────────────────────
    #[test]
    fn day_index_plus_days_left_equals_len_for_month() {
        let w = resolve(Period::Month, d(2026, 6, 19));
        assert_eq!(w.day_index() + w.days_left(), w.len_days());
    }
}
