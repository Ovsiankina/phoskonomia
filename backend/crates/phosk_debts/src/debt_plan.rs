//! Debt plan path — adjust the plan (monthly / day / term) and refinance (T17).
//!
//! A service over the [`DatabaseAdapter`] PORT, following [`crate::debt_write`]:
//! validate the request, merge it into the stored [`Debt`], validate the merged
//! record, stamp [`Provenance::user_modified`] and append one correction event
//! per changed field, dated with the wall clock like every other audit writer.
//! A request that changes nothing writes nothing — restating the stored
//! instalment, or refinancing at the stored rate, is not a change.
//!
//! The **effective date** `on` only counts the whole months elapsed since the
//! debt opened; it must lie between `since` and a year from today.
//!
//! **Which value is derived.** The read side ([`crate::debts`]) derives the
//! payoff horizon from `balance`, `monthly` and `apr` alone, so the instalment
//! and the remaining term are two ends of one amortisation. A request may set
//! at most one of them, and the other follows:
//!
//! - a new `monthly` → the remaining months come from the read side's own
//!   engine (`monthsToPayoff`), so the two can never disagree;
//! - a new `remaining_term` (months still to pay, counted from the effective
//!   date) → the annuity instalment `B·r / (1 − (1+r)^−n)`, rounded **up** to
//!   the centime (`⌈B / n⌉` at 0 %) and nudged up until `n` instalments clear
//!   the debt both in the read side's projection and through the payments the
//!   write path records (interest rounded to the centime each month);
//! - `day` only moves the due date; the amortisation is monthly, not daily.
//!
//! `Debt::term` keeps meaning the **whole contract length** (what
//! [`crate::debt_write::create_debt`] stores, and why a recorded payment leaves
//! it alone): after a replan it becomes the whole months elapsed since `since`
//! plus the horizon the new plan actually pays off in. A revolving debt (`term == 0`) stays revolving when
//! only its instalment or rate changes; only an explicit `remaining_term`
//! turns it into a fixed plan. A plan that never pays off (instalment at or
//! below the monthly interest) is rejected: that is not a plan.
//!
//! **Refinance** re-prices the *outstanding* balance: a new `apr`, optionally a
//! new lender and the same plan inputs as above. When neither `monthly` nor
//! `remaining_term` is given the instalment is kept and the horizon re-derived
//! at the new rate. To keep history truthful nothing about the past moves —
//! `balance`, `orig`, `since` and the recorded payments stay as they were, and
//! the old rate / lender / plan survive in the correction audit log. A debt
//! with no instalment (a zero-monthly revolving debt) needs a new `monthly` or
//! `remaining_term` to be refinanced.
//!
//! [`DatabaseAdapter`]: phosk_adapter_db::DatabaseAdapter

use chrono::{Datelike, Days, Months, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::{Debt, Provenance};

use crate::debt_write::{monthly_interest, normalized, validate, write_audited};
use crate::debts::months_to_payoff;

/// The read side's revolving sentinel: `monthsToPayoff` is pinned here when a
/// debt never amortises, so a real plan must be shorter.
const REVOLVING_MONTHS: i32 = 600;

/// How many centimes the rounded-up annuity may still be nudged to absorb float
/// noise before we give up (in practice zero or one step is needed).
const ANNUITY_NUDGE: i64 = 64;

/// How far ahead of today a plan change may take effect.
const MAX_LEAD_DAYS: u64 = 366;

/// A change to a debt's repayment plan. `None` leaves the value alone; at most
/// one of `monthly` / `remaining_term` may be set (the other is derived).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanAdjust {
    /// New scheduled instalment, exact centimes. Must pay off the debt.
    #[serde(default, with = "phosk_model::opt_money_centimes")]
    pub monthly: Option<Money>,
    /// New payment day-of-month, `1..=31`.
    pub day: Option<u32>,
    /// Months still to pay from the effective date, `1..600`.
    pub remaining_term: Option<u32>,
}

/// New terms for the outstanding balance of a debt.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refinance {
    /// New annual percentage rate, `0.0..=1.0`.
    pub apr: f64,
    /// New lender (`None` = same lender).
    pub lender: Option<String>,
    /// New instalment, exact centimes (see [`PlanAdjust::monthly`]).
    #[serde(default, with = "phosk_model::opt_money_centimes")]
    pub monthly: Option<Money>,
    /// New payment day-of-month.
    pub day: Option<u32>,
    /// Months still to pay (see [`PlanAdjust::remaining_term`]).
    pub remaining_term: Option<u32>,
}

/// Adjust a debt's repayment plan, effective `on`. Write path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no debt,
/// [`PhoskError::Invalid`] if the request is empty, sets both `monthly` and
/// `remaining_term`, targets a paid-off debt, is effective before the debt
/// opened or more than a year ahead, or yields a plan that is out of range or
/// never pays off; otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn adjust_plan(
    db: &dyn DatabaseAdapter,
    slug: &str,
    on: NaiveDate,
    adjust: PlanAdjust,
) -> Result<(), PhoskError> {
    if adjust == PlanAdjust::default() {
        return Err(PhoskError::Invalid(
            "a plan adjustment must change the instalment, the day or the term".to_owned(),
        ));
    }
    one_of(adjust.monthly, adjust.remaining_term)?;
    let current = outstanding(db, slug).await?;
    let elapsed = elapsed_months(&current, on)?;
    let mut next = current.clone();
    if let Some(day) = adjust.day {
        next.day = day;
    }
    // Restating the stored instalment is not a change: re-deriving the term
    // from it would rewrite a contract nobody asked to touch.
    let monthly = adjust.monthly.filter(|m| *m != current.monthly);
    replan(&mut next, monthly, adjust.remaining_term, elapsed)?;
    commit(db, &current, next).await
}

/// Refinance a debt's outstanding balance on new terms, effective `on`. Write
/// path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no debt,
/// [`PhoskError::Invalid`] if the rate is not in `0.0..=1.0`, the lender is
/// blank, the debt is paid off or has no instalment to keep, the effective
/// date is out of range, or the resulting plan is invalid (see
/// [`adjust_plan`]); otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn refinance(
    db: &dyn DatabaseAdapter,
    slug: &str,
    on: NaiveDate,
    refi: Refinance,
) -> Result<(), PhoskError> {
    if !(refi.apr.is_finite() && (0.0..=1.0).contains(&refi.apr)) {
        return Err(PhoskError::Invalid(format!(
            "apr must be a rate in 0.0..=1.0, got {}",
            refi.apr
        )));
    }
    one_of(refi.monthly, refi.remaining_term)?;
    let current = outstanding(db, slug).await?;
    let elapsed = elapsed_months(&current, on)?;
    let rate_changed = refi.apr.to_bits() != current.apr.to_bits();
    let mut next = current.clone();
    next.apr = refi.apr;
    if let Some(lender) = refi.lender {
        next.lender = lender;
    }
    if let Some(day) = refi.day {
        next.day = day;
    }
    // At the stored rate, restating the stored instalment changes nothing.
    let monthly = refi
        .monthly
        .filter(|m| rate_changed || *m != current.monthly);
    // A new rate re-derives the horizon even when the caller keeps the
    // instalment — which needs an instalment to keep.
    let monthly = match (monthly, refi.remaining_term) {
        (None, None) if rate_changed => {
            if current.monthly.centimes() <= 0 {
                return Err(PhoskError::Invalid(format!(
                    "{slug:?} has no instalment to keep: refinancing it needs a plan — a new monthly payment or remaining term"
                )));
            }
            Some(current.monthly)
        }
        (monthly, _) => monthly,
    };
    replan(&mut next, monthly, refi.remaining_term, elapsed)?;
    commit(db, &current, next).await
}

/// The debt behind `slug`, provided something is still owed on it.
async fn outstanding(db: &dyn DatabaseAdapter, slug: &str) -> Result<Debt, PhoskError> {
    let debt = db.debt_by_slug(slug).await?;
    if debt.balance.centimes() <= 0 {
        return Err(PhoskError::Invalid(format!(
            "debt {slug:?} is paid off; there is no plan to change"
        )));
    }
    Ok(debt)
}

/// The instalment and the remaining term derive each other: at most one.
fn one_of(monthly: Option<Money>, remaining: Option<u32>) -> Result<(), PhoskError> {
    if monthly.is_some() && remaining.is_some() {
        return Err(PhoskError::Invalid(
            "give the instalment or the remaining term, not both: one derives the other".to_owned(),
        ));
    }
    Ok(())
}

/// Set `debt`'s instalment and contract term from at most one of `monthly` /
/// `remaining` at its (already updated) rate, `elapsed` whole months into the
/// contract. Neither given leaves the plan alone.
fn replan(
    debt: &mut Debt,
    monthly: Option<Money>,
    remaining: Option<u32>,
    elapsed: u32,
) -> Result<(), PhoskError> {
    let rate = debt.apr / 12.0;
    match (monthly, remaining) {
        (Some(_), Some(_)) => one_of(monthly, remaining),
        (None, None) => Ok(()),
        (Some(monthly), None) => {
            if monthly.centimes() <= 0 {
                return Err(PhoskError::Invalid(format!(
                    "monthly payment must be positive, got {} centimes",
                    monthly.centimes()
                )));
            }
            let months = months_to_payoff(debt.balance, monthly, rate)?;
            if months >= REVOLVING_MONTHS {
                return Err(PhoskError::Invalid(format!(
                    "{} centimes a month never pays off {:?} at {}",
                    monthly.centimes(),
                    debt.slug,
                    debt.apr
                )));
            }
            debt.monthly = monthly;
            if debt.term != 0 {
                debt.term = contract_term(elapsed, u32::try_from(months).unwrap_or(0))?;
            }
            Ok(())
        }
        (None, Some(remaining)) => {
            if remaining == 0 || i32::try_from(remaining).map_or(true, |n| n >= REVOLVING_MONTHS) {
                return Err(PhoskError::Invalid(format!(
                    "remaining term must be 1..{REVOLVING_MONTHS} months, got {remaining}"
                )));
            }
            debt.monthly = annuity(debt.balance, debt.apr, remaining)?;
            // The real horizon: a rounded-up instalment can clear sooner
            // (100 c over 30 months at 0 % is 4 c a month, gone in 25).
            let months = months_to_payoff(debt.balance, debt.monthly, rate)?;
            debt.term = contract_term(elapsed, u32::try_from(months).unwrap_or(remaining))?;
            Ok(())
        }
    }
}

/// The instalment that clears `balance` in `months` at `apr`, rounded up to
/// the centime — then nudged up by whole centimes, if float noise or monthly
/// interest rounding requires it, until both the read side's engine and the
/// write path's recorded-payment recurrence agree it clears in time.
fn annuity(balance: Money, apr: f64, months: u32) -> Result<Money, PhoskError> {
    let rate = apr / 12.0;
    let n =
        i32::try_from(months).map_err(|_| PhoskError::Overflow("debt term overflow".to_owned()))?;
    #[allow(clippy::cast_precision_loss)]
    let b = balance.centimes() as f64;
    let raw = if rate > 0.0 {
        b * rate / (1.0 - (1.0 + rate).powi(-n))
    } else {
        b / f64::from(months)
    };
    if !raw.is_finite() {
        return Err(PhoskError::Overflow("debt instalment overflow".to_owned()));
    }
    #[allow(clippy::cast_possible_truncation)]
    let first = raw.ceil() as i64;
    for cents in first..first.saturating_add(ANNUITY_NUDGE) {
        let monthly = Money::from_centimes(cents);
        if months_to_payoff(balance, monthly, rate)? <= n
            && clears_when_recorded(balance, monthly, apr, months)?
        {
            return Ok(monthly);
        }
    }
    Err(PhoskError::Invalid(format!(
        "no instalment near {first} centimes clears the debt in {months} months"
    )))
}

/// Whether `months` scheduled instalments of `monthly`, recorded through
/// [`crate::debt_write::record_payment`] (`balance + round(balance · apr/12)
/// − monthly` each month, with the same rounding), clear `balance`.
fn clears_when_recorded(
    balance: Money,
    monthly: Money,
    apr: f64,
    months: u32,
) -> Result<bool, PhoskError> {
    let mut left = balance;
    for _ in 0..months {
        if left.centimes() <= 0 {
            break;
        }
        left = left
            .checked_add(monthly_interest(left, apr)?)?
            .checked_sub(monthly)?;
    }
    Ok(left.centimes() <= 0)
}

/// Whole months from `debt.since` to the effective date `on`: the largest `k`
/// with `since + k months ≤ on`, month ends clamping (31 JAN + 1 month is
/// 28 FEB). `on` must not precede `since` nor lie more than a year ahead.
fn elapsed_months(debt: &Debt, on: NaiveDate) -> Result<u32, PhoskError> {
    let since = debt.since;
    let latest = Utc::now().date_naive() + Days::new(MAX_LEAD_DAYS);
    if on < since || on > latest {
        return Err(PhoskError::Invalid(format!(
            "effective date {on} must be between {since} (when {:?} opened) and {latest}",
            debt.slug
        )));
    }
    let raw =
        i64::from(on.year() - since.year()) * 12 + i64::from(on.month()) - i64::from(since.month());
    let mut months = u32::try_from(raw).unwrap_or(0);
    if since
        .checked_add_months(Months::new(months))
        .is_none_or(|anniversary| anniversary > on)
    {
        months = months.saturating_sub(1);
    }
    Ok(months)
}

/// The whole contract length: months already elapsed plus months to go.
fn contract_term(elapsed: u32, remaining: u32) -> Result<u32, PhoskError> {
    elapsed
        .checked_add(remaining)
        .ok_or_else(|| PhoskError::Overflow("debt term overflow".to_owned()))
}

/// Validate the merged record and write it with its audit trail — unless it
/// equals what is stored, in which case nothing is written or re-stamped.
async fn commit(db: &dyn DatabaseAdapter, current: &Debt, next: Debt) -> Result<(), PhoskError> {
    let mut next = normalized(next);
    validate(&next)?;
    if next == *current {
        return Ok(());
    }
    next.provenance = Provenance::user_modified();
    write_audited(db, current, next, Utc::now().date_naive()).await
}
