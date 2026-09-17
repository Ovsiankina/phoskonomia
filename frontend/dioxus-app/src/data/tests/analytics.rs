//! `data::analytics`: spend history, stats, momentum, rhythm and insight.

use phosk_insights::analytics as svc;

use super::support::{assert_maps, fresh_db, today};
use crate::data::analytics::{
    get_analytics_insight, get_category_momentum, get_rhythm, get_spend_history, get_spend_stats,
};

#[tokio::test]
async fn get_spend_history_draws_twelve_cycles() {
    let h = get_spend_history().await.expect("history");
    assert_eq!(h.points.len(), 12);
    assert_eq!(h.points.last().map(|p| p.m.as_str()), Some("JUN"));

    let backend = svc::spend_history(&fresh_db(), today(), 12)
        .await
        .expect("backend");
    assert_maps(&h, &backend);
}

#[tokio::test]
async fn get_spend_stats_rolls_up_the_same_window() {
    let s = get_spend_stats().await.expect("stats");
    assert_eq!(s.cur.m, "JUN");
    assert_eq!(s.prev.m, "MAY");
    assert!(s.peak.spend >= s.low.spend);

    let backend = svc::spend_stats(&fresh_db(), today(), 12)
        .await
        .expect("backend");
    assert_maps(&s, &backend);
}

#[tokio::test]
async fn get_category_momentum_has_a_card_per_category() {
    let cards = get_category_momentum().await.expect("momentum");
    assert!(!cards.is_empty());
    assert!(cards.iter().all(|c| !c.name.is_empty()));

    let backend = svc::category_momentum(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&cards, &backend);
}

#[tokio::test]
async fn get_rhythm_covers_the_week() {
    let r = get_rhythm().await.expect("rhythm");
    assert_eq!(r.weekday.len(), 7);
    assert!(r.weekday.iter().any(|w| w.d == r.stats.peak.d));

    let backend = svc::weekday_rhythm(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&r, &backend);
}

#[tokio::test]
async fn get_analytics_insight_suggests_a_cap() {
    let i = get_analytics_insight().await.expect("insight");
    assert!(!i.model.is_empty() && !i.text.is_empty());
    assert!(!i.suggested_cap.signal_id.is_empty());

    let backend = svc::analytics_insight(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&i, &backend);
}
