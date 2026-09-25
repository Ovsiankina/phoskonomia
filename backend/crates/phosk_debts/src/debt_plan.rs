//! Debt plan path — adjust the plan (monthly / day / term) and refinance (T17).
//!
//! A service over the [`DatabaseAdapter`] PORT, following [`crate::debt_write`]:
//! validate the request, merge it into the stored [`Debt`], validate the merged
//! record, stamp [`Provenance::user_modified`] and append one correction event
//! per changed field. A request that changes nothing writes nothing.
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
//!   the centime so `n` instalments clear the debt (`⌈B / n⌉` at 0 %);
//! - `day` only moves the due date; the amortisation is monthly, not daily.
//!
//! `Debt::term` keeps meaning the **whole contract length** (what
//! [`crate::debt_write::create_debt`] stores, and why a recorded payment leaves
//! it alone): after a replan it becomes the whole months elapsed since `since`
//! plus the derived horizon. A revolving debt (`term == 0`) stays revolving when
//! only its instalment or rate changes; only an explicit `remaining_term`
//! turns it into a fixed plan. A plan that never pays off (instalment at or
//! below the monthly interest) is rejected: that is not a plan.
//!
//! **Refinance** re-prices the *outstanding* balance: a new `apr`, optionally a
//! new lender and the same plan inputs as above. When neither `monthly` nor
//! `remaining_term` is given the instalment is kept and the horizon re-derived
//! at the new rate. To keep history truthful nothing about the past moves —
//! `balance`, `orig`, `since` and the recorded payments stay as they were, and
//! the old rate / lender / plan survive in the correction audit log, dated with
//! the refinance's effective date.
//!
//! [`DatabaseAdapter`]: phosk_adapter_db::DatabaseAdapter

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::{Debt, Provenance};

use crate::debt_write::{normalized, validate, write_audited};
use crate::debts::months_to_payoff;

/// The read side's revolving sentinel: `monthsToPayoff` is pinned here when a
/// debt never amortises, so a real plan must be shorter.
const REVOLVING_MONTHS: i32 = 600;

/// How many centimes the rounded-up annuity may still be nudged to absorb float
/// noise before we give up (in practice zero or one step is needed).
const ANNUITY_NUDGE: i64 = 16;

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
/// `remaining_term`, targets a paid-off debt, or yields a plan that is out of
/// range or never pays off; otherwise any port error.
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
    let current = outstanding(db, slug).await?;
    let mut next = current.clone();
    if let Some(day) = adjust.day {
        next.day = day;
    }
    if adjust.monthly.is_some() || adjust.remaining_term.is_some() {
        replan(&mut next, adjust.monthly, adjust.remaining_term, on)?;
    }
    commit(db, &current, next, on).await
}

/// Refinance a debt's outstanding balance on new terms, effective `on`. Write
/// path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no debt,
/// [`PhoskError::Invalid`] if the rate is not in `0.0..=1.0`, the lender is
/// blank, the debt is paid off, or the resulting plan is invalid (see
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
    let current = outstanding(db, slug).await?;
    let mut next = current.clone();
    next.apr = refi.apr;
    if let Some(lender) = refi.lender {
        next.lender = lender;
    }
    if let Some(day) = refi.day {
        next.day = day;
    }
    // The rate changed, so the horizon must be re-derived even when the caller
    // keeps the instalment.
    let monthly = match (refi.monthly, refi.remaining_term) {
        (None, None) => Some(current.monthly),
        (monthly, _) => monthly,
    };
    replan(&mut next, monthly, refi.remaining_term, on)?;
    commit(db, &current, next, on).await
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

/// Set `debt`'s instalment and contract term from exactly one of `monthly` /
/// `remaining` at its (already updated) rate.
fn replan(
    debt: &mut Debt,
    monthly: Option<Money>,
    remaining: Option<u32>,
    on: NaiveDate,
) -> Result<(), PhoskError> {
    let rate = debt.apr / 12.0;
    let elapsed = elapsed_months(debt.since, on);
    match (monthly, remaining) {
        (Some(_), Some(_)) => Err(PhoskError::Invalid(
            "give the instalment or the remaining term, not both: one derives the other".to_owned(),
        )),
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
            debt.monthly = annuity(debt.balance, rate, remaining)?;
            debt.term = contract_term(elapsed, remaining)?;
            Ok(())
        }
    }
}

/// The instalment that clears `balance` in `months` at `rate` per month,
/// rounded up to the centime — then nudged up by whole centimes, if float noise
/// requires it, until the read side's engine agrees it clears in time.
fn annuity(balance: Money, rate: f64, months: u32) -> Result<Money, PhoskError> {
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
        if months_to_payoff(balance, monthly, rate)? <= n {
            return Ok(monthly);
        }
    }
    Err(PhoskError::Invalid(format!(
        "no instalment near {first} centimes clears the debt in {months} months"
    )))
}

/// Whole months from `since` to `on` (0 when `on` is not after `since`).
fn elapsed_months(since: NaiveDate, on: NaiveDate) -> u32 {
    let mut months =
        i64::from(on.year() - since.year()) * 12 + i64::from(on.month()) - i64::from(since.month());
    if on.day() < since.day() {
        months -= 1;
    }
    u32::try_from(months).unwrap_or(0)
}

/// The whole contract length: months already elapsed plus months to go.
fn contract_term(elapsed: u32, remaining: u32) -> Result<u32, PhoskError> {
    elapsed
        .checked_add(remaining)
        .ok_or_else(|| PhoskError::Overflow("debt term overflow".to_owned()))
}

/// Validate the merged record and write it with its audit trail — unless it
/// equals what is stored, in which case nothing is written or re-stamped.
async fn commit(
    db: &dyn DatabaseAdapter,
    current: &Debt,
    next: Debt,
    on: NaiveDate,
) -> Result<(), PhoskError> {
    let mut next = normalized(next);
    validate(&next)?;
    if next == *current {
        return Ok(());
    }
    next.provenance = Provenance::user_modified();
    write_audited(db, current, next, on).await
}
