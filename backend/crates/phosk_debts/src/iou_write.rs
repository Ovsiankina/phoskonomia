//! Personal-IOU write path — create · edit · delete · record payment · settle
//! (T18).
//!
//! A service over the `DatabaseAdapter` PORT, like the read side in
//! [`crate::personal_ious`]: it validates caller input, stamps
//! [`Provenance`](phosk_model::Provenance) and delegates the storage to the
//! port.
//!
//! **Two amounts, one invariant.** [`PersonalIou::of`] is the ORIGINAL sum and
//! [`PersonalIou::amount`] what is still outstanding; the read side derives
//! `repaidPct` from the gap between them. Every write here keeps
//! `0 ≤ amount ≤ of`, so the read model can never show a repaid fraction
//! outside `0..=1`.
//!
//! **Identity.** An IOU is addressed on the wire by its `slug` (that is what
//! [`crate::personal_ious::PersonalIouDto::id`] carries). It is derived from the
//! person's name once, at creation, then frozen: the same person may lend twice,
//! so a taken slug gets a `-2`, `-3`, … suffix rather than being rejected, and
//! renaming a person does not move the id.
//!
//! **Provenance.** A created IOU is [`Provenance::user_entered`]; any later
//! change — an edit, a payment, a settlement — flips it to
//! [`Provenance::user_modified`] and appends the changed fields to the
//! correction audit log, which is the only payment history an IOU has.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{CorrectionId, PersonalIouId};
use phosk_model::{CorrectionEvent, PersonalIou, Provenance};
use serde::{Deserialize, Serialize};

/// How many initials the avatar shows when they are derived from a name.
const MAX_INITIALS: usize = 2;

/// A new informal debt, as typed in by the user.
///
/// The IOU starts outstanding in full: `of` and `amount` both begin at
/// [`Self::amount`], and only a payment moves them apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPersonalIou {
    /// `"in"` (owed to you) or `"out"` (you owe), case-insensitive.
    pub dir: String,
    /// The other person's name; also the source of the slug.
    pub person: String,
    /// Avatar initials; derived from [`Self::person`] when blank.
    pub initials: String,
    /// The sum lent or borrowed, exact centimes. Must be > 0.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Reason / memo.
    pub reason: String,
    /// The date it originated.
    pub since: NaiveDate,
}

/// A partial edit of an existing IOU: `None` leaves the field alone.
///
/// There is deliberately no field for the outstanding amount — that moves only
/// through [`record_iou_payment`] / [`settle_personal_iou`]. Correcting
/// [`Self::of`] (the original sum) shifts the outstanding amount by the same
/// delta, so what has already been repaid stays a fact.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalIouEdit {
    /// New direction.
    pub dir: Option<String>,
    /// New person name (the slug stays as it was).
    pub person: Option<String>,
    /// New avatar initials.
    pub initials: Option<String>,
    /// Corrected ORIGINAL sum, exact centimes.
    #[serde(default, with = "phosk_model::opt_money_centimes")]
    pub of: Option<Money>,
    /// New reason / memo.
    pub reason: Option<String>,
    /// New origination date.
    pub since: Option<NaiveDate>,
}

/// Create a personal IOU from user input. Write path.
///
/// Returns the new IOU's slug — the id the read DTOs and the UI use.
///
/// # Errors
/// [`PhoskError::Invalid`] if the input is unusable (blank or unslugifiable
/// person, a direction that is neither `"in"` nor `"out"`, a non-positive
/// amount); otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn create_personal_iou(
    db: &dyn DatabaseAdapter,
    input: NewPersonalIou,
) -> Result<String, PhoskError> {
    let person = input.person.trim().to_owned();
    let base = slugify(&person);
    if base.is_empty() {
        return Err(PhoskError::Invalid(format!(
            "IOU person {:?} has no usable slug",
            input.person
        )));
    }
    let slug = free_slug(db, &base).await?;

    let iou = normalized(PersonalIou {
        id: PersonalIouId::new(),
        slug: slug.clone(),
        dir: input.dir,
        initials: if input.initials.trim().is_empty() {
            initials_of(&person)
        } else {
            input.initials
        },
        person,
        // Nothing has been repaid yet: outstanding == original.
        amount: input.amount,
        of: input.amount,
        reason: input.reason,
        since: input.since,
        provenance: Provenance::user_entered(),
    });
    validate(&iou)?;
    db.upsert_personal_iou(iou).await?;
    Ok(slug)
}

/// Edit an existing IOU, addressed by its slug. Write path.
///
/// Only the `Some` fields of `edit` change; the merged record is validated
/// before anything is written, so a rejected edit leaves the store untouched.
/// Every changed field is appended to the correction audit log.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no IOU,
/// [`PhoskError::Invalid`] if the merged record would be inconsistent — an
/// unknown direction, a blank person, a non-positive original, or an original
/// below what has already been repaid; otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn edit_personal_iou(
    db: &dyn DatabaseAdapter,
    slug: &str,
    edit: PersonalIouEdit,
) -> Result<(), PhoskError> {
    let current = db.personal_iou_by_slug(slug).await?;

    let mut next = current.clone();
    if let Some(dir) = edit.dir {
        next.dir = dir;
    }
    if let Some(person) = edit.person {
        person.trim().clone_into(&mut next.person);
    }
    if let Some(initials) = edit.initials {
        next.initials = initials;
    }
    if let Some(of) = edit.of {
        // The original moves; what was already repaid does not.
        let repaid = current.of.checked_sub(current.amount)?;
        next.of = of;
        next.amount = of.checked_sub(repaid)?;
    }
    if let Some(reason) = edit.reason {
        next.reason = reason;
    }
    if let Some(since) = edit.since {
        next.since = since;
    }

    let next = normalized(next);
    validate(&next)?;
    write_change(db, &current, next).await
}

/// Delete a personal IOU, addressed by its slug. Write path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no IOU; otherwise any port
/// error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn delete_personal_iou(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError> {
    let iou = db.personal_iou_by_slug(slug).await?;
    db.delete_personal_iou(iou.id).await
}

/// Record a (partial or full) repayment against an IOU. Write path.
///
/// Returns what is still outstanding afterwards; a payment of exactly the
/// outstanding amount leaves `Money::ZERO` — the IOU stays in the list, fully
/// repaid, rather than disappearing. The original sum never moves.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no IOU,
/// [`PhoskError::Invalid`] if `payment` is not positive or exceeds what is
/// outstanding; otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn record_iou_payment(
    db: &dyn DatabaseAdapter,
    slug: &str,
    payment: Money,
) -> Result<Money, PhoskError> {
    let current = db.personal_iou_by_slug(slug).await?;
    if payment.centimes() <= 0 {
        return Err(PhoskError::Invalid(format!(
            "IOU payment must be positive, got {} centimes",
            payment.centimes()
        )));
    }
    if payment.centimes() > current.amount.centimes() {
        return Err(PhoskError::Invalid(format!(
            "IOU payment of {} centimes exceeds the {} centimes outstanding",
            payment.centimes(),
            current.amount.centimes()
        )));
    }

    let left = current.amount.checked_sub(payment)?;
    let next = PersonalIou {
        amount: left,
        ..current.clone()
    };
    write_change(db, &current, next).await?;
    Ok(left)
}

/// Settle an IOU: clear whatever is still outstanding in one go. Write path.
///
/// Returns the amount that was cleared — `Money::ZERO` for an IOU that was
/// already settled, which makes a second settle a no-op rather than an error.
/// The original sum never moves, so the read side still shows what it was for.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no IOU; otherwise any port
/// error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn settle_personal_iou(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<Money, PhoskError> {
    let current = db.personal_iou_by_slug(slug).await?;
    let cleared = current.amount;
    if cleared == Money::ZERO {
        return Ok(Money::ZERO);
    }
    let next = PersonalIou {
        amount: Money::ZERO,
        ..current.clone()
    };
    write_change(db, &current, next).await?;
    Ok(cleared)
}

// ── shared write step ─────────────────────────────────────────────────────────

/// Stamp `next` as user-modified, store it, and append the fields it changed to
/// the correction audit log.
async fn write_change(
    db: &dyn DatabaseAdapter,
    current: &PersonalIou,
    next: PersonalIou,
) -> Result<(), PhoskError> {
    let next = PersonalIou {
        provenance: Provenance::user_modified(),
        ..next
    };
    let changes = changed_fields(current, &next);
    db.upsert_personal_iou(next).await?;
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

/// `base`, or the first free `base-2`, `base-3`, … — the same person may be
/// owed twice, so a taken slug is disambiguated instead of rejected.
async fn free_slug(db: &dyn DatabaseAdapter, base: &str) -> Result<String, PhoskError> {
    let taken: Vec<String> = db
        .personal_ious()
        .await?
        .into_iter()
        .map(|i| i.slug)
        .collect();
    if !taken.iter().any(|s| s == base) {
        return Ok(base.to_owned());
    }
    // Bounded by the number of existing IOUs + 1, so a free suffix always
    // exists within the range and the loop cannot run away.
    for n in 2..=taken.len().saturating_add(2) {
        let candidate = format!("{base}-{n}");
        if !taken.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(PhoskError::Invalid(format!(
        "no free IOU slug for {base:?}"
    )))
}

/// The first letter of up to [`MAX_INITIALS`] words of a name, upper-cased.
fn initials_of(person: &str) -> String {
    person
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(MAX_INITIALS)
        .flat_map(char::to_uppercase)
        .collect()
}

/// Trim the free-text fields and lower-case the direction. Validation runs on
/// the result.
fn normalized(iou: PersonalIou) -> PersonalIou {
    PersonalIou {
        dir: iou.dir.trim().to_ascii_lowercase(),
        person: iou.person.trim().to_owned(),
        initials: iou.initials.trim().to_owned(),
        reason: iou.reason.trim().to_owned(),
        ..iou
    }
}

/// Reject a record the read side could not make sense of.
fn validate(iou: &PersonalIou) -> Result<(), PhoskError> {
    if iou.person.is_empty() {
        return Err(PhoskError::Invalid("IOU person is empty".to_owned()));
    }
    if iou.dir != "in" && iou.dir != "out" {
        return Err(PhoskError::Invalid(format!(
            "IOU direction must be \"in\" or \"out\", got {:?}",
            iou.dir
        )));
    }
    if iou.of.centimes() <= 0 {
        return Err(PhoskError::Invalid(format!(
            "IOU amount must be positive, got {} centimes",
            iou.of.centimes()
        )));
    }
    if iou.amount.centimes() < 0 {
        return Err(PhoskError::Invalid(format!(
            "IOU outstanding amount would be negative ({} centimes): \
             more has been repaid than the original",
            iou.amount.centimes()
        )));
    }
    if iou.amount.centimes() > iou.of.centimes() {
        return Err(PhoskError::Invalid(format!(
            "IOU outstanding {} centimes exceeds the original {} centimes",
            iou.amount.centimes(),
            iou.of.centimes()
        )));
    }
    Ok(())
}

/// The `(field, old, new)` triples a write actually changed, for the audit log.
fn changed_fields(
    before: &PersonalIou,
    after: &PersonalIou,
) -> Vec<(&'static str, String, String)> {
    let mut out = Vec::new();
    let mut push = |field: &'static str, old: String, new: String| {
        if old != new {
            out.push((field, old, new));
        }
    };
    push("dir", before.dir.clone(), after.dir.clone());
    push("person", before.person.clone(), after.person.clone());
    push("initials", before.initials.clone(), after.initials.clone());
    push(
        "amount",
        before.amount.centimes().to_string(),
        after.amount.centimes().to_string(),
    );
    push(
        "of",
        before.of.centimes().to_string(),
        after.of.centimes().to_string(),
    );
    push("reason", before.reason.clone(), after.reason.clone());
    push("since", before.since.to_string(), after.since.to_string());
    out
}
