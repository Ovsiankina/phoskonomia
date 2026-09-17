//! `phosk_queue` — the Raspberry-Pi DMZ blob queue, as a library.
//!
//! ## Trust model (ADR-007)
//!
//! This server runs on an **untrusted** Pi sitting in a DMZ. It is the *only*
//! legitimate inbound network socket in Phoskonomia. The trusted desktop never
//! exposes a socket; it polls this queue **outbound** and drains it.
//!
//! Because everything that arrives here is **hostile input**, the queue does the
//! absolute minimum and trusts nothing:
//!
//! * It treats every blob as **opaque bytes**. It never decodes, never runs OCR
//!   or an LLM, never persists to a real datastore — those happen later, in a
//!   network-less sandbox on the desktop.
//! * On ingest it only does cheap, bounded gating: a **size cap** and a
//!   **magic-byte MIME sniff** (JPEG / PNG / WebP). A `Content-Type` *header* is
//!   advisory and is *not* trusted — the bytes themselves must look like an
//!   image. This rejects the obvious "POST a 2 GB zip / an executable" abuse
//!   without ever parsing attacker-controlled structure.
//! * It enforces a **capacity limit** (count of queued blobs) so a flood cannot
//!   exhaust memory; over capacity it returns `503`.
//!
//! ## Protocol
//!
//! | Method | Path             | Purpose                                         |
//! |--------|------------------|-------------------------------------------------|
//! | `POST` | `/queue/photo`   | Enqueue one blob (raw body bytes). `201` + JSON.|
//! | `GET`  | `/queue/next`    | Long-poll **pop** the oldest blob. `200`/`204`. |
//! | `GET`  | `/queue/stats`   | Depth / capacity (no blob contents). `200` JSON.|
//! | `GET`  | `/healthz`       | Liveness. `200`.                                |
//!
//! `POST /queue/photo` body is the **raw image bytes**; the response is JSON
//! `{ "id": "...", "bytes": N }` (an opaque id + the accepted length, never the
//! contents echoed back). Errors: `413` too large, `415` not an image, `503`
//! queue full.
//!
//! `GET /queue/next?wait_ms=N` is the desktop poller's drain. If a blob is
//! ready it returns `200` with the **raw blob bytes** as the body and headers
//! `x-phosk-blob-id` + `content-type` (the sniffed type). If the queue is empty
//! it waits up to `wait_ms` (clamped) for one to arrive, then returns `204` if
//! still empty. The pop is **destructive and exactly-once**: a blob handed out
//! is removed from the queue (the desktop owns it from then on).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify};

/// The sniffed image kind of an accepted blob. The Pi only ever distinguishes
/// the handful of container formats a phone camera produces; anything else is
/// rejected at ingest, so this enum is closed on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageKind {
    /// JPEG (`FF D8 FF`). The common phone-photo format.
    Jpeg,
    /// PNG (`89 50 4E 47 0D 0A 1A 0A`).
    Png,
    /// WebP (`RIFF....WEBP`).
    Webp,
}

impl ImageKind {
    /// The canonical MIME string handed back to the drainer in `content-type`.
    #[must_use]
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }

    /// Sniff the container by **magic bytes only** — the header `Content-Type`
    /// is attacker-controlled and never trusted. Returns `None` if the bytes do
    /// not begin with a recognised image signature (the blob is then rejected
    /// `415`). This reads a fixed-size prefix and never parses the body.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        const PNG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
            return Some(Self::Jpeg);
        }
        if bytes.len() >= 8 && bytes[0..8] == PNG {
            return Some(Self::Png);
        }
        // RIFF container with a WEBP fourcc at offset 8.
        if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            return Some(Self::Webp);
        }
        None
    }
}

/// One opaque blob sitting in the FIFO. The queue knows its assigned id, its
/// sniffed [`ImageKind`], and its bytes — and nothing else. It does **not**
/// decode or interpret `data`.
#[derive(Debug, Clone)]
pub struct Blob {
    /// Server-assigned opaque id (monotonic per process). Lets the drainer log
    /// and de-duplicate without the Pi ever trusting a client-supplied id.
    pub id: String,
    /// The sniffed container kind (the only thing the Pi "knows" about content).
    pub kind: ImageKind,
    /// The raw, opaque bytes. Never decoded on the Pi.
    pub data: Bytes,
}

/// Limits the queue enforces. Defaults are conservative for a single household
/// of phones feeding one desktop; override via [`QueueConfig`] / env in `main`.
#[derive(Debug, Clone, Copy)]
pub struct QueueConfig {
    /// Largest single blob accepted, in bytes. Larger → `413`. A receipt photo
    /// is a few MB; the cap exists to bound one hostile request.
    pub max_blob_bytes: usize,
    /// Largest number of blobs held at once. At capacity, `POST` → `503` so a
    /// flood cannot exhaust memory.
    pub max_queue_len: usize,
    /// Hard ceiling on a drainer's `wait_ms` long-poll, in milliseconds. Caps
    /// how long one request may park a connection.
    pub max_wait_ms: u64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_blob_bytes: 16 * 1024 * 1024, // 16 MiB
            max_queue_len: 256,
            max_wait_ms: 30_000, // 30 s
        }
    }
}

/// Shared queue state behind the router. Cloneable (it is all `Arc`), so axum
/// can hand a clone to every handler. The FIFO is a plain `VecDeque` behind an
/// async `Mutex`; a `Notify` wakes parked drainers the instant a blob arrives.
#[derive(Clone)]
pub struct QueueState {
    inner: Arc<QueueInner>,
}

struct QueueInner {
    config: QueueConfig,
    fifo: Mutex<VecDeque<Blob>>,
    /// Notified on every successful enqueue to wake long-polling drainers.
    arrived: Notify,
    /// Monotonic id counter (centimes-style: never reused within a process).
    next_id: Mutex<u64>,
}

/// Why an enqueue was rejected. Each maps to a single HTTP status at the edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueError {
    /// Body exceeded [`QueueConfig::max_blob_bytes`] → `413`.
    TooLarge,
    /// Body did not begin with a recognised image signature → `415`.
    NotAnImage,
    /// Queue already at [`QueueConfig::max_queue_len`] → `503`.
    Full,
}

impl QueueState {
    /// Build an empty queue with the given limits.
    #[must_use]
    pub fn new(config: QueueConfig) -> Self {
        Self {
            inner: Arc::new(QueueInner {
                config,
                fifo: Mutex::new(VecDeque::new()),
                arrived: Notify::new(),
                next_id: Mutex::new(0),
            }),
        }
    }

    /// The active limits (read-only view, for `/queue/stats` and tests).
    #[must_use]
    pub fn config(&self) -> &QueueConfig {
        &self.inner.config
    }

    /// Current number of queued blobs.
    pub async fn len(&self) -> usize {
        self.inner.fifo.lock().await.len()
    }

    /// Whether the queue currently holds no blobs.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Validate and enqueue one opaque blob, returning its assigned id.
    ///
    /// This is the full hostile-input gate, in order: size cap → magic-byte
    /// sniff → capacity check. Only bytes that pass all three are stored. The
    /// blob is appended to the FIFO tail; a parked drainer is woken.
    pub async fn enqueue(&self, data: Bytes) -> Result<String, EnqueueError> {
        if data.len() > self.inner.config.max_blob_bytes {
            return Err(EnqueueError::TooLarge);
        }
        let kind = ImageKind::sniff(&data).ok_or(EnqueueError::NotAnImage)?;

        let mut fifo = self.inner.fifo.lock().await;
        if fifo.len() >= self.inner.config.max_queue_len {
            return Err(EnqueueError::Full);
        }
        let id = {
            let mut counter = self.inner.next_id.lock().await;
            let id = format!("blob-{counter:020}");
            *counter = counter.wrapping_add(1);
            id
        };
        fifo.push_back(Blob {
            id: id.clone(),
            kind,
            data,
        });
        drop(fifo);
        self.inner.arrived.notify_one();
        Ok(id)
    }

    /// Pop the oldest blob now, without waiting. Destructive: the returned blob
    /// is removed from the queue.
    pub async fn try_pop(&self) -> Option<Blob> {
        self.inner.fifo.lock().await.pop_front()
    }

    /// Long-poll pop: return the oldest blob, waiting up to `wait` for one to
    /// arrive if the queue is empty. Returns `None` only if it is still empty
    /// when `wait` elapses. The pop is exactly-once.
    pub async fn pop_wait(&self, wait: Duration) -> Option<Blob> {
        if let Some(blob) = self.try_pop().await {
            return Some(blob);
        }
        // Register the wake interest BEFORE the final emptiness check to avoid a
        // lost-wakeup race with a concurrent enqueue.
        let notified = self.inner.arrived.notified();
        if let Some(blob) = self.try_pop().await {
            return Some(blob);
        }
        match tokio::time::timeout(wait, notified).await {
            Ok(()) => self.try_pop().await, // woken — but another drainer may have raced us
            Err(_) => None,                 // timed out empty
        }
    }
}

/// Build the queue [`Router`] with the given shared state injected. Exposed so
/// integration tests (and `main`) drive the exact same routes.
pub fn build_app(state: QueueState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/queue/photo", post(enqueue_handler))
        .route("/queue/next", get(drain_handler))
        .route("/queue/stats", get(stats_handler))
        .with_state(state)
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}

/// JSON returned on a successful enqueue. Deliberately minimal: an opaque id and
/// the accepted byte count — the contents are never echoed back.
#[derive(Debug, Serialize, Deserialize)]
pub struct EnqueueAck {
    /// The server-assigned opaque blob id.
    pub id: String,
    /// The number of bytes accepted.
    pub bytes: usize,
}

async fn enqueue_handler(State(state): State<QueueState>, body: Bytes) -> Response {
    let bytes = body.len();
    match state.enqueue(body).await {
        Ok(id) => {
            tracing::debug!(%id, bytes, "queue: enqueued opaque blob");
            (StatusCode::CREATED, axum::Json(EnqueueAck { id, bytes })).into_response()
        }
        Err(EnqueueError::TooLarge) => {
            (StatusCode::PAYLOAD_TOO_LARGE, "blob exceeds max size").into_response()
        }
        Err(EnqueueError::NotAnImage) => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "body is not a recognised image (jpeg/png/webp)",
        )
            .into_response(),
        Err(EnqueueError::Full) => {
            (StatusCode::SERVICE_UNAVAILABLE, "queue at capacity").into_response()
        }
    }
}

/// Query for the drain long-poll.
#[derive(Debug, Deserialize)]
pub struct DrainQuery {
    /// Milliseconds to wait for a blob if the queue is empty (clamped to
    /// [`QueueConfig::max_wait_ms`]). Absent → `0` (non-blocking).
    #[serde(default)]
    pub wait_ms: u64,
}

async fn drain_handler(State(state): State<QueueState>, Query(q): Query<DrainQuery>) -> Response {
    let wait = Duration::from_millis(q.wait_ms.min(state.config().max_wait_ms));
    match state.pop_wait(wait).await {
        Some(blob) => {
            tracing::debug!(id = %blob.id, bytes = blob.data.len(), "queue: drained blob");
            // Build the header map explicitly: a mixed `HeaderName`/`&str` array
            // literal does not type-unify, and a bad blob id must not be able to
            // poison the response — an unrepresentable value is simply dropped.
            let mut headers = header::HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static(blob.kind.mime()),
            );
            if let Ok(value) = header::HeaderValue::from_str(&blob.id) {
                headers.insert(header::HeaderName::from_static("x-phosk-blob-id"), value);
            }
            (StatusCode::OK, headers, blob.data).into_response()
        }
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

/// Depth / capacity snapshot. No blob contents — the Pi never exposes payloads
/// via a metadata endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct QueueStats {
    /// Blobs currently queued.
    pub depth: usize,
    /// Maximum blobs the queue will hold.
    pub capacity: usize,
    /// Largest single blob accepted, in bytes.
    pub max_blob_bytes: usize,
}

async fn stats_handler(State(state): State<QueueState>) -> Response {
    let snapshot = QueueStats {
        depth: state.len().await,
        capacity: state.config().max_queue_len,
        max_blob_bytes: state.config().max_blob_bytes,
    };
    (StatusCode::OK, axum::Json(snapshot)).into_response()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn jpeg(n: usize) -> Bytes {
        let mut v = vec![0xFF, 0xD8, 0xFF];
        v.resize(n.max(3), 0x00);
        Bytes::from(v)
    }

    #[test]
    fn sniff_recognises_each_format_and_rejects_junk() {
        assert_eq!(
            ImageKind::sniff(&[0xFF, 0xD8, 0xFF, 0x00]),
            Some(ImageKind::Jpeg)
        );
        let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x01];
        assert_eq!(ImageKind::sniff(&png), Some(ImageKind::Png));
        let mut webp = b"RIFF\x00\x00\x00\x00WEBP".to_vec();
        webp.push(0);
        assert_eq!(ImageKind::sniff(&webp), Some(ImageKind::Webp));
        assert_eq!(ImageKind::sniff(b"not an image"), None);
        assert_eq!(ImageKind::sniff(&[]), None);
    }

    #[tokio::test]
    async fn enqueue_then_pop_is_fifo() {
        let q = QueueState::new(QueueConfig::default());
        let id1 = q.enqueue(jpeg(10)).await.expect("first enqueue");
        let id2 = q.enqueue(jpeg(20)).await.expect("second enqueue");
        assert_ne!(id1, id2);
        assert_eq!(q.len().await, 2);

        let first = q.try_pop().await.expect("first pop");
        assert_eq!(first.id, id1);
        let second = q.try_pop().await.expect("second pop");
        assert_eq!(second.id, id2);
        assert!(q.is_empty().await);
    }

    #[tokio::test]
    async fn enqueue_rejects_oversize_and_junk_and_full() {
        let cfg = QueueConfig {
            max_blob_bytes: 100,
            max_queue_len: 1,
            max_wait_ms: 10,
        };
        let q = QueueState::new(cfg);

        assert_eq!(q.enqueue(jpeg(1000)).await, Err(EnqueueError::TooLarge));
        assert_eq!(
            q.enqueue(Bytes::from_static(b"hello world, not an image"))
                .await,
            Err(EnqueueError::NotAnImage)
        );
        q.enqueue(jpeg(10)).await.expect("fills the single slot");
        assert_eq!(q.enqueue(jpeg(10)).await, Err(EnqueueError::Full));
    }

    #[tokio::test]
    async fn pop_wait_returns_none_when_empty() {
        let q = QueueState::new(QueueConfig::default());
        assert!(q.pop_wait(Duration::from_millis(20)).await.is_none());
    }

    #[tokio::test]
    async fn pop_wait_wakes_on_late_arrival() {
        let q = QueueState::new(QueueConfig::default());
        let q2 = q.clone();
        let writer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            q2.enqueue(jpeg(10)).await.expect("late enqueue");
        });
        let got = q.pop_wait(Duration::from_millis(500)).await;
        assert!(got.is_some(), "drainer should wake on the late enqueue");
        writer.await.expect("writer task");
    }
}
