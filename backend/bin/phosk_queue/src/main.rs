//! `phosk_queue` — the Raspberry-Pi DMZ blob-queue server (ADR-007).
//!
//! The ONE legitimate inbound socket in Phoskonomia. Phones POST receipt photos
//! in; the trusted desktop daemon polls them out. The Pi treats every blob as
//! opaque and hostile (size cap + magic-byte sniff only); it runs no OCR/LLM.
//!
//! Run: `cargo run -p phosk_queue`. Override the bind with
//! `PHOSK_QUEUE_ADDR=0.0.0.0:8765` (the Pi binds its DMZ interface; the default
//! is loopback for safe local testing). Limits: `PHOSK_QUEUE_MAX_BLOB_BYTES`,
//! `PHOSK_QUEUE_MAX_LEN`, `PHOSK_QUEUE_MAX_WAIT_MS`.

use std::error::Error;
use std::net::SocketAddr;

use phosk_queue::{QueueConfig, QueueState, build_app};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();

    let config = config_from_env()?;
    let state = QueueState::new(config);
    let app = build_app(state);

    // Default to loopback so a stray `cargo run` never exposes a socket to the
    // LAN. On the real Pi the operator sets PHOSK_QUEUE_ADDR to the DMZ
    // interface. Avoid 3000–3002 (commonly taken) and the app ports (3717/3819).
    let addr: SocketAddr = std::env::var("PHOSK_QUEUE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8765".to_string())
        .parse()?;

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "phosk_queue listening — opaque FIFO, hostile-input gated");
    println!("phosk_queue listening on http://{addr} — opaque blob FIFO (ADR-007 DMZ)");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Read the queue limits from the environment, falling back to
/// [`QueueConfig::default`]. A malformed override is a hard startup error rather
/// than a silently-ignored value (fail loud at the composition root).
fn config_from_env() -> Result<QueueConfig, Box<dyn Error>> {
    let mut cfg = QueueConfig::default();
    if let Ok(v) = std::env::var("PHOSK_QUEUE_MAX_BLOB_BYTES") {
        cfg.max_blob_bytes = v.parse()?;
    }
    if let Ok(v) = std::env::var("PHOSK_QUEUE_MAX_LEN") {
        cfg.max_queue_len = v.parse()?;
    }
    if let Ok(v) = std::env::var("PHOSK_QUEUE_MAX_WAIT_MS") {
        cfg.max_wait_ms = v.parse()?;
    }
    Ok(cfg)
}

/// Install the tracing subscriber in **debug builds only**; in release the
/// events are compiled out (`release_max_level_off`) and this is a no-op.
#[cfg(debug_assertions)]
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();
}

/// Release no-op (tracing is compiled out).
#[cfg(not(debug_assertions))]
const fn init_tracing() {}
