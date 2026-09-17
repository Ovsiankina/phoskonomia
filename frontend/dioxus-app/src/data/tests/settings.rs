//! `data::settings`: the `/config` preferences view, and set / reset with
//! server-side validation. Writes run on a fresh store, never the global one.

use dioxus::prelude::ServerFnError;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_model::{BudgetConfig, Source};

use super::support::fresh_db;
use crate::data::settings::{
    get_preferences_with, reset_all_preferences_with, reset_preference_with, set_preference_with,
    PreferenceRowDto, SettingsDto,
};

const KEYS: [&str; 5] = [
    "momentum_baseline_cycles",
    "currency",
    "cycle_period",
    "low_confidence_threshold",
    "telemetry",
];

fn row<'a>(view: &'a SettingsDto, key: &str) -> &'a PreferenceRowDto {
    view.rows
        .iter()
        .find(|r| r.key == key)
        .unwrap_or_else(|| panic!("row `{key}` present"))
}

#[tokio::test]
async fn get_lists_the_known_keys_at_the_seed() {
    let db = fresh_db();
    let view = get_preferences_with(&db).await.expect("read");

    let keys: Vec<&str> = view.rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, KEYS, "known keys, in display order");

    let momentum = row(&view, "momentum_baseline_cycles");
    assert_eq!(momentum.value, "3");
    assert_eq!(momentum.default_value, "3");
    assert_eq!(momentum.allowed.len(), 12, "1 to 12 cycles");
    assert!(!momentum.user_modified);
    assert_eq!(row(&view, "currency").allowed, ["CHF"], "fixed");
    assert_eq!(
        row(&view, "low_confidence_threshold").allowed,
        ["0.7"],
        "fixed until the AI pipeline reads it"
    );
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
    let db = fresh_db();
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
    let db = fresh_db();
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
    let db = fresh_db();
    let before = db.preferences().await.expect("list");

    for (key, value) in [
        ("momentum_baseline_cycles", "0"),
        ("momentum_baseline_cycles", "13"),
        ("momentum_baseline_cycles", "abc"),
        ("currency", "EUR"),
        ("cycle_period", "week"),
        ("low_confidence_threshold", "0.5"),
        ("low_confidence_threshold", "0.9"),
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
    let db = fresh_db();
    let marker = "zz-submitted-value-zz";
    let err = set_preference_with(&db, "currency", marker)
        .await
        .expect_err("disallowed value");
    let shown = ServerFnError::new(err.to_string()).to_string();
    assert!(!shown.contains(marker), "leaked input: {shown}");
}

#[tokio::test]
async fn reset_restores_the_default_and_clears_the_override() {
    let db = fresh_db();
    set_preference_with(&db, "momentum_baseline_cycles", "6")
        .await
        .expect("valid write");
    let view = reset_preference_with(&db, "momentum_baseline_cycles")
        .await
        .expect("reset");

    let r = row(&view, "momentum_baseline_cycles");
    assert_eq!(r.value, "3");
    assert!(!r.user_modified);
    assert_eq!(
        view.changed_count, 2,
        "only the two seeded overrides remain"
    );

    let stored = db
        .preference("momentum_baseline_cycles")
        .await
        .expect("stored");
    assert_eq!(stored.value, "3");
    assert_eq!(stored.provenance.source, Source::RuleGenerated);
}

#[tokio::test]
async fn reset_rejects_an_unknown_key_without_writing() {
    let db = fresh_db();
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
    let db = fresh_db();
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
