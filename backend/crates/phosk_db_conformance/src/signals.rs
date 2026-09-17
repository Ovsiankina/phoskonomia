//! Signals: list, lookups, occurrences, track/untrack and delete.
//! The port has no signal insert, so the checks work on the seeded signals.

use phosk_adapter_db::DatabaseAdapter;
use phosk_id::SignalId;
use phosk_model::Signal;

use crate::support::{Outcome, ensure, ensure_eq, ensure_not_found};

/// The list, the id lookup and the slug lookup return the same records;
/// unknown ids and slugs are `NotFound`.
pub async fn signals_lookup_by_id_and_slug_agree(db: &dyn DatabaseAdapter) -> Outcome {
    let sigs = db.signals().await?;
    ensure_eq(&sigs.len(), &5, "signal count")?;
    let tracked = sigs.iter().filter(|s| s.tracked).count();
    ensure_eq(&tracked, &4, "tracked signal count")?;
    for s in &sigs {
        ensure_eq(&db.signal(s.id).await?, s, "signal(id)")?;
        ensure_eq(&db.signal_by_slug(&s.slug).await?, s, "signal_by_slug")?;
    }
    ensure_not_found(db.signal(SignalId::new()).await, "signal")?;
    ensure_not_found(db.signal_by_slug("conf-none").await, "signal_by_slug")
}

/// Occurrences are scoped to their signal; an unknown signal has none.
pub async fn signal_occurrences_belong_to_their_signal(db: &dyn DatabaseAdapter) -> Outcome {
    for s in db.signals().await? {
        let occ = db.signal_occurrences(s.id).await?;
        let scoped = occ.iter().all(|o| o.signal_id == s.id);
        ensure(scoped, "occurrences carry their signal id")?;
    }
    let coffee = db.signal_by_slug("coffee").await?;
    let occ = db.signal_occurrences(coffee.id).await?;
    let amounts: Vec<i64> = occ.iter().map(|o| o.amount.centimes()).collect();
    ensure_eq(&amounts, &vec![8_960], "coffee occurrences")?;
    let none = db.signal_occurrences(SignalId::new()).await?;
    ensure(none.is_empty(), "unknown signal has no occurrences")
}

/// `set_signal_tracked` flips only the flag of the target signal.
pub async fn set_signal_tracked_flips_the_flag(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.signals().await?;
    let candidate = before.iter().find(|s| !s.tracked);
    let candidate = candidate.ok_or("the seed has an untracked candidate")?;

    db.set_signal_tracked(candidate.id, true).await?;
    let promoted = Signal {
        tracked: true,
        ..candidate.clone()
    };
    ensure_eq(&db.signal(candidate.id).await?, &promoted, "tracked")?;
    db.set_signal_tracked(candidate.id, false).await?;
    ensure_eq(&db.signal(candidate.id).await?, candidate, "untracked")?;
    for s in before.iter().filter(|s| s.id != candidate.id) {
        ensure_eq(&db.signal(s.id).await?, s, "other signals untouched")?;
    }
    let unknown = db.set_signal_tracked(SignalId::new(), true).await;
    ensure_not_found(unknown, "set_signal_tracked(unknown)")
}

/// A deleted signal is gone from every read path and cannot be deleted or
/// tracked again.
pub async fn delete_signal_removes_it_everywhere(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.signals().await?;
    let victim = before.first().ok_or("the seed has signals")?;
    db.delete_signal(victim.id).await?;

    let after = db.signals().await?;
    ensure_eq(&after.len(), &(before.len() - 1), "count after delete")?;
    let gone = after.iter().all(|s| s.id != victim.id);
    ensure(gone, "signals() no longer lists it")?;
    ensure_not_found(db.signal(victim.id).await, "signal(deleted)")?;
    ensure_not_found(db.signal_by_slug(&victim.slug).await, "by slug")?;
    ensure_not_found(db.delete_signal(victim.id).await, "delete again")?;
    let track = db.set_signal_tracked(victim.id, true).await;
    ensure_not_found(track, "set_signal_tracked(deleted)")
}
