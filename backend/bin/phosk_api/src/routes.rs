//! The REAL frontend-facing API surface — every endpoint the Phoskonomia
//! frontend needs, derived from the design prototype (window.PHOSK → real API).
//!
//! Handlers are HONEST STUBS: `501 Not Implemented` + a `todo` tag naming the
//! backend context that must fill it. NO mock data. `/health` is real.
//! Path params use axum 0.8 `{param}` syntax; references are human terms
//! (category NAME, shop NAME, stable slug ids) — never UUIDs (ADR-008).
//!
//! One module per domain under `routes/`; each exposes a single
//! `pub fn <domain>() -> Router` that `v1()` merges. Shared bits live here:
//! the `not_impl` stub helper and the `ni!` route macro.
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use phosk_core::error::PhoskError;
use serde_json::json;

/// A `501 Not Implemented` response tagged with the backend context (`todo`)
/// that must eventually fill the endpoint. Used directly by handlers with
/// multiple methods/closures; single-method stubs go through `ni!`.
pub(crate) fn not_impl(feature: &'static str) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({ "error": "not_implemented", "todo": feature })),
    )
        .into_response()
}

/// Edge error type: maps the one domain taxonomy ([`PhoskError`], ADR-010) to an
/// HTTP response — a status code + a small JSON body `{error, message}`. Handlers
/// return `Result<_, ApiError>` and use `?`; `PhoskError` converts in via `From`.
pub(crate) struct ApiError(PhoskError);

impl From<PhoskError> for ApiError {
    fn from(err: PhoskError) -> Self {
        Self(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.0.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        tracing::warn!(
            code = self.0.code(),
            status = status.as_u16(),
            "request failed"
        );
        (
            status,
            Json(json!({ "error": self.0.code(), "message": self.0.to_string() })),
        )
            .into_response()
    }
}

// ── Macros ──────────────────────────────────────────────────────────
#[macro_use]
mod macros;

// ── Domain modules (one file each) ──────────────────────────────────
mod ai;
mod alerts;
mod analytics;
mod categories_budget;
mod cycle;
mod dashboard;
mod debts;
mod exports;
mod personal_ious;
mod recurring;
mod settings_account;
mod shell;
mod shops;
mod signals;
mod subscriptions;
mod transactions;

pub fn api() -> Router {
    Router::new().nest("/api/v1", v1())
}

fn v1() -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async {
                (
                    StatusCode::OK,
                    Json(json!({ "status": "ok", "service": "phosk_api" })),
                )
                    .into_response()
            }),
        )
        .merge(cycle::cycle())
        .merge(dashboard::dashboard())
        .merge(transactions::transactions())
        .merge(categories_budget::categories_budget())
        .merge(shops::shops())
        .merge(alerts::alerts())
        .merge(recurring::recurring())
        .merge(subscriptions::subscriptions())
        .merge(debts::debts())
        .merge(personal_ious::personal_ious())
        .merge(signals::signals())
        .merge(analytics::analytics())
        .merge(ai::ai())
        .merge(settings_account::settings_account())
        .merge(shell::shell())
        .merge(exports::exports())
}
