//! Personal IOUs (informal, person-to-person).
use axum::Router;
use axum::routing::{get, patch};

use super::not_impl;

pub fn personal_ious() -> Router {
    Router::new()
        .route(
            "/personal-ious",
            get(|| async { not_impl("debts: list personal IOUs (dir in/out, repaid %)") })
                .post(|| async { not_impl("debts: create personal IOU") }),
        )
        .route(
            "/personal-ious/stats",
            ni!(get, "debts: IOU stats (owedToYou/youOwe/net)"),
        )
        .route(
            "/personal-ious/{id}",
            patch(|| async { not_impl("debts: edit personal IOU") })
                .delete(|| async { not_impl("debts: delete personal IOU") }),
        )
        .route(
            "/personal-ious/{id}/payments",
            ni!(post, "debts: record an IOU payment"),
        )
        .route(
            "/personal-ious/{id}/settle",
            ni!(post, "debts: mark IOU settled"),
        )
        .route(
            "/personal-ious/{id}/settle-up",
            ni!(post, "debts: settle up an IOU"),
        )
        .route(
            "/personal-ious/{id}/remind",
            ni!(post, "debts: send an IOU reminder"),
        )
}
