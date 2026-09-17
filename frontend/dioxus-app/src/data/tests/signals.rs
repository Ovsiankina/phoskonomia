//! `data::signals`: tracked item-signals, candidates, movers, detail, and the
//! track / dismiss write paths.

use phosk_ledger::signals as svc;

use super::support::{assert_maps, fresh_db, json, money, server_error, today};
use crate::data::signals::{
    dismiss_signal_with, get_movers, get_signal, get_signal_candidates, get_signals,
    track_signal_with,
};

#[tokio::test]
async fn get_signals_lists_the_four_tracked_signals() {
    let sigs = get_signals().await.expect("signals");
    let mut ids: Vec<&str> = sigs.iter().map(|s| s.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["beer", "coffee", "gruyere", "pain"]);
    assert!(sigs.iter().all(|s| !s.candidate));

    let coffee = sigs.iter().find(|s| s.id == "coffee").expect("coffee");
    assert_eq!(coffee.label, "Oat-milk flat white");
    assert_eq!(coffee.parent, "Coffee & snacks");
    assert_eq!(coffee.unit, "cups");
    assert_eq!(coffee.since, "MAR 2026");
    assert_eq!(coffee.delta_pct, 28);
    assert!((coffee.cycle_qty - 16.0).abs() < f64::EPSILON);
    assert_eq!(coffee.cycle_spend, money(8_960));
    assert_eq!(
        json(coffee)["cycleSpend"],
        8_960,
        "money crosses as i64 centimes"
    );

    let backend = svc::list_signals(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&sigs, &backend);
}

#[tokio::test]
async fn get_signal_candidates_offers_the_untracked_llm_proposal() {
    let cands = get_signal_candidates().await.expect("candidates");
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].id, "energy-drink");
    assert!(cands[0].candidate);
    assert_eq!(cands[0].desc, "Showing up 3× this cycle — track it?");

    let backend = svc::signal_candidates(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&cands, &backend);
}

#[tokio::test]
async fn get_movers_ranks_the_fastest_riser_and_faller() {
    let m = get_movers().await.expect("movers");
    assert_eq!(m.riser.id, "coffee");
    assert_eq!(m.riser.delta_pct, 28);
    assert_eq!(m.faller.id, "beer");
    assert_eq!(m.faller.delta_pct, -22);
    assert_eq!(m.all.len(), 4);

    let backend = svc::movers(&fresh_db(), today()).await.expect("backend");
    assert_maps(&m, &backend);
}

#[tokio::test]
async fn get_signal_returns_the_inspector_payload() {
    let d = get_signal("coffee".into()).await.expect("detail");
    assert_eq!(d.signal.id, "coffee");
    assert_eq!(d.recent[0].shop, "Migros");
    assert_eq!(d.recent[0].amount, money(8_960));
    assert!(!d.guidance.is_empty());
    // `signal` is flattened: its fields sit beside the detail extras.
    assert_eq!(json(&d)["label"], "Oat-milk flat white");

    let backend = svc::signal_detail(&fresh_db(), today(), "coffee")
        .await
        .expect("backend");
    assert_maps(&d, &backend);
}

#[tokio::test]
async fn get_signal_rejects_an_unknown_slug() {
    let msg = server_error(get_signal("no-such-signal".into()).await);
    assert!(msg.starts_with("not found"), "{msg}");
}

#[tokio::test]
async fn track_promotes_a_candidate_to_tracked() {
    let db = fresh_db();
    track_signal_with(&db, "energy-drink").await.expect("track");

    let tracked = svc::list_signals(&db, today()).await.expect("signals");
    let energy = tracked.iter().find(|s| s.id == "energy-drink");
    assert!(energy.is_some_and(|s| !s.candidate), "now tracked");
    let cands = svc::signal_candidates(&db, today())
        .await
        .expect("candidates");
    assert!(cands.is_empty());
}

#[tokio::test]
async fn dismiss_removes_the_candidate() {
    let db = fresh_db();
    dismiss_signal_with(&db, "energy-drink")
        .await
        .expect("dismiss");

    let cands = svc::signal_candidates(&db, today())
        .await
        .expect("candidates");
    assert!(cands.is_empty());
    let tracked = svc::list_signals(&db, today()).await.expect("signals");
    assert_eq!(tracked.len(), 4, "tracked signals are untouched");
    assert!(svc::signal_detail(&db, today(), "energy-drink")
        .await
        .is_err());
}

#[tokio::test]
async fn track_and_dismiss_reject_an_unknown_slug() {
    let db = fresh_db();
    let msg = server_error(track_signal_with(&db, "no-such-signal").await);
    assert!(msg.starts_with("not found"), "{msg}");
    let msg = server_error(dismiss_signal_with(&db, "no-such-signal").await);
    assert!(msg.starts_with("not found"), "{msg}");
    assert_eq!(
        svc::list_signals(&db, today())
            .await
            .expect("signals")
            .len(),
        4
    );
}
