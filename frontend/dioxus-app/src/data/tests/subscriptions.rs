//! `data::subscriptions`: standing charges, stats, billing sweep and detail.

use phosk_recurring::subscriptions as svc;

use super::support::{assert_maps, assert_maps_except, fresh_db, money, server_error, today};
use crate::data::subscriptions::{
    get_billing_sweep, get_subscription, get_subscription_stats, list_subscriptions, SubFilter,
    SubscriptionDto,
};

/// The wire list sorted by `sort`, checked against the backend on a fresh store.
async fn listed(sort: &str) -> Vec<SubscriptionDto> {
    let wire = list_subscriptions(SubFilter {
        sort: sort.into(),
        ..SubFilter::default()
    })
    .await
    .expect("list");
    let svc_filter = svc::SubFilter {
        sort: sort.into(),
        ..svc::SubFilter::default()
    };
    let backend = svc::list_subscriptions(&fresh_db(), today(), svc_filter)
        .await
        .expect("backend");
    assert_maps(&wire, &backend);
    wire
}

#[tokio::test]
async fn list_subscriptions_serves_the_six_charges() {
    let subs = listed("").await;
    let mut ids: Vec<&str> = subs.iter().map(|s| s.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        ["domain", "gym", "icloud", "netflix", "nyt", "spotify"]
    );

    let nyt = subs.iter().find(|s| s.id == "nyt").expect("nyt");
    assert_eq!(nyt.cadence, "yearly");
    assert_eq!(nyt.amount, money(1_700));
    assert_eq!(nyt.monthly_equiv, money(141), "17.00 / 12, truncated");
}

#[tokio::test]
async fn list_subscriptions_honours_the_sort_key() {
    let by_amount = listed("amount").await;
    assert_eq!(by_amount[0].id, "gym");
    assert!(by_amount
        .windows(2)
        .all(|w| w[0].monthly_equiv >= w[1].monthly_equiv));

    let by_name = listed("name").await;
    assert!(by_name.windows(2).all(|w| w[0].name <= w[1].name));

    let by_due = listed("due").await;
    assert!(by_due
        .windows(2)
        .all(|w| w[0].days_until <= w[1].days_until));

    assert_eq!(
        listed("bogus").await,
        listed("").await,
        "unknown sort keeps order"
    );
}

#[tokio::test]
async fn get_subscription_stats_rolls_up_the_charges() {
    let s = get_subscription_stats().await.expect("stats");
    assert_eq!(s.count, 6);
    assert_eq!(s.monthly, money(13_975));

    let backend = svc::subscription_stats(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&s, &backend);
}

#[tokio::test]
async fn get_billing_sweep_frames_the_cycle() {
    let b = get_billing_sweep().await.expect("sweep");
    assert_eq!((b.cycle.day, b.cycle.days), (18, 30));
    assert!(!b.impulses.is_empty());

    let backend = svc::billing_sweep(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&b, &backend);
}

#[tokio::test]
async fn get_subscription_inspects_one_charge() {
    let d = get_subscription("netflix".into()).await.expect("detail");
    assert_eq!(d.subscription.id, "netflix");
    assert_eq!(d.subscription.name, "Netflix");
    assert_eq!(d.subscription.amount, money(1_990));
    assert_eq!(d.recent.len(), 3);
    assert!(!d.guidance.text.is_empty());

    let backend = svc::subscription_detail(&fresh_db(), today(), "netflix")
        .await
        .expect("backend");
    // Charge ids are minted per store, so two stores never share them.
    assert_maps_except(&d, &backend, "id");
}

#[tokio::test]
async fn get_subscription_rejects_an_unknown_slug() {
    let msg = server_error(get_subscription("no-such-sub".into()).await, 404);
    assert!(msg.starts_with("not found"), "{msg}");
}
