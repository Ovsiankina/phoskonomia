//! `data::dashboard`: totals, spend series, top shops, recurring, alerts,
//! insight, and the `act_on_alert` write path.

use phosk_planning::alerts as alert_svc;

use super::support::{assert_maps, fresh_db, json, money, server_error, today};
use crate::data::dashboard::{
    act_on_alert_with, get_alerts, get_insight, get_recurring, get_spend_series, get_top_shops,
    get_totals,
};

#[tokio::test]
async fn get_totals_reports_the_seeded_cycle_kpis() {
    let t = get_totals().await.expect("totals");
    assert_eq!(t.budget, money(420_000));
    assert_eq!(t.savings_target, money(90_000));
    assert_eq!(
        t.remaining.centimes(),
        t.budget.centimes() - t.spent.centimes()
    );
    assert!(t.spent > money(0));
    assert_eq!(json(&t)["budget"], 420_000, "money crosses as i64 centimes");

    let backend = phosk_insights::dashboard_totals(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&t, &backend);
}

#[tokio::test]
async fn get_spend_series_spans_the_thirty_day_cycle() {
    let s = get_spend_series().await.expect("series");
    assert_eq!(s.daily.len(), 30);
    assert_eq!(s.cumulative.len(), 30);
    assert_eq!(s.pace.len(), 30);
    assert_eq!(s.today_index, 17, "18 JUN is the 18th day");
    assert!(s.last_cycle_cumulative.is_some(), "the dashboard compares");
    let totals = get_totals().await.expect("totals");
    assert_eq!(s.cumulative[s.today_index], totals.spent);

    let backend = phosk_insights::spend_series(&fresh_db(), today(), true)
        .await
        .expect("backend");
    assert_maps(&s, &backend);
}

#[tokio::test]
async fn get_top_shops_ranks_at_most_eight_shops() {
    let s = get_top_shops().await.expect("top shops");
    assert!(!s.shops.is_empty() && s.shops.len() <= 8);
    assert!(s.shops.windows(2).all(|w| w[0].total >= w[1].total));
    assert_eq!(s.max_total, s.shops[0].total);

    let backend = phosk_insights::top_shops(&fresh_db(), today(), 8)
        .await
        .expect("backend");
    assert_maps(&s, &backend);
}

#[tokio::test]
async fn get_recurring_lists_the_subscriptions_soonest_first() {
    let r = get_recurring().await.expect("recurring");
    assert_eq!(r.recurring.len(), 6);
    assert!(r
        .recurring
        .windows(2)
        .all(|w| w[0].days_until <= w[1].days_until));
    // 19.90 + 15.95 + 9.99 + 89.00 + 17.00/12 + 42.00/12 (truncated centimes).
    assert_eq!(r.monthly_total, money(13_975));
    let netflix = r
        .recurring
        .iter()
        .find(|x| x.id == "netflix")
        .expect("netflix");
    assert_eq!(netflix.amount, money(1_990));

    let backend = phosk_recurring::subscriptions::recurring_summary(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&r, &backend);
}

#[tokio::test]
async fn get_alerts_serves_the_active_alert_log() {
    let a = get_alerts().await.expect("alerts");
    let ids: Vec<&str> = a.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(ids, ["a1", "a2", "a3"]);
    assert_eq!(a[0].tone, "alert");
    assert_eq!(a[0].actions, ["VIEW", "RAISE CAP", "DISMISS"]);

    let backend = alert_svc::alerts(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&a, &backend);
}

#[tokio::test]
async fn get_insight_carries_the_model_line_and_saving() {
    let i = get_insight().await.expect("insight");
    assert_eq!(i.model, "GEMMA4");
    assert!(!i.text.is_empty());
    assert_eq!(i.estimated_savings, money(4_200));

    let backend = phosk_ai::ai_features::dashboard_insight(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&i, &backend);
}

/// The ids of the active alerts in `db`.
async fn active_ids(db: &phosk_db_memory::MemoryDb) -> Vec<String> {
    let alerts = alert_svc::alerts(db, today()).await.expect("alerts");
    alerts.into_iter().map(|a| a.id).collect()
}

#[tokio::test]
async fn dismiss_and_snooze_take_an_alert_off_the_list() {
    let db = fresh_db();
    act_on_alert_with(&db, "a2", "dismiss")
        .await
        .expect("dismiss");
    assert_eq!(active_ids(&db).await, ["a1", "a3"]);
    act_on_alert_with(&db, "a3", "snooze")
        .await
        .expect("snooze");
    assert_eq!(active_ids(&db).await, ["a1"]);
}

#[tokio::test]
async fn apply_raises_the_targeted_cap_by_a_tenth() {
    let db = fresh_db();
    act_on_alert_with(&db, "a1", "apply").await.expect("apply");

    let cats = phosk_planning::budgets::categories(&db, today())
        .await
        .expect("categories");
    let going_out = cats.iter().find(|c| c.name == "Going out").expect("cap");
    // 400.00 + 10 % + 1 centime (the raise always strictly increases).
    assert_eq!(going_out.budget, money(44_001));
    assert_eq!(active_ids(&db).await.len(), 3, "apply leaves the alert");
}

#[tokio::test]
async fn act_on_alert_rejects_bad_input_without_side_effects() {
    let db = fresh_db();
    let msg = server_error(act_on_alert_with(&db, "no-such-alert", "dismiss").await);
    assert!(msg.starts_with("not found"), "{msg}");
    let msg = server_error(act_on_alert_with(&db, "a1", "explode").await);
    assert!(msg.starts_with("invalid input"), "{msg}");
    // a3 has no target envelope to raise.
    let msg = server_error(act_on_alert_with(&db, "a3", "apply").await);
    assert!(msg.starts_with("invalid input"), "{msg}");
    assert_eq!(active_ids(&db).await, ["a1", "a2", "a3"]);
}
