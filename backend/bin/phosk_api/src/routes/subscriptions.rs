//! Subscriptions (full management page).
use axum::Router;
use axum::routing::get;

use super::not_impl;

pub fn subscriptions() -> Router {
    Router::new()
        .route(
            "/subscriptions",
            get(|| async {
                not_impl("recurring: list subscriptions (cadence/status/hist/next-due)")
            })
            .post(|| async { not_impl("recurring: create subscription") }),
        )
        .route(
            "/subscriptions/stats",
            ni!(
                get,
                "recurring: subscription roll-up (monthly/annual/next30/flagged)"
            ),
        )
        .route(
            "/subscriptions/billing-sweep",
            ni!(get, "recurring: billing-sweep timeline (impulse train)"),
        )
        .route(
            "/subscriptions/detect",
            ni!(
                post,
                "ai: detect recurring charges from transaction history"
            ),
        )
        .route(
            "/subscriptions/{id}",
            get(|| async { not_impl("recurring: subscription detail + AI guidance") })
                .patch(|| async { not_impl("recurring: edit subscription") }),
        )
        .route(
            "/subscriptions/{id}/pause",
            ni!(post, "recurring: pause subscription"),
        )
        .route(
            "/subscriptions/{id}/resume",
            ni!(post, "recurring: resume subscription"),
        )
        .route(
            "/subscriptions/{id}/cancel",
            ni!(post, "recurring: cancel subscription"),
        )
        .route(
            "/subscriptions/{id}/mark-paid",
            ni!(post, "recurring: mark subscription charge paid"),
        )
        .route(
            "/subscriptions/{id}/charges",
            get(|| async { not_impl("recurring: list subscription charges") })
                .post(|| async { not_impl("recurring: record a subscription charge/payment") }),
        )
        .route(
            "/subscriptions/{id}/confirm",
            ni!(post, "recurring: confirm AI-detected subscription"),
        )
        .route(
            "/subscriptions/{id}/dismiss",
            ni!(post, "recurring: dismiss AI subscription suggestion"),
        )
        .route(
            "/subscriptions/{id}/snooze",
            ni!(post, "recurring: snooze subscription alert"),
        )
}
