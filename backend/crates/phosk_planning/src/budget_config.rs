//! Global budget write side: the monthly budget ceiling and the savings target.
//!
//! [`BudgetConfig`] is a singleton the read side already consumes
//! ([`budget_totals`](crate::budgets::budget_totals), [`totals`](crate::totals)),
//! so a setter only has to validate, write it through the port, and append a
//! [`BudgetChange`] history entry — the entry carries the edit's
//! [`Provenance`] (`UserModified`), since the config row has none of its own.
//!
//! ## Rules
//! - The monthly budget must be strictly positive: a zero ceiling makes every
//!   remaining / pace figure meaningless.
//! - The savings target may be zero ("no target") but never negative.
//! - Setting the current value is a successful no-op with no history entry,
//!   mirroring how a same-name rename or an unchanged subscription field
//!   records nothing.
//! - Each setter reads the config, then writes it back whole: two setters
//!   running at once can lose one field while the history records both (see
//!   [`DatabaseAdapter::set_budget_config`]). The app has a single local
//!   user, so this is not guarded yet.

use chrono::NaiveDate;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::BudgetChangeId;
use phosk_model::{BudgetChange, BudgetConfig, Provenance};

/// The `field` of a monthly-budget [`BudgetChange`].
pub const FIELD_MONTHLY_BUDGET: &str = "monthly_budget";
/// The `field` of a savings-target [`BudgetChange`].
pub const FIELD_SAVINGS_TARGET: &str = "savings_target";

/// Set the cycle's overall spend ceiling. Write path; returns the resulting
/// config.
///
/// # Errors
/// [`PhoskError::Invalid`] if `amount` is not strictly positive; otherwise any
/// port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn set_monthly_budget(
    db: &dyn DatabaseAdapter,
    amount: Money,
    as_of: NaiveDate,
) -> Result<BudgetConfig, PhoskError> {
    if amount <= Money::ZERO {
        return Err(PhoskError::Invalid(
            "the monthly budget must be greater than zero".to_owned(),
        ));
    }
    let current = db.budget_config().await?;
    let old = current.monthly_budget;
    let next = BudgetConfig {
        monthly_budget: amount,
        ..current
    };
    apply(db, next, FIELD_MONTHLY_BUDGET, old, amount, as_of).await
}

/// Set the cycle's savings target (zero means "no target"). Write path;
/// returns the resulting config.
///
/// # Errors
/// [`PhoskError::Invalid`] if `amount` is negative; otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn set_savings_target(
    db: &dyn DatabaseAdapter,
    amount: Money,
    as_of: NaiveDate,
) -> Result<BudgetConfig, PhoskError> {
    if amount < Money::ZERO {
        return Err(PhoskError::Invalid(
            "the savings target cannot be negative".to_owned(),
        ));
    }
    let current = db.budget_config().await?;
    let old = current.savings_target;
    let next = BudgetConfig {
        savings_target: amount,
        ..current
    };
    apply(db, next, FIELD_SAVINGS_TARGET, old, amount, as_of).await
}

/// The global budget's change history, oldest→newest.
///
/// # Errors
/// Any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn budget_changes(db: &dyn DatabaseAdapter) -> Result<Vec<BudgetChange>, PhoskError> {
    db.budget_changes().await
}

/// Write `next` with its history entry, unless nothing changed.
async fn apply(
    db: &dyn DatabaseAdapter,
    next: BudgetConfig,
    field: &str,
    old_value: Money,
    new_value: Money,
    as_of: NaiveDate,
) -> Result<BudgetConfig, PhoskError> {
    if old_value == new_value {
        return Ok(next);
    }
    let change = BudgetChange {
        id: BudgetChangeId::new(),
        field: field.to_owned(),
        old_value,
        new_value,
        at: as_of,
        provenance: Provenance::user_modified(),
    };
    db.set_budget_config(next.clone(), change).await?;
    Ok(next)
}
