//! `phosk_api` — the Phoskonomia frontend-facing HTTP API, as a library.
//!
//! The binary (`main.rs`) is a thin composition root: it builds the in-memory
//! adapter, calls [`build_app`], binds the loopback socket, and serves. Exposing
//! the router here lets integration tests drive it in-process (no socket) with a
//! seeded `Extension`, asserting the dashboard contract without a real network
//! round-trip.
//!
//! Single-tenant, local-first: the server binds **loopback** (ADR-007). The
//! router is the real, complete frontend contract; handlers return honest `501`
//! until the backend behind them exists.
use std::sync::Arc;

use axum::{Extension, Router};
use phosk_adapter_db::DatabaseAdapter;
use tower_http::cors::CorsLayer;

pub mod routes;

/// Build the full API [`Router`], with the DB PORT dependency-injected as an
/// `Arc<dyn DatabaseAdapter>` `Extension` (ADR-010: the adapter is chosen by the
/// caller — the bin composition root or a test — never by feature code).
///
/// CORS is permissive for dev (the frontend is a separate origin, e.g.
/// `http://localhost:3001`); TODO lock to the frontend origin + add the local
/// auth secret (ADR-007).
pub fn build_app(db: Arc<dyn DatabaseAdapter>) -> Router {
    routes::api()
        .layer(Extension(db))
        .layer(CorsLayer::permissive())
}
