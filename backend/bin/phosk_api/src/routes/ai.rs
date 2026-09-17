//! AI assistant / feed / chat.
use axum::Router;
use axum::routing::get;

use super::not_impl;

pub fn ai() -> Router {
    Router::new()
        .route(
            "/ai/feed",
            ni!(
                get,
                "ai: activity feed (categorize/reprocess/suggest/detect)"
            ),
        )
        .route(
            "/ai/feed/{id}/dismiss",
            ni!(post, "ai: dismiss a feed item"),
        )
        .route(
            "/ai/chat",
            get(|| async { not_impl("ai: chat message history") }).post(|| async {
                not_impl("ai: send chat message (local GEMMA4, may emit track/cap intents)")
            }),
        )
        .route(
            "/ai/status",
            ni!(
                get,
                "ai: assistant status (model/engine/online/watched signals)"
            ),
        )
        .route(
            "/ai/reprocess",
            ni!(post, "ai: reprocess low-confidence items (global scope)"),
        )
}
