//! `phosk_daemon` — the trusted desktop poller (ADR-007).
//!
//! Polls the Pi `phosk_queue` OUTBOUND, drains opaque blobs, and hands each to
//! the receipt-import pipeline via the [`phosk_daemon::ReceiptIngest`] seam. The
//! desktop never opens an inbound socket.
//!
//! Run: `cargo run -p phosk_daemon`. Point it at the Pi with
//! `PHOSK_QUEUE_URL=http://pi.local:8765` (default `http://127.0.0.1:8765` for
//! local testing against a co-hosted `phosk_queue`). Tune pacing with
//! `PHOSK_DAEMON_LONG_POLL_MS` and `PHOSK_DAEMON_BACKOFF_MS`.
//!
//! `phosk_pipeline_receipt` exists (`intake_receipt`) but is not yet composed
//! in behind a `ReceiptIngest` impl, so this binary still wires the
//! deterministic [`phosk_daemon::NullIngest`] seam: the poll/drain loop runs and
//! logs end to end, but drained blobs are not imported yet.

use std::error::Error;
use std::time::Duration;

use phosk_daemon::{Daemon, NullIngest, PollConfig, QueueClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();

    let base = std::env::var("PHOSK_QUEUE_URL").unwrap_or_else(|_| "http://127.0.0.1:8765".into());
    let config = config_from_env()?;

    let client = QueueClient::new(&base)?;
    // Seam stand-in until `phosk_pipeline_receipt` is composed in behind a
    // `ReceiptIngest` impl (ADR-005/010): the concrete OCR/LLM adapters are wired
    // at the desktop server context, not here.
    let ingest = NullIngest::new();
    let daemon = Daemon::new(client, ingest, config);

    tracing::info!(%base, "phosk_daemon polling Pi queue (outbound only)");
    println!("phosk_daemon polling {base} (outbound; desktop exposes no inbound socket)");

    // Graceful shutdown on Ctrl-C; the loop exits at the next boundary.
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let processed = daemon.run(shutdown).await;
    println!("phosk_daemon stopped after processing {processed} blob(s)");
    Ok(())
}

/// Read poll pacing from the environment, falling back to [`PollConfig::default`].
/// A malformed override is a hard startup error (fail loud at the root).
fn config_from_env() -> Result<PollConfig, Box<dyn Error>> {
    let mut cfg = PollConfig::default();
    if let Ok(v) = std::env::var("PHOSK_DAEMON_LONG_POLL_MS") {
        cfg.long_poll = Duration::from_millis(v.parse()?);
    }
    if let Ok(v) = std::env::var("PHOSK_DAEMON_BACKOFF_MS") {
        cfg.idle_backoff = Duration::from_millis(v.parse()?);
    }
    Ok(cfg)
}

/// Tracing subscriber, debug builds only (release compiles the events out).
#[cfg(debug_assertions)]
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();
}

/// Release no-op.
#[cfg(not(debug_assertions))]
const fn init_tracing() {}
