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

// ═══ preferences: get / set / reset (server fns, inner fns, tests) ═══════════

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
    let rule = phosk_settings::preference_rule(key)
        .ok_or_else(|| PhoskError::Invalid("unknown preference key".to_owned()))?;
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

#[cfg(all(test, feature = "server-deps"))]
mod preferences_tests {
    use super::*;
    use phosk_adapter_db::DatabaseAdapter;
    use phosk_core::error::PhoskError;
    use phosk_core::money::Money;
    use phosk_db_memory::MemoryDb;
    use phosk_model::{BudgetConfig, Source};

    const KEYS: [&str; 5] = [
        "momentum_baseline_cycles",
        "currency",
        "cycle_period",
        "low_confidence_threshold",
        "telemetry",
    ];

    /// A FRESH seeded store per test — never the process-global stack.
    fn fresh() -> MemoryDb {
        MemoryDb::seeded().expect("the deterministic seed builds")
    }

    fn row<'a>(view: &'a SettingsDto, key: &str) -> &'a PreferenceRowDto {
        view.rows
            .iter()
            .find(|r| r.key == key)
            .unwrap_or_else(|| panic!("row `{key}` present"))
    }

    #[tokio::test]
    async fn get_lists_the_known_keys_at_the_seed() {
        let db = fresh();
        let view = get_preferences_with(&db).await.expect("read");

        let keys: Vec<&str> = view.rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, KEYS, "known keys, in display order");

        let momentum = row(&view, "momentum_baseline_cycles");
        assert_eq!(momentum.value, "3");
        assert_eq!(momentum.default_value, "3");
        assert_eq!(momentum.allowed.len(), 12, "1 to 12 cycles");
        assert!(!momentum.user_modified);
        assert_eq!(row(&view, "currency").allowed, ["CHF"], "fixed");
        assert!(row(&view, "low_confidence_threshold").user_modified);
        assert!(row(&view, "telemetry").user_modified);

        assert_eq!(view.changed_count, 2, "the two seeded overrides");
        assert_eq!(view.engine, "OLLAMA");
        assert_eq!(view.model, "GEMMA4");
    }

    #[tokio::test]
    async fn get_falls_back_to_defaults_for_unstored_keys() {
        let db = MemoryDb::new(
            Vec::new(),
            Vec::new(),
            BudgetConfig {
                monthly_budget: Money::ZERO,
                savings_target: Money::ZERO,
            },
        );
        let view = get_preferences_with(&db).await.expect("read");

        assert_eq!(view.rows.len(), KEYS.len(), "every known key is listed");
        for r in &view.rows {
            assert_eq!(r.value, r.default_value, "`{}` shows its default", r.key);
            assert!(!r.user_modified);
        }
        assert_eq!(view.changed_count, 0);
    }

    #[tokio::test]
    async fn set_persists_a_valid_value_and_the_backend_stamps_provenance() {
        let db = fresh();
        let view = set_preference_with(&db, "momentum_baseline_cycles", "6")
            .await
            .expect("a valid write succeeds");

        let r = row(&view, "momentum_baseline_cycles");
        assert_eq!(r.value, "6");
        assert!(r.user_modified);
        assert_eq!(view.changed_count, 3);

        let stored = db
            .preference("momentum_baseline_cycles")
            .await
            .expect("stored");
        assert_eq!(stored.value, "6");
        assert_eq!(stored.provenance.source, Source::UserModified);
        let n = phosk_settings::momentum_baseline_cycles(&db)
            .await
            .expect("accessor reads");
        assert_eq!(n, 6, "analytics read the new baseline");
    }

    #[tokio::test]
    async fn set_rejects_an_unknown_key_without_writing() {
        let db = fresh();
        let err = set_preference_with(&db, "theme", "dark")
            .await
            .expect_err("unknown key");

        assert!(matches!(err, PhoskError::Invalid(_)), "got {err:?}");
        assert!(matches!(
            db.preference("theme").await,
            Err(PhoskError::NotFound(_))
        ));
        assert_eq!(db.preferences().await.expect("list").len(), KEYS.len());
    }

    #[tokio::test]
    async fn set_rejects_disallowed_values_and_leaves_the_store_untouched() {
        let db = fresh();
        let before = db.preferences().await.expect("list");

        for (key, value) in [
            ("momentum_baseline_cycles", "0"),
            ("momentum_baseline_cycles", "13"),
            ("momentum_baseline_cycles", "abc"),
            ("currency", "EUR"),
            ("cycle_period", "week"),
            ("low_confidence_threshold", "0.5"),
            ("telemetry", "on"),
        ] {
            let err = set_preference_with(&db, key, value)
                .await
                .expect_err("disallowed value");
            assert!(
                matches!(err, PhoskError::Invalid(_)),
                "{key}={value}: {err:?}"
            );
        }

        assert_eq!(db.preferences().await.expect("list"), before);
    }

    #[tokio::test]
    async fn rejection_text_does_not_echo_the_submitted_value() {
        let db = fresh();
        let marker = "zz-submitted-value-zz";
        let err = set_preference_with(&db, "currency", marker)
            .await
            .expect_err("disallowed value");
        let shown = ServerFnError::new(err.to_string()).to_string();
        assert!(!shown.contains(marker), "leaked input: {shown}");
    }

    #[tokio::test]
    async fn reset_restores_the_default_and_clears_the_override() {
        let db = fresh();
        set_preference_with(&db, "low_confidence_threshold", "0.9")
            .await
            .expect("valid write");
        let view = reset_preference_with(&db, "low_confidence_threshold")
            .await
            .expect("reset");

        let r = row(&view, "low_confidence_threshold");
        assert_eq!(r.value, "0.7");
        assert!(!r.user_modified);
        assert_eq!(view.changed_count, 1, "only telemetry is still an override");

        let stored = db
            .preference("low_confidence_threshold")
            .await
            .expect("stored");
        assert_eq!(stored.provenance.source, Source::RuleGenerated);
    }

    #[tokio::test]
    async fn reset_rejects_an_unknown_key_without_writing() {
        let db = fresh();
        let err = reset_preference_with(&db, "theme")
            .await
            .expect_err("unknown key");

        assert!(matches!(err, PhoskError::Invalid(_)), "got {err:?}");
        assert!(matches!(
            db.preference("theme").await,
            Err(PhoskError::NotFound(_))
        ));
        assert_eq!(db.preferences().await.expect("list").len(), KEYS.len());
    }

    #[tokio::test]
    async fn reset_all_returns_every_key_to_its_default() {
        let db = fresh();
        set_preference_with(&db, "momentum_baseline_cycles", "12")
            .await
            .expect("valid write");
        let view = reset_all_preferences_with(&db).await.expect("reset all");

        assert_eq!(view.changed_count, 0);
        for r in &view.rows {
            assert_eq!(r.value, r.default_value, "`{}` is back at default", r.key);
            assert!(!r.user_modified);
        }
        for p in db.preferences().await.expect("list") {
            assert_eq!(p.provenance.source, Source::RuleGenerated, "`{}`", p.key);
        }
    }
}
