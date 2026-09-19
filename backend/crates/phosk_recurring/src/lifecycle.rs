//! Subscription lifecycle — pause · resume · cancel · mark-paid · record
//! charge (T15), plus the derived status those transitions maintain.
//!
//! A service over the `DatabaseAdapter` PORT, like the create/edit/delete side
//! in [`crate::subscription_write`]: it validates the transition, writes
//! through the port and re-stamps [`Provenance`]. Every status change is also
//! appended to the correction audit log.
//!
//! **The status vocabulary.** [`Subscription::status`] carries one of
//! [`STATUS_OK`], [`STATUS_SOON`], [`STATUS_DUE`], [`STATUS_WATCH`],
//! [`STATUS_PAUSED`], [`STATUS_CANCELLED`]. The first three are *derived* from
//! the billing cycle and are recomputed by every transition here; the last two
//! are *lifecycle* states the user sets explicitly, and a charge in one of them
//! is no longer active — it stays listed (so it can be resumed or inspected)
//! but leaves the run-rate, the next-30 window and the billing sweep (see
//! [`is_active`]).
//!
//! **Derivation.** A cycle is *settled* when a charge is recorded on or after
//! the cycle's billing day; an unsettled cycle whose billing day has passed is
//! `due` ("not seen"), otherwise the charge is `soon` within
//! [`SOON_DAYS`] of the next billing day and `ok` beyond it. `watch` is a
//! review flag about the charge's *value*, not its cycle, so paying or
//! recording a charge leaves it alone; only an explicit resume — a fresh
//! decision by the user — clears it.
//!
//! **Cancel is not delete.** Cancelling ends a standing charge but keeps its
//! recorded history for the ledger; [`crate::subscription_write::delete_subscription`]
//! is the destructive one.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{ChargeId, CorrectionId};
use phosk_model::{Charge, CorrectionEvent, Provenance, Subscription};
use serde::{Deserialize, Serialize};

use crate::subscriptions::{days_until, last_charge_date};

/// Derived: the cycle is settled and the next charge is far off.
pub const STATUS_OK: &str = "ok";
/// Derived: the next charge lands within [`SOON_DAYS`].
pub const STATUS_SOON: &str = "soon";
/// Derived: the current cycle's charge was never recorded.
pub const STATUS_DUE: &str = "due";
/// Flag: the charge is under review (set outside the billing cycle).
pub const STATUS_WATCH: &str = "watch";
/// Lifecycle: the user suspended the charge.
pub const STATUS_PAUSED: &str = "paused";
/// Lifecycle: the user ended the charge (history kept).
pub const STATUS_CANCELLED: &str = "cancelled";

/// How many days ahead of the next charge counts as "due soon".
const SOON_DAYS: i32 = 7;

/// A billing event observed for a standing charge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCharge {
    /// When it was billed. Must be within the charge's life: on or after
    /// `since`, never in the future.
    pub date: NaiveDate,
    /// Amount billed, exact centimes. Must be > 0.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Free-text note, e.g. `"confirmed"`.
    pub note: String,
}

/// Whether a status key describes a charge that still costs money: everything
/// except [`STATUS_PAUSED`] and [`STATUS_CANCELLED`].
#[must_use]
pub fn is_active(status: &str) -> bool {
    !matches!(status, STATUS_PAUSED | STATUS_CANCELLED)
}

/// Suspend a standing charge. Write path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no subscription,
/// [`PhoskError::Invalid`] if it is already paused or cancelled; otherwise any
/// port error.
#[tracing::instrument(level = "debug", skip(db))]
pub async fn pause_subscription(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError> {
    let sub = db.subscription_by_slug(slug).await?;
    match sub.status.as_str() {
        STATUS_PAUSED => Err(PhoskError::Invalid(format!(
            "{:?} is already paused",
            sub.name
        ))),
        STATUS_CANCELLED => Err(PhoskError::Invalid(format!(
            "{:?} is cancelled and cannot be paused",
            sub.name
        ))),
        _ => set_status(db, sub, STATUS_PAUSED).await,
    }
}

/// Un-suspend a standing charge, recomputing its status from the billing cycle
/// as it stands at `as_of`. Write path.
///
/// Resuming is a fresh decision about the charge, so it also clears a
/// [`STATUS_WATCH`] review flag the charge carried before it was paused.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no subscription,
/// [`PhoskError::Invalid`] if it is not paused; otherwise any port error.
#[tracing::instrument(level = "debug", skip(db))]
pub async fn resume_subscription(
    db: &dyn DatabaseAdapter,
    slug: &str,
    as_of: NaiveDate,
) -> Result<(), PhoskError> {
    let sub = db.subscription_by_slug(slug).await?;
    if sub.status != STATUS_PAUSED {
        return Err(PhoskError::Invalid(format!(
            "{:?} is not paused (status {:?})",
            sub.name, sub.status
        )));
    }
    let charges = db.subscription_charges(sub.id).await?;
    // No previous status: a resume re-derives the cycle from scratch.
    let status = derive_status("", &sub, &charges, as_of)?;
    set_status(db, sub, &status).await
}

/// End a standing charge, keeping its recorded history. Write path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no subscription,
/// [`PhoskError::Invalid`] if it is already cancelled; otherwise any port
/// error.
#[tracing::instrument(level = "debug", skip(db))]
pub async fn cancel_subscription(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError> {
    let sub = db.subscription_by_slug(slug).await?;
    if sub.status == STATUS_CANCELLED {
        return Err(PhoskError::Invalid(format!(
            "{:?} is already cancelled",
            sub.name
        )));
    }
    set_status(db, sub, STATUS_CANCELLED).await
}

/// Settle the current billing cycle: record a charge at the standing amount,
/// dated the cycle's billing day, and refresh the derived status. Write path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no subscription,
/// [`PhoskError::Invalid`] if the charge is paused/cancelled or the cycle
/// already holds a recorded charge; otherwise any port error.
#[tracing::instrument(level = "debug", skip(db))]
pub async fn mark_paid(
    db: &dyn DatabaseAdapter,
    slug: &str,
    as_of: NaiveDate,
) -> Result<(), PhoskError> {
    let sub = db.subscription_by_slug(slug).await?;
    require_active(&sub)?;
    let charges = db.subscription_charges(sub.id).await?;
    let billed_on = last_charge_date(as_of, sub.day, &sub.cadence, &sub.month)?;
    if cycle_settled(&charges, billed_on, as_of) {
        return Err(PhoskError::Invalid(format!(
            "{:?} already has a recorded charge for this cycle",
            sub.name
        )));
    }
    // A cycle that opened before the charge was being tracked is settled as of
    // today rather than back-dated into a period with no coverage.
    let date = if billed_on >= sub.since {
        billed_on
    } else {
        as_of
    };
    append_charge(
        db,
        &sub,
        Charge {
            id: ChargeId::new(),
            subscription_id: sub.id,
            date,
            amount: sub.amount,
            note: "marked paid".to_owned(),
            provenance: Provenance::user_entered(),
        },
        as_of,
    )
    .await
}

/// Record an observed billing event and refresh the derived status. Write path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no subscription,
/// [`PhoskError::Invalid`] if the charge is paused/cancelled, the amount is not
/// positive, or the date falls outside the charge's life (before `since`, or in
/// the future relative to `as_of`); otherwise any port error.
#[tracing::instrument(level = "debug", skip(db))]
pub async fn record_subscription_charge(
    db: &dyn DatabaseAdapter,
    slug: &str,
    as_of: NaiveDate,
    input: NewCharge,
) -> Result<(), PhoskError> {
    let sub = db.subscription_by_slug(slug).await?;
    require_active(&sub)?;
    if input.amount.centimes() <= 0 {
        return Err(PhoskError::Invalid(format!(
            "charge amount must be positive, got {} centimes",
            input.amount.centimes()
        )));
    }
    if input.date > as_of {
        return Err(PhoskError::Invalid(format!(
            "a charge cannot be recorded in the future ({} > {as_of})",
            input.date
        )));
    }
    if input.date < sub.since {
        return Err(PhoskError::Invalid(format!(
            "{:?} has only been tracked since {} ({} is earlier)",
            sub.name, sub.since, input.date
        )));
    }
    append_charge(
        db,
        &sub,
        Charge {
            id: ChargeId::new(),
            subscription_id: sub.id,
            date: input.date,
            amount: input.amount,
            note: input.note.trim().to_owned(),
            provenance: Provenance::user_entered(),
        },
        as_of,
    )
    .await
}

// ── derivation & helpers ──────────────────────────────────────────────────────

/// The status a charge's billing cycle implies at `as_of`.
///
/// `previous` is the status the charge carried before the transition: a
/// [`STATUS_WATCH`] review flag survives (it is a judgement about the charge,
/// not about the cycle). Pass `""` to derive from the cycle alone.
fn derive_status(
    previous: &str,
    sub: &Subscription,
    charges: &[Charge],
    as_of: NaiveDate,
) -> Result<String, PhoskError> {
    if previous == STATUS_WATCH {
        return Ok(STATUS_WATCH.to_owned());
    }
    let billed_on = last_charge_date(as_of, sub.day, &sub.cadence, &sub.month)?;
    // A cycle that opened before tracking started cannot be "not seen".
    if billed_on >= sub.since && !cycle_settled(charges, billed_on, as_of) {
        return Ok(STATUS_DUE.to_owned());
    }
    let days = days_until(as_of, sub.day, &sub.cadence, &sub.month)?;
    Ok(if days <= SOON_DAYS {
        STATUS_SOON
    } else {
        STATUS_OK
    }
    .to_owned())
}

/// Whether a charge has been recorded for the cycle that opened on
/// `billed_on`.
fn cycle_settled(charges: &[Charge], billed_on: NaiveDate, as_of: NaiveDate) -> bool {
    charges
        .iter()
        .any(|c| c.date >= billed_on && c.date <= as_of)
}

/// Reject a transition that only makes sense for a live charge.
fn require_active(sub: &Subscription) -> Result<(), PhoskError> {
    if is_active(&sub.status) {
        return Ok(());
    }
    Err(PhoskError::Invalid(format!(
        "{:?} is {} — resume it before recording charges",
        sub.name, sub.status
    )))
}

/// Record a charge, then refresh the subscription's derived status.
async fn append_charge(
    db: &dyn DatabaseAdapter,
    sub: &Subscription,
    charge: Charge,
    as_of: NaiveDate,
) -> Result<(), PhoskError> {
    db.record_charge(charge).await?;
    let charges = db.subscription_charges(sub.id).await?;
    let status = derive_status(&sub.status, sub, &charges, as_of)?;
    set_status(db, sub.clone(), &status).await
}

/// Write a new status, re-stamping provenance and appending the change to the
/// correction audit log. A no-op when the status is already `status`.
async fn set_status(
    db: &dyn DatabaseAdapter,
    mut sub: Subscription,
    status: &str,
) -> Result<(), PhoskError> {
    if sub.status == status {
        return Ok(());
    }
    let entity_id = sub.id.to_string();
    let old_value = std::mem::replace(&mut sub.status, status.to_owned());
    sub.provenance = Provenance::user_modified();
    db.upsert_subscription(sub).await?;
    db.record_correction(CorrectionEvent {
        id: CorrectionId::new(),
        entity_id,
        field: "status".to_owned(),
        old_value,
        new_value: status.to_owned(),
        at: chrono::Utc::now().date_naive(),
    })
    .await
}
