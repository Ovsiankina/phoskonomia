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
//! Model output is hostile: every text field is stripped of control chars and
//! clipped to the service's `MAX_TEXT_CHARS` before it crosses the wire, and
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
    /// Line total.
    #[serde(with = "phosk_model::money_centimes")]
    pub line_total: Money,
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
    /// The ledger slug the proposals would book under.
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
        let session = crate::data::build_session().await?;
        list_pending_proposals_with(session.db()).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
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
        let session = crate::data::build_session().await?;
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
        let session = crate::data::build_session().await?;
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
        let session = crate::data::build_session().await?;
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
use phosk_ai::ai_approval::{MAX_LINES, MAX_TEXT_CHARS, RECEIPT_KIND};
#[cfg(feature = "server-deps")]
use phosk_core::error::PhoskError;

#[cfg(feature = "server-deps")]
const BAD_ID: &str = "That is not a valid proposal id.";
#[cfg(feature = "server-deps")]
const BAD_SLUG: &str = "That is not a valid receipt.";
#[cfg(feature = "server-deps")]
const GONE: &str = "This proposal is no longer pending. Refresh to see the current queue.";
#[cfg(feature = "server-deps")]
const REFUSED: &str = "This proposal can't be booked: it failed validation, was already \
                       rejected, or conflicts with another open proposal for this receipt. \
                       Nothing was booked.";
#[cfg(feature = "server-deps")]
const BOOKED: &str = "This proposal is already booked. Correct the transaction instead.";
#[cfg(feature = "server-deps")]
const RETRY: &str = "Could not update the approval queue. Please try again.";

/// Which call failed, so `Invalid` maps to the text that fits it.
#[cfg(feature = "server-deps")]
#[derive(Clone, Copy)]
enum Action {
    Approve,
    Reject,
}

/// Map a service failure onto a fixed, user-facing message.
#[cfg(feature = "server-deps")]
fn action_error(e: &PhoskError, action: Action) -> ServerFnError {
    let msg = match (e, action) {
        (PhoskError::NotFound(_), _) => GONE,
        (PhoskError::Invalid(_) | PhoskError::InvalidDate(_), Action::Approve) => REFUSED,
        (PhoskError::Invalid(_) | PhoskError::InvalidDate(_), Action::Reject) => BOOKED,
        (PhoskError::Overflow(_), _) => RETRY,
    };
    ServerFnError::new(msg)
}

/// Strip control chars and clip hostile model text to `MAX_TEXT_CHARS`.
#[cfg(feature = "server-deps")]
fn clip(s: &str) -> String {
    let mut chars = s.chars().filter(|c| !c.is_control());
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
                receipt_slug: clip(&slug),
                proposals,
            })
        })
        .collect())
}

/// Resolve `id`, then approve that receipt proposal against `db`.
#[cfg(feature = "server-deps")]
pub(crate) async fn approve_proposal_with(
    db: &dyn DatabaseAdapter,
    id: &str,
) -> Result<ApprovalOutcomeDto, ServerFnError> {
    let s = resolve(db, id).await?;
    let o = phosk_ai::approve_suggestion(db, s.id)
        .await
        .map_err(|e| action_error(&e, Action::Approve))?;
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
        .map_err(|e| action_error(&e, Action::Reject))
}

/// Shape-check `slug`, then bulk-approve that receipt's open proposal(s).
#[cfg(feature = "server-deps")]
pub(crate) async fn approve_receipt_proposals_with(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<Vec<ApprovalOutcomeDto>, ServerFnError> {
    let well_formed = !slug.trim().is_empty()
        && slug.chars().count() <= MAX_TEXT_CHARS
        && !slug.chars().any(char::is_control);
    if !well_formed {
        return Err(ServerFnError::new(BAD_SLUG));
    }
    let outcomes = phosk_ai::approve_receipt(db, slug)
        .await
        .map_err(|e| action_error(&e, Action::Approve))?;
    Ok(outcomes
        .into_iter()
        .map(|o| ApprovalOutcomeDto {
            receipt_slug: o.receipt_slug,
            applied: o.applied,
        })
        .collect())
}
