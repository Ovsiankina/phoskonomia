//! `ai_features` — the AI feature surfaces over the panel spine.
//!
//! The activity-feed write (dismiss) and the dashboard AI insight one-liner +
//! estimated saving (`/insights/dashboard`). The richer feature surfaces
//! (auto-categorize, reprocess, suggestion accept/dismiss, candidate detection)
//! land later — see `backend-features-todo.md` §7.

use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;

/// The dashboard AI insight (`/insights/dashboard`): the GEMMA4 one-liner + the
/// estimated CHF saving it unlocks. Mirrors `dioxus-app/src/data/dashboard.rs::InsightDto`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightDto {
    /// Model badge, e.g. `"GEMMA4"`.
    pub model: String,
    /// The insight sentence.
    pub text: String,
    /// Estimated saving the suggestion unlocks, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub estimated_savings: Money,
}

/// Dismiss an AI activity-feed item by its stable id.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter writes ([`PhoskError::NotFound`]
/// when the id is unknown).
#[tracing::instrument(level = "debug", skip_all, fields(id))]
pub async fn dismiss_feed_item(db: &dyn DatabaseAdapter, id: &str) -> Result<(), PhoskError> {
    db.dismiss_feed_item(id).await
}

/// The dashboard AI insight for the cycle containing `as_of` (`/insights/dashboard`):
/// composes the GEMMA4 one-liner + estimated saving (seeded/canned for now).
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution or the adapter reads.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn dashboard_insight(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<InsightDto, PhoskError> {
    // The narrative one-liner + estimated saving the GEMMA4 model surfaces for the
    // cycle. Canned/seeded for now (real inference lands later); the sentence and
    // saving mirror `dashboard.rs::get_insight`. The `as_of` / `db` reads anchor
    // the cycle the line refers to.
    let _ = (db, as_of);
    Ok(InsightDto {
        model: "GEMMA4".to_owned(),
        text: "Coffee runs are up 28% this cycle. Capping them at CHF 70 keeps you on budget"
            .to_owned(),
        estimated_savings: Money::from_centimes(4_200),
    })
}
