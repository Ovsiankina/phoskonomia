//! `data::subscriptions` write path: create / edit / delete and the lifecycle
//! actions, each driven through its `*_with` inner fn on a fresh seeded store.

use dioxus::prelude::ServerFnError;
use phosk_adapter_db::DatabaseAdapter;
use phosk_db_memory::MemoryDb;
use phosk_model::Subscription;

use super::support::{fresh_db, money, today};
use crate::data::subscriptions::{
    create_subscription_with, delete_subscription_with, edit_subscription_with,
    get_subscription_with, record_subscription_charge_with, run_subscription_action_with,
    SubAction, SubscriptionForm,
};

/// A valid monthly form charged on the seeded "today" (the 18th).
fn form(name: &str) -> SubscriptionForm {
    SubscriptionForm {
        name: name.into(),
        amount: "12.90".into(),
        cadence: "monthly".into(),
        day: "18".into(),
        month: String::new(),
        category: "Entertainment".into(),
        glyph: String::new(),
        note: String::new(),
    }
}

/// The message of a rejected call; any other outcome fails the test.
#[track_caller]
fn rejected<T: std::fmt::Debug>(r: Result<T, ServerFnError>) -> String {
    match r {
        Err(ServerFnError::ServerError { message, .. }) => message,
        other => panic!("expected a rejection, got {other:?}"),
    }
}

async fn stored(db: &MemoryDb, id: &str) -> Subscription {
    db.subscription_by_slug(id).await.expect("stored")
}

async fn count(db: &MemoryDb) -> usize {
    db.subscriptions().await.expect("subscriptions").len()
}

#[tokio::test]
async fn create_parses_chf_to_exact_centimes_and_returns_the_slug() {
    let db = fresh_db();
    let before = count(&db).await;
    let id = create_subscription_with(&db, today(), form("Radio Swiss"))
        .await
        .expect("created");
    assert_eq!(id, "radio-swiss");
    let sub = stored(&db, &id).await;
    assert_eq!(sub.amount, money(1_290), "CHF 12.90 is 1290 centimes");
    assert_eq!(sub.day, 18);
    assert_eq!(sub.since, today(), "tracked from the day it is created");
    assert_eq!(sub.glyph, "R", "an empty glyph falls back to the initial");
    assert_eq!(count(&db).await, before + 1);
}

#[tokio::test]
async fn create_rejects_bad_input_without_writing() {
    let db = fresh_db();
    let before = count(&db).await;
    let cases = [
        SubscriptionForm {
            name: "  ".into(),
            ..form("x")
        },
        SubscriptionForm {
            amount: "12,90".into(),
            ..form("Comma")
        },
        SubscriptionForm {
            amount: "0".into(),
            ..form("Free")
        },
        SubscriptionForm {
            day: "32".into(),
            ..form("Late")
        },
        SubscriptionForm {
            cadence: "weekly".into(),
            ..form("Weekly")
        },
        SubscriptionForm {
            cadence: "yearly".into(),
            month: "FOO".into(),
            ..form("Yearly")
        },
        SubscriptionForm {
            category: String::new(),
            ..form("Nocat")
        },
    ];
    for f in cases {
        let msg = rejected(create_subscription_with(&db, today(), f.clone()).await);
        assert!(!msg.is_empty(), "{f:?} needs a reason");
        assert!(!msg.contains("12,90"), "the raw input is never echoed");
    }
    for (amount, want) in [
        ("1000000.01", "cannot exceed one million"),
        ("-5", "above zero"),
        ("-0", "above zero"),
        ("0", "above zero"),
    ] {
        let f = SubscriptionForm {
            amount: amount.into(),
            ..form("Pricey")
        };
        let msg = rejected(create_subscription_with(&db, today(), f).await);
        assert!(msg.contains(want), "{amount}: {msg}");
    }
    let max = SubscriptionForm {
        amount: "1000000".into(),
        ..form("Cap")
    };
    let id = create_subscription_with(&db, today(), max)
        .await
        .expect("CHF 1'000'000 is the inclusive cap");
    delete_subscription_with(&db, &id).await.expect("deleted");
    let dup = rejected(create_subscription_with(&db, today(), form("Netflix")).await);
    assert!(dup.contains("already exists"), "{dup}");
    assert_eq!(count(&db).await, before, "nothing was written");
}

#[tokio::test]
async fn create_accepts_a_yearly_charge() {
    let db = fresh_db();
    let f = SubscriptionForm {
        cadence: "yearly".into(),
        month: "mar".into(),
        day: String::new(),
        amount: "CHF 99".into(),
        ..form("Vignette")
    };
    let id = create_subscription_with(&db, today(), f)
        .await
        .expect("created");
    let sub = stored(&db, &id).await;
    assert_eq!(
        (sub.cadence.as_str(), sub.month.as_str()),
        ("yearly", "MAR")
    );
    assert_eq!(sub.amount, money(9_900));
}

#[tokio::test]
async fn edit_replaces_the_fields_and_keeps_the_id() {
    let db = fresh_db();
    let f = SubscriptionForm {
        name: "Netflix Premium".into(),
        amount: "24.90".into(),
        day: "3".into(),
        note: "family plan".into(),
        ..form("")
    };
    edit_subscription_with(&db, "netflix", f)
        .await
        .expect("edited");
    let sub = stored(&db, "netflix").await;
    assert_eq!(sub.name, "Netflix Premium");
    assert_eq!(sub.amount, money(2_490));
    assert_eq!(sub.day, 3);
    assert_eq!(sub.note, "family plan");
}

#[tokio::test]
async fn edit_rejects_bad_input_and_name_clashes() {
    let db = fresh_db();
    let before = stored(&db, "netflix").await;
    let bad = SubscriptionForm {
        amount: "-5".into(),
        ..form("Netflix")
    };
    assert!(!rejected(edit_subscription_with(&db, "netflix", bad).await).is_empty());
    let clash = rejected(edit_subscription_with(&db, "netflix", form("Spotify")).await);
    assert!(clash.contains("already exists"), "{clash}");
    let gone = rejected(edit_subscription_with(&db, "nope", form("Nope")).await);
    assert!(gone.contains("no longer exists"), "{gone}");
    assert_eq!(stored(&db, "netflix").await, before, "nothing changed");
}

#[tokio::test]
async fn edit_accepts_a_rename_the_backend_accepts() {
    let db = fresh_db();
    let unchanged = SubscriptionForm {
        amount: "21.90".into(),
        ..form("Netflix")
    };
    edit_subscription_with(&db, "netflix", unchanged)
        .await
        .expect("keeping the name is no clash");
    edit_subscription_with(&db, "netflix", form("网飞"))
        .await
        .expect("the slug is frozen, so a non-ASCII name is fine");
    let sub = stored(&db, "netflix").await;
    assert_eq!((sub.name.as_str(), sub.slug.as_str()), ("网飞", "netflix"));
    let blank = rejected(edit_subscription_with(&db, "netflix", form("  ")).await);
    assert!(blank.contains("name"), "{blank}");
}

#[tokio::test]
async fn delete_removes_the_subscription() {
    let db = fresh_db();
    let before = count(&db).await;
    delete_subscription_with(&db, "spotify")
        .await
        .expect("deleted");
    assert_eq!(count(&db).await, before - 1);
    assert!(db.subscription_by_slug("spotify").await.is_err());
    let again = rejected(delete_subscription_with(&db, "spotify").await);
    assert!(again.contains("no longer exists"), "{again}");
    let bad = rejected(delete_subscription_with(&db, "../x").await);
    assert!(!bad.contains("../x"), "the raw id is never echoed");
}

#[tokio::test]
async fn pause_then_resume_rederives_the_status() {
    let db = fresh_db();
    let id = create_subscription_with(&db, today(), form("Cloud Box"))
        .await
        .expect("created");
    run_subscription_action_with(&db, today(), &id, SubAction::Pause)
        .await
        .expect("paused");
    assert_eq!(stored(&db, &id).await.status, "paused");
    let again = rejected(run_subscription_action_with(&db, today(), &id, SubAction::Pause).await);
    assert!(again.contains("not available"), "{again}");

    run_subscription_action_with(&db, today(), &id, SubAction::Resume)
        .await
        .expect("resumed");
    assert_eq!(
        stored(&db, &id).await.status,
        "due",
        "billed today, nothing recorded yet"
    );
}

#[tokio::test]
async fn mark_paid_settles_the_cycle_once() {
    let db = fresh_db();
    let id = create_subscription_with(&db, today(), form("Cloud Box"))
        .await
        .expect("created");
    run_subscription_action_with(&db, today(), &id, SubAction::MarkPaid)
        .await
        .expect("paid");
    let sub = stored(&db, &id).await;
    assert_eq!(
        sub.status, "soon",
        "settled: no longer due, charge day is today"
    );
    let charges = db.subscription_charges(sub.id).await.expect("charges");
    assert_eq!(charges.len(), 1);
    assert_eq!(charges[0].amount, money(1_290));
    let twice =
        rejected(run_subscription_action_with(&db, today(), &id, SubAction::MarkPaid).await);
    assert!(twice.contains("not available"), "{twice}");
}

#[tokio::test]
async fn cancel_is_final() {
    let db = fresh_db();
    run_subscription_action_with(&db, today(), "spotify", SubAction::Cancel)
        .await
        .expect("cancelled");
    assert_eq!(stored(&db, "spotify").await.status, "cancelled");
    let resume =
        rejected(run_subscription_action_with(&db, today(), "spotify", SubAction::Resume).await);
    assert!(resume.contains("not available"), "{resume}");
}

#[tokio::test]
async fn record_charge_parses_amount_and_date() {
    let db = fresh_db();
    let id = create_subscription_with(&db, today(), form("Cloud Box"))
        .await
        .expect("created");
    record_subscription_charge_with(&db, today(), &id, "13.40", "")
        .await
        .expect("recorded");
    let sub = stored(&db, &id).await;
    let charges = db.subscription_charges(sub.id).await.expect("charges");
    assert_eq!(charges.len(), 1);
    assert_eq!(charges[0].amount, money(1_340));
    assert_eq!(charges[0].date, today(), "an empty date means today");
    assert_eq!(sub.status, "soon", "the charge settles the cycle");
}

#[tokio::test]
async fn record_charge_rejects_bad_input() {
    let db = fresh_db();
    let id = create_subscription_with(&db, today(), form("Cloud Box"))
        .await
        .expect("created");
    for (amount, date, want) in [
        ("abc", "", ""),
        ("0", "", "above zero"),
        ("-0", "", "above zero"),
        ("-5", "", "above zero"),
        ("1000000.01", "", "cannot exceed one million"),
        ("5", "18.06.2026", "YYYY-MM-DD"),
        ("5", "2026-06-19", "in the future"),
        ("5", "2026-01-01", "tracking start"),
    ] {
        let msg = rejected(record_subscription_charge_with(&db, today(), &id, amount, date).await);
        assert!(
            !msg.is_empty() && msg.contains(want),
            "{amount} / {date}: {msg}"
        );
    }
    run_subscription_action_with(&db, today(), &id, SubAction::Pause)
        .await
        .expect("paused");
    let paused = rejected(record_subscription_charge_with(&db, today(), &id, "5", "").await);
    assert!(paused.contains("not available"), "{paused}");
    let sub = stored(&db, &id).await;
    let charges = db.subscription_charges(sub.id).await.expect("charges");
    assert!(charges.is_empty(), "nothing was recorded");
}

/// Every seeded charge: the inspector payload offers an action (and RECORD
/// CHARGE) exactly when running it on a fresh store is then accepted.
#[tokio::test]
async fn offered_actions_are_exactly_the_accepted_ones() {
    use SubAction::{Cancel, MarkPaid, Pause, Resume};
    let db = fresh_db();
    let ids: Vec<String> = db
        .subscriptions()
        .await
        .expect("subscriptions")
        .into_iter()
        .map(|s| s.slug)
        .collect();
    let mut mark_paid_offered = false;
    for id in &ids {
        let detail = get_subscription_with(&db, today(), id)
            .await
            .expect("detail");
        for action in [MarkPaid, Pause, Resume, Cancel] {
            let accepted = run_subscription_action_with(&fresh_db(), today(), id, action)
                .await
                .is_ok();
            assert_eq!(
                detail.actions.contains(&action),
                accepted,
                "{id} {action:?}: offered ⇔ accepted"
            );
        }
        mark_paid_offered |= detail.actions.contains(&MarkPaid);
        let recorded = record_subscription_charge_with(&fresh_db(), today(), id, "5", "")
            .await
            .is_ok();
        assert_eq!(detail.can_record_charge, recorded, "{id} record charge");
    }
    assert!(mark_paid_offered, "the seed has unsettled cycles");

    // Paused and cancelled charges offer only what the backend still accepts.
    run_subscription_action_with(&db, today(), "netflix", Pause)
        .await
        .expect("paused");
    let paused = get_subscription_with(&db, today(), "netflix")
        .await
        .expect("detail");
    assert_eq!(paused.actions, [Resume, Cancel]);
    assert!(!paused.can_record_charge);
    run_subscription_action_with(&db, today(), "netflix", Cancel)
        .await
        .expect("cancelled");
    let cancelled = get_subscription_with(&db, today(), "netflix")
        .await
        .expect("detail");
    assert!(cancelled.actions.is_empty());
    assert!(!cancelled.can_record_charge);
}
