//! Item-signals (AI-maintained micro-categories).
use axum::Router;
use axum::routing::get;

use super::not_impl;

pub fn signals() -> Router {
    Router::new()
        .route(
            "/signals",
            get(|| async {
                not_impl("ledger/signals: list tracked item-signals (+ include candidates)")
            })
            .post(|| async {
                not_impl("ledger/signals: track a new signal (from candidate or name)")
            }),
        )
        .route(
            "/signals/candidates",
            ni!(get, "ai: list signal candidates the AI proposes to track"),
        )
        .route(
            "/signals/movers",
            ni!(get, "insights: signal movers (top riser/faller)"),
        )
        .route(
            "/signals/{id}",
            get(|| async { not_impl("ledger/signals: signal detail (series, recent lines)") })
                .delete(|| async { not_impl("ledger/signals: untrack / pause a signal") }),
        )
        .route(
            "/signals/{id}/cap",
            ni!(
                post,
                "planning: set a soft cap on a signal (nudge threshold)"
            ),
        )
        .route(
            "/signals/candidates/{id}/track",
            ni!(post, "ai: track a candidate signal (approve suggestion)"),
        )
        .route(
            "/signals/candidates/{id}/dismiss",
            ni!(post, "ai: dismiss a candidate signal"),
        )
}
