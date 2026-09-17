//! Settings read/write model for the `/config` page.
//!
//! The page's layout tweaks (inspector placement, sort orders, …) stay a
//! per-device `localStorage` store in `pages/config.rs`. The preferences the
//! backend owns — the `phosk_settings` key set — load, save and reset through
//! the `#[server]` fns below.
//!
//! Every write is checked on the server against
//! `phosk_settings::PREFERENCE_RULES` before it reaches the store: an unknown
//! key or a value outside the key's allowed set is rejected and nothing is
//! written. The client never decides what is valid; it only renders the
//! `allowed` values the server hands it.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// One known preference as the `/config` page renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceRowDto {
    /// Setting key, e.g. `"momentum_baseline_cycles"`.
    pub key: String,
    /// Current value (the factory default when the key is not stored).
    pub value: String,
    /// Factory value a reset restores.
    pub default_value: String,
    /// Every value the server accepts, in display order. One entry means the
    /// preference is fixed by design.
    pub allowed: Vec<String>,
    /// Whether the stored value is a user override.
    pub user_modified: bool,
}

/// The `/config` preferences view: the known rows plus the header labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    /// The known preferences, in display order.
    pub rows: Vec<PreferenceRowDto>,
    /// How many rows are user overrides.
    pub changed_count: u32,
    /// Active OCR/LLM engine label, e.g. `"OLLAMA"`.
    pub engine: String,
    /// Active model label, e.g. `"GEMMA4"`.
    pub model: String,
}

// ═══ preferences: get / set / reset (server fns + inner fns) ══════════════════
// Tests: `data/tests/settings.rs` drives the inner fns on a fresh store.

#[cfg(feature = "server-deps")]
use phosk_adapter_db::DatabaseAdapter;
#[cfg(feature = "server-deps")]
use phosk_core::error::PhoskError;

/// The `/config` preferences view.
///
/// REAL: composes `phosk_settings::preferences` + `settings_summary`.
#[server]
pub async fn get_preferences() -> Result<SettingsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        get_preferences_with(session.db())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// Set one known preference, returning the refreshed view.
///
/// REAL: `phosk_settings::validate_preference`, then `set_preference`.
#[server]
pub async fn set_preference(key: String, value: String) -> Result<SettingsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        set_preference_with(session.db(), &key, &value)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = (key, value);
        Err(ServerFnError::new("server-only"))
    }
}

/// Reset one known preference to its factory value, returning the refreshed view.
///
/// REAL: `phosk_settings::reset_preference` for a key in the rule table.
#[server]
pub async fn reset_preference(key: String) -> Result<SettingsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        reset_preference_with(session.db(), &key)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = key;
        Err(ServerFnError::new("server-only"))
    }
}

/// Reset every known preference (the page's "Reset all"), returning the view.
///
/// REAL: `phosk_settings::reset_preference` for each key in the rule table.
#[server]
pub async fn reset_all_preferences() -> Result<SettingsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        reset_all_preferences_with(session.db())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// Build the view: one row per known key, in rule-table order. A key missing
/// from the store shows its factory default.
#[cfg(feature = "server-deps")]
pub(crate) async fn get_preferences_with(
    db: &dyn DatabaseAdapter,
) -> Result<SettingsDto, PhoskError> {
    let stored = phosk_settings::preferences(db).await?;
    let summary = phosk_settings::settings_summary(db).await?;
    let rows: Vec<PreferenceRowDto> = phosk_settings::PREFERENCE_RULES
        .iter()
        .map(|rule| {
            let hit = stored.iter().find(|p| p.key == rule.key);
            PreferenceRowDto {
                key: rule.key.to_owned(),
                value: hit.map_or_else(|| rule.default.to_owned(), |p| p.value.clone()),
                default_value: rule.default.to_owned(),
                allowed: rule.allowed.iter().map(|v| (*v).to_owned()).collect(),
                user_modified: hit.is_some_and(|p| p.user_modified),
            }
        })
        .collect();
    let changed_count =
        u32::try_from(rows.iter().filter(|r| r.user_modified).count()).unwrap_or(u32::MAX);
    Ok(SettingsDto {
        rows,
        changed_count,
        engine: summary.engine,
        model: summary.model,
    })
}

/// Validate, then write. A rejected pair never reaches the store.
#[cfg(feature = "server-deps")]
pub(crate) async fn set_preference_with(
    db: &dyn DatabaseAdapter,
    key: &str,
    value: &str,
) -> Result<SettingsDto, PhoskError> {
    let rule = phosk_settings::validate_preference(key, value)?;
    phosk_settings::set_preference(db, rule.key, value).await?;
    get_preferences_with(db).await
}

/// Reset a key from the rule table; an unknown key is rejected, not created.
#[cfg(feature = "server-deps")]
pub(crate) async fn reset_preference_with(
    db: &dyn DatabaseAdapter,
    key: &str,
) -> Result<SettingsDto, PhoskError> {
    let rule = phosk_settings::known_preference(key)?;
    phosk_settings::reset_preference(db, rule.key).await?;
    get_preferences_with(db).await
}

/// Reset every key in the rule table, in order. Not atomic: a store failure
/// part-way leaves the earlier keys reset (each reset is itself idempotent).
#[cfg(feature = "server-deps")]
pub(crate) async fn reset_all_preferences_with(
    db: &dyn DatabaseAdapter,
) -> Result<SettingsDto, PhoskError> {
    for rule in phosk_settings::PREFERENCE_RULES {
        phosk_settings::reset_preference(db, rule.key).await?;
    }
    get_preferences_with(db).await
}
