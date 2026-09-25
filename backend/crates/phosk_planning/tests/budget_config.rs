#![allow(clippy::expect_used, clippy::panic)]
//! `phosk_planning::budget_config` — the global monthly budget and savings
//! target write side (T19).
//!
//! Requirement: each setter validates its amount, writes the config through
//! the port, and appends one history entry (field, old → new, day,
//! `UserModified` provenance) per real change. The read side that already
//! consumes the config (`budget_totals`, `totals`) shows the new value.

use chrono::NaiveDate;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::Period;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_model::Source;

use phosk_planning::budget_config::{budget_changes, set_monthly_budget, set_savings_target};
use phosk_planning::budgets::budget_totals;
use phosk_planning::totals;

const fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid test date")
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

const fn chf(centimes: i64) -> Money {
    Money::from_centimes(centimes)
}

#[tokio::test]
async fn set_monthly_budget_updates_the_config_and_the_read_side() {
    let db = seeded();
    let cfg = set_monthly_budget(&db, chf(450_000), today())
        .await
        .expect("set ok");
    assert_eq!(cfg.monthly_budget, chf(450_000));
    assert_eq!(cfg.savings_target, chf(90_000), "target untouched");
    assert_eq!(db.budget_config().await.expect("config"), cfg);

    let band = budget_totals(&db, today()).await.expect("totals");
    assert_eq!(band.budget, chf(450_000));
    let window = Period::Month.resolve(today()).expect("window");
    let kpi = totals(&db, window).await.expect("kpis");
    assert_eq!(kpi.budget, chf(450_000));
}

#[tokio::test]
async fn set_monthly_budget_appends_a_stamped_history_entry() {
    let db = seeded();
    set_monthly_budget(&db, chf(450_000), today())
        .await
        .expect("set ok");
    let history = budget_changes(&db).await.expect("history");
    assert_eq!(history.len(), 1);
    let entry = &history[0];
    assert_eq!(entry.field, "monthly_budget");
    assert_eq!(entry.old_value, chf(420_000));
    assert_eq!(entry.new_value, chf(450_000));
    assert_eq!(entry.at, today());
    assert_eq!(entry.provenance.source, Source::UserModified);
}

#[tokio::test]
async fn set_savings_target_updates_only_the_target_and_logs_it() {
    let db = seeded();
    let cfg = set_savings_target(&db, chf(120_000), today())
        .await
        .expect("set ok");
    assert_eq!(cfg.savings_target, chf(120_000));
    assert_eq!(cfg.monthly_budget, chf(420_000), "budget untouched");
    let window = Period::Month.resolve(today()).expect("window");
    let kpi = totals(&db, window).await.expect("kpis");
    assert_eq!(kpi.savings_target, chf(120_000));

    let history = budget_changes(&db).await.expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].field, "savings_target");
    assert_eq!(history[0].old_value, chf(90_000));
    assert_eq!(history[0].new_value, chf(120_000));
}

#[tokio::test]
async fn a_zero_savings_target_is_allowed() {
    let db = seeded();
    let cfg = set_savings_target(&db, Money::ZERO, today())
        .await
        .expect("no target is a valid choice");
    assert_eq!(cfg.savings_target, Money::ZERO);
}

#[tokio::test]
async fn invalid_amounts_are_rejected_without_any_write() {
    let db = seeded();
    let before = db.budget_config().await.expect("config");
    for bad in [Money::ZERO, chf(-1)] {
        let res = set_monthly_budget(&db, bad, today()).await;
        assert!(
            matches!(res, Err(PhoskError::Invalid(_))),
            "budget {bad}: {res:?}"
        );
    }
    let res = set_savings_target(&db, chf(-1), today()).await;
    assert!(matches!(res, Err(PhoskError::Invalid(_))), "{res:?}");

    assert_eq!(db.budget_config().await.expect("config"), before);
    assert!(budget_changes(&db).await.expect("history").is_empty());
}

#[tokio::test]
async fn setting_the_current_value_is_a_no_op_without_history() {
    let db = seeded();
    set_monthly_budget(&db, chf(420_000), today())
        .await
        .expect("same budget ok");
    set_savings_target(&db, chf(90_000), today())
        .await
        .expect("same target ok");
    assert!(budget_changes(&db).await.expect("history").is_empty());
}

#[tokio::test]
async fn history_reads_oldest_to_newest() {
    let db = seeded();
    set_monthly_budget(&db, chf(450_000), today())
        .await
        .expect("first");
    set_savings_target(&db, chf(100_000), today())
        .await
        .expect("second");
    set_monthly_budget(&db, chf(430_000), today())
        .await
        .expect("third");
    let history = budget_changes(&db).await.expect("history");
    let steps: Vec<(&str, i64, i64)> = history
        .iter()
        .map(|c| {
            (
                c.field.as_str(),
                c.old_value.centimes(),
                c.new_value.centimes(),
            )
        })
        .collect();
    assert_eq!(
        steps,
        [
            ("monthly_budget", 420_000, 450_000),
            ("savings_target", 90_000, 100_000),
            ("monthly_budget", 450_000, 430_000),
        ]
    );
}
