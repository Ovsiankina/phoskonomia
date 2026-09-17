//! Alerts.
use axum::Router;

pub fn alerts() -> Router {
    Router::new()
        .route(
            "/alerts",
            ni!(
                get,
                "planning: list alerts (tone/tag/head/body/actions/source)"
            ),
        )
        .route(
            "/alerts/{id}/apply",
            ni!(
                post,
                "planning: apply an alert suggestion (e.g. cap change + move to savings)"
            ),
        )
        .route(
            "/alerts/{id}/dismiss",
            ni!(post, "planning: dismiss an alert"),
        )
        .route(
            "/alerts/{id}/snooze",
            ni!(post, "planning: snooze an alert"),
        )
        .route(
            "/alerts/{id}/target",
            ni!(get, "planning: resolve an alert's deep-link filter target"),
        )
}
