//! The `[preferences]` feature: list / summarize / set / reset user preferences,
//! plus the canonical `momentum_baseline_cycles` accessor (build-contract §5.6, §6).
//!
//! DTOs mirror the `/config` page view shapes (camelCase keys); there is no
//! dioxus `data/*.rs` spec file for settings yet, so the shapes are derived from
//! the build contract §5.6.

use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_model::Source;

/// The default trailing-N momentum baseline when the preference is unset or
/// unparseable (build-contract §6).
pub const DEFAULT_MOMENTUM_BASELINE_CYCLES: u32 = 3;

/// The active OCR/LLM engine label surfaced in the settings summary (seeded).
const ENGINE_LABEL: &str = "OLLAMA";
/// The active model label surfaced in the settings summary (seeded).
const MODEL_LABEL: &str = "GEMMA4";

/// Built-in default values for the known preference keys, used by
/// [`reset_preference`] to clear a user override back to its factory value.
///
/// A key absent from this table resets in place (provenance cleared) keeping its
/// current value, since there is no factory value to fall back to.
const PREFERENCE_DEFAULTS: &[(&str, &str)] = &[
    ("momentum_baseline_cycles", "3"),
    ("currency", "CHF"),
    ("cycle_period", "month"),
    ("low_confidence_threshold", "0.7"),
    ("telemetry", "off"),
];

/// The factory default for `key`, if one is registered.
fn default_for(key: &str) -> Option<&'static str> {
    PREFERENCE_DEFAULTS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

/// One row of the `/config` preferences list.
///
/// A projection of [`phosk_model::Preference`]: the human `key`/`value`, the
/// `surface` (page/section) it belongs to, and whether it is stored on-device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceDto {
    /// Setting key, e.g. `"momentum_baseline_cycles"`, `"currency"`.
    pub key: String,
    /// Stringified value (the service parses to a typed form where needed).
    pub value: String,
    /// Page/section this preference belongs to.
    pub surface: String,
    /// Whether the value is stored on-device only.
    pub stored_on_device: bool,
}

/// The `/config` header summary card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSummaryDto {
    /// Total number of preferences.
    pub total_preferences: u32,
    /// Count of preferences whose `provenance.source == UserModified`.
    pub changed_count: u32,
    /// Active OCR/LLM engine label (seeded), e.g. `"OLLAMA"`.
    pub engine: String,
    /// Active model label (seeded), e.g. `"GEMMA4"`.
    pub model: String,
}

/// List every preference as a [`PreferenceDto`].
///
/// # Errors
/// Returns a [`PhoskError`] if the underlying store fails to answer.
#[tracing::instrument(skip(db))]
pub async fn preferences(db: &dyn DatabaseAdapter) -> Result<Vec<PreferenceDto>, PhoskError> {
    let prefs = db.preferences().await?;
    Ok(prefs
        .into_iter()
        .map(|p| PreferenceDto {
            key: p.key,
            value: p.value,
            surface: p.surface,
            stored_on_device: p.stored_on_device,
        })
        .collect())
}

/// The `/config` header summary: preference counts + active engine/model labels.
///
/// `changed_count` = preferences whose `provenance.source == UserModified`.
///
/// # Errors
/// Returns a [`PhoskError`] if the underlying store fails to answer.
#[tracing::instrument(skip(db))]
pub async fn settings_summary(db: &dyn DatabaseAdapter) -> Result<SettingsSummaryDto, PhoskError> {
    let prefs = db.preferences().await?;
    let total_preferences = u32::try_from(prefs.len()).unwrap_or(u32::MAX);
    let changed_count = u32::try_from(
        prefs
            .iter()
            .filter(|p| p.provenance.source == Source::UserModified)
            .count(),
    )
    .unwrap_or(u32::MAX);
    Ok(SettingsSummaryDto {
        total_preferences,
        changed_count,
        engine: ENGINE_LABEL.to_owned(),
        model: MODEL_LABEL.to_owned(),
    })
}

/// Set a preference value (creating the key if absent).
///
/// # Errors
/// Returns a [`PhoskError`] if the underlying store fails to persist.
#[tracing::instrument(skip(db))]
pub async fn set_preference(
    db: &dyn DatabaseAdapter,
    key: &str,
    value: &str,
) -> Result<(), PhoskError> {
    db.set_preference(key, value).await
}

/// Reset a preference to its default (clears the user-modified override).
///
/// # Errors
/// Returns a [`PhoskError`] if the underlying store fails to persist.
#[tracing::instrument(skip(db))]
pub async fn reset_preference(db: &dyn DatabaseAdapter, key: &str) -> Result<(), PhoskError> {
    let default_value = match default_for(key) {
        Some(v) => v.to_owned(),
        // No factory default registered: keep the current value, just clear the
        // user-modified override. NotFound ⇒ nothing to reset (treat as cleared).
        None => match db.preference(key).await {
            Ok(p) => p.value,
            Err(PhoskError::NotFound(_)) => return Ok(()),
            Err(e) => return Err(e),
        },
    };
    db.reset_preference(key, &default_value).await
}

/// THE momentum-baseline accessor every analytics / signal service reads
/// (build-contract §6).
///
/// Reads the `momentum_baseline_cycles` preference, parses its `value` to a
/// `u32`, and defaults to [`DEFAULT_MOMENTUM_BASELINE_CYCLES`] (3) on
/// [`PhoskError::NotFound`] or a parse failure.
///
/// # Errors
/// Returns a [`PhoskError`] only on an underlying store failure other than
/// `NotFound` (an unset / unparseable preference is the default, not an error).
#[tracing::instrument(skip(db))]
pub async fn momentum_baseline_cycles(db: &dyn DatabaseAdapter) -> Result<u32, PhoskError> {
    match db.preference("momentum_baseline_cycles").await {
        Ok(pref) => Ok(pref
            .value
            .parse::<u32>()
            .unwrap_or(DEFAULT_MOMENTUM_BASELINE_CYCLES)),
        Err(PhoskError::NotFound(_)) => Ok(DEFAULT_MOMENTUM_BASELINE_CYCLES),
        Err(e) => Err(e),
    }
}
