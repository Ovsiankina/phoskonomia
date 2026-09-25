//! End-to-end Pi-DMZ round-trip (ADR-007): a real `phosk_queue` server on an
//! ephemeral loopback port, a phone-style `POST` enqueue, and the `phosk_daemon`
//! polling it OUTBOUND to drain the blob into the pipeline seam. Asserts the
//! opaque bytes survive the full network round-trip unchanged.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use phosk_core::error::PhoskError;
use phosk_daemon::{
    Daemon, DrainedBlob, IngestOutcome, NullIngest, PollConfig, QueueClient, ReceiptIngest,
};
use phosk_queue::{QueueConfig, QueueState, build_app};

/// A minimal but magic-valid JPEG blob (the queue sniffs `FF D8 FF`).
fn jpeg_blob(tag: u8) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
    v.extend_from_slice(&[tag; 64]);
    v
}

/// Bind the real queue on an ephemeral loopback port and serve it in a
/// background task. Returns the base URL the daemon polls.
async fn spawn_queue(state: QueueState) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral loopback port");
    let addr = listener.local_addr().expect("local addr");
    let app = build_app(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("queue serve");
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn blob_round_trips_phone_to_queue_to_daemon() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state).await;

    // A phone POSTs a receipt photo to the Pi (raw bytes; opaque to the Pi).
    let blob = jpeg_blob(0xAB);
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/queue/photo"))
        .body(blob.clone())
        .send()
        .await
        .expect("enqueue request");
    assert_eq!(resp.status().as_u16(), 201, "phone enqueue accepted");
    let ack: serde_json::Value = resp.json().await.expect("ack json");
    assert_eq!(ack["bytes"].as_u64(), Some(blob.len() as u64));
    let enqueued_id = ack["id"].as_str().expect("ack id").to_string();

    // The trusted desktop daemon polls the Pi OUTBOUND and drains the blob into
    // the pipeline seam.
    let daemon = Daemon::new(
        QueueClient::new(&base).expect("queue client"),
        NullIngest::new(),
        PollConfig::default(),
    );
    let outcomes = daemon.drain_pending().await.expect("drain");

    assert_eq!(outcomes.len(), 1, "exactly one blob drained");
    assert_eq!(
        outcomes[0],
        IngestOutcome::Queued {
            blob_id: enqueued_id.clone()
        },
        "blob id survives the round-trip"
    );

    // The pipeline seam received the EXACT opaque bytes, unmodified, with the
    // sniffed MIME the Pi reported.
    let received = daemon.ingest().received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].id, enqueued_id);
    assert_eq!(received[0].mime, "image/jpeg");
    assert_eq!(received[0].data, blob, "opaque bytes unchanged end to end");
}

#[tokio::test]
async fn fifo_order_is_preserved_across_the_wire() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state).await;
    let client = reqwest::Client::new();

    let mut ids = Vec::new();
    for tag in 0u8..5 {
        let resp = client
            .post(format!("{base}/queue/photo"))
            .body(jpeg_blob(tag))
            .send()
            .await
            .expect("enqueue");
        let ack: serde_json::Value = resp.json().await.expect("ack");
        ids.push(ack["id"].as_str().expect("id").to_string());
    }

    let daemon = Daemon::new(
        QueueClient::new(&base).expect("client"),
        NullIngest::new(),
        PollConfig::default(),
    );
    daemon.drain_pending().await.expect("drain");

    let received: Vec<String> = daemon
        .ingest()
        .received()
        .await
        .into_iter()
        .map(|b| b.id)
        .collect();
    assert_eq!(
        received, ids,
        "drained in the same FIFO order they were posted"
    );
}

#[tokio::test]
async fn long_poll_wakes_on_a_late_arrival() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state).await;

    // The daemon long-polls an empty queue; a phone POSTs slightly later. The
    // poll must wake and drain it (the outbound long-poll is the whole point).
    let poll_base = base.clone();
    let poller = tokio::spawn(async move {
        let daemon = Daemon::new(
            QueueClient::new(&poll_base).expect("client"),
            NullIngest::new(),
            PollConfig::default(),
        );
        let outcome = daemon
            .poll_once(Duration::from_secs(5))
            .await
            .expect("poll ok");
        (outcome, daemon.ingest().received().await)
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    let client = reqwest::Client::new();
    client
        .post(format!("{base}/queue/photo"))
        .body(jpeg_blob(0x7E))
        .send()
        .await
        .expect("late enqueue");

    let (outcome, received) = poller.await.expect("poller task");
    assert!(outcome.is_some(), "long-poll woke on the late arrival");
    assert_eq!(received.len(), 1, "the late blob was drained");
}

/// Records a blob only after a delay, and signals when it has started — lets a
/// test fire shutdown while a drained blob is mid-ingest.
struct SlowIngest {
    started: Arc<tokio::sync::Notify>,
    done: tokio::sync::Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl ReceiptIngest for SlowIngest {
    async fn ingest(&self, blob: DrainedBlob) -> Result<IngestOutcome, PhoskError> {
        self.started.notify_one();
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.done.lock().await.push(blob.id.clone());
        Ok(IngestOutcome::Queued { blob_id: blob.id })
    }
}

#[tokio::test]
async fn shutdown_during_ingest_lets_the_drained_blob_finish() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state.clone()).await;
    reqwest::Client::new()
        .post(format!("{base}/queue/photo"))
        .body(jpeg_blob(0x11))
        .send()
        .await
        .expect("enqueue");

    let started = Arc::new(tokio::sync::Notify::new());
    let daemon = Daemon::new(
        QueueClient::new(&base).expect("client"),
        SlowIngest {
            started: started.clone(),
            done: tokio::sync::Mutex::new(Vec::new()),
        },
        PollConfig::default(),
    );

    // Ctrl-C arrives after the destructive pop, while the pipeline runs.
    let summary = daemon.run(async move { started.notified().await }).await;
    assert_eq!(summary.processed, 1, "the in-flight blob was not dropped");
    assert_eq!(daemon.ingest().done.lock().await.len(), 1);
    assert!(state.is_empty().await);
}

#[tokio::test]
async fn oversized_body_is_refused_before_it_is_read_fully() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state).await;
    reqwest::Client::new()
        .post(format!("{base}/queue/photo"))
        .body(jpeg_blob(0x22))
        .send()
        .await
        .expect("enqueue");

    let client = QueueClient::new(&base)
        .expect("client")
        .with_max_blob_bytes(16);
    let err = client.poll_next(Duration::from_millis(0)).await;
    assert!(
        matches!(&err, Err(PhoskError::Invalid(m)) if m.contains("exceeds")),
        "{err:?}"
    );
}
