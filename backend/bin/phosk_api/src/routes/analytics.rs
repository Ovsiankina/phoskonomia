//! Analytics (retrospective / cross-cycle).
use axum::Router;

pub fn analytics() -> Router {
    Router::new()
        .route(
            "/analytics/spend-history",
            ni!(get, "insights: 12-cycle spend/savings history"),
        )
        .route(
            "/analytics/spend-history/stats",
            ni!(
                get,
                "insights: spend-history stats (avg/peak/low/vs-avg/vs-prev)"
            ),
        )
        .route(
            "/analytics/category-momentum",
            ni!(get, "insights: per-category momentum (now vs 3-cycle avg)"),
        )
        .route(
            "/analytics/rhythm/weekday",
            ni!(get, "insights: weekday spending rhythm (discretionary)"),
        )
        .route(
            "/analytics/insights/movers",
            ni!(get, "ai: movers narrative + suggested cap (GEMMA4)"),
        )
}
