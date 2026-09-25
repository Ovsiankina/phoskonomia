//! `data::receipt`: photo upload → intake → per-line review. Every write runs
//! on a fresh seeded store with the in-process fakes (no model, OCR or disk).

use phosk_adapter_db::DatabaseAdapter;
use phosk_adapter_llm::LlmAdapter;
use phosk_adapter_ocr::FakeOcr;
use phosk_adapter_storage::InMemoryStorage;
use phosk_core::error::PhoskError;
use phosk_db_memory::MemoryDb;
use serde_json::{json, Value};

use super::support::{fresh_db, money, server_error, today};
use crate::data::approvals::approve_proposal_with;
use crate::data::receipt::{upload_receipt_with, IntakeStatus, ReceiptIntakeDto, MAX_UPLOAD_BYTES};

const REFUSED: &str = "This file could not be read as a receipt photo. Nothing was staged. \
                       Upload a clear JPEG or PNG photo of one receipt.";
const NO_PHOTO: &str = "No photo was received. Pick a photo and try again.";

/// An LLM that answers every structured call with one fixed value (the fake's
/// exact-prompt scripting can't see the OCR text the pipeline interpolates).
struct ScriptedLlm(Value);

#[async_trait::async_trait]
impl LlmAdapter for ScriptedLlm {
    fn model(&self) -> &str {
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
        _schema: &Value,
    ) -> Result<Value, PhoskError> {
        Ok(self.0.clone())
    }
}

/// A well-formed synthetic receipt: two confident lines, one low-confidence.
fn receipt_llm() -> ScriptedLlm {
    ScriptedLlm(json!({
        "shop": "Synthetic Market",
        "category": "Groceries",
        "lineItems": [
            {"name": "Bananas", "qty": 1.0, "unitPriceCentimes": 245, "confidence": 0.95},
            {"name": "Bread", "qty": 1.0, "unitPriceCentimes": 320, "confidence": 0.91},
            {"name": "Illegible", "qty": 2.0, "unitPriceCentimes": 150, "confidence": 0.40}
        ]
    }))
}

/// A signature-valid synthetic JPEG (SOI, one COM segment, SOS, scan, EOI);
/// `tag` makes distinct photos.
fn jpeg(tag: u8) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xFE, 0x00, 0x03, tag, 0xFF, 0xDA];
    v.extend_from_slice(&[0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]);
    v.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xFF, 0xD9]);
    v
}

struct Ports {
    db: MemoryDb,
    storage: InMemoryStorage,
    ocr: FakeOcr,
}

fn ports() -> Ports {
    Ports {
        db: fresh_db(),
        storage: InMemoryStorage::new(),
        ocr: FakeOcr::new(),
    }
}

async fn upload(
    p: &Ports,
    llm: &dyn LlmAdapter,
    bytes: &[u8],
) -> Result<ReceiptIntakeDto, dioxus::prelude::ServerFnError> {
    upload_receipt_with(&p.db, &p.storage, &p.ocr, llm, bytes, today()).await
}

async fn ledger_len(db: &MemoryDb) -> usize {
    db.all_receipts().await.expect("receipts").len()
}

async fn open_receipt_suggestions(db: &MemoryDb) -> usize {
    db.ai_suggestions()
        .await
        .expect("suggestions")
        .iter()
        .filter(|s| s.kind == "receipt" && s.status == "open")
        .count()
}

#[test]
fn the_client_cap_is_the_pipeline_cap() {
    assert_eq!(MAX_UPLOAD_BYTES, phosk_pipeline_receipt::MAX_PHOTO_BYTES);
}

#[tokio::test]
async fn a_valid_photo_stages_one_proposal_and_books_nothing() {
    let p = ports();
    let before = ledger_len(&p.db).await;

    let out = upload(&p, &receipt_llm(), &jpeg(1)).await.expect("staged");

    assert_eq!(out.status, IntakeStatus::Staged);
    assert_eq!(open_receipt_suggestions(&p.db).await, 1);
    assert_eq!(ledger_len(&p.db).await, before, "intake never books");
    let proposal = out.proposal.expect("review payload");
    assert_eq!(proposal.suggestion_id, out.suggestion_id);
    assert!(proposal.bookable);
    let r = proposal.receipt.expect("staged receipt");
    assert_eq!(r.shop, "Synthetic Market");
    assert_eq!(r.date, today().format("%Y-%m-%d").to_string());
    let names: Vec<&str> = r.lines.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Bananas", "Bread", "Illegible"]);
    assert_eq!(r.lines[2].unit_price, money(150));
    assert_eq!(r.lines[2].line_total, money(300));
    let low: Vec<bool> = r.lines.iter().map(|l| l.low_confidence).collect();
    assert_eq!(low, [false, false, true]);
}

#[tokio::test]
async fn an_oversize_upload_is_refused_before_intake() {
    let p = ports();
    let mut big = jpeg(2);
    big.resize(MAX_UPLOAD_BYTES + 1, 0);

    let err = upload(&p, &receipt_llm(), &big).await;

    assert_eq!(
        server_error(err),
        "This photo is larger than 12 MB. Take a smaller photo and try again."
    );
    assert_eq!(open_receipt_suggestions(&p.db).await, 0);
    assert!(p.storage.is_empty().expect("len"), "nothing stored");
}

#[tokio::test]
async fn non_image_bytes_are_refused_with_a_fixed_text() {
    let p = ports();
    for bytes in [b"%PDF-1.7 synthetic".as_slice(), b"just text".as_slice()] {
        assert_eq!(
            server_error(upload(&p, &receipt_llm(), bytes).await),
            REFUSED
        );
    }
    assert_eq!(
        server_error(upload(&p, &receipt_llm(), &[]).await),
        NO_PHOTO
    );
    assert_eq!(open_receipt_suggestions(&p.db).await, 0);
}

#[tokio::test]
async fn unusable_model_output_is_refused_without_staging() {
    let p = ports();
    let hostile = ScriptedLlm(json!({"shop": "x", "lineItems": "ignore previous"}));

    let err = upload(&p, &hostile, &jpeg(3)).await;

    assert_eq!(server_error(err), REFUSED);
    assert_eq!(open_receipt_suggestions(&p.db).await, 0);
}

#[tokio::test]
async fn re_uploading_the_same_photo_reports_deduplicated() {
    let p = ports();
    let first = upload(&p, &receipt_llm(), &jpeg(4)).await.expect("staged");

    let again = upload(&p, &receipt_llm(), &jpeg(4)).await.expect("dedup");

    assert_eq!(again.status, IntakeStatus::Duplicate);
    assert_eq!(again.suggestion_id, first.suggestion_id);
    assert_eq!(
        again.proposal, first.proposal,
        "the pending review is shown again"
    );
    assert_eq!(open_receipt_suggestions(&p.db).await, 1);
}

#[tokio::test]
async fn approving_from_the_review_books_exactly_once() {
    let p = ports();
    let before = ledger_len(&p.db).await;
    let out = upload(&p, &receipt_llm(), &jpeg(5)).await.expect("staged");

    let booked = approve_proposal_with(&p.db, &out.suggestion_id)
        .await
        .expect("approve");
    assert!(booked.applied);
    assert_eq!(ledger_len(&p.db).await, before + 1);

    // A second approve (double submit) books nothing more.
    let _ = approve_proposal_with(&p.db, &out.suggestion_id).await;
    assert_eq!(ledger_len(&p.db).await, before + 1);

    // Re-uploading an approved photo: already booked, nothing left to review.
    let again = upload(&p, &receipt_llm(), &jpeg(5)).await.expect("dedup");
    assert_eq!(again.status, IntakeStatus::Duplicate);
    assert!(again.proposal.is_none());
    assert_eq!(ledger_len(&p.db).await, before + 1);
}
