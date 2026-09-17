//! Exports (CSV).
use axum::Router;

pub fn exports() -> Router {
    Router::new()
        .route(
            "/exports/transactions.csv",
            ni!(get, "export: transactions CSV (mirrors list filters)"),
        )
        .route(
            "/exports/budget.csv",
            ni!(get, "export: budget CSV (envelopes/alerts/recurring)"),
        )
        .route(
            "/exports/subscriptions.csv",
            ni!(get, "export: subscriptions CSV"),
        )
}
