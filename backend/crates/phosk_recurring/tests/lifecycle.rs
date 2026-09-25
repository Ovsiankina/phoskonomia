#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors
    // `tests/subscription_write.rs`).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]
//! Integration tests for the subscription LIFECYCLE path
//! (`phosk_recurring::lifecycle`): pause · resume · cancel · mark-paid ·
//! record charge, and the derived status that follows from them.
//!
//! They drive the service through `phosk_db_memory::MemoryDb::seeded` (the
//! deterministic Swiss seed: six standing charges, three recorded charges each
//! on the 1st of MAR/APR/MAY 2026) and assert the observable effect on the read
//! side (`subscriptions::list_subscriptions` / `subscription_detail` /
//! `subscription_stats` / `billing_sweep`), never on internals.
//!
//! Anchor `as_of = 2026-06-18` — the seed's billing-cycle "TODAY", same as
//! `tests/subscriptions.rs`.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_recurring::lifecycle::{
    LifecycleAction, NewCharge, cancel_subscription, mark_paid, pause_subscription,
    record_subscription_charge, resume_subscription,
};
use phosk_recurring::subscription_write::{NewSubscription, create_subscription};
use phosk_recurring::subscriptions::{self, SubFilter};

// ── helpers ───────────────────────────────────────────────────────────────────

/// The seed's billing-cycle anchor ("18 JUN 2026").
const fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid anchor date")
}

/// A date literal, for readable fixtures.
const fn day(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid fixture date")
}

/// A seeded in-memory adapter.
fn db() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// The stored status of a charge (what the lifecycle writes).
async fn stored_status(db: &dyn DatabaseAdapter, slug: &str) -> String {
    db.subscription_by_slug(slug)
        .await
        .expect("seeded charge")
        .status
}

/// The status the read side publishes for a charge.
async fn listed(db: &dyn DatabaseAdapter, slug: &str) -> subscriptions::SubscriptionDto {
    subscriptions::list_subscriptions(db, as_of(), SubFilter::default())
        .await
        .expect("list ok")
        .into_iter()
        .find(|s| s.id == slug)
        .unwrap_or_else(|| panic!("{slug} is listed"))
}

/// Assert an `Invalid` error, quoting `what` on failure.
fn assert_invalid<T: std::fmt::Debug>(res: Result<T, PhoskError>, what: &str) {
    match res {
        Err(PhoskError::Invalid(_)) => {}
        other => panic!("{what}: expected PhoskError::Invalid, got {other:?}"),
    }
}

/// Assert a `NotFound` error, quoting `what` on failure.
fn assert_not_found<T: std::fmt::Debug>(res: Result<T, PhoskError>, what: &str) {
    match res {
        Err(PhoskError::NotFound(_)) => {}
        other => panic!("{what}: expected PhoskError::NotFound, got {other:?}"),
    }
}

/// A recorded charge for the current cycle of a monthly seed charge.
fn cycle_charge(date: NaiveDate, centimes: i64) -> NewCharge {
    NewCharge {
        date,
        amount: Money::from_centimes(centimes),
        note: "confirmed".to_owned(),
    }
}

// ── pause ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pause_flags_the_charge_paused_for_the_read_side() {
    let db = db();
    pause_subscription(&db, "netflix").await.expect("paused");

    assert_eq!(stored_status(&db, "netflix").await, "paused");
    let dto = listed(&db, "netflix").await;
    assert_eq!(dto.status, "paused");
    assert_eq!(dto.status_label, "PAUSED", "the label follows the key");

    let stored = db.subscription_by_slug("netflix").await.expect("stored");
    assert_eq!(
        stored.provenance,
        phosk_model::Provenance::user_modified(),
        "a lifecycle transition re-stamps provenance"
    );
    assert_eq!(
        stored.amount.centimes(),
        1_990,
        "pausing changes nothing but the status"
    );
}

#[tokio::test]
async fn a_paused_charge_leaves_the_run_rate_but_stays_listed() {
    let db = db();
    let before = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    pause_subscription(&db, "netflix").await.expect("paused");
    let after = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");

    assert_eq!(
        after.count,
        before.count - 1,
        "paused is not an active charge"
    );
    assert_eq!(
        after.monthly.centimes(),
        before.monthly.centimes() - 1_990,
        "netflix 19.90 leaves the monthly run-rate"
    );
    assert_eq!(
        after.annual.centimes(),
        after.monthly.centimes() * 12,
        "the annual roll-up follows the run-rate"
    );
    assert!(
        !after.next30.items.iter().any(|i| i.name == "Netflix"),
        "a paused charge is not upcoming"
    );

    let all = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    assert!(
        all.iter().any(|s| s.id == "netflix"),
        "the card stays on the page so it can be resumed"
    );
}

#[tokio::test]
async fn a_paused_charge_leaves_the_billing_sweep() {
    let db = db();
    let before = subscriptions::billing_sweep(&db, as_of())
        .await
        .expect("sweep ok");
    assert!(before.impulses.iter().any(|i| i.id == "netflix"));

    pause_subscription(&db, "netflix").await.expect("paused");
    let after = subscriptions::billing_sweep(&db, as_of())
        .await
        .expect("sweep ok");
    assert!(
        !after.impulses.iter().any(|i| i.id == "netflix"),
        "a paused charge emits no impulse"
    );
    assert_eq!(
        after.footer.still_due.centimes(),
        before.footer.still_due.centimes() - 1_990,
        "and nothing is still due for it"
    );
}

#[tokio::test]
async fn pausing_an_already_paused_charge_is_rejected() {
    let db = db();
    pause_subscription(&db, "netflix").await.expect("paused");
    assert_invalid(pause_subscription(&db, "netflix").await, "pause(paused)");
}

#[tokio::test]
async fn pausing_an_unknown_charge_is_not_found() {
    let db = db();
    assert_not_found(pause_subscription(&db, "no-such-charge").await, "pause");
}

// ── resume ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn resume_recomputes_a_settled_cycle_as_due_soon() {
    let db = db();
    // Netflix bills on the 22nd: settle the 22 MAY cycle, so the next charge
    // (22 JUN, four days out) is what the status must reflect.
    record_subscription_charge(
        &db,
        "netflix",
        as_of(),
        cycle_charge(day(2026, 5, 22), 1_990),
    )
    .await
    .expect("charge recorded");
    assert_eq!(stored_status(&db, "netflix").await, "soon");

    pause_subscription(&db, "netflix").await.expect("paused");
    assert_eq!(stored_status(&db, "netflix").await, "paused");

    resume_subscription(&db, "netflix", as_of())
        .await
        .expect("resumed");
    assert_eq!(
        stored_status(&db, "netflix").await,
        "soon",
        "22 JUN is four days after the 18th"
    );
    assert_eq!(listed(&db, "netflix").await.status_label, "DUE SOON");
}

#[tokio::test]
async fn resume_recomputes_an_unsettled_cycle_as_due() {
    let db = db();
    // Spotify bills on the 28th; the seed records no 28 MAY charge, so the
    // cycle that opened then was never seen.
    pause_subscription(&db, "spotify").await.expect("paused");
    resume_subscription(&db, "spotify", as_of())
        .await
        .expect("resumed");
    assert_eq!(stored_status(&db, "spotify").await, "due");
    assert_eq!(listed(&db, "spotify").await.status_label, "NOT SEEN");
}

#[tokio::test]
async fn resume_recomputes_a_far_off_cycle_as_ok() {
    let db = db();
    // iCloud bills on the 15th: settle the 15 JUN cycle and the next charge is
    // 15 JUL, far outside the "due soon" window.
    record_subscription_charge(&db, "icloud", as_of(), cycle_charge(day(2026, 6, 15), 999))
        .await
        .expect("charge recorded");
    pause_subscription(&db, "icloud").await.expect("paused");
    resume_subscription(&db, "icloud", as_of())
        .await
        .expect("resumed");
    assert_eq!(stored_status(&db, "icloud").await, "ok");
}

#[tokio::test]
async fn resume_clears_a_review_flag() {
    let db = db();
    assert_eq!(
        stored_status(&db, "gym").await,
        "watch",
        "the seed flags the gym for review"
    );
    pause_subscription(&db, "gym").await.expect("paused");
    resume_subscription(&db, "gym", as_of())
        .await
        .expect("resumed");
    assert_eq!(
        stored_status(&db, "gym").await,
        "due",
        "resuming is an explicit decision: the cycle, not the old flag, decides"
    );
}

#[tokio::test]
async fn resuming_a_charge_that_is_not_paused_is_rejected() {
    let db = db();
    assert_invalid(
        resume_subscription(&db, "netflix", as_of()).await,
        "resume(active)",
    );
}

#[tokio::test]
async fn resuming_an_unknown_charge_is_not_found() {
    let db = db();
    assert_not_found(
        resume_subscription(&db, "no-such-charge", as_of()).await,
        "resume",
    );
}

// ── cancel ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cancel_ends_the_charge_but_keeps_its_history() {
    let db = db();
    let before = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    cancel_subscription(&db, "netflix")
        .await
        .expect("cancelled");

    assert_eq!(stored_status(&db, "netflix").await, "cancelled");
    let detail = subscriptions::subscription_detail(&db, as_of(), "netflix")
        .await
        .expect("still inspectable");
    assert_eq!(detail.subscription.status, "cancelled");
    assert_eq!(detail.subscription.status_label, "CANCELLED");
    assert_eq!(
        detail.recent.len(),
        3,
        "cancelling keeps the recorded history"
    );

    let after = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    assert_eq!(after.count, before.count - 1);
    assert_eq!(
        after.monthly.centimes(),
        before.monthly.centimes() - 1_990,
        "a cancelled charge costs nothing"
    );
}

#[tokio::test]
async fn cancel_is_terminal() {
    let db = db();
    cancel_subscription(&db, "netflix")
        .await
        .expect("cancelled");
    assert_invalid(cancel_subscription(&db, "netflix").await, "cancel(twice)");
    assert_invalid(pause_subscription(&db, "netflix").await, "pause(cancelled)");
    assert_invalid(
        resume_subscription(&db, "netflix", as_of()).await,
        "resume(cancelled)",
    );
    assert_invalid(mark_paid(&db, "netflix", as_of()).await, "paid(cancelled)");
}

#[tokio::test]
async fn cancelling_an_unknown_charge_is_not_found() {
    let db = db();
    assert_not_found(cancel_subscription(&db, "no-such-charge").await, "cancel");
}

// ── mark paid ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn mark_paid_records_the_cycle_charge_and_clears_due() {
    let db = db();
    let sub = db.subscription_by_slug("icloud").await.expect("seeded");
    let before = db.subscription_charges(sub.id).await.expect("charges");

    mark_paid(&db, "icloud", as_of()).await.expect("marked");

    let after = db.subscription_charges(sub.id).await.expect("charges");
    assert_eq!(after.len(), before.len() + 1, "exactly one charge recorded");
    let new = after
        .iter()
        .find(|c| !before.iter().any(|b| b.id == c.id))
        .expect("the new charge");
    assert_eq!(
        new.date,
        day(2026, 6, 15),
        "the charge is dated the cycle's billing day, not today"
    );
    assert_eq!(new.amount.centimes(), 999, "at the standing amount");
    assert_eq!(new.provenance, phosk_model::Provenance::user_entered());

    assert_eq!(
        stored_status(&db, "icloud").await,
        "ok",
        "settled, and 15 JUL is far off"
    );
    let dto = listed(&db, "icloud").await;
    assert_eq!(dto.hist.len(), 4, "the new charge joins the price history");
}

#[tokio::test]
async fn mark_paid_uses_the_cycle_anchor_of_a_yearly_charge() {
    let db = db();
    // A fresh yearly charge (no recorded history) billed every February.
    let slug = create_subscription(
        &db,
        NewSubscription {
            name: "Atlas Maps".to_owned(),
            amount: Money::from_centimes(9_600),
            cadence: "yearly".to_owned(),
            day: 0,
            month: "FEB".to_owned(),
            category: "Tools".to_owned(),
            glyph: "▤".to_owned(),
            note: String::new(),
            since: day(2026, 1, 1),
        },
    )
    .await
    .expect("created");

    mark_paid(&db, &slug, as_of()).await.expect("marked");

    let sub = db.subscription_by_slug(&slug).await.expect("stored");
    let charges = db.subscription_charges(sub.id).await.expect("charges");
    assert_eq!(charges.len(), 1);
    assert_eq!(
        charges[0].date,
        day(2026, 2, 1),
        "a yearly charge settles on the 1st of its labelled month"
    );
    assert_eq!(charges[0].amount.centimes(), 9_600);
    assert_eq!(
        stored_status(&db, &slug).await,
        "ok",
        "the next February is far off"
    );
}

#[tokio::test]
async fn mark_paid_is_rejected_when_the_cycle_is_already_settled() {
    let db = db();
    // The seed records a charge in every subscription's current yearly cycle.
    assert_invalid(mark_paid(&db, "nyt", as_of()).await, "paid(settled yearly)");
}

#[tokio::test]
async fn mark_paid_twice_in_the_same_cycle_is_rejected() {
    let db = db();
    mark_paid(&db, "icloud", as_of()).await.expect("marked");
    assert_invalid(mark_paid(&db, "icloud", as_of()).await, "paid(twice)");
    let sub = db.subscription_by_slug("icloud").await.expect("seeded");
    let charges = db.subscription_charges(sub.id).await.expect("charges");
    assert_eq!(charges.len(), 4, "the rejected call records nothing");
}

#[tokio::test]
async fn mark_paid_keeps_a_review_flag() {
    let db = db();
    mark_paid(&db, "gym", as_of()).await.expect("marked");
    assert_eq!(
        stored_status(&db, "gym").await,
        "watch",
        "paying a flagged charge does not answer the review"
    );
}

#[tokio::test]
async fn mark_paid_is_rejected_for_a_paused_charge() {
    let db = db();
    pause_subscription(&db, "icloud").await.expect("paused");
    assert_invalid(mark_paid(&db, "icloud", as_of()).await, "paid(paused)");
}

#[tokio::test]
async fn marking_an_unknown_charge_paid_is_not_found() {
    let db = db();
    assert_not_found(mark_paid(&db, "no-such-charge", as_of()).await, "mark_paid");
}

// ── record charge ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn record_charge_adds_to_the_history_and_refreshes_the_status() {
    let db = db();
    record_subscription_charge(
        &db,
        "spotify",
        as_of(),
        NewCharge {
            date: day(2026, 5, 28),
            amount: Money::from_centimes(1_695),
            note: "  price rise  ".to_owned(),
        },
    )
    .await
    .expect("charge recorded");

    let detail = subscriptions::subscription_detail(&db, as_of(), "spotify")
        .await
        .expect("detail ok");
    let newest = detail.recent.first().expect("a newest charge");
    assert_eq!(newest.date, "2026-05-28");
    assert_eq!(newest.amount.centimes(), 1_695, "exact centimes, no float");
    assert_eq!(newest.note, "price rise", "the note is trimmed");
    assert!(
        detail.subscription.price_rose,
        "the dearer charge shows as a price rise"
    );
    assert_eq!(
        stored_status(&db, "spotify").await,
        "ok",
        "the 28 MAY cycle is settled and 28 JUN is ten days out"
    );
}

#[tokio::test]
async fn record_charge_rejects_a_non_positive_amount() {
    let db = db();
    assert_invalid(
        record_subscription_charge(&db, "spotify", as_of(), cycle_charge(day(2026, 5, 28), 0))
            .await,
        "record(zero)",
    );
    assert_invalid(
        record_subscription_charge(&db, "spotify", as_of(), cycle_charge(day(2026, 5, 28), -1))
            .await,
        "record(negative)",
    );
}

#[tokio::test]
async fn record_charge_rejects_a_date_outside_the_charges_life() {
    let db = db();
    assert_invalid(
        record_subscription_charge(
            &db,
            "spotify",
            as_of(),
            cycle_charge(day(2026, 7, 1), 1_595),
        )
        .await,
        "record(future)",
    );
    assert_invalid(
        record_subscription_charge(&db, "icloud", as_of(), cycle_charge(day(2026, 1, 15), 999))
            .await,
        "record(before tracking started)",
    );
    let sub = db.subscription_by_slug("spotify").await.expect("seeded");
    let charges = db.subscription_charges(sub.id).await.expect("charges");
    assert_eq!(charges.len(), 3, "a rejected charge writes nothing");
}

#[tokio::test]
async fn record_charge_is_rejected_for_a_paused_or_cancelled_charge() {
    let db = db();
    pause_subscription(&db, "spotify").await.expect("paused");
    assert_invalid(
        record_subscription_charge(
            &db,
            "spotify",
            as_of(),
            cycle_charge(day(2026, 5, 28), 1_595),
        )
        .await,
        "record(paused)",
    );
    cancel_subscription(&db, "netflix")
        .await
        .expect("cancelled");
    assert_invalid(
        record_subscription_charge(
            &db,
            "netflix",
            as_of(),
            cycle_charge(day(2026, 5, 22), 1_990),
        )
        .await,
        "record(cancelled)",
    );
}

#[tokio::test]
async fn recording_a_charge_on_an_unknown_subscription_is_not_found() {
    let db = db();
    assert_not_found(
        record_subscription_charge(
            &db,
            "no-such-charge",
            as_of(),
            cycle_charge(day(2026, 5, 28), 1_595),
        )
        .await,
        "record",
    );
}

// ── offered actions ⇔ accepted transitions ───────────────────────────────────

/// Put a fresh seeded store's `slug` into a lifecycle state, then return it.
async fn prepared(slug: &str, prep: &str) -> MemoryDb {
    let db = db();
    match prep {
        "paused" => pause_subscription(&db, slug).await.expect("pause prep"),
        "cancelled" => cancel_subscription(&db, slug).await.expect("cancel prep"),
        _ => {}
    }
    db
}

/// Every seeded charge, in every lifecycle state: the inspector offers an
/// action exactly when the backend then accepts it, and offers RECORD CHARGE
/// exactly when a charge dated today is accepted.
#[tokio::test]
async fn offered_actions_are_exactly_the_accepted_transitions() {
    let slugs: Vec<String> = db()
        .subscriptions()
        .await
        .expect("seed")
        .into_iter()
        .map(|s| s.slug)
        .collect();
    assert!(!slugs.is_empty(), "the seed has standing charges");
    let mut offered_paid = 0;
    for slug in &slugs {
        for prep in ["", "paused", "cancelled"] {
            let detail =
                subscriptions::subscription_detail(&prepared(slug, prep).await, as_of(), slug)
                    .await
                    .expect("detail");
            for action in [
                LifecycleAction::MarkPaid,
                LifecycleAction::Pause,
                LifecycleAction::Resume,
                LifecycleAction::Cancel,
            ] {
                let db = prepared(slug, prep).await;
                let accepted = match action {
                    LifecycleAction::MarkPaid => mark_paid(&db, slug, as_of()).await,
                    LifecycleAction::Pause => pause_subscription(&db, slug).await,
                    LifecycleAction::Resume => resume_subscription(&db, slug, as_of()).await,
                    LifecycleAction::Cancel => cancel_subscription(&db, slug).await,
                }
                .is_ok();
                assert_eq!(
                    detail.actions.contains(&action),
                    accepted,
                    "{slug} ({prep:?}) {action:?}: offered ⇔ accepted"
                );
            }
            offered_paid += usize::from(detail.actions.contains(&LifecycleAction::MarkPaid));
            let db = prepared(slug, prep).await;
            let recorded =
                record_subscription_charge(&db, slug, as_of(), cycle_charge(as_of(), 500))
                    .await
                    .is_ok();
            assert_eq!(
                detail.can_record_charge, recorded,
                "{slug} ({prep:?}) record charge"
            );
        }
    }
    assert!(
        offered_paid > 0,
        "the seed has unsettled cycles to mark paid"
    );
}
