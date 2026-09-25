//! `ai_approval` — the approval service: the ONLY path from a model proposal
//! to the ledger (design invariant: *AI proposes → human approves → only then
//! the ledger changes*).
//!
//! The receipt-intake pipeline stages a [`ReceiptProposal`] and enqueues one
//! `kind == "receipt"` [`AiSuggestion`] (`status == "open"`). Nothing is booked
//! until a human calls [`approve_suggestion`] / [`approve_receipt`] here, which
//! re-validates the (hostile) payload and applies it via `insert_receipt`
//! verbatim — model provenance (`Ocr` / `LlmInferred`) is kept, never
//! relabelled as user-entered.
//!
//! **Idempotency.** An `accepted` suggestion is never re-applied, and
//! `insert_receipt` is itself keyed on the receipt slug, so even two racing
//! approvals replace one row instead of double-booking.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::SuggestionId;
use phosk_model::{AiSuggestion, Provenance, ReceiptProposal, Source};

/// Suggestion kind whose payload is a staged [`ReceiptProposal`].
pub const RECEIPT_KIND: &str = "receipt";
/// Slug prefix of every pipeline-proposed receipt (`"rcpt:<sha256>"`). A
/// proposal may only ever write under it, so approving can never replace a
/// manual/seeded receipt through `insert_receipt`'s same-slug semantics.
pub const PROPOSAL_SLUG_PREFIX: &str = "rcpt:";
/// Max characters of any proposed text field (shop, category, line name).
pub const MAX_TEXT_CHARS: usize = 200;
/// Max line items on one proposed receipt.
pub const MAX_LINES: usize = 500;
/// Max proposed receipt total / line amount: CHF 100 000.00, in centimes.
pub const MAX_AMOUNT_CENTIMES: i64 = 10_000_000;
/// Max quantity on one proposed line.
pub const MAX_QTY: f64 = 10_000.0;

const OPEN: &str = "open";
const ACCEPTED: &str = "accepted";
const DISMISSED: &str = "dismissed";

/// One open suggestion plus its staged payload (receipt suggestions only), so
/// a review screen can render the per-line proposal without another read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingSuggestion {
    /// The queued suggestion.
    pub suggestion: AiSuggestion,
    /// The staged receipt proposal, if one exists.
    pub proposal: Option<ReceiptProposal>,
}

/// Open suggestions sharing one target receipt (`receipt_slug`); non-receipt
/// suggestions are collected in a single group with `receipt_slug == None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingGroup {
    /// The receipt slug the group's suggestions propose, or `None`.
    pub receipt_slug: Option<String>,
    /// The group's open suggestions, in queue order.
    pub suggestions: Vec<PendingSuggestion>,
}

/// What an approval did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalOutcome {
    /// The approved suggestion.
    pub suggestion_id: SuggestionId,
    /// The ledger receipt slug the proposal is booked under.
    pub receipt_slug: String,
    /// `false` when the suggestion was already accepted (nothing written).
    pub applied: bool,
}

/// Every `open` suggestion, grouped per target receipt in first-queued order.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter reads.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn pending_suggestions(
    db: &dyn DatabaseAdapter,
) -> Result<Vec<PendingGroup>, PhoskError> {
    let mut groups: Vec<PendingGroup> = Vec::new();
    for s in db.ai_suggestions().await? {
        if s.status != OPEN {
            continue;
        }
        let (slug, proposal) = if s.kind == RECEIPT_KIND {
            (s.target.clone(), db.receipt_proposal(s.id).await?)
        } else {
            (None, None)
        };
        let item = PendingSuggestion {
            suggestion: s,
            proposal,
        };
        match groups.iter_mut().find(|g| g.receipt_slug == slug) {
            Some(g) => g.suggestions.push(item),
            None => groups.push(PendingGroup {
                receipt_slug: slug,
                suggestions: vec![item],
            }),
        }
    }
    Ok(groups)
}

/// Approve one receipt suggestion: validate its staged proposal and apply it to
/// the ledger via `insert_receipt`, then mark it `accepted`. Approving an
/// already-accepted suggestion writes nothing (`applied == false`).
///
/// # Errors
/// - [`PhoskError::NotFound`] if the suggestion or its staged proposal is missing.
/// - [`PhoskError::Invalid`] if it is not a receipt suggestion, was rejected,
///   or its proposal fails validation (the ledger is left untouched).
#[tracing::instrument(level = "debug", skip_all)]
pub async fn approve_suggestion(
    db: &dyn DatabaseAdapter,
    id: SuggestionId,
) -> Result<ApprovalOutcome, PhoskError> {
    let s = find(db, id).await?;
    let slug = receipt_target(&s)?.to_owned();
    if s.status == ACCEPTED {
        return Ok(outcome(id, slug, false));
    }
    let p = checked_proposal(db, &s).await?;
    apply(db, p).await?;
    Ok(outcome(id, slug, true))
}

/// Bulk-approve every `open` suggestion of one receipt. All payloads are
/// validated before any is applied; returns one outcome per applied suggestion
/// (empty when none is left open).
///
/// # Errors
/// - [`PhoskError::NotFound`] if no receipt suggestion targets `slug`, or a
///   staged proposal is missing.
/// - [`PhoskError::Invalid`] if any open proposal fails validation (nothing applied).
#[tracing::instrument(level = "debug", skip_all)]
pub async fn approve_receipt(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<Vec<ApprovalOutcome>, PhoskError> {
    let targeting: Vec<AiSuggestion> = db
        .ai_suggestions()
        .await?
        .into_iter()
        .filter(|s| s.kind == RECEIPT_KIND && s.target.as_deref() == Some(slug))
        .collect();
    if targeting.is_empty() {
        return Err(PhoskError::NotFound(format!(
            "receipt suggestions for {slug}"
        )));
    }
    let mut checked = Vec::new();
    for s in targeting.iter().filter(|s| s.status == OPEN) {
        checked.push(checked_proposal(db, s).await?);
    }
    let mut outcomes = Vec::with_capacity(checked.len());
    for p in checked {
        let id = p.suggestion_id;
        apply(db, p).await?;
        outcomes.push(outcome(id, slug.to_owned(), true));
    }
    Ok(outcomes)
}

/// Reject a suggestion (`dismissed`); the ledger is never touched. Rejecting an
/// already-dismissed suggestion is a no-op.
///
/// # Errors
/// - [`PhoskError::NotFound`] if no such suggestion.
/// - [`PhoskError::Invalid`] if it was already accepted (undo is a ledger edit).
#[tracing::instrument(level = "debug", skip_all)]
pub async fn reject_suggestion(
    db: &dyn DatabaseAdapter,
    id: SuggestionId,
) -> Result<(), PhoskError> {
    let s = find(db, id).await?;
    match s.status.as_str() {
        DISMISSED => Ok(()),
        ACCEPTED => Err(PhoskError::Invalid(format!(
            "suggestion {id} is already applied"
        ))),
        _ => db.update_suggestion_status(id, DISMISSED).await,
    }
}

/// Schema-validate a (hostile) receipt proposal against the suggestion's
/// target slug. Public so a review screen can pre-flight a proposal.
///
/// # Errors
/// [`PhoskError::Invalid`] naming the first violated rule.
pub fn validate_proposal(p: &ReceiptProposal, target_slug: &str) -> Result<(), PhoskError> {
    let r = &p.receipt;
    if r.slug != target_slug || !r.slug.starts_with(PROPOSAL_SLUG_PREFIX) {
        return invalid("receipt slug does not match the suggestion target");
    }
    bounded_text(&r.slug, "slug")?;
    bounded_text(&r.shop, "shop")?;
    bounded_text(&r.category, "category")?;
    if !(earliest()?..=latest()?).contains(&r.date) {
        return invalid("receipt date out of range");
    }
    model_provenance(r.provenance)?;
    if p.line_items.is_empty() || p.line_items.len() > MAX_LINES {
        return invalid("line count out of range");
    }
    let mut total = Money::ZERO;
    for l in &p.line_items {
        if l.receipt_id != r.id {
            return invalid("line bound to another receipt");
        }
        bounded_text(&l.name, "line name")?;
        bounded_text(&l.category, "line category")?;
        if !l.qty.is_finite() || l.qty <= 0.0 || l.qty > MAX_QTY {
            return invalid("line quantity out of range");
        }
        bounded_amount(l.unit_price, "unit price")?;
        bounded_amount(l.line_total, "line total")?;
        model_provenance(l.provenance)?;
        total = total.checked_add(l.line_total)?;
    }
    bounded_amount(r.amount, "receipt total")?;
    if total != r.amount {
        return invalid("receipt total differs from the sum of its lines");
    }
    Ok(())
}

async fn find(db: &dyn DatabaseAdapter, id: SuggestionId) -> Result<AiSuggestion, PhoskError> {
    db.ai_suggestions()
        .await?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| PhoskError::NotFound(format!("suggestion {id}")))
}

/// The receipt slug a receipt suggestion targets.
fn receipt_target(s: &AiSuggestion) -> Result<&str, PhoskError> {
    match (s.kind.as_str(), s.target.as_deref()) {
        (RECEIPT_KIND, Some(slug)) => Ok(slug),
        _ => Err(PhoskError::Invalid(format!(
            "suggestion {} ({}) has no ledger effect to apply",
            s.id, s.kind
        ))),
    }
}

/// Load + validate the staged proposal of an `open` receipt suggestion.
async fn checked_proposal(
    db: &dyn DatabaseAdapter,
    s: &AiSuggestion,
) -> Result<ReceiptProposal, PhoskError> {
    let slug = receipt_target(s)?;
    if s.status != OPEN {
        return invalid(&format!("suggestion {} is {}, not open", s.id, s.status));
    }
    let p = db
        .receipt_proposal(s.id)
        .await?
        .ok_or_else(|| PhoskError::NotFound(format!("proposal for suggestion {}", s.id)))?;
    if p.suggestion_id != s.id {
        return invalid("proposal belongs to another suggestion");
    }
    validate_proposal(&p, slug)?;
    Ok(p)
}

/// Book a validated proposal verbatim (provenance intact), then accept it.
async fn apply(db: &dyn DatabaseAdapter, p: ReceiptProposal) -> Result<(), PhoskError> {
    let id = p.suggestion_id;
    db.insert_receipt(p.receipt, p.line_items).await?;
    db.update_suggestion_status(id, ACCEPTED).await
}

const fn outcome(
    suggestion_id: SuggestionId,
    receipt_slug: String,
    applied: bool,
) -> ApprovalOutcome {
    ApprovalOutcome {
        suggestion_id,
        receipt_slug,
        applied,
    }
}

fn invalid<T>(why: &str) -> Result<T, PhoskError> {
    Err(PhoskError::Invalid(format!("receipt proposal: {why}")))
}

fn bounded_text(s: &str, what: &str) -> Result<(), PhoskError> {
    if s.trim().is_empty() || s.chars().count() > MAX_TEXT_CHARS || s.chars().any(char::is_control)
    {
        return invalid(&format!(
            "{what} is empty, too long or has control characters"
        ));
    }
    Ok(())
}

fn bounded_amount(m: Money, what: &str) -> Result<(), PhoskError> {
    if !(0..=MAX_AMOUNT_CENTIMES).contains(&m.centimes()) {
        return invalid(&format!("{what} out of range"));
    }
    Ok(())
}

/// Only machine provenance may arrive through a proposal: a model can never
/// claim a value was typed or edited by the user.
fn model_provenance(p: Provenance) -> Result<(), PhoskError> {
    if !matches!(p.source, Source::Ocr | Source::LlmInferred) {
        return invalid("provenance must be Ocr or LlmInferred");
    }
    if !(0.0..=1.0).contains(&p.confidence) {
        return invalid("confidence out of range");
    }
    Ok(())
}

fn earliest() -> Result<NaiveDate, PhoskError> {
    NaiveDate::from_ymd_opt(2000, 1, 1).ok_or_else(|| PhoskError::Invalid("date bound".to_owned()))
}

fn latest() -> Result<NaiveDate, PhoskError> {
    NaiveDate::from_ymd_opt(2100, 12, 31)
        .ok_or_else(|| PhoskError::Invalid("date bound".to_owned()))
}
