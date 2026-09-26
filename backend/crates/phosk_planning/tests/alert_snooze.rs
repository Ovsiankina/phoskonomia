#![allow(clippy::expect_used, clippy::panic)]
//! `phosk_planning::alerts::snooze_alert` — snooze with re-trigger (T19).
//!
//! Requirement: a snoozed alert leaves the list until its term ends (a date,
//! or the start of the next cycle) and comes back on that day — or earlier,
//! as soon as its target category's rules-engine level rises above the level
//! it had when it was snoozed (at risk → over budget).

use chrono::NaiveDate;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_id::ReceiptId;
use phosk_model::{Provenance, Receipt};

use phosk_planning::alerts::{SnoozeUntil, act_on_alert, alerts, snooze_alert};
use phosk_planning::budgets::categories;

const fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
}

const fn today() -> NaiveDate {
    naive(2026, 6, 18)
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

async fn listed(db: &MemoryDb, as_of: NaiveDate, slug: &str) -> bool {
    let list = alerts(db, as_of).await.expect("alerts ok");
    list.iter().any(|a| a.id == slug)
}

/// Groceries (`a2`'s target) spent and cap at `today`.
async fn groceries(db: &MemoryDb) -> (i64, i64) {
    envelope(db, "Groceries").await
}

/// One envelope's spent and cap at `today`.
async fn envelope(db: &MemoryDb, name: &str) -> (i64, i64) {
    let cats = categories(db, today()).await.expect("categories");
    let g = cats
        .iter()
        .find(|c| c.name == name)
        .expect("seeded envelope");
    (g.spent.centimes(), g.budget.centimes())
}

async fn spend_groceries(db: &MemoryDb, slug: &str, centimes: i64) {
    spend(db, slug, "Groceries", centimes).await;
}

async fn spend(db: &MemoryDb, slug: &str, category: &str, centimes: i64) {
    db.insert_receipt(
        Receipt {
            id: ReceiptId::new(),
            slug: slug.to_owned(),
            shop: "Test Market".to_owned(),
            date: today(),
            category: category.to_owned(),
            amount: Money::from_centimes(centimes),
            fixed: false,
            provenance: Provenance::user_entered(),
            source_kind: "MANUAL".to_owned(),
            ocr_engine: String::new(),
            ocr_regions: 0,
        },
        Vec::new(),
    )
    .await
    .expect("receipt stored");
}

#[tokio::test]
async fn snooze_until_a_date_hides_the_alert_until_that_day() {
    let db = seeded();
    let term = snooze_alert(&db, "a3", SnoozeUntil::Date(naive(2026, 6, 25)), today())
        .await
        .expect("snooze ok");
    assert_eq!(term.until, naive(2026, 6, 25));

    assert!(!listed(&db, today(), "a3").await, "hidden right away");
    assert!(
        !listed(&db, naive(2026, 6, 24), "a3").await,
        "hidden the day before"
    );
    assert!(
        listed(&db, naive(2026, 6, 25), "a3").await,
        "back on the day"
    );
}

#[tokio::test]
async fn snooze_until_next_cycle_ends_on_the_next_cycle_start() {
    let db = seeded();
    let term = snooze_alert(&db, "a3", SnoozeUntil::NextCycle, today())
        .await
        .expect("snooze ok");
    assert_eq!(term.until, naive(2026, 7, 1));
    assert!(!listed(&db, naive(2026, 6, 30), "a3").await);
    assert!(listed(&db, naive(2026, 7, 1), "a3").await);
}

#[tokio::test]
async fn a_worsening_condition_brings_the_alert_back_early() {
    let db = seeded();
    let (spent, cap) = groceries(&db).await;
    assert!(spent <= cap, "precondition: Groceries is under its cap");

    let term = snooze_alert(&db, "a2", SnoozeUntil::NextCycle, today())
        .await
        .expect("snooze ok");
    assert_eq!(term.level, 0, "no Groceries rule fires when snoozed");
    assert!(!listed(&db, today(), "a2").await);

    spend_groceries(&db, "t-over", cap - spent + 1).await;
    assert!(listed(&db, today(), "a2").await, "over budget re-triggers");
}

#[tokio::test]
async fn more_spend_at_the_same_level_keeps_it_snoozed() {
    let db = seeded();
    let (spent, cap) = groceries(&db).await;
    spend_groceries(&db, "t-over", cap - spent + 1).await;
    let term = snooze_alert(&db, "a2", SnoozeUntil::NextCycle, today())
        .await
        .expect("snooze ok");
    assert_eq!(term.level, 2, "snoozed while over budget");

    // Further over the cap is still "over budget": no re-trigger.
    spend_groceries(&db, "t-more", 50_000).await;
    assert!(!listed(&db, today(), "a2").await);
    assert!(
        listed(&db, naive(2026, 7, 1), "a2").await,
        "back next cycle"
    );
}

#[tokio::test]
async fn invalid_snoozes_are_rejected() {
    let db = seeded();
    for until in [today(), naive(2026, 6, 1)] {
        let res = snooze_alert(&db, "a3", SnoozeUntil::Date(until), today()).await;
        assert!(
            matches!(res, Err(PhoskError::Invalid(_))),
            "{until}: {res:?}"
        );
    }
    let res = snooze_alert(&db, "nope", SnoozeUntil::NextCycle, today()).await;
    assert!(matches!(res, Err(PhoskError::NotFound(_))), "{res:?}");

    act_on_alert(&db, "a1", "dismiss", today())
        .await
        .expect("dismiss");
    let res = snooze_alert(&db, "a1", SnoozeUntil::NextCycle, today()).await;
    assert!(matches!(res, Err(PhoskError::Invalid(_))), "{res:?}");

    let a3 = db.alerts().await.expect("alerts");
    let a3 = a3.iter().find(|a| a.slug == "a3").expect("a3");
    assert_eq!(a3.status, "active", "rejected snoozes write nothing");
    assert_eq!(a3.snooze, None);
}

#[tokio::test]
async fn an_at_risk_level_brings_a_quiet_alert_back_early() {
    let db = seeded();
    let (spent, cap) = groceries(&db).await;
    let term = snooze_alert(&db, "a2", SnoozeUntil::NextCycle, today())
        .await
        .expect("snooze ok");
    assert_eq!(term.level, 0, "no Groceries rule fires when snoozed");

    // Just under the cap: the run-rate / 80 % rule fires, the over rule not.
    spend_groceries(&db, "t-risk", cap - spent - 100).await;
    assert!(listed(&db, today(), "a2").await, "at risk re-triggers");
}

#[tokio::test]
async fn at_risk_holds_up_to_the_cap_and_re_triggers_past_it() {
    let db = seeded();
    let (spent, cap) = groceries(&db).await;
    spend_groceries(&db, "t-risk", cap - spent - 100).await;
    let term = snooze_alert(&db, "a2", SnoozeUntil::NextCycle, today())
        .await
        .expect("snooze ok");
    assert_eq!(term.level, 1, "snoozed while at risk");

    spend_groceries(&db, "t-more", 50).await;
    assert!(!listed(&db, today(), "a2").await, "still at risk: held");

    // `spent == cap` is not over budget (the rule is `spent > cap`).
    spend_groceries(&db, "t-cap", 50).await;
    assert_eq!(groceries(&db).await.0, cap, "precondition: spent == cap");
    assert!(!listed(&db, today(), "a2").await, "at the cap: held");

    spend_groceries(&db, "t-over", 1).await;
    assert!(listed(&db, today(), "a2").await, "over budget re-triggers");
}

#[tokio::test]
async fn snoozing_at_the_cap_records_at_risk_not_over() {
    let db = seeded();
    let (spent, cap) = groceries(&db).await;
    spend_groceries(&db, "t-cap", cap - spent).await;
    let term = snooze_alert(&db, "a2", SnoozeUntil::NextCycle, today())
        .await
        .expect("snooze ok");
    assert_eq!(term.level, 1, "spent == cap is at risk, not over");
}

#[tokio::test]
async fn the_snooze_action_snoozes_until_the_next_cycle() {
    let db = seeded();
    act_on_alert(&db, "a3", "snooze", today())
        .await
        .expect("snooze ok");
    let all = db.alerts().await.expect("alerts");
    let a3 = all.iter().find(|a| a.slug == "a3").expect("a3");
    assert_eq!(a3.status, "snoozed");
    let term = a3.snooze.expect("the action records a term");
    assert_eq!(term.until, naive(2026, 7, 1), "the cycle after as_of");
    assert!(!listed(&db, naive(2026, 6, 30), "a3").await);
    assert!(listed(&db, naive(2026, 7, 1), "a3").await);
}

#[tokio::test]
async fn snooze_alert_keeps_an_at_risk_alert_hidden_at_as_of() {
    let db = seeded();
    // a1 targets Going out; put it at risk (under the cap) on the demo day.
    // a1's seeded actions are VIEW/RAISE CAP/DISMISS (no SNOOZE button), so this
    // exercises `snooze_alert` directly rather than through `act_on_alert`,
    // which now rejects a kind the alert doesn't offer (F1).
    let (spent, cap) = envelope(&db, "Going out").await;
    spend(&db, "t-risk", "Going out", cap - spent - 100).await;
    snooze_alert(&db, "a1", SnoozeUntil::NextCycle, today())
        .await
        .expect("snooze ok");
    let all = db.alerts().await.expect("alerts");
    let a1 = all.iter().find(|a| a.slug == "a1").expect("a1");
    let term = a1.snooze.expect("the action records a term");
    assert_eq!(term.level, 1, "level measured at as_of");
    assert!(!listed(&db, today(), "a1").await, "hidden at as_of");
}
