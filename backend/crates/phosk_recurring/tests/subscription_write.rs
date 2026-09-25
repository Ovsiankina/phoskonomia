#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors
    // `tests/subscriptions.rs`).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]
//! Integration tests for the subscription WRITE path
//! (`phosk_recurring::subscription_write`): create · edit · delete.
//!
//! They drive the service through `phosk_db_memory::MemoryDb::seeded` (the
//! deterministic Swiss seed: six standing charges) and assert the observable
//! effect on the read side (`subscriptions::list_subscriptions` /
//! `subscription_detail` / `subscription_stats`), never on internals.
//!
//! Anchor `as_of = 2026-06-18` — the seed's billing-cycle "TODAY", same as
//! `tests/subscriptions.rs`.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_recurring::subscription_write::{
    self, NewSubscription, SubscriptionEdit, create_subscription, delete_subscription,
    edit_subscription,
};
use phosk_recurring::subscriptions::{self, SubFilter};

// ── helpers ───────────────────────────────────────────────────────────────────

/// The seed's billing-cycle anchor ("18 JUN 2026").
const fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid anchor date")
}

/// A seeded in-memory adapter.
fn db() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// A valid monthly input: "Nebula Cloud", CHF 12.50, the 9th of every month.
fn nebula() -> NewSubscription {
    NewSubscription {
        name: "Nebula Cloud".to_owned(),
        amount: Money::from_centimes(1_250),
        cadence: "monthly".to_owned(),
        day: 9,
        month: String::new(),
        category: "Tech".to_owned(),
        glyph: "☁".to_owned(),
        note: "Backup storage".to_owned(),
        since: NaiveDate::from_ymd_opt(2026, 6, 1).expect("valid since date"),
    }
}

/// A valid yearly input: "Atlas Atlas", CHF 96.00, every February.
fn atlas() -> NewSubscription {
    NewSubscription {
        cadence: "yearly".to_owned(),
        day: 0,
        month: "feb".to_owned(),
        name: "Atlas Maps".to_owned(),
        amount: Money::from_centimes(9_600),
        ..nebula()
    }
}

/// The slugs currently listed by the read side.
async fn slugs(db: &dyn DatabaseAdapter) -> Vec<String> {
    subscriptions::list_subscriptions(db, as_of(), SubFilter::default())
        .await
        .expect("list ok")
        .into_iter()
        .map(|s| s.id)
        .collect()
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

// ── create ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_adds_a_user_entered_subscription_visible_to_the_read_side() {
    let db = db();
    let slug = create_subscription(&db, nebula()).await.expect("created");
    assert_eq!(slug, "nebula-cloud", "slug is derived from the name");

    let listed = slugs(&db).await;
    assert_eq!(listed.len(), 7, "the six seeded charges plus the new one");
    assert!(listed.contains(&slug), "the new charge is listed");

    let stored = db.subscription_by_slug(&slug).await.expect("stored");
    assert_eq!(stored.name, "Nebula Cloud");
    assert_eq!(stored.amount.centimes(), 1_250, "exact centimes, no float");
    assert_eq!(stored.cadence, "monthly");
    assert_eq!(stored.day, 9);
    assert_eq!(stored.month, "", "monthly charges carry no month label");
    assert_eq!(stored.category, "Tech");
    assert_eq!(stored.glyph, "☁");
    assert_eq!(stored.note, "Backup storage");
    assert_eq!(stored.status, "ok", "a fresh charge starts healthy");
    assert_eq!(stored.source, phosk_model::Source::UserEntered);
    assert_eq!(stored.provenance, phosk_model::Provenance::user_entered());
}

#[tokio::test]
async fn create_normalises_a_yearly_charge_to_an_upper_case_month_and_day_zero() {
    let db = db();
    let slug = create_subscription(&db, atlas()).await.expect("created");
    let stored = db.subscription_by_slug(&slug).await.expect("stored");
    assert_eq!(stored.cadence, "yearly");
    assert_eq!(stored.month, "FEB", "month labels are upper-case");
    assert_eq!(stored.day, 0, "yearly charges carry no day-of-month");
}

#[tokio::test]
async fn create_trims_and_slugifies_the_name() {
    let db = db();
    let slug = create_subscription(
        &db,
        NewSubscription {
            name: "  NYT   Games! ".to_owned(),
            ..nebula()
        },
    )
    .await
    .expect("created");
    assert_eq!(slug, "nyt-games");
    let stored = db.subscription_by_slug(&slug).await.expect("stored");
    assert_eq!(stored.name, "NYT   Games!", "only the edges are trimmed");
}

#[tokio::test]
async fn create_rejects_a_name_that_collides_with_an_existing_slug() {
    let db = db();
    let res = create_subscription(
        &db,
        NewSubscription {
            name: "Netflix".to_owned(),
            ..nebula()
        },
    )
    .await;
    assert_invalid(res, "duplicate slug");
    assert_eq!(slugs(&db).await.len(), 6, "nothing was written");
}

#[tokio::test]
async fn create_rejects_a_blank_or_unslugifiable_name() {
    let db = db();
    for name in ["", "   ", "!!!"] {
        let res = create_subscription(
            &db,
            NewSubscription {
                name: name.to_owned(),
                ..nebula()
            },
        )
        .await;
        assert_invalid(res, &format!("name {name:?}"));
    }
    assert_eq!(slugs(&db).await.len(), 6, "nothing was written");
}

#[tokio::test]
async fn create_rejects_a_non_positive_amount() {
    let db = db();
    for cents in [0, -1_250] {
        let res = create_subscription(
            &db,
            NewSubscription {
                amount: Money::from_centimes(cents),
                ..nebula()
            },
        )
        .await;
        assert_invalid(res, "amount");
    }
}

#[tokio::test]
async fn create_rejects_an_unknown_cadence() {
    let db = db();
    let res = create_subscription(
        &db,
        NewSubscription {
            cadence: "weekly".to_owned(),
            ..nebula()
        },
    )
    .await;
    assert_invalid(res, "cadence");
}

#[tokio::test]
async fn create_rejects_an_out_of_range_day_for_a_monthly_charge() {
    let db = db();
    for day in [0, 32] {
        let res = create_subscription(&db, NewSubscription { day, ..nebula() }).await;
        assert_invalid(res, "day");
    }
}

#[tokio::test]
async fn create_rejects_a_yearly_charge_without_a_valid_month() {
    let db = db();
    for month in ["", "SMARCH"] {
        let res = create_subscription(
            &db,
            NewSubscription {
                month: month.to_owned(),
                ..atlas()
            },
        )
        .await;
        assert_invalid(res, "month");
    }
}

#[tokio::test]
async fn create_rejects_a_blank_category() {
    let db = db();
    let res = create_subscription(
        &db,
        NewSubscription {
            category: "  ".to_owned(),
            ..nebula()
        },
    )
    .await;
    assert_invalid(res, "category");
}

#[tokio::test]
async fn a_created_charge_feeds_the_derived_read_models() {
    let db = db();
    let before = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    let slug = create_subscription(&db, nebula()).await.expect("created");
    let after = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");

    assert_eq!(after.count, before.count + 1);
    assert_eq!(
        after.monthly.centimes(),
        before.monthly.centimes() + 1_250,
        "the monthly run-rate grows by the charge amount"
    );

    let detail = subscriptions::subscription_detail(&db, as_of(), &slug)
        .await
        .expect("detail ok");
    assert_eq!(detail.subscription.name, "Nebula Cloud");
    assert_eq!(detail.subscription.source, "user");
    assert!(detail.recent.is_empty(), "a new charge has no history yet");
    assert!(
        !detail.candidate,
        "a user-entered charge is not a candidate"
    );
}

// ── edit ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn edit_applies_only_the_supplied_fields_and_stamps_user_modified() {
    let db = db();
    let before = db.subscription_by_slug("netflix").await.expect("seeded");
    edit_subscription(
        &db,
        "netflix",
        SubscriptionEdit {
            amount: Some(Money::from_centimes(2_490)),
            note: Some("Price rise from July".to_owned()),
            ..SubscriptionEdit::default()
        },
    )
    .await
    .expect("edited");

    let after = db
        .subscription_by_slug("netflix")
        .await
        .expect("still there");
    assert_eq!(after.amount.centimes(), 2_490);
    assert_eq!(after.note, "Price rise from July");
    assert_eq!(after.id, before.id, "identity is stable across an edit");
    assert_eq!(after.name, before.name, "untouched fields are untouched");
    assert_eq!(after.day, before.day);
    assert_eq!(after.category, before.category);
    assert_eq!(after.since, before.since);
    assert_eq!(after.provenance, phosk_model::Provenance::user_modified());
}

#[tokio::test]
async fn edit_with_no_fields_set_leaves_the_record_and_provenance_untouched() {
    let db = db();
    let before = db.subscription_by_slug("netflix").await.expect("seeded");
    assert_eq!(
        before.provenance,
        phosk_model::Provenance::user_entered(),
        "netflix starts user-entered, not yet edited"
    );

    edit_subscription(&db, "netflix", SubscriptionEdit::default())
        .await
        .expect("a no-op edit is not an error");

    let after = db
        .subscription_by_slug("netflix")
        .await
        .expect("still there");
    assert_eq!(
        after, before,
        "an all-None edit changes nothing, provenance included"
    );
}

#[tokio::test]
async fn edit_setting_fields_to_their_current_values_leaves_provenance_untouched() {
    let db = db();
    let before = db.subscription_by_slug("netflix").await.expect("seeded");
    assert_eq!(before.provenance, phosk_model::Provenance::user_entered());

    edit_subscription(
        &db,
        "netflix",
        SubscriptionEdit {
            amount: Some(before.amount),
            note: Some(before.note.clone()),
            category: Some(before.category.clone()),
            ..SubscriptionEdit::default()
        },
    )
    .await
    .expect("re-supplying the current values is not an error");

    let after = db
        .subscription_by_slug("netflix")
        .await
        .expect("still there");
    assert_eq!(
        after, before,
        "an edit that resolves to the current values must not launder into \
         a user decision: provenance stays user-entered, not user-modified"
    );
}

#[tokio::test]
async fn edit_keeps_the_slug_stable_when_the_name_changes() {
    let db = db();
    edit_subscription(
        &db,
        "netflix",
        SubscriptionEdit {
            name: Some("Netflix Standard".to_owned()),
            ..SubscriptionEdit::default()
        },
    )
    .await
    .expect("edited");

    let stored = db.subscription_by_slug("netflix").await.expect("same slug");
    assert_eq!(stored.name, "Netflix Standard");
    assert_eq!(slugs(&db).await.len(), 6, "a rename is not a new charge");
}

#[tokio::test]
async fn edit_can_switch_a_monthly_charge_to_yearly() {
    let db = db();
    edit_subscription(
        &db,
        "netflix",
        SubscriptionEdit {
            cadence: Some("yearly".to_owned()),
            month: Some("mar".to_owned()),
            ..SubscriptionEdit::default()
        },
    )
    .await
    .expect("edited");

    let stored = db.subscription_by_slug("netflix").await.expect("stored");
    assert_eq!(stored.cadence, "yearly");
    assert_eq!(stored.month, "MAR");
    assert_eq!(stored.day, 0, "the stale day-of-month is cleared");
}

#[tokio::test]
async fn edit_validates_the_merged_record_not_just_the_patch() {
    let db = db();
    // yearly without a month: the patch alone looks fine, the merge does not.
    let res = edit_subscription(
        &db,
        "netflix",
        SubscriptionEdit {
            cadence: Some("yearly".to_owned()),
            ..SubscriptionEdit::default()
        },
    )
    .await;
    assert_invalid(res, "yearly without a month");

    // monthly, keeping the seeded day, but with a day the patch makes invalid.
    let res = edit_subscription(
        &db,
        "netflix",
        SubscriptionEdit {
            day: Some(0),
            ..SubscriptionEdit::default()
        },
    )
    .await;
    assert_invalid(res, "day 0 on a monthly charge");

    let unchanged = db.subscription_by_slug("netflix").await.expect("stored");
    assert_eq!(unchanged.cadence, "monthly");
    assert_eq!(unchanged.day, 22, "a rejected edit writes nothing");
    assert_eq!(
        unchanged.provenance,
        phosk_model::Provenance::user_entered(),
        "a rejected edit does not re-stamp provenance"
    );
}

#[tokio::test]
async fn edit_rejects_a_name_colliding_with_another_charges_slug() {
    let db = db();
    let res = edit_subscription(
        &db,
        "netflix",
        SubscriptionEdit {
            name: Some("Spotify".to_owned()),
            ..SubscriptionEdit::default()
        },
    )
    .await;
    assert_invalid(res, "rename onto another slug");
}

#[tokio::test]
async fn edit_rejects_a_non_positive_amount() {
    let db = db();
    let res = edit_subscription(
        &db,
        "netflix",
        SubscriptionEdit {
            amount: Some(Money::ZERO),
            ..SubscriptionEdit::default()
        },
    )
    .await;
    assert_invalid(res, "amount");
}

#[tokio::test]
async fn edit_of_an_unknown_slug_is_not_found() {
    let db = db();
    let res = edit_subscription(
        &db,
        "no-such-charge",
        SubscriptionEdit {
            note: Some("x".to_owned()),
            ..SubscriptionEdit::default()
        },
    )
    .await;
    assert_not_found(res, "edit(unknown)");
}

#[tokio::test]
async fn an_edited_amount_flows_into_the_derived_read_models() {
    let db = db();
    let before = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    edit_subscription(
        &db,
        "netflix",
        SubscriptionEdit {
            amount: Some(Money::from_centimes(2_490)),
            ..SubscriptionEdit::default()
        },
    )
    .await
    .expect("edited");
    let after = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    assert_eq!(after.count, before.count, "editing is not creating");
    assert_eq!(
        after.monthly.centimes(),
        before.monthly.centimes() + 500,
        "netflix 19.90 → 24.90 lifts the run-rate by 5.00"
    );
}

// ── delete ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_removes_the_charge_and_its_history() {
    let db = db();
    let victim = db.subscription_by_slug("netflix").await.expect("seeded");
    let charges = db.subscription_charges(victim.id).await.expect("charges");
    assert!(!charges.is_empty(), "the seed gives netflix a history");

    delete_subscription(&db, "netflix").await.expect("deleted");

    assert_not_found(db.subscription(victim.id).await, "subscription(deleted)");
    assert_not_found(db.subscription_by_slug("netflix").await, "by slug");
    let listed = slugs(&db).await;
    assert_eq!(listed.len(), 5);
    assert!(!listed.iter().any(|s| s == "netflix"));
    let orphans = db.subscription_charges(victim.id).await.expect("charges");
    assert!(orphans.is_empty(), "its charges go with it");
}

#[tokio::test]
async fn delete_leaves_every_other_charge_alone() {
    let db = db();
    let spotify = db.subscription_by_slug("spotify").await.expect("seeded");
    let history = db.subscription_charges(spotify.id).await.expect("charges");

    delete_subscription(&db, "netflix").await.expect("deleted");

    assert_eq!(
        db.subscription_by_slug("spotify").await.expect("kept"),
        spotify
    );
    assert_eq!(
        db.subscription_charges(spotify.id).await.expect("charges"),
        history
    );
}

#[tokio::test]
async fn delete_of_an_unknown_slug_is_not_found() {
    let db = db();
    assert_not_found(
        delete_subscription(&db, "no-such-charge").await,
        "delete(unknown)",
    );
    assert_eq!(slugs(&db).await.len(), 6, "nothing was removed");
}

#[tokio::test]
async fn delete_is_not_idempotent_the_second_call_reports_not_found() {
    let db = db();
    delete_subscription(&db, "netflix").await.expect("deleted");
    assert_not_found(delete_subscription(&db, "netflix").await, "delete twice");
}

// ── slugs ─────────────────────────────────────────────────────────────────────

#[test]
fn slugify_lower_cases_and_collapses_separators() {
    assert_eq!(subscription_write::slugify("Nebula Cloud"), "nebula-cloud");
    assert_eq!(subscription_write::slugify("NYT   Games!"), "nyt-games");
    assert_eq!(subscription_write::slugify("  --Gym--  "), "gym");
    assert_eq!(subscription_write::slugify("iCloud+ 2TB"), "icloud-2tb");
    assert_eq!(subscription_write::slugify("!!!"), "");
}
