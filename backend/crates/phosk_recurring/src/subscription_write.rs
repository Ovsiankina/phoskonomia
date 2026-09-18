//! Subscription write path — create · edit · delete (T14).
//!
//! A service over the `DatabaseAdapter` PORT, like the read side in
//! [`crate::subscriptions`]: it validates caller input, stamps
//! [`Provenance`](phosk_model::Provenance) and delegates the storage to the
//! port. Lifecycle transitions (pause/resume/cancel/mark-paid/record charge)
//! are a separate concern and are NOT here.
//!
//! **Identity.** A subscription is addressed on the wire by its `slug` (that is
//! what [`crate::subscriptions::SubscriptionDto::id`] carries). The slug is
//! derived from the name once, at creation, and then frozen: renaming
//! "Netflix" to "Netflix Standard" must not break a link, a bookmark or a
//! stored reference. Two charges may therefore not share a name.
//!
//! **Provenance.** `Subscription::source` records where the RECORD came from
//! (typed in, or detected by the LLM) and drives the "auto-detected" KPI, so an
//! edit leaves it alone; `Subscription::provenance` describes the CURRENT
//! values and flips to [`Provenance::user_modified`] on every edit. Each
//! changed field is also appended to the correction audit log.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{CorrectionId, SubscriptionId};
use phosk_model::{CorrectionEvent, Provenance, Source, Subscription};
use serde::{Deserialize, Serialize};

use crate::subscriptions::month_from_abbr;

/// The lowest day-of-month a monthly charge may land on.
const MIN_DAY: u32 = 1;
/// The highest day-of-month a monthly charge may land on. Short months clamp on
/// read (see `crate::subscriptions`), so the 31st is accepted here.
const MAX_DAY: u32 = 31;

/// A new standing charge, as typed in by the user.
///
/// `day` is only read for `cadence == "monthly"`, `month` only for
/// `"yearly"`; the irrelevant one is normalised away (`0` / `""`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSubscription {
    /// Display name; also the source of the slug.
    pub name: String,
    /// Per-charge amount, exact centimes. Must be > 0.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// `"monthly"` or `"yearly"` (case-insensitive).
    pub cadence: String,
    /// Day-of-month for a monthly charge, `1..=31`.
    pub day: u32,
    /// Month label for a yearly charge, e.g. `"FEB"` (case-insensitive).
    pub month: String,
    /// Category name.
    pub category: String,
    /// Display glyph.
    pub glyph: String,
    /// Free-text note.
    pub note: String,
    /// Date the subscription began.
    pub since: NaiveDate,
}

/// A partial edit of an existing charge: `None` leaves the field alone.
///
/// The merged record is validated as a whole, so a patch that is fine in
/// isolation (`cadence: "yearly"`) is still rejected when the result would be
/// inconsistent (yearly without a month).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionEdit {
    /// New display name (the slug stays as it was).
    pub name: Option<String>,
    /// New per-charge amount, exact centimes.
    #[serde(default, with = "phosk_model::opt_money_centimes")]
    pub amount: Option<Money>,
    /// New cadence.
    pub cadence: Option<String>,
    /// New day-of-month.
    pub day: Option<u32>,
    /// New month label.
    pub month: Option<String>,
    /// New category.
    pub category: Option<String>,
    /// New glyph.
    pub glyph: Option<String>,
    /// New note.
    pub note: Option<String>,
    /// New tracking-since date.
    pub since: Option<NaiveDate>,
}

/// Create a standing charge from user input. Write path.
///
/// Returns the new charge's slug — the id the read DTOs and the UI use.
///
/// # Errors
/// [`PhoskError::Invalid`] if the input is unusable (blank/unslugifiable name,
/// a name whose slug is already taken, a non-positive amount, an unknown
/// cadence, a day outside `1..=31` for a monthly charge, an unknown month for a
/// yearly one, a blank category); otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn create_subscription(
    db: &dyn DatabaseAdapter,
    input: NewSubscription,
) -> Result<String, PhoskError> {
    let name = input.name.trim().to_owned();
    let slug = slugify(&name);
    if slug.is_empty() {
        return Err(PhoskError::Invalid(format!(
            "subscription name {:?} has no usable slug",
            input.name
        )));
    }
    if db.subscription_by_slug(&slug).await.is_ok() {
        return Err(PhoskError::Invalid(format!(
            "a subscription named {name:?} already exists"
        )));
    }

    let sub = normalized(Subscription {
        id: SubscriptionId::new(),
        slug: slug.clone(),
        name,
        amount: input.amount,
        cadence: input.cadence,
        day: input.day,
        month: input.month,
        // A fresh charge is healthy; the lifecycle states are set elsewhere.
        status: "ok".to_owned(),
        category: input.category,
        glyph: input.glyph,
        since: input.since,
        note: input.note,
        source: Source::UserEntered,
        provenance: Provenance::user_entered(),
    });
    validate(&sub)?;
    db.upsert_subscription(sub).await?;
    Ok(slug)
}

/// Edit an existing standing charge, addressed by its slug. Write path.
///
/// Only the `Some` fields of `edit` change; the merged record is validated
/// before anything is written, so a rejected edit leaves the store untouched.
/// Every changed field is appended to the correction audit log and the record's
/// provenance becomes [`Provenance::user_modified`].
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no subscription,
/// [`PhoskError::Invalid`] if the merged record would be inconsistent (see
/// [`create_subscription`]); otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn edit_subscription(
    db: &dyn DatabaseAdapter,
    slug: &str,
    edit: SubscriptionEdit,
) -> Result<(), PhoskError> {
    let current = db.subscription_by_slug(slug).await?;

    let mut next = current.clone();
    if let Some(name) = edit.name {
        name.trim().clone_into(&mut next.name);
    }
    if let Some(amount) = edit.amount {
        next.amount = amount;
    }
    if let Some(cadence) = edit.cadence {
        next.cadence = cadence;
        // A cadence switch invalidates the other cadence's anchor unless the
        // same patch supplies a new one; `normalized` clears the stale field.
        if edit.day.is_none() && edit.month.is_none() {
            next.day = 0;
            next.month = String::new();
        }
    }
    if let Some(day) = edit.day {
        next.day = day;
    }
    if let Some(month) = edit.month {
        next.month = month;
    }
    if let Some(category) = edit.category {
        next.category = category;
    }
    if let Some(glyph) = edit.glyph {
        next.glyph = glyph;
    }
    if let Some(note) = edit.note {
        next.note = note;
    }
    if let Some(since) = edit.since {
        next.since = since;
    }

    let mut next = normalized(next);
    validate(&next)?;
    if next.name != current.name {
        // The slug is frozen at creation, but a rename must not collide with
        // another charge's identity either.
        let renamed = slugify(&next.name);
        if renamed != current.slug && db.subscription_by_slug(&renamed).await.is_ok() {
            return Err(PhoskError::Invalid(format!(
                "a subscription named {:?} already exists",
                next.name
            )));
        }
    }
    next.provenance = Provenance::user_modified();

    let changes = changed_fields(&current, &next);
    db.upsert_subscription(next).await?;
    for (field, old_value, new_value) in changes {
        db.record_correction(CorrectionEvent {
            id: CorrectionId::new(),
            entity_id: current.id.to_string(),
            field: field.to_owned(),
            old_value,
            new_value,
            at: chrono::Utc::now().date_naive(),
        })
        .await?;
    }
    Ok(())
}

/// Delete a standing charge and its recorded charges, addressed by its slug.
/// Write path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no subscription; otherwise
/// any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn delete_subscription(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError> {
    let sub = db.subscription_by_slug(slug).await?;
    db.delete_subscription(sub.id).await
}

// ── validation & normalisation ────────────────────────────────────────────────

/// A url/id-safe slug: lower-case ASCII alphanumerics, every other run of
/// characters collapsed to a single `-`, no leading/trailing `-`. Returns an
/// empty string when nothing usable is left (the caller rejects that).
#[must_use]
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// Trim the free-text fields and normalise the cadence anchor: a monthly charge
/// keeps `day` and drops `month`, a yearly charge keeps an upper-case `month`
/// and drops `day`. Validation runs on the result.
fn normalized(sub: Subscription) -> Subscription {
    let cadence = sub.cadence.trim().to_ascii_lowercase();
    let yearly = cadence == "yearly";
    Subscription {
        name: sub.name.trim().to_owned(),
        cadence,
        day: if yearly { 0 } else { sub.day },
        month: if yearly {
            sub.month.trim().to_ascii_uppercase()
        } else {
            String::new()
        },
        category: sub.category.trim().to_owned(),
        glyph: sub.glyph.trim().to_owned(),
        note: sub.note.trim().to_owned(),
        ..sub
    }
}

/// Reject a record the read side could not make sense of.
fn validate(sub: &Subscription) -> Result<(), PhoskError> {
    if sub.name.is_empty() {
        return Err(PhoskError::Invalid("subscription name is empty".to_owned()));
    }
    if sub.amount.centimes() <= 0 {
        return Err(PhoskError::Invalid(format!(
            "subscription amount must be positive, got {} centimes",
            sub.amount.centimes()
        )));
    }
    if sub.category.is_empty() {
        return Err(PhoskError::Invalid(
            "subscription category is empty".to_owned(),
        ));
    }
    match sub.cadence.as_str() {
        "monthly" => {
            if !(MIN_DAY..=MAX_DAY).contains(&sub.day) {
                return Err(PhoskError::Invalid(format!(
                    "monthly charge day must be {MIN_DAY}..={MAX_DAY}, got {}",
                    sub.day
                )));
            }
        }
        "yearly" => {
            if month_from_abbr(&sub.month).is_none() {
                return Err(PhoskError::Invalid(format!(
                    "yearly charge needs a month like \"FEB\", got {:?}",
                    sub.month
                )));
            }
        }
        other => {
            return Err(PhoskError::Invalid(format!(
                "cadence must be \"monthly\" or \"yearly\", got {other:?}"
            )));
        }
    }
    Ok(())
}

/// The `(field, old, new)` triples an edit actually changed, for the audit log.
fn changed_fields(
    before: &Subscription,
    after: &Subscription,
) -> Vec<(&'static str, String, String)> {
    let mut out = Vec::new();
    let mut push = |field: &'static str, old: String, new: String| {
        if old != new {
            out.push((field, old, new));
        }
    };
    push("name", before.name.clone(), after.name.clone());
    push(
        "amount",
        before.amount.centimes().to_string(),
        after.amount.centimes().to_string(),
    );
    push("cadence", before.cadence.clone(), after.cadence.clone());
    push("day", before.day.to_string(), after.day.to_string());
    push("month", before.month.clone(), after.month.clone());
    push("category", before.category.clone(), after.category.clone());
    push("glyph", before.glyph.clone(), after.glyph.clone());
    push("note", before.note.clone(), after.note.clone());
    push("since", before.since.to_string(), after.since.to_string());
    out
}
