//! The real `phosk_pipeline_receipt` ingest behind the daemon's seam, driven
//! end to end against a real `phosk_queue` on loopback and the PORT fakes (no
//! Ollama, no `PaddleOCR`, no disk). A drained photo must end up as a PENDING
//! approval suggestion — never a ledger receipt. The pop is destructive, so a
//! dependency outage must NOT drain the queue: the daemon checks readiness
//! before each pop, and stops (drain) or backs off (run) after a failure.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::json;

use phosk_adapter_db::DatabaseAdapter;
use phosk_adapter_llm::{FakeLlm, LlmAdapter};
use phosk_adapter_ocr::{FakeOcr, OcrAdapter, OcrResult};
use phosk_adapter_storage::InMemoryStorage;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_daemon::{
    Daemon, IngestOutcome, OcrProbe, PipelineIngest, PollConfig, QueueClient, RunSummary,
};
use phosk_db_memory::MemoryDb;
use phosk_model::BudgetConfig;
use phosk_queue::{QueueConfig, QueueState, build_app};

/// A synthetic, magic-valid JPEG (SOI, APP0, SOS, scan bytes, EOI). `tag`
/// varies the scan bytes so two photos hash differently.
fn jpeg(tag: u8) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
    v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]);
    v.extend_from_slice(&[tag, 0xBB, 0xCC, 0xDD, 0xFF, 0xD9]);
    v
}

fn empty_db() -> Arc<MemoryDb> {
    Arc::new(MemoryDb::new(
        Vec::new(),
        Vec::new(),
        BudgetConfig {
            monthly_budget: Money::from_centimes(420_000),
            savings_target: Money::from_centimes(90_000),
        },
    ))
}

/// Returns a fixed synthetic receipt for any prompt; the first `fail_first`
/// structured calls fail instead (a model that is down, then back).
struct ScriptedLlm {
    fail_first: usize,
    calls: AtomicUsize,
}

impl ScriptedLlm {
    const fn new(fail_first: usize) -> Self {
        Self {
            fail_first,
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl LlmAdapter for ScriptedLlm {
    fn model(&self) -> &'static str {
        "scripted-test"
    }
    async fn health(&self) -> Result<bool, PhoskError> {
        Ok(true)
    }
    async fn complete(&self, _prompt: &str) -> Result<String, PhoskError> {
        Ok(String::new())
    }
    async fn generate_structured(
        &self,
        _prompt: &str,
        _schema: &serde_json::Value,
    ) -> Result<serde_json::Value, PhoskError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) < self.fail_first {
            return Err(PhoskError::Invalid("model unavailable".to_owned()));
        }
        Ok(json!({
            "shop": "Test Shop",
            "category": "Groceries",
            "lineItems": [
                {"name": "Item A", "qty": 1.0, "unitPriceCentimes": 245, "confidence": 0.95},
                {"name": "Item B", "qty": 2.0, "unitPriceCentimes": 150, "confidence": 0.90}
            ]
        }))
    }
}

/// An OCR engine that is always down.
struct DownOcr;

#[async_trait::async_trait]
impl OcrAdapter for DownOcr {
    async fn extract(&self, _image: &[u8]) -> Result<OcrResult, PhoskError> {
        Err(PhoskError::Invalid("ocr service unreachable".to_owned()))
    }
}

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

async fn enqueue(base: &str, body: Vec<u8>) -> String {
    let resp = reqwest::Client::new()
        .post(format!("{base}/queue/photo"))
        .body(body)
        .send()
        .await
        .expect("enqueue request");
    assert_eq!(resp.status().as_u16(), 201, "queue accepted the photo");
    let ack: serde_json::Value = resp.json().await.expect("ack json");
    ack["id"].as_str().expect("ack id").to_string()
}

fn pipeline(
    db: &Arc<MemoryDb>,
    ocr: Arc<dyn OcrAdapter>,
    llm: Arc<dyn LlmAdapter>,
) -> PipelineIngest {
    PipelineIngest::new(db.clone(), Arc::new(InMemoryStorage::new()), ocr, llm)
}

#[tokio::test]
async fn queued_photo_becomes_a_pending_suggestion_and_ledger_is_unchanged() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state.clone()).await;
    let id = enqueue(&base, jpeg(1)).await;

    let db = empty_db();
    let ledger_before = db.all_receipts().await.expect("receipts").len();
    let daemon = Daemon::new(
        QueueClient::new(&base).expect("client"),
        pipeline(&db, Arc::new(FakeOcr::new()), Arc::new(ScriptedLlm::new(0))),
        PollConfig::default(),
    );

    let outcomes = daemon.drain_pending().await.expect("drain");
    assert_eq!(outcomes, vec![IngestOutcome::Queued { blob_id: id }]);

    // Exactly one PENDING receipt proposal awaits human approval...
    let pending = phosk_ai::pending_suggestions(db.as_ref())
        .await
        .expect("pending");
    assert_eq!(pending.len(), 1, "one receipt group pending");
    assert_eq!(pending[0].suggestions.len(), 1);
    let s = &pending[0].suggestions[0];
    assert_eq!(s.suggestion.status, "open");
    assert_eq!(s.suggestion.kind, "receipt");
    assert!(s.proposal.is_some(), "the proposal payload is staged");
    // ...and the ledger did not change: only approval books it.
    assert_eq!(
        db.all_receipts().await.expect("receipts").len(),
        ledger_before
    );
    assert!(state.is_empty().await, "the photo was drained");
}

#[tokio::test]
async fn failing_llm_stops_the_drain_and_leaves_the_rest_queued() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state.clone()).await;
    let first = enqueue(&base, jpeg(1)).await;
    let second = enqueue(&base, jpeg(2)).await;

    let db = empty_db();
    let daemon = Daemon::new(
        QueueClient::new(&base).expect("client"),
        // The model fails on the first photo, then recovers.
        pipeline(&db, Arc::new(FakeOcr::new()), Arc::new(ScriptedLlm::new(1))),
        PollConfig::default(),
    );

    // The failure stops this pass: the next photo stays on the Pi instead of
    // being popped into a pipeline that just failed.
    let outcomes = daemon.drain_pending().await.expect("drain");
    assert_eq!(
        outcomes,
        vec![IngestOutcome::Failed {
            blob_id: first,
            // A PII-free error code, never the adapter's message.
            reason: "invalid_input".to_owned(),
        }]
    );
    assert_eq!(state.len().await, 1, "the second photo is still queued");

    // The next pass (model recovered) stages it.
    let outcomes = daemon.drain_pending().await.expect("second drain");
    assert_eq!(outcomes, vec![IngestOutcome::Queued { blob_id: second }]);
    let pending = phosk_ai::pending_suggestions(db.as_ref())
        .await
        .expect("pending");
    assert_eq!(pending.len(), 1, "only the second photo was staged");
    assert!(db.all_receipts().await.expect("receipts").is_empty());
    assert!(state.is_empty().await);
}

#[tokio::test]
async fn unreachable_llm_leaves_the_queue_untouched() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state.clone()).await;
    enqueue(&base, jpeg(1)).await;
    enqueue(&base, jpeg(2)).await;

    let db = empty_db();
    let ingest = pipeline(
        &db,
        Arc::new(FakeOcr::new()),
        Arc::new(FakeLlm::new().reachable(false).fail_completions(true)),
    );
    let daemon = Daemon::new(
        QueueClient::new(&base).expect("client"),
        ingest,
        PollConfig {
            long_poll: Duration::from_millis(0),
            idle_backoff: Duration::from_millis(10),
        },
    );

    assert!(daemon.drain_pending().await.expect("drain").is_empty());
    assert_eq!(state.len().await, 2, "nothing popped while the LLM is down");

    // The service loop backs off instead of popping, for as long as it runs.
    let summary = daemon
        .run(tokio::time::sleep(Duration::from_millis(200)))
        .await;
    assert_eq!(summary, RunSummary::default());
    assert_eq!(state.len().await, 2, "the loop never drained the queue");
    assert!(db.ai_suggestions().await.expect("suggestions").is_empty());
}

#[tokio::test]
async fn unreachable_ocr_probe_leaves_the_queue_untouched() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state.clone()).await;
    enqueue(&base, jpeg(1)).await;

    let db = empty_db();
    let down: OcrProbe = Arc::new(|| Box::pin(async { false }));
    let daemon = Daemon::new(
        QueueClient::new(&base).expect("client"),
        pipeline(&db, Arc::new(FakeOcr::new()), Arc::new(ScriptedLlm::new(0))).with_ocr_probe(down),
        PollConfig::default(),
    );

    assert!(daemon.drain_pending().await.expect("drain").is_empty());
    assert_eq!(state.len().await, 1);
}

#[tokio::test]
async fn run_counts_failures_separately() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state.clone()).await;
    enqueue(&base, jpeg(1)).await;
    enqueue(&base, jpeg(2)).await;

    let db = empty_db();
    let daemon = Daemon::new(
        QueueClient::new(&base).expect("client"),
        pipeline(&db, Arc::new(FakeOcr::new()), Arc::new(ScriptedLlm::new(1))),
        PollConfig {
            long_poll: Duration::from_millis(0),
            idle_backoff: Duration::from_millis(10),
        },
    );

    // Stop once the Pi is empty; the in-flight photo still finishes.
    let probe = state.clone();
    let shutdown = async move {
        while !probe.is_empty().await {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    };
    let summary = daemon.run(shutdown).await;
    assert_eq!(
        summary,
        RunSummary {
            processed: 1,
            failed: 1
        }
    );
    let pending = phosk_ai::pending_suggestions(db.as_ref())
        .await
        .expect("pending");
    assert_eq!(pending.len(), 1);
}

#[tokio::test]
async fn failing_ocr_marks_the_blob_failed_and_stages_nothing() {
    let state = QueueState::new(QueueConfig::default());
    let base = spawn_queue(state).await;
    let id = enqueue(&base, jpeg(3)).await;

    let db = empty_db();
    let daemon = Daemon::new(
        QueueClient::new(&base).expect("client"),
        pipeline(&db, Arc::new(DownOcr), Arc::new(ScriptedLlm::new(0))),
        PollConfig::default(),
    );

    let outcomes = daemon.drain_pending().await.expect("drain");
    assert_eq!(
        outcomes,
        vec![IngestOutcome::Failed {
            blob_id: id,
            reason: "invalid_input".to_owned(),
        }]
    );
    assert!(db.ai_suggestions().await.expect("suggestions").is_empty());
    assert!(db.all_receipts().await.expect("receipts").is_empty());
}
