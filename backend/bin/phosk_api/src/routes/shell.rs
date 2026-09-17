//! Shell / nav / ui config.
use axum::Router;
use axum::routing::get;

use super::not_impl;

pub fn shell() -> Router {
    Router::new()
        .route(
            "/nav/pages",
            ni!(
                get,
                "shell: navigation pages (key/abbr/glyph/href/available)"
            ),
        )
        .route(
            "/config",
            get(|| async { not_impl("settings: UI config subset (topDateFmt, ...)") })
                .patch(|| async { not_impl("settings: save UI config") }),
        )
}
