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

/// A known preference key: its factory default and the closed set of values a
/// user may write through [`validate_preference`].
///
/// The table is the authoritative key set of the `/config` write path: a key
/// absent from [`PREFERENCE_RULES`] is unknown and rejected by
/// [`validate_preference`]. The raw [`reset_preference`] still resets such a key
/// in place (provenance cleared, current value kept), since it has no factory
/// value to fall back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreferenceRule {
    /// Setting key, e.g. `"momentum_baseline_cycles"`.
    pub key: &'static str,
    /// Factory value [`reset_preference`] restores.
    pub default: &'static str,
    /// Every value a user may set, in display order. A single entry (the
    /// default) means the preference is fixed by design.
    pub allowed: &'static [&'static str],
}

/// The known preferences, in `/config` display order.
///
/// * `momentum_baseline_cycles` — 1 to 12 trailing cycles (one year of monthly
///   cycles at most; 0 would average nothing).
/// * `currency` — `CHF` only: `Money` is CHF centimes with no conversion, so any
///   other label would mislabel every amount.
/// * `cycle_period` — `month` only: the budgeting cycle is `Period::Month`.
/// * `low_confidence_threshold` — `0.7`/`0.8`/`0.9`: a user may flag AI
///   proposals more strictly, never below the architecture's 0.7 floor.
/// * `telemetry` — `off` only: zero telemetry is a product invariant.
pub const PREFERENCE_RULES: &[PreferenceRule] = &[
    PreferenceRule {
        key: "momentum_baseline_cycles",
        default: "3",
        allowed: &[
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12",
        ],
    },
    PreferenceRule {
        key: "currency",
        default: "CHF",
        allowed: &["CHF"],
    },
    PreferenceRule {
        key: "cycle_period",
        default: "month",
        allowed: &["month"],
    },
    PreferenceRule {
        key: "low_confidence_threshold",
        default: "0.7",
        allowed: &["0.7", "0.8", "0.9"],
    },
    PreferenceRule {
        key: "telemetry",
        default: "off",
        allowed: &["off"],
    },
];

/// The rule for `key`, if it is a known preference.
#[must_use]
pub fn preference_rule(key: &str) -> Option<&'static PreferenceRule> {
    PREFERENCE_RULES.iter().find(|r| r.key == key)
}

/// Check a user-supplied `(key, value)` pair against [`PREFERENCE_RULES`].
///
/// [`set_preference`] stores whatever it is given; every write path that takes
/// user input must call this first. The error text is fixed and never echoes
/// the caller's key or value.
///
/// # Errors
/// [`PhoskError::Invalid`] if `key` is unknown or `value` is not one of the
/// key's allowed values.
pub fn validate_preference(key: &str, value: &str) -> Result<&'static PreferenceRule, PhoskError> {
    let rule = preference_rule(key)
        .ok_or_else(|| PhoskError::Invalid("unknown preference key".to_owned()))?;
    if rule.allowed.contains(&value) {
        Ok(rule)
    } else {
        Err(PhoskError::Invalid(
            "value not allowed for this preference".to_owned(),
        ))
    }
}

/// The factory default for `key`, if one is registered.
fn default_for(key: &str) -> Option<&'static str> {
    preference_rule(key).map(|r| r.default)
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
    /// Whether the stored value is a user override
    /// (`provenance.source == UserModified`).
    pub user_modified: bool,
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
            user_modified: p.provenance.source == Source::UserModified,
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
/// This is the raw store write: it does not validate. Callers holding user
/// input check it with [`validate_preference`] first.
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
