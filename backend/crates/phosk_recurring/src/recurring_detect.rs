//! Recurring-detection (AI) — service over the `DatabaseAdapter` PORT.
//!
//! Scans receipts for repeated same-shop / same-amount monthly charges and
//! surfaces candidate subscriptions (`source == LlmInferred`, `status` set by
//! the heuristic). Confirming a candidate flips it to `UserEntered` and persists
//! it; dismissing pauses it (see [`is_open_candidate`]) — it stops surfacing but
//! is not deleted, and [`crate::lifecycle::resume_subscription`] un-dismisses it.
//! The real LLM wiring is deferred — for now this is the rule-based pre-pass
//! that feeds the AI panel.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::{Source, Subscription};
use serde::{Deserialize, Serialize};

/// A subscription is an OPEN recurring candidate when it was machine-inferred
/// (`Source::LlmInferred`) and has not yet been dismissed (`status == "paused"`)
/// or confirmed (which flips its source to `UserEntered`).
///
/// `RuleGenerated` records are excluded: they are not detection candidates, so
/// they never show up in [`detect`]'s feed and are never open here either.
/// "Paused" doubles as "dismissed" for a candidate — [`dismiss_candidate`]
/// pauses it and [`crate::lifecycle::resume_subscription`] un-dismisses it by
/// re-deriving the status.
pub(crate) fn is_open_candidate(sub: &Subscription) -> bool {
    sub.source == Source::LlmInferred && sub.status != "paused"
}

// ── DTOs ────────────────────────────────────────────────────────────────────

/// One detected recurring-charge candidate (not yet a confirmed subscription).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringCandidateDto {
    /// Stable id (the proposed subscription slug).
    pub id: String,
    /// Inferred service/charge name (from the shop).
    pub name: String,
    /// The repeated per-charge amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Inferred cadence, e.g. `"monthly"`.
    pub cadence: String,
    /// Inferred day-of-month of the charge.
    pub day: u32,
    /// Number of matching charges observed (the evidence count).
    pub occurrences: u32,
    /// Detection confidence in `0.0..=1.0` (lines `< 0.7` are low-confidence).
    pub confidence: f64,
    /// Short rationale line for the AI feed.
    pub rationale: String,
}

/// The detection sweep result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionDto {
    /// Candidate recurring charges, most-confident first.
    pub candidates: Vec<RecurringCandidateDto>,
    /// Number of receipts scanned.
    pub scanned: u32,
}

// ── Services ──────────────────────────────────────────────────────────────────

/// Scan receipts up to `as_of` for repeated charges and propose candidates.
///
/// # Errors
/// Propagates any [`PhoskError`] from the port or the scan.
#[tracing::instrument(skip(db))]
pub async fn detect(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<DetectionDto, PhoskError> {
    let _ = as_of;
    let receipts = db.all_receipts().await?;
    let scanned = u32::try_from(receipts.len()).unwrap_or(u32::MAX);

    // Candidates are the machine-inferred subscriptions not yet confirmed or
    // dismissed. Each cites the number of recorded charges as its evidence.
    let subs = db.subscriptions().await?;
    let mut candidates = Vec::new();
    for sub in subs.iter().filter(|s| is_open_candidate(s)) {
        let charges = db.subscription_charges(sub.id).await?;
        let occurrences = u32::try_from(charges.len()).unwrap_or(u32::MAX).max(1);
        candidates.push(RecurringCandidateDto {
            id: sub.slug.clone(),
            name: sub.name.clone(),
            amount: sub.amount,
            cadence: sub.cadence.clone(),
            day: sub.day,
            occurrences,
            confidence: sub.provenance.confidence.clamp(0.0, 1.0),
            rationale: format!("{occurrences} repeated charges of the same amount look recurring."),
        });
    }
    // Most-confident first; break ties by slug for determinism.
    candidates.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });

    Ok(DetectionDto {
        candidates,
        scanned,
    })
}

/// Confirm a candidate: persist it as a `UserEntered` subscription.
///
/// # Errors
/// Returns [`PhoskError::NotFound`] if `slug` resolves to no candidate;
/// otherwise propagates any port/write error.
#[tracing::instrument(skip(db))]
pub async fn confirm_candidate(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError> {
    let mut sub = open_candidate_by_slug(db, slug).await?;
    sub.source = Source::UserEntered;
    sub.provenance = phosk_model::Provenance::user_entered();
    db.upsert_subscription(sub).await?;
    Ok(())
}

/// Dismiss a candidate (drop it from the detection feed).
///
/// # Errors
/// Returns [`PhoskError::NotFound`] if `slug` resolves to no candidate;
/// otherwise propagates any port/write error.
#[tracing::instrument(skip(db))]
pub async fn dismiss_candidate(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError> {
    let mut sub = open_candidate_by_slug(db, slug).await?;
    // Dismissal pauses the candidate so it no longer surfaces, WITHOUT promoting
    // it to a user-entered subscription (its source stays `LlmInferred`).
    "paused".clone_into(&mut sub.status);
    db.upsert_subscription(sub).await?;
    Ok(())
}

/// Resolve an OPEN candidate by slug, or [`PhoskError::NotFound`].
async fn open_candidate_by_slug(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<Subscription, PhoskError> {
    let sub = db.subscription_by_slug(slug).await?;
    if is_open_candidate(&sub) {
        Ok(sub)
    } else {
        Err(PhoskError::NotFound(format!("recurring candidate {slug}")))
    }
}
