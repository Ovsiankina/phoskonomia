//! Recurring (dashboard/budget summary view of standing charges).
use axum::Router;

pub fn recurring() -> Router {
    Router::new()
        .route(
            "/recurring",
            ni!(
                get,
                "recurring: list standing charges (dashboard summary, monthlyTotal)"
            ),
        )
        .route(
            "/recurring/{name}/mark-paid",
            ni!(post, "recurring: mark a standing charge paid"),
        )
        .route(
            "/recurring/{name}/confirm",
            ni!(post, "recurring: confirm an AI-detected recurring charge"),
        )
}
