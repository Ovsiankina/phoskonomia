//! Debts (institutional).
use axum::Router;
use axum::routing::get;

use super::not_impl;

pub fn debts() -> Router {
    Router::new()
        .route(
            "/debts",
            get(|| async {
                not_impl("debts: list institutional debts (+ derived payoff/interest)")
            })
            .post(|| async { not_impl("debts: create debt") }),
        )
        .route(
            "/debts/stats",
            ni!(
                get,
                "debts: portfolio stats (totalOwed/weightedApr/debt-free horizon/targets)"
            ),
        )
        .route(
            "/debts/trajectory",
            ni!(
                get,
                "debts: combined payoff trajectory (history + projection)"
            ),
        )
        .route(
            "/debts/strategy",
            ni!(put, "debts: set payoff strategy (avalanche/snowball/none)"),
        )
        .route(
            "/debts/{id}",
            get(|| async { not_impl("debts: debt detail (decay series, payoff guidance)") })
                .patch(|| async { not_impl("debts: edit debt") })
                .delete(|| async { not_impl("debts: delete debt") }),
        )
        .route(
            "/debts/{id}/payments",
            get(|| async { not_impl("debts: payment history") })
                .post(|| async { not_impl("debts: record an extra payment") }),
        )
        .route(
            "/debts/{id}/plan",
            ni!(patch, "debts: adjust payment plan (monthly/day/term)"),
        )
        .route(
            "/debts/{id}/refinance",
            ni!(post, "debts: refinance (new apr/lender)"),
        )
}
