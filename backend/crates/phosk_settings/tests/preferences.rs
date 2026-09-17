#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors the
    // dashboard integration test).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::bool_assert_comparison,
    clippy::needless_collect,
    clippy::comparison_chain,
    clippy::redundant_closure_for_method_calls,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_possible_truncation
)]
//! RED integration tests for the `[preferences]` feature of `phosk_settings`.
//!
//! These drive the public service fns against the deterministic Swiss
//! May/June-2026 seed in [`phosk_db_memory::MemoryDb`] and assert the EXACT
//! values the seed pins. They are written before the implementation: every body
//! is `todo!()`, so each test compiles and then PANICS at runtime (red).
//!
//! The seed (see `phosk_db_memory::seed::seed_preferences`) is the source of
//! truth for the expected values:
//!
//! | key                       | value | surface   | storedOnDevice | provenance     |
//! |---------------------------|-------|-----------|----------------|----------------|
//! | momentum_baseline_cycles  | "3"   | analytics | true           | RuleGenerated  |
//! | currency                  | "CHF" | general   | true           | RuleGenerated  |
//! | cycle_period              | "month"| general  | true           | RuleGenerated  |
//! | low_confidence_threshold  | "0.7" | ai        | true           | UserModified   |
//! | telemetry                 | "off" | privacy   | true           | UserModified   |
//!
//! ⇒ totalPreferences = 5, changedCount = 2 (the two `UserModified` rows).

use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_model::BudgetConfig;
use phosk_settings::preferences::DEFAULT_MOMENTUM_BASELINE_CYCLES;
use phosk_settings::{
    PreferenceDto, SettingsSummaryDto, momentum_baseline_cycles, preferences, reset_preference,
    set_preference, settings_summary,
};

/// The five seeded keys, as `(key, value, surface, stored_on_device)`.
const SEEDED: [(&str, &str, &str, bool); 5] = [
    ("momentum_baseline_cycles", "3", "analytics", true),
    ("currency", "CHF", "general", true),
    ("cycle_period", "month", "general", true),
    ("low_confidence_threshold", "0.7", "ai", true),
    ("telemetry", "off", "privacy", true),
];

/// The deterministic Swiss seed (5 preferences, 2 `UserModified`).
fn seeded_db() -> MemoryDb {
    MemoryDb::seeded().expect("the deterministic seed must build")
}

/// A db with NO preferences at all (the dashboard trio is empty too).
fn empty_db() -> MemoryDb {
    MemoryDb::new(
        Vec::new(),
        Vec::new(),
        BudgetConfig {
            monthly_budget: Money::ZERO,
            savings_target: Money::ZERO,
        },
    )
}

fn find<'a>(prefs: &'a [PreferenceDto], key: &str) -> &'a PreferenceDto {
    prefs
        .iter()
        .find(|p| p.key == key)
        .unwrap_or_else(|| panic!("preference `{key}` must be present"))
}

// ── const sanity ───────────────────────────────────────────────────────────

#[test]
fn default_momentum_baseline_cycles_const_is_three() {
    assert_eq!(DEFAULT_MOMENTUM_BASELINE_CYCLES, 3);
}

// ── preferences(): listing ──────────────────────────────────────────────────

#[tokio::test]
async fn preferences_lists_every_seeded_key() {
    let db = seeded_db();
    let prefs = preferences(&db).await.expect("preferences should succeed");

    assert_eq!(
        prefs.len(),
        SEEDED.len(),
        "the seed pins exactly {} preferences",
        SEEDED.len()
    );

    let mut keys: Vec<&str> = prefs.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    let mut expected: Vec<&str> = SEEDED.iter().map(|(k, _, _, _)| *k).collect();
    expected.sort_unstable();
    assert_eq!(
        keys, expected,
        "every seeded key must be listed exactly once"
    );
}

#[tokio::test]
async fn preferences_carry_exact_value_surface_and_stored_on_device() {
    let db = seeded_db();
    let prefs = preferences(&db).await.expect("preferences should succeed");

    for (key, value, surface, on_device) in SEEDED {
        let dto = find(&prefs, key);
        assert_eq!(dto.value, value, "value of `{key}`");
        assert_eq!(dto.surface, surface, "surface of `{key}`");
        assert_eq!(dto.stored_on_device, on_device, "storedOnDevice of `{key}`");
    }
}

#[tokio::test]
async fn preferences_have_no_duplicate_keys() {
    let db = seeded_db();
    let prefs = preferences(&db).await.expect("preferences should succeed");

    let mut seen = std::collections::HashSet::new();
    for p in &prefs {
        assert!(
            seen.insert(p.key.clone()),
            "duplicate preference key `{}`",
            p.key
        );
    }
}

#[tokio::test]
async fn preference_dto_serializes_camel_case_stored_on_device() {
    let db = seeded_db();
    let prefs = preferences(&db).await.expect("preferences should succeed");
    let momentum = find(&prefs, "momentum_baseline_cycles");

    let json = serde_json::to_value(momentum).expect("serialize PreferenceDto");
    let obj = json
        .as_object()
        .expect("PreferenceDto serializes to an object");

    assert!(obj.contains_key("storedOnDevice"), "camelCase key present");
    assert!(
        !obj.contains_key("stored_on_device"),
        "snake_case key must NOT be emitted"
    );
    assert_eq!(
        obj.get("key").and_then(|v| v.as_str()),
        Some("momentum_baseline_cycles")
    );
    assert_eq!(obj.get("value").and_then(|v| v.as_str()), Some("3"));
    assert_eq!(
        obj.get("surface").and_then(|v| v.as_str()),
        Some("analytics")
    );
    assert_eq!(
        obj.get("storedOnDevice")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

// ── settings_summary() ──────────────────────────────────────────────────────

#[tokio::test]
async fn settings_summary_total_preferences_matches_seed() {
    let db = seeded_db();
    let summary = settings_summary(&db)
        .await
        .expect("settings_summary should succeed");
    assert_eq!(summary.total_preferences, 5);
}

#[tokio::test]
async fn settings_summary_changed_count_is_user_modified_count() {
    let db = seeded_db();
    let summary = settings_summary(&db)
        .await
        .expect("settings_summary should succeed");
    // low_confidence_threshold + telemetry are the two UserModified rows.
    assert_eq!(summary.changed_count, 2);
}

#[tokio::test]
async fn settings_summary_engine_and_model_are_non_empty() {
    let db = seeded_db();
    let summary = settings_summary(&db)
        .await
        .expect("settings_summary should succeed");
    assert!(!summary.engine.is_empty(), "engine label must be populated");
    assert!(!summary.model.is_empty(), "model label must be populated");
}

#[tokio::test]
async fn settings_summary_changed_count_reacts_to_a_user_edit() {
    let db = seeded_db();
    // Editing a currently-RuleGenerated pref flips it to UserModified, so the
    // changed_count must increase by exactly one (5 keys stay, 2 -> 3 changed).
    set_preference(&db, "currency", "EUR")
        .await
        .expect("set_preference should succeed");

    let summary = settings_summary(&db)
        .await
        .expect("settings_summary should succeed");
    assert_eq!(summary.total_preferences, 5, "editing must not add a key");
    assert_eq!(summary.changed_count, 3, "currency becomes UserModified");
}

#[tokio::test]
async fn settings_summary_dto_serializes_camel_case() {
    // Pure DTO serialization shape — independent of the service body.
    let dto = SettingsSummaryDto {
        total_preferences: 5,
        changed_count: 2,
        engine: "OLLAMA".to_owned(),
        model: "GEMMA4".to_owned(),
    };
    let json = serde_json::to_value(&dto).expect("serialize SettingsSummaryDto");
    let obj = json.as_object().expect("serializes to an object");

    assert!(obj.contains_key("totalPreferences"));
    assert!(obj.contains_key("changedCount"));
    assert!(obj.contains_key("engine"));
    assert!(obj.contains_key("model"));
    assert!(!obj.contains_key("total_preferences"));
    assert!(!obj.contains_key("changed_count"));
    assert_eq!(
        obj.get("totalPreferences")
            .and_then(serde_json::Value::as_u64),
        Some(5)
    );
    assert_eq!(
        obj.get("changedCount").and_then(serde_json::Value::as_u64),
        Some(2)
    );
}

// ── set_preference() ────────────────────────────────────────────────────────

#[tokio::test]
async fn set_preference_updates_an_existing_value() {
    let db = seeded_db();
    set_preference(&db, "currency", "EUR")
        .await
        .expect("set_preference should succeed");

    let prefs = preferences(&db).await.expect("preferences should succeed");
    assert_eq!(prefs.len(), 5, "updating must not add a key");
    assert_eq!(find(&prefs, "currency").value, "EUR");
}

#[tokio::test]
async fn set_preference_creates_an_absent_key() {
    let db = seeded_db();
    set_preference(&db, "theme", "dark")
        .await
        .expect("set_preference should succeed");

    let prefs = preferences(&db).await.expect("preferences should succeed");
    assert_eq!(prefs.len(), 6, "a brand-new key is appended");
    assert_eq!(find(&prefs, "theme").value, "dark");
}

#[tokio::test]
async fn set_preference_changes_momentum_baseline_read_back() {
    let db = seeded_db();
    set_preference(&db, "momentum_baseline_cycles", "6")
        .await
        .expect("set_preference should succeed");

    let n = momentum_baseline_cycles(&db)
        .await
        .expect("momentum_baseline_cycles should succeed");
    assert_eq!(n, 6, "the accessor reads the freshly-set value");
}

// ── reset_preference() ──────────────────────────────────────────────────────

#[tokio::test]
async fn reset_preference_restores_the_default_momentum_baseline() {
    let db = seeded_db();
    // Move it away from the default, then reset it back.
    set_preference(&db, "momentum_baseline_cycles", "9")
        .await
        .expect("set_preference should succeed");
    reset_preference(&db, "momentum_baseline_cycles")
        .await
        .expect("reset_preference should succeed");

    let n = momentum_baseline_cycles(&db)
        .await
        .expect("momentum_baseline_cycles should succeed");
    assert_eq!(
        n, DEFAULT_MOMENTUM_BASELINE_CYCLES,
        "reset returns momentum baseline to its default of 3"
    );
}

#[tokio::test]
async fn reset_preference_drops_user_modified_from_changed_count() {
    let db = seeded_db();
    // telemetry is seeded UserModified; resetting it must un-count it.
    reset_preference(&db, "telemetry")
        .await
        .expect("reset_preference should succeed");

    let summary = settings_summary(&db)
        .await
        .expect("settings_summary should succeed");
    assert_eq!(
        summary.changed_count, 1,
        "resetting telemetry leaves only low_confidence_threshold changed"
    );
}

// ── momentum_baseline_cycles(): THE canonical accessor ───────────────────────

#[tokio::test]
async fn momentum_baseline_cycles_reads_seeded_three() {
    let db = seeded_db();
    let n = momentum_baseline_cycles(&db)
        .await
        .expect("momentum_baseline_cycles should succeed");
    assert_eq!(n, 3, "the seed pins momentum_baseline_cycles = \"3\"");
}

#[tokio::test]
async fn momentum_baseline_cycles_defaults_when_key_absent() {
    // An empty (un-seeded) db has no momentum_baseline_cycles key ⇒ NotFound ⇒
    // the accessor must DEFAULT to 3, not error.
    let db = empty_db();
    let n = momentum_baseline_cycles(&db)
        .await
        .expect("absent key must default, not error");
    assert_eq!(n, DEFAULT_MOMENTUM_BASELINE_CYCLES);
}

#[tokio::test]
async fn momentum_baseline_cycles_defaults_on_unparseable_value() {
    let db = seeded_db();
    set_preference(&db, "momentum_baseline_cycles", "not-a-number")
        .await
        .expect("set_preference should succeed");

    let n = momentum_baseline_cycles(&db)
        .await
        .expect("a garbage value must default, not error");
    assert_eq!(
        n, DEFAULT_MOMENTUM_BASELINE_CYCLES,
        "an unparseable value falls back to the default of 3"
    );
}

#[tokio::test]
async fn momentum_baseline_cycles_defaults_on_negative_value() {
    let db = seeded_db();
    // u32 cannot hold a negative ⇒ parse fail ⇒ default.
    set_preference(&db, "momentum_baseline_cycles", "-2")
        .await
        .expect("set_preference should succeed");

    let n = momentum_baseline_cycles(&db)
        .await
        .expect("a negative value must default, not error");
    assert_eq!(n, DEFAULT_MOMENTUM_BASELINE_CYCLES);
}

#[tokio::test]
async fn momentum_baseline_cycles_reads_a_larger_explicit_setting() {
    let db = seeded_db();
    set_preference(&db, "momentum_baseline_cycles", "12")
        .await
        .expect("set_preference should succeed");

    let n = momentum_baseline_cycles(&db)
        .await
        .expect("momentum_baseline_cycles should succeed");
    assert_eq!(n, 12, "N is a user setting, not capped at the default");
}

// ── validate_preference(): the authoritative write rule for user input ──────

use phosk_core::error::PhoskError;
use phosk_model::Source;
use phosk_settings::preferences::{PREFERENCE_RULES, preference_rule, validate_preference};

#[test]
fn every_seeded_key_has_a_rule_whose_default_is_the_seeded_value() {
    assert_eq!(
        PREFERENCE_RULES.len(),
        SEEDED.len(),
        "one rule per seeded key"
    );
    for (key, value, _, _) in SEEDED {
        let rule = preference_rule(key).unwrap_or_else(|| panic!("rule for `{key}`"));
        assert_eq!(rule.default, value, "factory default of `{key}`");
        assert!(
            validate_preference(key, rule.default).is_ok(),
            "the default of `{key}` must itself be a valid value"
        );
    }
}

#[test]
fn validate_preference_rejects_an_unknown_key() {
    let err = validate_preference("theme", "dark").expect_err("unknown key must be rejected");
    assert!(matches!(err, PhoskError::Invalid(_)), "got {err:?}");
    assert!(preference_rule("theme").is_none());
}

#[test]
fn validate_preference_accepts_momentum_baseline_one_to_twelve() {
    for n in 1..=12 {
        let v = n.to_string();
        assert!(
            validate_preference("momentum_baseline_cycles", &v).is_ok(),
            "{n} cycles must be accepted"
        );
    }
}

#[test]
fn validate_preference_rejects_bad_momentum_baselines() {
    for bad in ["0", "13", "-2", "not-a-number", " 3", "3.0", ""] {
        let err = validate_preference("momentum_baseline_cycles", bad)
            .expect_err("out-of-range / malformed baseline must be rejected");
        assert!(matches!(err, PhoskError::Invalid(_)), "{bad:?} → {err:?}");
    }
}

#[test]
fn validate_preference_never_lowers_the_confidence_floor() {
    for ok in ["0.7", "0.8", "0.9"] {
        assert!(
            validate_preference("low_confidence_threshold", ok).is_ok(),
            "{ok}"
        );
    }
    for bad in ["0.6", "0.5", "0", "0.75", "1.5", "NaN", "0.70"] {
        assert!(
            validate_preference("low_confidence_threshold", bad).is_err(),
            "{bad:?} must be rejected"
        );
    }
}

#[test]
fn validate_preference_locks_currency_cycle_period_and_telemetry() {
    assert!(
        validate_preference("currency", "EUR").is_err(),
        "money is CHF-only"
    );
    assert!(
        validate_preference("cycle_period", "week").is_err(),
        "cycles are monthly"
    );
    assert!(
        validate_preference("telemetry", "on").is_err(),
        "zero telemetry"
    );
    for key in ["currency", "cycle_period", "telemetry"] {
        let rule = preference_rule(key).expect("known key");
        assert_eq!(
            rule.allowed,
            [rule.default],
            "`{key}` is fixed to its default"
        );
    }
}

#[test]
fn validate_preference_messages_do_not_echo_caller_input() {
    let marker = "zz-caller-input-zz";
    let unknown = validate_preference(marker, "x").expect_err("unknown key");
    let bad_value = validate_preference("currency", marker).expect_err("bad value");
    for err in [unknown, bad_value] {
        assert!(
            !err.to_string().contains(marker),
            "error text must not echo caller input: {err}"
        );
    }
}

// ── PreferenceDto::user_modified ────────────────────────────────────────────

#[tokio::test]
async fn preference_dto_flags_user_modified_rows() {
    let db = seeded_db();
    let prefs = preferences(&db).await.expect("preferences should succeed");
    for (key, _, _, _) in SEEDED {
        let expected = matches!(key, "low_confidence_threshold" | "telemetry");
        assert_eq!(
            find(&prefs, key).user_modified,
            expected,
            "userModified of `{key}`"
        );
    }
}

#[tokio::test]
async fn preference_dto_user_modified_follows_set_and_reset() {
    let db = seeded_db();
    set_preference(&db, "cycle_period", "month")
        .await
        .expect("set_preference should succeed");
    reset_preference(&db, "telemetry")
        .await
        .expect("reset_preference should succeed");

    let prefs = preferences(&db).await.expect("preferences should succeed");
    assert!(
        find(&prefs, "cycle_period").user_modified,
        "a set stamps UserModified"
    );
    assert!(
        !find(&prefs, "telemetry").user_modified,
        "a reset clears it"
    );

    let stored = phosk_adapter_db::DatabaseAdapter::preference(&db, "telemetry")
        .await
        .expect("telemetry is stored");
    assert_eq!(stored.provenance.source, Source::RuleGenerated);
}

#[tokio::test]
async fn preference_dto_serializes_user_modified_camel_case() {
    let db = seeded_db();
    let prefs = preferences(&db).await.expect("preferences should succeed");
    let json = serde_json::to_value(find(&prefs, "telemetry")).expect("serialize");
    assert_eq!(
        json.get("userModified")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert!(json.get("user_modified").is_none());
}
