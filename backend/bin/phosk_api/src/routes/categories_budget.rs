//! Categories / budget envelopes.
use axum::Router;
use axum::routing::get;

use super::not_impl;

pub fn categories_budget() -> Router {
    Router::new()
        .route(
            "/categories",
            get(|| async {
                not_impl("ledger: list categories with budget rollup (spent/cap/used/proj/spark)")
            })
            .post(|| async { not_impl("planning: create category / budget envelope") }),
        )
        .route(
            "/categories/{name}",
            get(|| async { not_impl("ledger: category detail") })
                .patch(|| async { not_impl("planning: set / raise / lower category cap") }),
        )
        .route(
            "/categories/{name}/transactions",
            ni!(get, "ledger: transactions within a category"),
        )
        .route(
            "/budget/totals",
            ni!(
                get,
                "planning: budget totals (budget/allocated/spent/projected/remaining)"
            ),
        )
        .route(
            "/budget/allocation",
            ni!(get, "planning: allocation breakdown + GEMMA4 trim advice"),
        )
}
