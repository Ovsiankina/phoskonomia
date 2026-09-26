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
//! ## Environment
//!
//! | Variable                    | Values                          | Default                   |
//! |-----------------------------|---------------------------------|---------------------------|
//! | `PHOSK_QUEUE_URL`           | Pi queue base URL               | `http://127.0.0.1:8765`   |
//! | `PHOSK_DAEMON_LONG_POLL_MS` | long-poll window (ms)           | `20000`                   |
//! | `PHOSK_DAEMON_BACKOFF_MS`   | back-off: Pi down, pipeline not ready, after a failure | `5000` |
//! | `PHOSK_DAEMON_MAX_BLOB_BYTES` | largest blob accepted (match the Pi's `PHOSK_QUEUE_MAX_BLOB_BYTES`) | `16777216` |
//! | `PHOSK_INGEST`              | `null` \| `pipeline`            | `null` (nothing imported) |
//!
//! With `PHOSK_INGEST=pipeline` each drained photo runs through
//! `phosk_pipeline_receipt::intake_receipt` and is STAGED as a pending approval
//! suggestion (never booked). The adapters are chosen with the same variables
//! as the Dioxus composition root (`frontend/dioxus-app/src/data/mod.rs`):
//!
//! | Variable          | Values                         | Daemon behaviour                           |
//! |-------------------|--------------------------------|--------------------------------------------|
//! | `PHOSK_DB`        | `surreal`                      | required: `<data_dir>/surreal/phosk.db`    |
//! | `PHOSK_DATA_DIR`  | an absolute path               | **required** (the UI's data dir); photos in `<data_dir>/photos` |
//! | `PHOSK_LLM_MODEL` | any Ollama model tag           | `qwen3.6:35b-custom` (localhost Ollama); must be healthy at startup |
//! | `PHOSK_OCR`       | `paddle` \| `vision` \| `auto` | `auto`; **no fake fallback** — no reachable engine is a startup error |
//!
//! `PHOSK_OCR_URL`, `PHOSK_OCR_VISION_URL` and `PHOSK_OCR_VISION_MODEL` are read
//! by the OCR adapters themselves.
//!
//! **One process per store.** The embedded `SurrealKV` store is single-process
//! (`SurrealDb::file` holds an exclusive lock), so pipeline mode and the Dioxus
//! UI cannot run at the same time on one `PHOSK_DATA_DIR`: whichever starts
//! second fails at open. Run the daemon while the UI is closed; the UI sees the
//! staged proposals when it next opens the store.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use phosk_adapter_llm::LlmAdapter;
use phosk_adapter_ocr::OcrAdapter;
use phosk_core::error::PhoskError;
use phosk_daemon::{
    Daemon, IngestKind, NullIngest, OcrProbe, PipelineIngest, PollConfig, QueueClient,
    ReceiptIngest, RunSummary, pipeline_data_dir,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();

    let base = std::env::var("PHOSK_QUEUE_URL").unwrap_or_else(|_| "http://127.0.0.1:8765".into());
    let config = config_from_env()?;

    let kind = IngestKind::select(
        std::env::var("PHOSK_INGEST").ok().as_deref(),
        std::env::var("PHOSK_DB").ok().as_deref(),
    )?;

    let mut client = QueueClient::new(&base)?;
    if let Ok(v) = std::env::var("PHOSK_DAEMON_MAX_BLOB_BYTES") {
        client = client.with_max_blob_bytes(v.trim().parse()?);
    }
    tracing::info!(%base, ?kind, "phosk_daemon polling Pi queue (outbound only)");
    println!(
        "phosk_daemon polling {base} with {kind:?} ingest (outbound; desktop exposes no inbound socket)"
    );

    let summary = match kind {
        IngestKind::Null => serve(Daemon::new(client, NullIngest::new(), config)).await,
        IngestKind::Pipeline => serve(Daemon::new(client, build_pipeline().await?, config)).await,
    };
    println!(
        "phosk_daemon stopped: {} blob(s) processed, {} failed (failed photos were popped from \
         the Pi and not staged)",
        summary.processed, summary.failed
    );
    Ok(())
}

/// Run the loop until Ctrl-C. A blob already popped finishes its ingest first.
async fn serve<I: ReceiptIngest>(daemon: Daemon<I>) -> RunSummary {
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    daemon.run(shutdown).await
}

/// Compose [`PipelineIngest`] from the concrete adapters. The only place they
/// are named. The DB is opened WITHOUT the demo seed (the daemon never writes
/// demo data into the user's store). The LLM must answer its health probe.
async fn build_pipeline() -> Result<PipelineIngest, PhoskError> {
    let data_dir = pipeline_data_dir(std::env::var_os("PHOSK_DATA_DIR"))?;
    let dir = data_dir.join("surreal");
    std::fs::create_dir_all(&dir)
        .map_err(|e| PhoskError::Invalid(format!("create surreal dir: {e}")))?;
    let db = phosk_db_surreal::SurrealDb::file(&dir.join("phosk.db").to_string_lossy()).await?;
    let storage = phosk_storage_fs::FsPhotoStorage::open(data_dir.join("photos"))?;
    let llm = phosk_llm_ollama::OllamaLlm::from_env()?;
    if !matches!(llm.health().await, Ok(true)) {
        return Err(PhoskError::Invalid(
            "the Ollama model (PHOSK_LLM_MODEL) is not reachable/ready".to_owned(),
        ));
    }
    let (ocr, probe) = build_ocr().await?;
    Ok(
        PipelineIngest::new(Arc::new(db), Arc::new(storage), ocr, Arc::new(llm))
            .with_ocr_probe(probe),
    )
}

/// Select a LIVE OCR engine (`PHOSK_OCR=paddle|vision|auto`, default `auto`).
/// Unlike the UI there is no `FakeOcr` fallback: canned text would stage a
/// fabricated receipt for every photo, so no reachable engine is an error.
/// Also returns the engine's readiness probe, checked before each pop.
async fn build_ocr() -> Result<(Arc<dyn OcrAdapter>, OcrProbe), PhoskError> {
    let kind = std::env::var("PHOSK_OCR").unwrap_or_else(|_| "auto".to_owned());
    let kind = kind.trim();
    let auto = kind.eq_ignore_ascii_case("auto");
    if !auto && !kind.eq_ignore_ascii_case("paddle") && !kind.eq_ignore_ascii_case("vision") {
        return Err(PhoskError::Invalid(
            "PHOSK_OCR must be `paddle`, `vision` or `auto`".to_owned(),
        ));
    }
    if (auto || kind.eq_ignore_ascii_case("paddle"))
        && let Ok(p) = phosk_ocr_paddle::PaddleOcr::from_env()
        && p.is_reachable().await
    {
        let p = Arc::new(p);
        let probe_p = p.clone();
        let probe: OcrProbe = Arc::new(move || {
            let p = probe_p.clone();
            Box::pin(async move { p.is_reachable().await })
        });
        return Ok((p, probe));
    }
    if (auto || kind.eq_ignore_ascii_case("vision"))
        && let Ok(v) = phosk_ocr_vision::OllamaVisionOcr::from_env()
        && matches!(v.has_vision_model().await, Ok(true))
    {
        let v = Arc::new(v);
        let probe_v = v.clone();
        let probe: OcrProbe = Arc::new(move || {
            let v = probe_v.clone();
            Box::pin(async move { matches!(v.has_vision_model().await, Ok(true)) })
        });
        return Ok((v, probe));
    }
    Err(PhoskError::Invalid(
        "no reachable OCR engine for PHOSK_OCR (PaddleOCR service or Ollama vision model)"
            .to_owned(),
    ))
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
