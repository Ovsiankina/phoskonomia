//! Settings / preferences / account.
use axum::Router;
use axum::routing::get;

use super::not_impl;

pub fn settings_account() -> Router {
    Router::new()
        .route(
            "/settings/preferences",
            get(|| async { not_impl("settings: get UI preferences") })
                .patch(|| async { not_impl("settings: update preferences") })
                .delete(|| async { not_impl("settings: reset preferences") }),
        )
        .route(
            "/settings/preferences/defaults",
            ni!(get, "settings: preference defaults"),
        )
        .route(
            "/settings/summary",
            ni!(
                get,
                "settings: config summary (counts/engine/model/storedOnDevice)"
            ),
        )
        .route(
            "/account",
            get(|| async { not_impl("settings: account profile (holder/iban/model/engine)") })
                .patch(|| async { not_impl("settings: edit account (holder/iban)") }),
        )
        .route(
            "/account/ai/engines",
            ni!(get, "settings: list available local AI engines/models"),
        )
        .route(
            "/account/ai/engine",
            ni!(put, "settings: select AI engine/model"),
        )
}
