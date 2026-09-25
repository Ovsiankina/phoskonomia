//! `data::approvals` — the AI approval queue (T42): pending receipt proposals,
//! approve / reject one, bulk-approve per receipt.
//!
//! A proposal is a model's READING of a receipt photo, staged by intake. It
//! reaches the ledger only through the `phosk_ai::ai_approval` service, which
//! re-validates the payload; these fns never write the ledger themselves.
//! Each `#[server]` fn builds the session and delegates to its `*_with` inner
//! fn, which holds the logic and is what the tests drive (so the `/receipt`
//! review flow can reuse the same fns).
//!
//! Model output is hostile: every text field is stripped of the characters
//! the approval service refuses (`is_unsafe_text_char`: control, bidi and
//! zero-width chars) and clipped to the service's `MAX_TEXT_CHARS` before it crosses the wire, and
//! lines are capped at `MAX_LINES`. Errors map to fixed user-facing texts; a
//! `PhoskError` string (ids, slugs, validation detail) never reaches the client.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One proposed line item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalLineDto {
    /// Item name as read (clipped).
    pub name: String,
    /// Quantity as read (not money).
    pub qty: f64,
    /// Unit price.
    #[serde(with = "phosk_model::money_centimes")]
    pub unit_price: Money,
    /// Line total — the amount approval books for this line.
    #[serde(with = "phosk_model::money_centimes")]
    pub line_total: Money,
    /// `qty × unit_price` (rounded once to centimes) differs from `line_total`
    /// by more than one centime: the line books something other than it reads.
    pub mismatch: bool,
    /// Proposed category (clipped).
    pub category: String,
    /// Model confidence in `0.0..=1.0` (not money).
    pub confidence: f64,
    /// Below the shared `0.7` review threshold (`Provenance::is_low_confidence`).
    pub low_confidence: bool,
}

/// The receipt a proposal would book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedReceiptDto {
    /// Shop as read (clipped).
    pub shop: String,
    /// Receipt date, ISO `YYYY-MM-DD`.
    pub date: String,
    /// Receipt category (clipped).
    pub category: String,
    /// Receipt total.
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
    /// The proposed lines (at most `MAX_LINES`).
    pub lines: Vec<ProposalLineDto>,
}

/// One open receipt proposal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDto {
    /// The suggestion id — the approve / reject target.
    pub suggestion_id: String,
    /// Whether the staged payload passes `validate_proposal` (approve would
    /// be refused otherwise; the user can still reject it).
    pub bookable: bool,
    /// The staged receipt, `None` when no payload could be read.
    pub receipt: Option<ProposedReceiptDto>,
}

/// The open proposals targeting one receipt slug — the bulk-approve target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptGroupDto {
    /// The ledger slug the proposals would book under, verbatim (clipping it
    /// would break the bulk-approve match): an action target, not display text.
    pub receipt_slug: String,
    /// Its open proposals (more than one = conflicting; approve is refused).
    pub proposals: Vec<ProposalDto>,
}

/// What one approval did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalOutcomeDto {
    /// The ledger slug the receipt is booked under.
    pub receipt_slug: String,
    /// `false` when it was already booked (nothing written).
    pub applied: bool,
}

/// Every open receipt proposal, grouped per receipt. Non-receipt suggestions
/// have no ledger effect and are not part of the queue.
///
/// REAL: composes `phosk_ai::pending_suggestions` (read-only).
#[server]
pub async fn list_pending_proposals() -> Result<Vec<ReceiptGroupDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session()
            .await
            .map_err(|_| ServerFnError::new(LOAD_FAILED))?;
        list_pending_proposals_with(session.db()).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// One open receipt proposal by suggestion id (for a single-proposal review
/// screen); a decided or unknown id is "no longer pending".
///
/// REAL: composes `DatabaseAdapter::receipt_proposal` (read-only).
#[server]
pub async fn get_proposal(suggestion_id: String) -> Result<ProposalDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session()
            .await
            .map_err(|_| ServerFnError::new(LOAD_FAILED))?;
        get_proposal_with(session.db(), &suggestion_id).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = suggestion_id;
        Err(ServerFnError::new("server-only"))
    }
}

/// Approve one proposal: the human decision that books it.
///
/// REAL: composes `phosk_ai::approve_suggestion`.
#[server]
pub async fn approve_proposal(suggestion_id: String) -> Result<ApprovalOutcomeDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session()
            .await
            .map_err(|_| ServerFnError::new(RETRY))?;
        approve_proposal_with(session.db(), &suggestion_id).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = suggestion_id;
        Err(ServerFnError::new("server-only"))
    }
}

/// Reject one proposal; the ledger is never touched.
///
/// REAL: composes `phosk_ai::reject_suggestion`.
#[server]
pub async fn reject_proposal(suggestion_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session()
            .await
            .map_err(|_| ServerFnError::new(RETRY))?;
        reject_proposal_with(session.db(), &suggestion_id).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = suggestion_id;
        Err(ServerFnError::new("server-only"))
    }
}

/// Bulk-approve the open proposal(s) of one receipt (the page confirms first).
///
/// REAL: composes `phosk_ai::approve_receipt`.
#[server]
pub async fn approve_receipt_proposals(
    receipt_slug: String,
) -> Result<Vec<ApprovalOutcomeDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session()
            .await
            .map_err(|_| ServerFnError::new(RETRY))?;
        approve_receipt_proposals_with(session.db(), &receipt_slug).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = receipt_slug;
        Err(ServerFnError::new("server-only"))
    }
}

#[cfg(feature = "server-deps")]
use phosk_adapter_db::DatabaseAdapter;
#[cfg(feature = "server-deps")]
use phosk_ai::ai_approval::{is_unsafe_text_char, MAX_LINES, MAX_TEXT_CHARS, RECEIPT_KIND};
#[cfg(feature = "server-deps")]
use phosk_core::error::PhoskError;

#[cfg(feature = "server-deps")]
const BAD_ID: &str = "That is not a valid proposal id.";
#[cfg(feature = "server-deps")]
const BAD_SLUG: &str = "That is not a valid receipt.";
#[cfg(feature = "server-deps")]
const GONE: &str = "This proposal is no longer pending. Refresh to see the current queue.";
#[cfg(feature = "server-deps")]
pub(crate) const REFUSED: &str = "This proposal can't be booked: it failed validation, was \
                                  already rejected, conflicts with another open proposal for \
                                  this receipt, or could not be saved. Nothing was booked.";
#[cfg(feature = "server-deps")]
pub(crate) const BOOKED: &str = "This proposal is already booked. Correct the transaction instead.";
#[cfg(feature = "server-deps")]
pub(crate) const HALF_DONE: &str = "This receipt is in the ledger, but its proposal could not be \
                                    marked approved. Refresh the queue; do not enter the receipt \
                                    again.";
#[cfg(feature = "server-deps")]
pub(crate) const UNCONFIRMED: &str = "Could not confirm whether this receipt was booked. Refresh \
                                      the queue and check the ledger before trying again.";
#[cfg(feature = "server-deps")]
pub(crate) const RETRY: &str = "Could not update the approval queue. Please try again.";
#[cfg(feature = "server-deps")]
const LOAD_FAILED: &str = "Could not load the approval queue.";

/// What the ledger holds under the target slug after an approve failed. The
/// service reports storage failures as `Invalid` too, so its error alone never
/// says whether `apply` got as far as inserting the receipt.
#[cfg(feature = "server-deps")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Booked {
    Yes,
    No,
    Unknown,
}

/// Re-check the ledger under `slug` after a failed approve.
#[cfg(feature = "server-deps")]
async fn booked(db: &dyn DatabaseAdapter, slug: Option<&str>) -> Booked {
    let Some(slug) = slug else {
        return Booked::No; // no target: nothing can have been booked
    };
    match db.receipt_by_slug(slug).await {
        Ok(_) => Booked::Yes,
        Err(PhoskError::NotFound(_)) => Booked::No,
        Err(_) => Booked::Unknown,
    }
}

/// The fixed text for a failed approve, given what the ledger holds now.
#[cfg(feature = "server-deps")]
pub(crate) fn approve_error_text(e: &PhoskError, booked: Booked) -> &'static str {
    match (booked, e) {
        (Booked::Yes, _) => HALF_DONE,
        (Booked::Unknown, _) => UNCONFIRMED,
        (Booked::No, PhoskError::NotFound(_)) => GONE,
        (Booked::No, PhoskError::Invalid(_) | PhoskError::InvalidDate(_)) => REFUSED,
        (Booked::No, PhoskError::Overflow(_)) => RETRY,
    }
}

/// The fixed text for a failed reject, given the suggestion's status before
/// the call: the service refuses only an `accepted` one; any other `Invalid`
/// is a storage failure.
#[cfg(feature = "server-deps")]
pub(crate) fn reject_error_text(e: &PhoskError, status: &str) -> &'static str {
    match e {
        PhoskError::NotFound(_) => GONE,
        PhoskError::Invalid(_) if status == "accepted" => BOOKED,
        _ => RETRY,
    }
}

/// Strip the chars approval refuses and clip hostile model text to
/// `MAX_TEXT_CHARS`.
#[cfg(feature = "server-deps")]
fn clip(s: &str) -> String {
    let mut chars = s.chars().filter(|&c| !is_unsafe_text_char(c));
    let mut out: String = chars.by_ref().take(MAX_TEXT_CHARS).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

/// Resolve a client-supplied id to an open-or-decided RECEIPT suggestion. The
/// raw input is shape-checked (hyphenated UUID) and never echoed back.
#[cfg(feature = "server-deps")]
async fn resolve(
    db: &dyn DatabaseAdapter,
    id: &str,
) -> Result<phosk_model::AiSuggestion, ServerFnError> {
    let well_formed = id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
    if !well_formed {
        return Err(ServerFnError::new(BAD_ID));
    }
    let id = id.to_ascii_lowercase();
    db.ai_suggestions()
        .await
        .map_err(|_| ServerFnError::new(RETRY))?
        .into_iter()
        .find(|s| s.kind == RECEIPT_KIND && s.id.to_string() == id)
        .ok_or_else(|| ServerFnError::new(GONE))
}

/// `qty × unit` rounded once to centimes (half away from zero) differs from
/// `total` by more than one centime. Money stays integer; only the model's
/// `f64` quantity forces one float product, which is exact at the service's
/// bounds (`MAX_QTY × MAX_AMOUNT_CENTIMES` < 2^53). Non-finite → mismatch.
#[cfg(feature = "server-deps")]
pub(crate) fn line_mismatch(qty: f64, unit: Money, total: Money) -> bool {
    #[allow(clippy::cast_precision_loss)] // |centimes| ≤ 2^53 at the service's bounds
    let product = (qty * unit.centimes() as f64).round();
    #[allow(clippy::cast_precision_loss)]
    let fits = product.is_finite() && product.abs() < i64::MAX as f64;
    if !fits {
        return true;
    }
    #[allow(clippy::cast_possible_truncation)] // finite, in range, already rounded
    let expected = product as i64;
    expected.abs_diff(total.centimes()) > 1
}

#[cfg(feature = "server-deps")]
fn map_proposal(s: phosk_ai::PendingSuggestion, slug: &str) -> ProposalDto {
    let bookable = s
        .proposal
        .as_ref()
        .is_some_and(|p| phosk_ai::validate_proposal(p, slug).is_ok());
    let receipt = s.proposal.map(|p| ProposedReceiptDto {
        shop: clip(&p.receipt.shop),
        date: p.receipt.date.format("%Y-%m-%d").to_string(),
        category: clip(&p.receipt.category),
        total: p.receipt.amount,
        lines: p
            .line_items
            .into_iter()
            .take(MAX_LINES)
            .map(|l| ProposalLineDto {
                low_confidence: l.provenance.is_low_confidence(),
                confidence: l.provenance.confidence,
                mismatch: line_mismatch(l.qty, l.unit_price, l.line_total),
                name: clip(&l.name),
                qty: l.qty,
                unit_price: l.unit_price,
                line_total: l.line_total,
                category: clip(&l.category),
            })
            .collect(),
    });
    ProposalDto {
        suggestion_id: s.suggestion.id.to_string(),
        bookable,
        receipt,
    }
}

/// The open receipt proposals in `db`, grouped per receipt. Read-only.
#[cfg(feature = "server-deps")]
pub(crate) async fn list_pending_proposals_with(
    db: &dyn DatabaseAdapter,
) -> Result<Vec<ReceiptGroupDto>, ServerFnError> {
    let groups = phosk_ai::pending_suggestions(db)
        .await
        .map_err(|_| ServerFnError::new("Could not load the approval queue."))?;
    Ok(groups
        .into_iter()
        .filter_map(|g| {
            let slug = g.receipt_slug?;
            let proposals = g
                .suggestions
                .into_iter()
                .map(|s| map_proposal(s, &slug))
                .collect();
            Some(ReceiptGroupDto {
                receipt_slug: slug,
                proposals,
            })
        })
        .collect())
}

/// Resolve `id` to an OPEN receipt suggestion and map it with its payload.
#[cfg(feature = "server-deps")]
pub(crate) async fn get_proposal_with(
    db: &dyn DatabaseAdapter,
    id: &str,
) -> Result<ProposalDto, ServerFnError> {
    let s = resolve(db, id).await?;
    if s.status != "open" {
        return Err(ServerFnError::new(GONE));
    }
    let proposal = db
        .receipt_proposal(s.id)
        .await
        .map_err(|_| ServerFnError::new(LOAD_FAILED))?;
    let slug = s.target.clone().unwrap_or_default();
    Ok(map_proposal(
        phosk_ai::PendingSuggestion {
            suggestion: s,
            proposal,
        },
        &slug,
    ))
}

/// Resolve `id`, then approve that receipt proposal against `db`.
#[cfg(feature = "server-deps")]
pub(crate) async fn approve_proposal_with(
    db: &dyn DatabaseAdapter,
    id: &str,
) -> Result<ApprovalOutcomeDto, ServerFnError> {
    let s = resolve(db, id).await?;
    let o = match phosk_ai::approve_suggestion(db, s.id).await {
        Ok(o) => o,
        Err(e) => {
            let b = booked(db, s.target.as_deref()).await;
            return Err(ServerFnError::new(approve_error_text(&e, b)));
        }
    };
    Ok(ApprovalOutcomeDto {
        receipt_slug: o.receipt_slug,
        applied: o.applied,
    })
}

/// Resolve `id`, then reject that receipt proposal against `db`.
#[cfg(feature = "server-deps")]
pub(crate) async fn reject_proposal_with(
    db: &dyn DatabaseAdapter,
    id: &str,
) -> Result<(), ServerFnError> {
    let s = resolve(db, id).await?;
    phosk_ai::reject_suggestion(db, s.id)
        .await
        .map_err(|e| ServerFnError::new(reject_error_text(&e, &s.status)))
}

/// Shape-check `slug`, then bulk-approve that receipt's open proposal(s).
#[cfg(feature = "server-deps")]
pub(crate) async fn approve_receipt_proposals_with(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<Vec<ApprovalOutcomeDto>, ServerFnError> {
    let well_formed = !slug.trim().is_empty()
        && slug.chars().count() <= MAX_TEXT_CHARS
        && !slug.chars().any(is_unsafe_text_char);
    if !well_formed {
        return Err(ServerFnError::new(BAD_SLUG));
    }
    let outcomes = match phosk_ai::approve_receipt(db, slug).await {
        Ok(o) => o,
        Err(e) => {
            let b = booked(db, Some(slug)).await;
            return Err(ServerFnError::new(approve_error_text(&e, b)));
        }
    };
    Ok(outcomes
        .into_iter()
        .map(|o| ApprovalOutcomeDto {
            receipt_slug: o.receipt_slug,
            applied: o.applied,
        })
        .collect())
}
