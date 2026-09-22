//! Debt write path — create · edit · delete · record payment · extra payment (T16).
//!
//! A service over the [`DatabaseAdapter`] PORT, like the read side in
//! [`crate::debts`]: it validates caller input, stamps
//! [`Provenance`] and delegates the storage to the port. Plan changes
//! (monthly/day/term adjustment, refinance) are a separate concern and are NOT
//! here.
//!
//! **Identity.** A debt is addressed on the wire by its `slug` (that is what
//! [`crate::debts::DebtDto::id`] carries). The slug is derived from the name
//! once, at creation, and then frozen: renaming "VW lease" must not break a
//! link, a bookmark or a stored reference. Two debts may therefore not share a
//! name.
//!
//! **The two payment kinds are different operations**, and that difference is
//! the whole point of the task's "amortisation outputs stay correct":
//!
//! - [`record_payment`] is the **scheduled instalment**. The read side's
//!   amortisation engine advances a balance by `balance + balance·apr/12 −
//!   monthly` each month, so recording an instalment must accrue exactly one
//!   month of interest before applying the amount. Paying exactly `monthly`
//!   therefore lands on the balance [`crate::debts::debt_detail`] had projected
//!   for next month, and `monthsToPayoff` drops by exactly one.
//! - [`extra_payment`] is an **ad-hoc principal reduction** (a lump sum outside
//!   the schedule). No interest accrues: the balance drops by the full amount,
//!   and the payoff horizon shortens by more than one month. That is precisely
//!   what makes overpaying worth doing.
//!
//! Both record a [`DebtPayment`] carrying the balance it left behind, so the
//! inspector's payment history and the debt agree.
//!
//! **Provenance.** `Debt::source` records where the RECORD came from (typed in,
//! or detected by the LLM) and drives the "auto-detected" KPI, so an edit or a
//! payment leaves it alone; `Debt::provenance` describes the CURRENT values and
//! flips to [`Provenance::user_modified`] whenever they change. Each changed
//! field of an edit is also appended to the correction audit log.
//!
//! [`DatabaseAdapter`]: phosk_adapter_db::DatabaseAdapter

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{CorrectionId, DebtId, PaymentId};
use phosk_model::{CorrectionEvent, Debt, DebtPayment, Provenance, Source};

/// The lowest day-of-month an instalment may land on.
const MIN_DAY: u32 = 1;
/// The highest day-of-month an instalment may land on. Short months clamp on
/// read (see [`crate::debts`]), so the 31st is accepted here.
const MAX_DAY: u32 = 31;

/// The debt kinds the read model knows how to group and label
/// (`Debt::kind`'s documented domain).
const KINDS: [&str; 6] = ["LEASE", "LOAN", "CARD", "TAX", "BNPL", "MEDICAL"];

/// The status keys [`crate::debts`] maps to a human label.
const STATUSES: [&str; 4] = ["high", "due", "watch", "ok"];

/// A new institutional debt, as typed in by the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDebt {
    /// Display name; also the source of the slug.
    pub name: String,
    /// Lender / creditor.
    pub lender: String,
    /// One of `LEASE`, `LOAN`, `CARD`, `TAX`, `BNPL`, `MEDICAL`
    /// (case-insensitive).
    pub kind: String,
    /// Current outstanding balance, exact centimes. `0..=orig`.
    #[serde(with = "phosk_model::money_centimes")]
    pub balance: Money,
    /// Original amount borrowed, exact centimes. Must be > 0.
    #[serde(with = "phosk_model::money_centimes")]
    pub orig: Money,
    /// Scheduled monthly payment, exact centimes. `0` = no schedule
    /// (revolving).
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly: Money,
    /// Annual percentage rate as a `0.0..=1.0` fraction.
    pub apr: f64,
    /// Payment day-of-month, `1..=31`.
    pub day: u32,
    /// Term in months (`0` = revolving).
    pub term: u32,
    /// Display glyph.
    pub glyph: String,
    /// Date the debt was opened.
    pub since: NaiveDate,
    /// Free-text note; also the inspector's guidance line when set.
    pub note: String,
}

/// A partial edit of an existing debt: `None` leaves the field alone.
///
/// The merged record is validated as a whole, so a patch that is fine in
/// isolation (`orig: 1_000_000`) is still rejected when the result would be
/// inconsistent (an original amount below the outstanding balance).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtEdit {
    /// New display name (the slug stays as it was).
    pub name: Option<String>,
    /// New lender.
    pub lender: Option<String>,
    /// New kind.
    pub kind: Option<String>,
    /// New outstanding balance, exact centimes.
    #[serde(default, with = "phosk_model::opt_money_centimes")]
    pub balance: Option<Money>,
    /// New original amount, exact centimes.
    #[serde(default, with = "phosk_model::opt_money_centimes")]
    pub orig: Option<Money>,
    /// New scheduled monthly payment, exact centimes.
    #[serde(default, with = "phosk_model::opt_money_centimes")]
    pub monthly: Option<Money>,
    /// New APR, `0.0..=1.0`.
    pub apr: Option<f64>,
    /// New payment day-of-month.
    pub day: Option<u32>,
    /// New term in months.
    pub term: Option<u32>,
    /// New status key (`"high"|"due"|"watch"|"ok"`).
    pub status: Option<String>,
    /// New glyph.
    pub glyph: Option<String>,
    /// New opened-on date.
    pub since: Option<NaiveDate>,
    /// New note.
    pub note: Option<String>,
}

/// One payment against a debt, as entered by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDebtPayment {
    /// When it was paid.
    pub date: NaiveDate,
    /// Amount paid, exact centimes. Must be > 0.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
}

/// Create an institutional debt from user input. Write path.
///
/// Returns the new debt's slug — the id the read DTOs and the UI use. A fresh
/// debt starts `"ok"`: the alert statuses are an analysis concern, not
/// something the creating form decides.
///
/// # Errors
/// [`PhoskError::Invalid`] if the input is unusable (blank/unslugifiable name,
/// a name whose slug is already taken, a blank lender, an unknown kind, a
/// negative balance or monthly payment, a non-positive original amount, a
/// balance above the original amount, an APR outside `0.0..=1.0` or not a
/// number, a day outside `1..=31`); otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn create_debt(db: &dyn DatabaseAdapter, input: NewDebt) -> Result<String, PhoskError> {
    let name = input.name.trim().to_owned();
    let slug = slugify(&name);
    if slug.is_empty() {
        return Err(PhoskError::Invalid(format!(
            "debt name {:?} has no usable slug",
            input.name
        )));
    }
    if db.debt_by_slug(&slug).await.is_ok() {
        return Err(PhoskError::Invalid(format!(
            "a debt named {name:?} already exists"
        )));
    }

    let debt = normalized(Debt {
        id: DebtId::new(),
        slug: slug.clone(),
        name,
        lender: input.lender,
        kind: input.kind,
        balance: input.balance,
        orig: input.orig,
        monthly: input.monthly,
        apr: input.apr,
        day: input.day,
        term: input.term,
        // A fresh debt is on track; the alert statuses are derived elsewhere.
        status: "ok".to_owned(),
        glyph: input.glyph,
        since: input.since,
        note: input.note,
        source: Source::UserEntered,
        provenance: Provenance::user_entered(),
    });
    validate(&debt)?;
    db.upsert_debt(debt).await?;
    Ok(slug)
}

/// Edit an existing debt, addressed by its slug. Write path.
///
/// Only the `Some` fields of `edit` change; the merged record is validated
/// before anything is written, so a rejected edit leaves the store untouched.
/// Every changed field is appended to the correction audit log and the record's
/// provenance becomes [`Provenance::user_modified`].
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no debt,
/// [`PhoskError::Invalid`] if the merged record would be inconsistent (see
/// [`create_debt`]) or if a rename would collide with another debt's identity;
/// otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn edit_debt(
    db: &dyn DatabaseAdapter,
    slug: &str,
    edit: DebtEdit,
) -> Result<(), PhoskError> {
    let current = db.debt_by_slug(slug).await?;

    let mut next = current.clone();
    if let Some(name) = edit.name {
        name.trim().clone_into(&mut next.name);
    }
    if let Some(lender) = edit.lender {
        next.lender = lender;
    }
    if let Some(kind) = edit.kind {
        next.kind = kind;
    }
    if let Some(balance) = edit.balance {
        next.balance = balance;
    }
    if let Some(orig) = edit.orig {
        next.orig = orig;
    }
    if let Some(monthly) = edit.monthly {
        next.monthly = monthly;
    }
    if let Some(apr) = edit.apr {
        next.apr = apr;
    }
    if let Some(day) = edit.day {
        next.day = day;
    }
    if let Some(term) = edit.term {
        next.term = term;
    }
    if let Some(status) = edit.status {
        next.status = status;
    }
    if let Some(glyph) = edit.glyph {
        next.glyph = glyph;
    }
    if let Some(since) = edit.since {
        next.since = since;
    }
    if let Some(note) = edit.note {
        next.note = note;
    }

    let mut next = normalized(next);
    validate(&next)?;
    if next.name != current.name {
        // The slug is frozen at creation, but a rename must not collide with
        // another debt's identity either.
        let renamed = slugify(&next.name);
        if renamed != current.slug && db.debt_by_slug(&renamed).await.is_ok() {
            return Err(PhoskError::Invalid(format!(
                "a debt named {:?} already exists",
                next.name
            )));
        }
    }
    next.provenance = Provenance::user_modified();

    let changes = changed_fields(&current, &next);
    db.upsert_debt(next).await?;
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

/// Delete a debt and its recorded payments, addressed by its slug. Write path.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no debt; otherwise any port
/// error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn delete_debt(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError> {
    let debt = db.debt_by_slug(slug).await?;
    db.delete_debt(debt.id).await
}

/// Record a **scheduled instalment** against a debt. Write path.
///
/// One month of interest accrues first (`round(balance · apr/12)`), then the
/// amount applies — the same step the read side's amortisation projection
/// takes. Returns the balance left outstanding.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no debt,
/// [`PhoskError::Invalid`] if the amount is not positive or exceeds the payoff
/// amount (balance plus this month's interest); otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn record_payment(
    db: &dyn DatabaseAdapter,
    slug: &str,
    payment: NewDebtPayment,
) -> Result<Money, PhoskError> {
    let debt = db.debt_by_slug(slug).await?;
    let interest = monthly_interest(debt.balance, debt.apr)?;
    let payoff = debt.balance.checked_add(interest)?;
    apply_payment(db, debt, payment, payoff).await
}

/// Record an **extra payment** — a lump sum outside the schedule. Write path.
///
/// No interest accrues: the balance drops by the full amount, which is why an
/// extra payment shortens the payoff horizon by more than one month. Returns
/// the balance left outstanding.
///
/// # Errors
/// [`PhoskError::NotFound`] if `slug` resolves to no debt,
/// [`PhoskError::Invalid`] if the amount is not positive or exceeds the
/// outstanding balance; otherwise any port error.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn extra_payment(
    db: &dyn DatabaseAdapter,
    slug: &str,
    payment: NewDebtPayment,
) -> Result<Money, PhoskError> {
    let debt = db.debt_by_slug(slug).await?;
    let payoff = debt.balance;
    apply_payment(db, debt, payment, payoff).await
}

/// The shared tail of both payment kinds: validate the amount against `payoff`
/// (what it would take to clear the debt under that kind's rules), write the
/// new balance and append the payment row.
async fn apply_payment(
    db: &dyn DatabaseAdapter,
    debt: Debt,
    payment: NewDebtPayment,
    payoff: Money,
) -> Result<Money, PhoskError> {
    if payment.amount.centimes() <= 0 {
        return Err(PhoskError::Invalid(format!(
            "payment amount must be positive, got {} centimes",
            payment.amount.centimes()
        )));
    }
    if payment.amount.centimes() > payoff.centimes() {
        return Err(PhoskError::Invalid(format!(
            "payment of {} centimes exceeds the {} centimes needed to clear {:?}",
            payment.amount.centimes(),
            payoff.centimes(),
            debt.slug
        )));
    }
    let balance_after = payoff.checked_sub(payment.amount)?;

    db.upsert_debt(Debt {
        balance: balance_after,
        provenance: Provenance::user_modified(),
        ..debt.clone()
    })
    .await?;
    db.record_debt_payment(DebtPayment {
        id: PaymentId::new(),
        debt_id: debt.id,
        date: payment.date,
        amount: payment.amount,
        balance_after,
        provenance: Provenance::user_entered(),
    })
    .await?;
    Ok(balance_after)
}

// ── validation & normalisation ────────────────────────────────────────────────

/// One month of interest on `balance` at `apr`: `round(balance · apr / 12)`,
/// exact i64 centimes. The same rounding the read side's `annualInterest` uses,
/// so a recorded instalment and the projection agree to the centime.
fn monthly_interest(balance: Money, apr: f64) -> Result<Money, PhoskError> {
    #[allow(clippy::cast_precision_loss)]
    let raw = balance.centimes() as f64 * apr / 12.0;
    if !raw.is_finite() {
        return Err(PhoskError::Overflow("debt interest overflow".to_owned()));
    }
    #[allow(clippy::cast_possible_truncation)]
    let cents = raw.round() as i64;
    Ok(Money::from_centimes(cents))
}

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

/// Trim the free-text fields and upper-case the kind, so `"lease"` and
/// `"LEASE"` are the same bucket to [`crate::debts`]'s `groupLabel` map.
fn normalized(debt: Debt) -> Debt {
    Debt {
        name: debt.name.trim().to_owned(),
        lender: debt.lender.trim().to_owned(),
        kind: debt.kind.trim().to_ascii_uppercase(),
        status: debt.status.trim().to_ascii_lowercase(),
        glyph: debt.glyph.trim().to_owned(),
        note: debt.note.trim().to_owned(),
        ..debt
    }
}

/// Reject a record the read side could not make sense of.
fn validate(debt: &Debt) -> Result<(), PhoskError> {
    if debt.name.is_empty() {
        return Err(PhoskError::Invalid("debt name is empty".to_owned()));
    }
    if debt.lender.is_empty() {
        return Err(PhoskError::Invalid("debt lender is empty".to_owned()));
    }
    if !KINDS.contains(&debt.kind.as_str()) {
        return Err(PhoskError::Invalid(format!(
            "debt kind must be one of {KINDS:?}, got {:?}",
            debt.kind
        )));
    }
    if !STATUSES.contains(&debt.status.as_str()) {
        return Err(PhoskError::Invalid(format!(
            "debt status must be one of {STATUSES:?}, got {:?}",
            debt.status
        )));
    }
    if debt.orig.centimes() <= 0 {
        return Err(PhoskError::Invalid(format!(
            "original amount must be positive, got {} centimes",
            debt.orig.centimes()
        )));
    }
    if debt.balance.centimes() < 0 {
        return Err(PhoskError::Invalid(format!(
            "balance cannot be negative, got {} centimes",
            debt.balance.centimes()
        )));
    }
    if debt.balance.centimes() > debt.orig.centimes() {
        return Err(PhoskError::Invalid(format!(
            "balance {} centimes exceeds the original {} centimes",
            debt.balance.centimes(),
            debt.orig.centimes()
        )));
    }
    if debt.monthly.centimes() < 0 {
        return Err(PhoskError::Invalid(format!(
            "monthly payment cannot be negative, got {} centimes",
            debt.monthly.centimes()
        )));
    }
    if !(debt.apr.is_finite() && (0.0..=1.0).contains(&debt.apr)) {
        return Err(PhoskError::Invalid(format!(
            "apr must be a rate in 0.0..=1.0, got {}",
            debt.apr
        )));
    }
    if !(MIN_DAY..=MAX_DAY).contains(&debt.day) {
        return Err(PhoskError::Invalid(format!(
            "payment day must be {MIN_DAY}..={MAX_DAY}, got {}",
            debt.day
        )));
    }
    Ok(())
}

/// The `(field, old, new)` triples an edit actually changed, for the audit log.
fn changed_fields(before: &Debt, after: &Debt) -> Vec<(&'static str, String, String)> {
    let mut out = Vec::new();
    let mut push = |field: &'static str, old: String, new: String| {
        if old != new {
            out.push((field, old, new));
        }
    };
    push("name", before.name.clone(), after.name.clone());
    push("lender", before.lender.clone(), after.lender.clone());
    push("kind", before.kind.clone(), after.kind.clone());
    push(
        "balance",
        before.balance.centimes().to_string(),
        after.balance.centimes().to_string(),
    );
    push(
        "orig",
        before.orig.centimes().to_string(),
        after.orig.centimes().to_string(),
    );
    push(
        "monthly",
        before.monthly.centimes().to_string(),
        after.monthly.centimes().to_string(),
    );
    push("apr", before.apr.to_string(), after.apr.to_string());
    push("day", before.day.to_string(), after.day.to_string());
    push("term", before.term.to_string(), after.term.to_string());
    push("status", before.status.clone(), after.status.clone());
    push("glyph", before.glyph.clone(), after.glyph.clone());
    push("since", before.since.to_string(), after.since.to_string());
    push("note", before.note.clone(), after.note.clone());
    out
}
