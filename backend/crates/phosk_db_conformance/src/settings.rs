//! Settings: preferences by key, set and reset.

use phosk_adapter_db::DatabaseAdapter;
use phosk_model::{Preference, Source};

use crate::support::{Outcome, ensure_eq, ensure_not_found};

/// The list and the keyed lookup return the same records.
pub async fn preferences_lookup_by_key(db: &dyn DatabaseAdapter) -> Outcome {
    let prefs = db.preferences().await?;
    ensure_eq(&prefs.len(), &5, "preference count")?;
    for p in &prefs {
        ensure_eq(&db.preference(&p.key).await?, p, "preference(key)")?;
    }
    let baseline = db.preference("momentum_baseline_cycles").await?.value;
    ensure_eq(&baseline.as_str(), &"3", "momentum baseline")?;
    ensure_not_found(db.preference("conf-none").await, "preference(unknown)")
}

/// Setting an existing key changes only its value and stamps `UserModified`;
/// setting an unknown key creates it.
pub async fn set_preference_updates_or_creates(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.preference("currency").await?;
    db.set_preference("currency", "EUR").await?;
    let got = db.preference("currency").await?;
    ensure_eq(&got.provenance.source, &Source::UserModified, "provenance")?;
    let want = Preference {
        value: "EUR".to_owned(),
        provenance: got.provenance,
        ..before
    };
    ensure_eq(&got, &want, "only value and provenance changed")?;

    db.set_preference("conf_new_key", "on").await?;
    let created = db.preference("conf_new_key").await?;
    ensure_eq(&created.key.as_str(), &"conf_new_key", "created key")?;
    ensure_eq(&created.value.as_str(), &"on", "created value")?;
    let source = created.provenance.source;
    ensure_eq(&source, &Source::UserModified, "created provenance")?;
    ensure_eq(&db.preferences().await?.len(), &6, "one preference created")
}

/// Reset writes the default and returns provenance to `RuleGenerated`, for an
/// existing key and for an unknown one (which it creates).
pub async fn reset_preference_restores_the_default(db: &dyn DatabaseAdapter) -> Outcome {
    db.set_preference("currency", "EUR").await?;
    db.reset_preference("currency", "CHF").await?;
    let got = db.preference("currency").await?;
    ensure_eq(&got.value.as_str(), &"CHF", "reset value")?;
    ensure_eq(
        &got.provenance.source,
        &Source::RuleGenerated,
        "reset provenance",
    )?;

    db.reset_preference("conf_reset_key", "7").await?;
    let created = db.preference("conf_reset_key").await?;
    ensure_eq(&created.value.as_str(), &"7", "created default")?;
    let source = created.provenance.source;
    ensure_eq(&source, &Source::RuleGenerated, "created provenance")
}
