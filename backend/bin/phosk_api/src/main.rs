//! `phosk_api` — the Phoskonomia frontend-facing HTTP API server.
//!
//! Single-tenant, local-first: binds **loopback** (ADR-007). The router is the
//! real, complete frontend contract; handlers return honest `501` until the
//! backend behind them exists. Run: `cargo run -p phosk_api`
//! (override the address with `PHOSK_API_ADDR=127.0.0.1:3000`).
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use phosk_adapter_db::DatabaseAdapter;
use phosk_api::build_app;
use phosk_db_memory::MemoryDb;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();

    // Dependency injection of the DB PORT (ADR-010): the concrete adapter is
    // wired in ONLY here, the composition root, and shared with the handlers as
    // an `Arc<dyn DatabaseAdapter>` via an axum `Extension` layer (in `build_app`).
    // Today that is the deterministic in-memory seed (`MemoryDb`); swapping to
    // SurrealDB is a one-line change here and a new adapter impl, never a handler
    // edit.
    let db: Arc<dyn DatabaseAdapter> = Arc::new(MemoryDb::seeded()?);
    let app = build_app(db);

    // Fixed loopback port 3819 (ADR-007). 3000–3002 are commonly occupied on
    // dev machines, so we stay clear of them AND of the Vite dev server (3717).
    // Must match the frontend client default (api.js `BASE`). Override with
    // PHOSK_API_ADDR if needed.
    let addr: SocketAddr = std::env::var("PHOSK_API_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3819".to_string())
        .parse()?;

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "phosk_api listening (loopback) — CORS permissive (dev)");
    println!("phosk_api listening on http://{addr} (loopback) — CORS permissive (dev)");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Install the tracing subscriber — **debug builds only**. In release the
/// `tracing` events are already compiled out (`release_max_level_off`) and this
/// is a no-op, so production carries zero tracing footprint.
///
/// Override verbosity with `RUST_LOG` (e.g. `RUST_LOG=phosk_core=trace`).
#[cfg(debug_assertions)]
fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("phosk_api=debug,phosk_core=trace,info"));
    fmt().with_env_filter(filter).with_target(true).init();
}

#[cfg(not(debug_assertions))]
fn init_tracing() {}
