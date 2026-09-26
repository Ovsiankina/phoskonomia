//! `ai_approval` — the approval service: the ONLY path from a model proposal
//! to the ledger (design invariant: *AI proposes → human approves → only then
//! the ledger changes*).
//!
//! The receipt-intake pipeline stages a [`ReceiptProposal`] and enqueues one
//! `kind == "receipt"` [`AiSuggestion`] (`status == "open"`). Nothing is booked
//! until a human calls [`approve_suggestion`] / [`approve_receipt`] here, which
//! re-validates the (hostile) payload and applies it via `insert_receipt` —
//! model provenance (`Ocr` / `LlmInferred`) is kept, never relabelled as
//! user-entered; staged ids and ledger flags are not trusted (ids are derived
//! from the suggestion id, `fixed` / `signal_id` dropped).
//!
//! **Idempotency.** An `accepted` suggestion is never re-applied, and a
//! receipt already booked under the slug is never re-inserted (so a booked,
//! possibly user-corrected receipt can't be silently replaced): the approval
//! is recorded with `applied == false`. At most one proposal per receipt may
//! be open at approval time; conflicting proposals are refused, so the audit
//! trail never marks an unbooked proposal as applied.
//!
//! The booked-slug check and the write are not atomic (the port has no
//! transaction), so two concurrent approvals of one proposal (a double-click)
//! can both write. That is safe because the written rows are a pure function
//! of the proposal: the receipt and line ids are derived from the suggestion id
//! (SHA-256, domain-separated), so the second `insert_receipt` overwrites the
//! first row-for-row (same receipt, projection and line keys) instead of
//! booking a duplicate. Deriving (rather than re-using the staged ids) also
//! keeps a tampered payload from aiming its ids at an unrelated stored row.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{LineItemId, ReceiptId, SuggestionId};
use phosk_model::{AiSuggestion, LineItem, Provenance, Receipt, ReceiptProposal, Source};
use sha2::{Digest, Sha256};
use uuid::{Builder, Uuid};

/// Suggestion kind whose payload is a staged [`ReceiptProposal`].
pub const RECEIPT_KIND: &str = "receipt";
/// Slug prefix of every pipeline-proposed receipt (`"rcpt:<sha256>"`). A
/// proposal may only ever write under it, and its ids are derived from the
/// suggestion id on apply, so approving can never replace a manual/seeded
/// receipt (by slug or by id).
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
    /// The group's open suggestions, in adapter listing order (the port
    /// promises no particular order).
    pub suggestions: Vec<PendingSuggestion>,
}

/// What an approval did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalOutcome {
    /// The approved suggestion.
    pub suggestion_id: SuggestionId,
    /// The ledger receipt slug the proposal is booked under.
    pub receipt_slug: String,
    /// `false` when nothing was written to the ledger: the suggestion was
    /// already accepted, or a receipt was already booked under the slug.
    pub applied: bool,
}

/// Every `open` suggestion, grouped per target receipt (groups and members in
/// the adapter's listing order, which the port does not specify).
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
/// already-accepted suggestion, or one whose receipt is already booked, writes
/// nothing to the ledger (`applied == false`).
///
/// # Errors
/// - [`PhoskError::NotFound`] if the suggestion or its staged proposal is missing.
/// - [`PhoskError::Invalid`] if it is not a receipt suggestion, was rejected,
///   another proposal for the same receipt is also open, or its proposal fails
///   validation (the ledger is left untouched).
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
    let open = open_for(db, &slug).await?;
    single_open(&open, &slug)?;
    let applied = apply(db, p).await?;
    Ok(outcome(id, slug, applied))
}

/// Bulk-approve the `open` suggestion of one receipt. Every open payload is
/// validated before anything is applied; returns one outcome per accepted
/// suggestion (empty when none is left open).
///
/// # Errors
/// - [`PhoskError::NotFound`] if no receipt suggestion targets `slug`, or a
///   staged proposal is missing.
/// - [`PhoskError::Invalid`] if any open proposal fails validation, or more
///   than one is open (conflicting proposals) — nothing is applied.
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
    let open: Vec<AiSuggestion> = targeting.into_iter().filter(|s| s.status == OPEN).collect();
    let mut checked = Vec::with_capacity(open.len());
    for s in &open {
        checked.push(checked_proposal(db, s).await?);
    }
    single_open(&open, slug)?;
    let mut outcomes = Vec::with_capacity(checked.len());
    for p in checked {
        let id = p.suggestion_id;
        let applied = apply(db, p).await?;
        outcomes.push(outcome(id, slug.to_owned(), applied));
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
    bounded_text(&r.ocr_engine, "ocr engine")?;
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

/// The `open` receipt suggestions targeting `slug`.
async fn open_for(db: &dyn DatabaseAdapter, slug: &str) -> Result<Vec<AiSuggestion>, PhoskError> {
    Ok(db
        .ai_suggestions()
        .await?
        .into_iter()
        .filter(|s| s.kind == RECEIPT_KIND && s.status == OPEN && s.target.as_deref() == Some(slug))
        .collect())
}

/// Refuse conflicting proposals: only one may be open per receipt.
fn single_open(open: &[AiSuggestion], slug: &str) -> Result<(), PhoskError> {
    if open.len() > 1 {
        return invalid(&format!(
            "{} conflicting open proposals for {slug}; reject all but one",
            open.len()
        ));
    }
    Ok(())
}

/// Book a validated proposal (provenance intact), then accept it. Returns
/// whether the ledger was written: a receipt already booked under the slug is
/// never re-inserted. Ids are derived from the suggestion id (see the module
/// doc) and ledger-only flags (`fixed`, `signal_id`) are dropped.
async fn apply(db: &dyn DatabaseAdapter, p: ReceiptProposal) -> Result<bool, PhoskError> {
    let id = p.suggestion_id;
    let applied = match db.receipt_by_slug(&p.receipt.slug).await {
        Ok(_) => false,
        Err(PhoskError::NotFound(_)) => {
            let (receipt, lines) = sanitised(p);
            db.insert_receipt(receipt, lines).await?;
            true
        }
        Err(e) => return Err(e),
    };
    db.update_suggestion_status(id, ACCEPTED).await?;
    Ok(applied)
}

fn sanitised(p: ReceiptProposal) -> (Receipt, Vec<LineItem>) {
    let sid = p.suggestion_id;
    let receipt_id = ReceiptId::from_uuid(derived_id(sid, "receipt", 0));
    let receipt = Receipt {
        id: receipt_id,
        fixed: false,
        source_kind: "PHOTO".to_owned(),
        ..p.receipt
    };
    let lines = p
        .line_items
        .into_iter()
        .zip(0_u64..)
        .map(|(l, i)| LineItem {
            id: LineItemId::from_uuid(derived_id(sid, "line", i)),
            receipt_id,
            signal_id: None,
            ..l
        })
        .collect();
    (receipt, lines)
}

/// A stable id for the `index`-th `what` row booked by approving suggestion
/// `sid`: the first 16 bytes of a domain-separated SHA-256, stamped as a v4
/// (random-format) UUID. Same inputs → same id; an unrelated stored row can
/// only be hit by a SHA-256 collision.
fn derived_id(sid: SuggestionId, what: &str, index: u64) -> Uuid {
    let digest = Sha256::new()
        .chain_update(b"phoskonomia/ai_approval/v1\0")
        .chain_update(what.as_bytes())
        .chain_update(b"\0")
        .chain_update(sid.as_uuid().as_bytes())
        .chain_update(index.to_le_bytes())
        .finalize();
    let mut bytes = [0_u8; 16];
    for (b, d) in bytes.iter_mut().zip(digest) {
        *b = d;
    }
    Builder::from_random_bytes(bytes).into_uuid()
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

/// Characters a proposed text may not contain: control characters (Cc), plus
/// the invisible format characters that can make a reviewer read something
/// other than what is booked — bidi embeddings / overrides / isolates
/// (U+202A–U+202E, U+2066–U+2069) and zero-width characters (U+200B–U+200D,
/// U+2060, U+FEFF). ZWJ (U+200D) is included too: receipt text has no use for
/// emoji sequences, and it can hide a join between look-alike words.
/// The intake pipeline strips these; approval refuses them.
pub fn is_unsafe_text_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
                | '\u{200B}'..='\u{200D}'
                | '\u{2060}'
                | '\u{FEFF}'
        )
}

fn bounded_text(s: &str, what: &str) -> Result<(), PhoskError> {
    if s.trim().is_empty()
        || s.chars().count() > MAX_TEXT_CHARS
        || s.chars().any(is_unsafe_text_char)
    {
        return invalid(&format!(
            "{what} is empty, too long or has control / invisible format characters"
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
