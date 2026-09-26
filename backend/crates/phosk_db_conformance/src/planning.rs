//! Planning: category caps, budget history, spend history and alerts.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_id::{AlertId, BudgetChangeId};
use phosk_model::{
    Alert, AlertSnooze, BudgetChange, BudgetConfig, BudgetHistory, CategoryCap, Provenance, Source,
};

use crate::support::{Outcome, date, ensure, ensure_eq, ensure_not_found, sorted};

/// The caps list and the by-name lookup return the same records.
pub async fn category_caps_lookup_by_name_agrees(db: &dyn DatabaseAdapter) -> Outcome {
    let caps = db.category_caps().await?;
    ensure_eq(&caps.len(), &8, "category cap count")?;
    for cap in &caps {
        let by_name = db.category_cap_by_name(&cap.name).await?;
        ensure_eq(&by_name, cap, "category_cap_by_name")?;
    }
    let rent = db.category_cap_by_name("Rent").await?;
    ensure(rent.fixed, "Rent is a fixed envelope")?;
    ensure_eq(&rent.cap.map(Money::centimes), &Some(168_000), "Rent cap")?;
    let unknown = db.category_cap_by_name("conf-none").await;
    ensure_not_found(unknown, "category_cap_by_name(unknown)")
}

/// `set_category_cap` sets and clears the cap, stamps `UserModified`, and
/// leaves every other field and every other cap alone.
pub async fn set_category_cap_sets_clears_and_stamps_provenance(
    db: &dyn DatabaseAdapter,
) -> Outcome {
    let caps = db.category_caps().await?;
    let target = caps.iter().find(|c| c.name == "Transport");
    let target = target.ok_or("Transport cap missing")?;

    let new_cap = Some(Money::from_centimes(20_000));
    db.set_category_cap("Transport", new_cap).await?;
    let got = db.category_cap_by_name("Transport").await?;
    ensure_eq(&got.provenance.source, &Source::UserModified, "provenance")?;
    let want = CategoryCap {
        cap: new_cap,
        provenance: got.provenance,
        ..target.clone()
    };
    ensure_eq(&got, &want, "only cap and provenance changed")?;

    db.set_category_cap("Transport", None).await?;
    let cleared = db.category_cap_by_name("Transport").await?;
    ensure_eq(&cleared.cap, &None, "cleared cap")?;

    for other in caps.iter().filter(|c| c.name != "Transport") {
        let now = db.category_cap_by_name(&other.name).await?;
        ensure_eq(&now, other, "other caps untouched")?;
    }
    let unknown = db.set_category_cap("conf-none", None).await;
    ensure_not_found(unknown, "set_category_cap(unknown)")
}

/// Each category's history comes back oldest→newest, as the port documents
/// (the trailing-average and spark consumers rely on it); a category that
/// does not exist has an empty history.
pub async fn budget_history_is_oldest_to_newest(db: &dyn DatabaseAdapter) -> Outcome {
    for cap in db.category_caps().await? {
        let hist = db.budget_history(&cap.name).await?;
        let ordered = hist.windows(2).all(|w| w[0].cycle_start < w[1].cycle_start);
        ensure(
            ordered,
            &format!("{} history is not oldest→newest", cap.name),
        )?;
    }
    let groceries = db.budget_history("Groceries").await?;
    let spent: Vec<i64> = groceries.iter().map(|h| h.spent.centimes()).collect();
    let want = vec![72_000, 69_000, 81_000, 74_000, 76_000, 88_000];
    ensure_eq(&spent, &want, "Groceries spends in cycle order")?;
    let oldest = groceries.first().map(|h| h.cycle_start);
    ensure_eq(&oldest, &Some(date(2025, 12, 1)?), "oldest Groceries cycle")?;
    let unknown = db.budget_history("conf-none").await?;
    ensure(unknown.is_empty(), "unknown category has no history")
}

/// The spend-history fast path returns the recorded cycles: for the six
/// seeded cycles it is exactly the union of the per-category histories.
pub async fn spend_history_returns_the_recorded_cycles(db: &dyn DatabaseAdapter) -> Outcome {
    let key = |h: &BudgetHistory| format!("{}|{}", h.category_id, h.cycle_start);
    let mut union = Vec::new();
    for cap in db.category_caps().await? {
        union.extend(db.budget_history(&cap.name).await?);
    }
    let spend = db.spend_history(6, date(2026, 6, 19)?).await?;
    ensure_eq(&spend.len(), &48, "eight categories × six cycles")?;
    let (spend, union) = (sorted(spend, key), sorted(union, key));
    ensure_eq(
        &spend,
        &union,
        "spend_history equals the per-category histories",
    )
}

/// `set_budget_config` replaces the config and appends its history entry;
/// the history reads back in append order even for same-day entries, and
/// re-sending a change with the same id replaces it rather than duplicating.
pub async fn set_budget_config_replaces_it_and_appends_history(
    db: &dyn DatabaseAdapter,
) -> Outcome {
    ensure(db.budget_changes().await?.is_empty(), "seed has no changes")?;
    let before = db.budget_config().await?;
    let day = date(2026, 6, 18)?;
    let change = |field: &str, old: i64, new: i64| BudgetChange {
        id: BudgetChangeId::new(),
        field: field.to_owned(),
        old_value: Money::from_centimes(old),
        new_value: Money::from_centimes(new),
        at: day,
        provenance: Provenance::user_modified(),
    };
    let raised = BudgetConfig {
        monthly_budget: Money::from_centimes(450_000),
        ..before.clone()
    };
    let first = change("monthly_budget", 420_000, 450_000);
    db.set_budget_config(raised.clone(), first.clone()).await?;
    ensure_eq(&db.budget_config().await?, &raised, "config replaced")?;

    let saved = BudgetConfig {
        savings_target: Money::from_centimes(100_000),
        ..raised
    };
    let second = change("savings_target", 90_000, 100_000);
    db.set_budget_config(saved.clone(), second.clone()).await?;
    ensure_eq(&db.budget_config().await?, &saved, "second config")?;
    let want = vec![first.clone(), second.clone()];
    ensure_eq(
        &db.budget_changes().await?,
        &want,
        "history in append order",
    )?;

    // Re-sending the older entry replaces it where it stands: no duplicate,
    // and it does not move behind the newer one.
    let resent = BudgetChange {
        new_value: Money::from_centimes(460_000),
        ..first
    };
    db.set_budget_config(saved, resent.clone()).await?;
    ensure_eq(
        &db.budget_changes().await?,
        &vec![resent, second],
        "same id replaces in place",
    )
}

/// `snooze_alert` sets the status and the term, touching nothing else.
pub async fn snooze_alert_sets_status_and_term(db: &dyn DatabaseAdapter) -> Outcome {
    let alerts = db.alerts().await?;
    let (target, rest) = alerts.split_first().ok_or("no alerts")?;
    ensure_eq(&target.snooze, &None, "seeded alerts are not snoozed")?;
    let snooze = AlertSnooze {
        until: date(2026, 7, 1)?,
        level: 1,
    };
    db.snooze_alert(target.id, snooze).await?;
    let want = Alert {
        status: "snoozed".to_owned(),
        snooze: Some(snooze),
        ..target.clone()
    };
    ensure_eq(&db.alert(target.id).await?, &want, "snoozed alert")?;
    for other in rest {
        ensure_eq(&db.alert(other.id).await?, other, "other alerts untouched")?;
    }
    let unknown = db.snooze_alert(AlertId::new(), snooze).await;
    ensure_not_found(unknown, "snooze_alert(unknown)")
}

/// Alerts: list, id lookup, and a status update that touches only its target.
pub async fn alerts_lookup_and_status_update(db: &dyn DatabaseAdapter) -> Outcome {
    let alerts = db.alerts().await?;
    ensure_eq(&alerts.len(), &3, "alert count")?;
    let active = alerts.iter().all(|a| a.status == "active");
    ensure(active, "seeded alerts are active")?;
    for a in &alerts {
        ensure_eq(&db.alert(a.id).await?, a, "alert(id)")?;
    }

    let (target, rest) = alerts.split_first().ok_or("no alerts")?;
    db.update_alert_status(target.id, "snoozed").await?;
    let want = Alert {
        status: "snoozed".to_owned(),
        ..target.clone()
    };
    ensure_eq(&db.alert(target.id).await?, &want, "updated alert")?;
    for other in rest {
        ensure_eq(&db.alert(other.id).await?, other, "other alerts untouched")?;
    }

    ensure_not_found(db.alert(AlertId::new()).await, "alert(unknown)")?;
    let unknown = db.update_alert_status(AlertId::new(), "dismissed").await;
    ensure_not_found(unknown, "update_alert_status(unknown)")
}
