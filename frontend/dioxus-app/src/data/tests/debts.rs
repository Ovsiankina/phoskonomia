//! `data::debts`: open balances, stats, trajectory, detail, payments and IOUs.

use phosk_debts::{debts as svc, personal_ious};

use super::support::{assert_maps, assert_maps_except, fresh_db, money, server_error, today};
use crate::data::debts::{
    get_debt, get_debt_payments, get_debt_stats, get_iou_stats, get_trajectory, list_debts,
    list_personal_ious,
};

#[tokio::test]
async fn list_debts_serves_the_four_balances() {
    let debts = list_debts().await.expect("debts");
    let mut ids: Vec<&str> = debts.iter().map(|d| d.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["card", "loan", "tax", "vw"]);
    let card = debts.iter().find(|d| d.id == "card").expect("card");
    assert_eq!(card.balance, money(340_000));
    assert!((card.apr - 0.129).abs() < f64::EPSILON);

    assert!(
        debts.iter().all(|d| d.actions.pay),
        "every seeded debt is owed"
    );

    let backend = svc::list_debts(&fresh_db(), today())
        .await
        .expect("backend");
    // `actions` is wire-only: the server's verdict on what the page may offer.
    assert_maps_except(&debts, &backend, "actions");
}

#[tokio::test]
async fn get_debt_stats_rolls_up_the_balances() {
    let s = get_debt_stats().await.expect("stats");
    assert_eq!(s.count, 4);
    // 18'200 + 3'400 + 9'800 + 2'100
    assert_eq!(s.total_owed, money(3_350_000));
    // 450 + 150 + 320 + 350
    assert_eq!(s.total_monthly, money(127_000));

    let backend = svc::debt_stats(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&s, &backend);
}

#[tokio::test]
async fn get_trajectory_forwards_the_strategy_hint() {
    let backend = svc::trajectory(&fresh_db(), today(), "avalanche")
        .await
        .expect("backend");
    let avalanche = get_trajectory("avalanche".into())
        .await
        .expect("trajectory");
    assert!(!avalanche.points.is_empty());
    assert!(!avalanche.debt_free_label.is_empty());
    assert_maps(&avalanche, &backend);

    // The hint only orders payoffs; the combined curve is the same for all,
    // and an unknown hint is not an error.
    for strategy in ["snowball", "none", "bogus", ""] {
        let t = get_trajectory(strategy.into()).await.expect("trajectory");
        assert_eq!(t, avalanche, "strategy {strategy:?}");
    }
}

#[tokio::test]
async fn get_debt_and_payments_inspect_one_balance() {
    let d = get_debt("vw".into()).await.expect("detail");
    assert!(!d.decay_series.hist.is_empty());
    assert!(!d.guidance.is_empty());
    let backend = svc::debt_detail(&fresh_db(), "vw").await.expect("backend");
    assert_maps(&d, &backend);

    let p = get_debt_payments("vw".into()).await.expect("payments");
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].amount, money(45_000));
    assert!(!p[0].id.is_empty());
    let backend = svc::debt_payments(&fresh_db(), "vw")
        .await
        .expect("backend");
    // Payment ids are minted per store, so two stores never share them.
    assert_maps_except(&p, &backend, "id");
}

#[tokio::test]
async fn unknown_debts_are_not_found() {
    let msg = server_error(get_debt("no-such-debt".into()).await);
    assert!(msg.starts_with("not found"), "{msg}");
    let msg = server_error(get_debt_payments("no-such-debt".into()).await);
    assert!(msg.starts_with("not found"), "{msg}");
}

#[tokio::test]
async fn list_personal_ious_serves_both_directions() {
    let ious = list_personal_ious().await.expect("ious");
    let mut ids: Vec<&str> = ious.iter().map(|i| i.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["i1", "i2", "i3", "i4"]);
    let marco = ious.iter().find(|i| i.id == "i2").expect("i2");
    assert_eq!((marco.dir.as_str(), marco.person.as_str()), ("in", "Marco"));
    assert_eq!((marco.amount, marco.of), (money(4_500), money(9_000)));

    assert!(ious.iter().all(|i| i.actions.pay && i.actions.settle));

    let backend = personal_ious::list_personal_ious(&fresh_db())
        .await
        .expect("backend");
    assert_maps_except(&ious, &backend, "actions");
}

#[tokio::test]
async fn get_iou_stats_nets_the_positions() {
    let s = get_iou_stats().await.expect("stats");
    assert_eq!(s.owed_to_you, money(16_500), "120 + 45");
    assert_eq!(s.you_owe, money(26_000), "60 + 200");
    assert_eq!(s.net, money(-9_500));
    assert_eq!((s.count_in, s.count_out), (2, 2));

    let backend = personal_ious::iou_stats(&fresh_db())
        .await
        .expect("backend");
    assert_maps(&s, &backend);
}
