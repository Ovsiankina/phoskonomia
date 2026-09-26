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
use crate::data::receipt::{
    read_capped, upload_receipt_with, IntakeStatus, ReceiptIntakeDto, MAX_UPLOAD_BYTES,
};

const NOT_JPEG_PNG: &str = "This file is not a JPEG or PNG photo. Nothing was staged. \
                            Upload a JPEG or PNG photo of one receipt.";
const UNREADABLE: &str = "Could not read a receipt from this photo. Nothing was staged. \
                          Try a clearer photo or try again later.";
const TOO_LARGE: &str = "This photo is larger than 12 MB. Take a smaller photo and try again.";
const CUT: &str = "The upload failed or went over 12 MB. Try again with a smaller photo.";
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

/// An LLM that is down: every call fails the way the Ollama adapter reports
/// a transport failure (`Invalid`).
struct DownLlm;

#[async_trait::async_trait]
impl LlmAdapter for DownLlm {
    fn model(&self) -> &str {
        "down-test"
    }
    async fn health(&self) -> Result<bool, PhoskError> {
        Ok(false)
    }
    async fn complete(&self, _prompt: &str) -> Result<String, PhoskError> {
        Err(PhoskError::Invalid("model request failed".into()))
    }
    async fn generate_structured(
        &self,
        _prompt: &str,
        _schema: &Value,
    ) -> Result<Value, PhoskError> {
        Err(PhoskError::Invalid("model request failed".into()))
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

    assert_eq!(server_error(err, 500), TOO_LARGE);
    assert_eq!(open_receipt_suggestions(&p.db).await, 0);
    assert!(p.storage.is_empty().expect("len"), "nothing stored");
}

#[tokio::test]
async fn non_image_bytes_are_refused_with_a_fixed_text() {
    let p = ports();
    for bytes in [b"%PDF-1.7 synthetic".as_slice(), b"just text".as_slice()] {
        assert_eq!(
            server_error(upload(&p, &receipt_llm(), bytes).await, 500),
            NOT_JPEG_PNG
        );
    }
    assert_eq!(
        server_error(upload(&p, &receipt_llm(), &[]).await, 500),
        NO_PHOTO
    );
    assert_eq!(open_receipt_suggestions(&p.db).await, 0);
}

#[tokio::test]
async fn unusable_model_output_is_refused_without_staging() {
    let p = ports();
    let hostile = ScriptedLlm(json!({"shop": "x", "lineItems": "ignore previous"}));

    let err = upload(&p, &hostile, &jpeg(3)).await;

    assert_eq!(server_error(err, 500), UNREADABLE);
    assert_eq!(open_receipt_suggestions(&p.db).await, 0);
}

#[tokio::test]
async fn a_model_outage_does_not_blame_the_photo() {
    let p = ports();

    let err = upload(&p, &DownLlm, &jpeg(6)).await;

    assert_eq!(server_error(err, 500), UNREADABLE);
    assert_eq!(open_receipt_suggestions(&p.db).await, 0);
}

/// A little-endian TIFF whose IFD0 points at an Exif IFD holding a GPS tag.
fn tiff_with_exif() -> Vec<u8> {
    let mut v = b"II*\0".to_vec();
    v.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset
    v.extend_from_slice(&1u16.to_le_bytes()); // one entry
    v.extend_from_slice(&0x8825u16.to_le_bytes()); // GPSInfo IFD pointer
    v.extend_from_slice(&4u16.to_le_bytes()); // LONG
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend_from_slice(&26u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    v.extend_from_slice(b"GPS 47.3769N 8.5417E synthetic");
    v
}

/// A RIFF/WebP container carrying an `EXIF` chunk.
fn webp_with_exif() -> Vec<u8> {
    let exif = b"Exif\0\0MM\0*GPS 47.3769N 8.5417E synthetic";
    let mut body = b"WEBPVP8X".to_vec();
    body.extend_from_slice(&10u32.to_le_bytes());
    body.extend_from_slice(&[0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // EXIF flag
    body.extend_from_slice(b"EXIF");
    body.extend_from_slice(&u32::try_from(exif.len()).expect("len").to_le_bytes());
    body.extend_from_slice(exif);
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&u32::try_from(body.len()).expect("len").to_le_bytes());
    v.extend_from_slice(&body);
    v
}

#[tokio::test]
async fn images_intake_cannot_strip_are_refused_before_storage() {
    let p = ports();
    for (kind, bytes) in [("tiff", tiff_with_exif()), ("webp", webp_with_exif())] {
        assert_eq!(
            server_error(upload(&p, &receipt_llm(), &bytes).await, 500),
            NOT_JPEG_PNG,
            "{kind}"
        );
    }
    assert!(p.storage.is_empty().expect("len"), "nothing stored");
    assert_eq!(open_receipt_suggestions(&p.db).await, 0);
}

#[tokio::test]
async fn the_review_shows_the_amount_each_line_books() {
    let p = ports();
    let llm = ScriptedLlm(json!({
        "shop": "Synthetic Market",
        "category": "Groceries",
        "lineItems": [
            {"name": "Cheese", "qty": 1.5, "unitPriceCentimes": 101, "confidence": 0.9},
            {"name": "Eggs", "qty": 2.0, "unitPriceCentimes": 245, "confidence": 0.9}
        ]
    }));

    let out = upload(&p, &llm, &jpeg(7)).await.expect("staged");

    let r = out.proposal.expect("review").receipt.expect("receipt");
    let totals: Vec<_> = r.lines.iter().map(|l| l.line_total).collect();
    assert_eq!(totals, [money(152), money(490)]);
    assert!(
        r.lines.iter().all(|l| !l.mismatch),
        "backend-derived totals"
    );
    assert_eq!(r.total, money(642));
    assert!(!out.conflicting);
}

#[tokio::test]
async fn a_rival_open_proposal_marks_the_review_conflicting() {
    let p = ports();
    let first = upload(&p, &receipt_llm(), &jpeg(8)).await.expect("staged");
    let mine =
        p.db.ai_suggestions()
            .await
            .expect("suggestions")
            .into_iter()
            .find(|s| s.id.to_string() == first.suggestion_id)
            .expect("own suggestion");
    p.db.enqueue_suggestion(phosk_model::AiSuggestion {
        id: Default::default(),
        ..mine
    })
    .await
    .expect("rival");

    let again = upload(&p, &receipt_llm(), &jpeg(8)).await.expect("dedup");

    assert!(again.proposal.is_some());
    assert!(again.conflicting);
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

fn stream(declared: Option<u64>, body: Vec<u8>) -> dioxus::fullstack::FileStream {
    use dioxus::fullstack::body::Body;
    dioxus::fullstack::FileStream::from_raw(
        "photo.jpg".into(),
        declared,
        "image/jpeg".into(),
        Body::from(body).into_data_stream(),
    )
}

#[tokio::test]
async fn the_streaming_read_stops_at_the_cap() {
    let ok = read_capped(stream(None, jpeg(9))).await.expect("small");
    assert_eq!(ok.as_ref(), jpeg(9).as_slice());

    let at_cap = read_capped(stream(None, vec![0; MAX_UPLOAD_BYTES])).await;
    assert_eq!(at_cap.expect("at cap").len(), MAX_UPLOAD_BYTES);

    let over = read_capped(stream(None, vec![0; MAX_UPLOAD_BYTES + 1])).await;
    assert_eq!(server_error(over, 500), CUT);
}

#[tokio::test]
async fn a_declared_oversize_is_refused_before_reading() {
    let declared = u64::try_from(MAX_UPLOAD_BYTES + 1).expect("fits");
    let err = read_capped(stream(Some(declared), jpeg(10))).await;
    assert_eq!(server_error(err, 500), TOO_LARGE);
}
