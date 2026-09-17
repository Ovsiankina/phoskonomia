//! `phosk_daemon` — the trusted desktop poller, as a library.
//!
//! ## Trust model (ADR-007)
//!
//! The desktop is trusted and the Pi is not — so the desktop **never exposes an
//! inbound socket**. It reaches the Pi `phosk_queue` purely **outbound**: it
//! `GET`s `/queue/next`, pops one opaque blob, and hands it to the receipt
//! import pipeline. Nothing on the LAN can connect *to* the desktop.
//!
//! ## What lives here vs. behind the seam
//!
//! A drained blob is still **hostile input**. The actual decode → OCR → LLM →
//! persist pipeline runs in a network-less sandbox and is reached only through
//! the [`ReceiptIngest`] port (the seam). The concrete pipeline + its OCR/LLM
//! adapters are composed *elsewhere* (the desktop server context), never here —
//! this bin only owns the **poll/drain loop** and the outbound HTTP client.
//!
//! Until `phosk_pipeline_receipt` exists, [`NullIngest`] is the deterministic
//! fake the loop and its tests run against. Swapping in the real pipeline is a
//! one-line change at the composition root and a new `impl ReceiptIngest`.
//!
//! ## The poll protocol
//!
//! 1. The daemon `GET`s `{base}/queue/next?wait_ms=N` (a **long-poll**).
//! 2. `200` → body is the raw blob bytes; headers carry `x-phosk-blob-id` and
//!    `content-type`. The daemon wraps them in a [`DrainedBlob`] and calls
//!    [`ReceiptIngest::ingest`]. The pop was destructive, so the desktop now
//!    owns the only copy — ingest failures are logged, not retried against the
//!    queue (the blob is gone).
//! 3. `204` → the queue was empty for the whole long-poll window; loop again
//!    immediately (the wait already provided the pacing).
//! 4. transport error (Pi down / unreachable) → back off [`PollConfig::idle_backoff`]
//!    and retry. The Pi being offline is the normal case, not a fault.

use std::time::Duration;

use async_trait::async_trait;
use phosk_core::error::PhoskError;

/// One opaque blob the daemon drained from the Pi queue. Still hostile input:
/// the bytes are **not** decoded here — only the seam (the sandboxed pipeline)
/// may interpret them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrainedBlob {
    /// The queue's opaque blob id (from the `x-phosk-blob-id` header). Used for
    /// logging and de-duplication; never trusted as a filename or path.
    pub id: String,
    /// The MIME the Pi *sniffed* (from `content-type`). Advisory — the pipeline
    /// re-validates the bytes in its sandbox before trusting them.
    pub mime: String,
    /// The raw, opaque blob bytes.
    pub data: Vec<u8>,
}

/// The seam to the sandboxed receipt-import pipeline (`phosk_pipeline_receipt`,
/// not yet built). The daemon depends only on this port, never on the OCR/LLM
/// adapters behind it (ADR-005/010). It is `async` (the pipeline is I/O-bound)
/// and object-safe so the daemon holds an `Arc<dyn ReceiptIngest + Send + Sync>`
/// chosen by the composition root.
#[async_trait]
pub trait ReceiptIngest: Send + Sync {
    /// Hand one drained blob to the pipeline. Returns the pipeline's outcome, or
    /// a [`PhoskError`] if ingest failed. The daemon does **not** re-queue on
    /// failure — the pop was destructive and the queue no longer holds the blob.
    async fn ingest(&self, blob: DrainedBlob) -> Result<IngestOutcome, PhoskError>;
}

/// What the pipeline did with a blob. Kept coarse on purpose: the daemon only
/// needs to log and count, not understand receipt structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestOutcome {
    /// The blob produced a receipt draft that was enqueued for human approval
    /// (AI never auto-writes — ADR). `blob_id` echoes the source for the log.
    Queued {
        /// The source blob id this draft came from.
        blob_id: String,
    },
    /// The blob was rejected by the pipeline's own sandbox validation (bad MIME
    /// on second look, decode bomb, etc.). Dropped, with a reason for the log.
    Rejected {
        /// The source blob id.
        blob_id: String,
        /// Why the sandbox refused it.
        reason: String,
    },
}

/// A deterministic, side-effect-free [`ReceiptIngest`] for the loop's tests and
/// for running the daemon before `phosk_pipeline_receipt` exists. It records the
/// blobs it received (so a test can assert the round-trip) and always reports
/// `Queued`. It NEVER decodes the bytes — it is a seam stand-in, not a pipeline.
#[derive(Debug, Default)]
pub struct NullIngest {
    received: tokio::sync::Mutex<Vec<DrainedBlob>>,
}

impl NullIngest {
    /// A fresh recorder with no captured blobs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A snapshot copy of every blob handed to [`ReceiptIngest::ingest`] so far,
    /// in arrival order. Lets a test assert end-to-end round-trip.
    pub async fn received(&self) -> Vec<DrainedBlob> {
        self.received.lock().await.clone()
    }
}

#[async_trait]
impl ReceiptIngest for NullIngest {
    async fn ingest(&self, blob: DrainedBlob) -> Result<IngestOutcome, PhoskError> {
        let blob_id = blob.id.clone();
        self.received.lock().await.push(blob);
        Ok(IngestOutcome::Queued { blob_id })
    }
}

/// Outbound HTTP client for the Pi queue. The desktop's *only* contact with the
/// Pi — always client-side, never a listener. Wraps a `reqwest::Client` and the
/// queue base URL. All `reqwest`/HTTP types die inside this type; callers see
/// [`DrainedBlob`] and [`PhoskError`].
#[derive(Clone)]
pub struct QueueClient {
    http: reqwest::Client,
    base: String,
}

impl QueueClient {
    /// Build a client against `base` (e.g. `http://pi.local:8765`). Trailing
    /// slashes are trimmed so URL joins are stable.
    ///
    /// # Errors
    /// Returns [`PhoskError::Invalid`] if the underlying HTTP client cannot be
    /// constructed (e.g. the TLS backend failed to initialise).
    pub fn new(base: impl Into<String>) -> Result<Self, PhoskError> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| PhoskError::Invalid(format!("http client init failed: {e}")))?;
        Ok(Self {
            http,
            base: base.into().trim_end_matches('/').to_string(),
        })
    }

    /// Long-poll the queue for the next blob, waiting up to `wait` for one.
    ///
    /// Returns `Ok(Some(blob))` on a `200`, `Ok(None)` on a `204` (empty after
    /// the long-poll), and [`PhoskError::Invalid`] on a transport error or an
    /// unexpected status (the Pi being unreachable surfaces here, and the loop
    /// treats it as "back off and retry", not a crash).
    pub async fn poll_next(&self, wait: Duration) -> Result<Option<DrainedBlob>, PhoskError> {
        let url = format!("{}/queue/next", self.base);
        let resp = self
            .http
            .get(&url)
            .query(&[("wait_ms", wait.as_millis().to_string())])
            .send()
            .await
            .map_err(|e| PhoskError::Invalid(format!("queue poll transport error: {e}")))?;

        match resp.status().as_u16() {
            204 => Ok(None),
            200 => {
                // Pull the id + mime from headers BEFORE consuming the body.
                let id = resp
                    .headers()
                    .get("x-phosk-blob-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown")
                    .to_string();
                let mime = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let data = resp
                    .bytes()
                    .await
                    .map_err(|e| PhoskError::Invalid(format!("queue body read error: {e}")))?
                    .to_vec();
                Ok(Some(DrainedBlob { id, mime, data }))
            }
            other => Err(PhoskError::Invalid(format!(
                "queue returned unexpected status {other}"
            ))),
        }
    }
}

/// Pacing for the poll loop.
#[derive(Debug, Clone, Copy)]
pub struct PollConfig {
    /// `wait_ms` sent on each `/queue/next` long-poll. The server clamps it to
    /// its own ceiling; a generous value keeps the loop mostly parked server-side
    /// instead of busy-spinning.
    pub long_poll: Duration,
    /// How long to sleep after a **transport failure** (Pi unreachable) before
    /// retrying. The Pi being offline is normal, so this is gentle, not alarming.
    pub idle_backoff: Duration,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            long_poll: Duration::from_secs(20),
            idle_backoff: Duration::from_secs(5),
        }
    }
}

/// The desktop poller. Owns the outbound [`QueueClient`], the [`ReceiptIngest`]
/// seam, and the loop pacing. It exposes both a bounded [`Daemon::drain_pending`]
/// (drain everything ready *now*, used by the integration test and by a
/// foreground "catch up" pass) and an unbounded [`Daemon::run`] service loop.
pub struct Daemon<I: ReceiptIngest> {
    client: QueueClient,
    ingest: I,
    config: PollConfig,
}

impl<I: ReceiptIngest> Daemon<I> {
    /// Assemble a daemon from its three collaborators.
    #[must_use]
    pub const fn new(client: QueueClient, ingest: I, config: PollConfig) -> Self {
        Self {
            client,
            ingest,
            config,
        }
    }

    /// Borrow the ingest seam (e.g. to read a [`NullIngest`]'s captured blobs in
    /// a test).
    pub const fn ingest(&self) -> &I {
        &self.ingest
    }

    /// Poll once with a short non-blocking wait and, if a blob is ready, hand it
    /// to the pipeline. Returns `Ok(Some(outcome))` when a blob was processed,
    /// `Ok(None)` when the queue was empty, or the poll/ingest error.
    ///
    /// `wait` is the long-poll window for this single poll (use `0` to peek).
    pub async fn poll_once(&self, wait: Duration) -> Result<Option<IngestOutcome>, PhoskError> {
        match self.client.poll_next(wait).await? {
            Some(blob) => {
                tracing::debug!(id = %blob.id, bytes = blob.data.len(), "daemon: drained blob, handing to pipeline");
                let outcome = self.ingest.ingest(blob).await?;
                Ok(Some(outcome))
            }
            None => Ok(None),
        }
    }

    /// Drain every blob the queue has ready right now, in FIFO order, until an
    /// empty poll. Each blob is handed to the pipeline; the outcomes are
    /// returned in order. Bounded: it stops at the first empty poll (it does
    /// **not** long-poll for future arrivals — that is [`Daemon::run`]'s job).
    ///
    /// Used by the integration test and as a one-shot "catch up" pass.
    pub async fn drain_pending(&self) -> Result<Vec<IngestOutcome>, PhoskError> {
        let mut outcomes = Vec::new();
        // wait_ms = 0: a non-blocking pop. Loop until the queue reports empty.
        while let Some(outcome) = self.poll_once(Duration::from_millis(0)).await? {
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    /// The long-running service loop: long-poll the queue forever, ingesting
    /// each blob as it arrives. Transport errors (Pi offline) are logged and
    /// retried after [`PollConfig::idle_backoff`]; ingest errors are logged and
    /// dropped (the destructive pop already removed the blob). The loop runs
    /// until `shutdown` resolves, then returns the number of blobs processed.
    ///
    /// `shutdown` is any future (e.g. `tokio::signal::ctrl_c()`); when it
    /// completes the loop exits at the next iteration boundary.
    pub async fn run<S>(&self, shutdown: S) -> u64
    where
        S: std::future::Future<Output = ()> + Send,
    {
        let mut processed: u64 = 0;
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                biased;
                () = &mut shutdown => {
                    tracing::info!(processed, "daemon: shutdown requested, stopping poll loop");
                    return processed;
                }
                result = self.poll_once(self.config.long_poll) => {
                    match result {
                        Ok(Some(_outcome)) => {
                            processed = processed.saturating_add(1);
                        }
                        Ok(None) => {
                            // Empty long-poll: loop again immediately (the server
                            // already provided the pacing by parking the request).
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "daemon: poll/ingest failed, backing off");
                            tokio::time::sleep(self.config.idle_backoff).await;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_ingest_records_and_reports_queued() {
        let ingest = NullIngest::new();
        let blob = DrainedBlob {
            id: "blob-1".into(),
            mime: "image/jpeg".into(),
            data: vec![0xFF, 0xD8, 0xFF],
        };
        let outcome = ingest.ingest(blob.clone()).await.expect("ingest ok");
        assert_eq!(
            outcome,
            IngestOutcome::Queued {
                blob_id: "blob-1".into()
            }
        );
        assert_eq!(ingest.received().await, vec![blob]);
    }

    #[test]
    fn queue_client_trims_trailing_slash() {
        let c = QueueClient::new("http://pi.local:8765///").expect("client builds");
        assert_eq!(c.base, "http://pi.local:8765");
    }

    #[tokio::test]
    async fn poll_next_on_dead_endpoint_is_an_error_not_a_panic() {
        // Nothing is listening on this port; the transport error must surface as
        // a PhoskError, which the loop treats as "back off and retry".
        let c = QueueClient::new("http://127.0.0.1:1").expect("client builds");
        let err = c.poll_next(Duration::from_millis(50)).await;
        assert!(matches!(err, Err(PhoskError::Invalid(_))));
    }
}
