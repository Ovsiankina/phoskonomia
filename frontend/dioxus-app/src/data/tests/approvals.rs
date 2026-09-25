//! `data::approvals`: the AI approval queue — list, approve, reject and
//! bulk-approve staged receipt proposals. Writes run on a fresh seeded store.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_db_memory::MemoryDb;
use phosk_model::{AiSuggestion, LineItem, Provenance, Receipt, ReceiptProposal, Source};

use super::support::{fresh_db, money, server_error};
use crate::data::approvals::{
    approve_error_text, approve_proposal_with, approve_receipt_proposals_with, get_proposal_with,
    line_mismatch, list_pending_proposals, list_pending_proposals_with, reject_error_text,
    reject_proposal_with, Booked, ReceiptGroupDto, BOOKED, HALF_DONE, REFUSED, RETRY, UNCONFIRMED,
};

const SLUG: &str = "rcpt:synthetic-a";

fn line(name: &str, cents: i64, confidence: f64) -> LineItem {
    LineItem {
        id: Default::default(),
        receipt_id: Default::default(),
        name: name.to_owned(),
        qty: 1.0,
        unit_price: money(cents),
        line_total: money(cents),
        category: "Groceries".to_owned(),
        signal_id: None,
        provenance: Provenance {
            source: Source::LlmInferred,
            confidence,
        },
    }
}

/// A valid model-shaped proposal (Ocr receipt, one low-confidence line).
fn proposal(slug: &str) -> ReceiptProposal {
    let receipt = Receipt {
        id: Default::default(),
        slug: slug.to_owned(),
        shop: "Synthetic Market".to_owned(),
        date: NaiveDate::from_ymd_opt(2026, 6, 17).expect("date"),
        category: "Groceries".to_owned(),
        amount: money(865),
        fixed: false,
        provenance: Provenance {
            source: Source::Ocr,
            confidence: 0.82,
        },
        source_kind: "PHOTO".to_owned(),
        ocr_engine: "OCR".to_owned(),
        ocr_regions: 3,
    };
    let lines = [
        line("Bananas", 245, 0.95),
        line("Bread", 320, 0.91),
        line("Illegible", 300, 0.40),
    ]
    .into_iter()
    .map(|l| LineItem {
        receipt_id: receipt.id,
        ..l
    })
    .collect();
    ReceiptProposal {
        suggestion_id: Default::default(),
        receipt,
        line_items: lines,
    }
}

const GONE: &str = "This proposal is no longer pending. Refresh to see the current queue.";

/// Stage `p` and enqueue its open `receipt` suggestion, as intake does.
async fn enqueue(db: &MemoryDb, p: &ReceiptProposal) -> String {
    db.stage_receipt_proposal(p.clone()).await.expect("stage");
    enqueue_only(db, p).await
}

/// Enqueue `p`'s open `receipt` suggestion WITHOUT staging its payload.
async fn enqueue_only(db: &MemoryDb, p: &ReceiptProposal) -> String {
    db.enqueue_suggestion(AiSuggestion {
        id: p.suggestion_id,
        kind: "receipt".to_owned(),
        text: "Import receipt".to_owned(),
        confidence: p.receipt.provenance.confidence,
        target: Some(p.receipt.slug.clone()),
        estimated_savings: None,
        status: "open".to_owned(),
    })
    .await
    .expect("enqueue");
    p.suggestion_id.to_string()
}

async fn ledger_len(db: &MemoryDb) -> usize {
    db.all_receipts().await.expect("receipts").len()
}

async fn queue(db: &MemoryDb) -> Vec<ReceiptGroupDto> {
    list_pending_proposals_with(db).await.expect("queue")
}

#[tokio::test]
async fn the_seed_has_no_receipt_proposals_pending() {
    // Read-only against the global (hermetic) stack; seeded non-receipt
    // suggestions have no ledger effect and are not in the queue.
    assert!(list_pending_proposals().await.expect("list").is_empty());
}

#[tokio::test]
async fn a_staged_proposal_is_listed_with_its_lines() {
    let db = fresh_db();
    let id = enqueue(&db, &proposal(SLUG)).await;

    let groups = queue(&db).await;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].receipt_slug, SLUG);
    let p = &groups[0].proposals[0];
    assert_eq!(p.suggestion_id, id);
    assert!(p.bookable);
    let r = p.receipt.as_ref().expect("proposal inline");
    assert_eq!(r.shop, "Synthetic Market");
    assert_eq!(r.date, "2026-06-17");
    assert_eq!(r.total, money(865));
    let names: Vec<&str> = r.lines.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Bananas", "Bread", "Illegible"]);
    assert_eq!(r.lines[1].unit_price, money(320));
    let low: Vec<bool> = r.lines.iter().map(|l| l.low_confidence).collect();
    assert_eq!(low, [false, false, true], "the shared < 0.7 rule");
}

#[tokio::test]
async fn approve_books_exactly_once() {
    let db = fresh_db();
    let before = ledger_len(&db).await;
    let id = enqueue(&db, &proposal(SLUG)).await;
    assert_eq!(ledger_len(&db).await, before, "queued is not booked");

    let first = approve_proposal_with(&db, &id).await.expect("approve");
    assert!(first.applied);
    assert_eq!(first.receipt_slug, SLUG);
    assert_eq!(ledger_len(&db).await, before + 1);
    let booked = db.receipt_by_slug(SLUG).await.expect("booked");
    assert_eq!(booked.amount, money(865));

    let again = approve_proposal_with(&db, &id).await.expect("idempotent");
    assert!(!again.applied, "a second approve writes nothing");
    assert_eq!(ledger_len(&db).await, before + 1);
    assert!(queue(&db).await.is_empty(), "approved leaves the queue");
}

#[tokio::test]
async fn reject_leaves_the_ledger_unchanged() {
    let db = fresh_db();
    let before = db.all_receipts().await.expect("receipts");
    let id = enqueue(&db, &proposal(SLUG)).await;

    reject_proposal_with(&db, &id).await.expect("reject");
    reject_proposal_with(&db, &id)
        .await
        .expect("rejecting twice is a no-op");
    assert_eq!(db.all_receipts().await.expect("receipts"), before);
    assert!(db.receipt_by_slug(SLUG).await.is_err());
    assert!(queue(&db).await.is_empty(), "rejected leaves the queue");

    let msg = server_error(approve_proposal_with(&db, &id).await);
    assert!(msg.contains("can't be booked"), "{msg}");
    assert_eq!(db.all_receipts().await.expect("receipts"), before);
}

#[tokio::test]
async fn an_applied_proposal_cannot_be_rejected() {
    let db = fresh_db();
    let id = enqueue(&db, &proposal(SLUG)).await;
    approve_proposal_with(&db, &id).await.expect("approve");
    let msg = server_error(reject_proposal_with(&db, &id).await);
    assert_eq!(
        msg,
        "This proposal is already booked. Correct the transaction instead."
    );
}

#[tokio::test]
async fn bulk_approve_books_the_receipt() {
    let db = fresh_db();
    let before = ledger_len(&db).await;
    let other = enqueue(&db, &proposal("rcpt:synthetic-b")).await;
    let id = enqueue(&db, &proposal(SLUG)).await;

    let outcomes = approve_receipt_proposals_with(&db, SLUG)
        .await
        .expect("bulk");
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].applied);
    assert_eq!(ledger_len(&db).await, before + 1);

    let left = queue(&db).await;
    assert_eq!(left.len(), 1, "only the other receipt is still pending");
    assert_eq!(left[0].proposals[0].suggestion_id, other);
    assert_ne!(other, id);
}

#[tokio::test]
async fn an_invalid_proposal_is_refused_with_a_user_safe_message() {
    let db = fresh_db();
    let before = ledger_len(&db).await;
    let mut p = proposal(SLUG);
    p.receipt.amount = money(999); // ≠ Σ lines
    let id = enqueue(&db, &p).await;

    let listed = queue(&db).await;
    assert!(!listed[0].proposals[0].bookable, "flagged before approval");

    let fixed = REFUSED;
    let single = server_error(approve_proposal_with(&db, &id).await);
    let bulk = server_error(approve_receipt_proposals_with(&db, SLUG).await);
    assert_eq!(single, fixed);
    assert_eq!(bulk, fixed);
    assert_eq!(ledger_len(&db).await, before, "nothing booked");
    assert_eq!(queue(&db).await.len(), 1, "still pending for a reject");
}

#[tokio::test]
async fn malformed_and_unknown_ids_get_fixed_texts() {
    let db = fresh_db();
    let hostile = "<script>x</script>\u{7}";
    let msg = server_error(approve_proposal_with(&db, hostile).await);
    assert_eq!(msg, "That is not a valid proposal id.");
    let msg = server_error(reject_proposal_with(&db, hostile).await);
    assert_eq!(msg, "That is not a valid proposal id.");
    let msg = server_error(approve_receipt_proposals_with(&db, hostile).await);
    assert_eq!(msg, "That is not a valid receipt.");

    let unknown = "00000000-0000-4000-8000-000000000000";
    let msg = server_error(approve_proposal_with(&db, unknown).await);
    assert_eq!(
        msg,
        "This proposal is no longer pending. Refresh to see the current queue."
    );
    let msg = server_error(approve_receipt_proposals_with(&db, "rcpt:none").await);
    assert_eq!(
        msg,
        "This proposal is no longer pending. Refresh to see the current queue."
    );
}

#[tokio::test]
async fn hostile_model_text_is_bounded() {
    let db = fresh_db();
    let mut p = proposal(SLUG);
    p.receipt.shop = format!("{}\u{1b}[31m", "A".repeat(10_000));
    p.line_items[0].name = "Bana\u{202E}sen\u{200B}ab".to_owned();
    enqueue(&db, &p).await;

    let groups = queue(&db).await;
    let r = groups[0].proposals[0].receipt.as_ref().expect("inline");
    assert!(r.shop.chars().count() <= 201, "clipped (+ ellipsis)");
    assert!(!r.shop.chars().any(char::is_control), "no control chars");
    assert_eq!(r.lines[0].name, "Banasenab", "bidi + zero-width stripped");
    assert!(!groups[0].proposals[0].bookable);

    // The bulk-approve slug check uses the same rule as the service.
    let msg = server_error(approve_receipt_proposals_with(&db, "rcpt:a\u{202E}b").await);
    assert_eq!(msg, "That is not a valid receipt.");
}

#[tokio::test]
async fn every_line_shows_the_amount_it_books_and_mismatches_are_flagged() {
    let db = fresh_db();
    let mut p = proposal(SLUG);
    // Reads as "1 × CHF 2.45" but books CHF 245.00; the total still adds up,
    // so the service would book it.
    p.line_items[0].line_total = money(24_500);
    p.receipt.amount = money(24_500 + 320 + 300);
    enqueue(&db, &p).await;

    let groups = queue(&db).await;
    let prop = &groups[0].proposals[0];
    assert!(prop.bookable, "validation alone does not catch it");
    let r = prop.receipt.as_ref().expect("inline");
    assert_eq!(r.lines[0].line_total, money(24_500));
    assert_eq!(r.lines[0].unit_price, money(245));
    let flags: Vec<bool> = r.lines.iter().map(|l| l.mismatch).collect();
    assert_eq!(flags, [true, false, false]);
}

#[test]
fn line_mismatch_rounds_once_to_centimes_with_one_centime_tolerance() {
    assert!(!line_mismatch(1.0, money(245), money(245)));
    assert!(line_mismatch(1.0, money(245), money(24_500)));
    // 3 × 3.33 = 9.99 vs 10.00: within one centime.
    assert!(!line_mismatch(3.0, money(333), money(1_000)));
    assert!(line_mismatch(2.0, money(100), money(203)));
    // 0.5 × 2.45 = 1.225 → 1.23 (half away from zero).
    assert!(!line_mismatch(0.5, money(245), money(123)));
    assert!(!line_mismatch(0.5, money(245), money(122)), "tolerance");
    assert!(line_mismatch(0.5, money(245), money(121)));
    assert!(line_mismatch(f64::NAN, money(245), money(245)));
    assert!(line_mismatch(f64::INFINITY, money(245), money(245)));
}

#[test]
fn approve_failures_never_claim_what_the_ledger_does_not_show() {
    let storage = PhoskError::Invalid("db /var/lib/phosk.db locked".to_owned());
    // `apply` inserted the receipt, then the status update failed.
    assert_eq!(approve_error_text(&storage, Booked::Yes), HALF_DONE);
    assert!(!HALF_DONE.contains("Nothing was booked"));
    assert_eq!(approve_error_text(&storage, Booked::Unknown), UNCONFIRMED);
    assert_eq!(approve_error_text(&storage, Booked::No), REFUSED);
    let gone = PhoskError::NotFound("suggestion".to_owned());
    assert_eq!(approve_error_text(&gone, Booked::No), GONE);
    assert_eq!(approve_error_text(&gone, Booked::Yes), HALF_DONE);
    let overflow = PhoskError::Overflow("sum".to_owned());
    assert_eq!(approve_error_text(&overflow, Booked::No), RETRY);
}

#[test]
fn reject_says_booked_only_for_an_accepted_proposal() {
    let refused = PhoskError::Invalid("already applied".to_owned());
    assert_eq!(reject_error_text(&refused, "accepted"), BOOKED);
    // Any other `Invalid` is a storage failure, not a ledger fact.
    assert_eq!(reject_error_text(&refused, "open"), RETRY);
    assert_eq!(
        reject_error_text(&PhoskError::NotFound("x".to_owned()), "open"),
        GONE
    );
}

#[tokio::test]
async fn conflicting_proposals_are_refused_single_and_bulk() {
    let db = fresh_db();
    let before = ledger_len(&db).await;
    let a = enqueue(&db, &proposal(SLUG)).await;
    let b = enqueue(&db, &proposal(SLUG)).await;
    assert_eq!(queue(&db).await[0].proposals.len(), 2);

    for id in [&a, &b] {
        let msg = server_error(approve_proposal_with(&db, id).await);
        assert_eq!(msg, REFUSED);
    }
    let msg = server_error(approve_receipt_proposals_with(&db, SLUG).await);
    assert_eq!(msg, REFUSED);
    assert_eq!(ledger_len(&db).await, before, "nothing booked");
    assert!(db.receipt_by_slug(SLUG).await.is_err());
    assert_eq!(queue(&db).await[0].proposals.len(), 2, "both still open");
}

#[tokio::test]
async fn a_non_receipt_suggestion_is_gone_and_not_dismissed() {
    let db = fresh_db();
    let seeded = db
        .ai_suggestions()
        .await
        .expect("suggestions")
        .into_iter()
        .find(|s| s.kind != "receipt" && s.status == "open")
        .expect("the seed has a non-receipt suggestion");
    let id = seeded.id.to_string();

    assert_eq!(server_error(approve_proposal_with(&db, &id).await), GONE);
    assert_eq!(server_error(reject_proposal_with(&db, &id).await), GONE);
    assert_eq!(server_error(get_proposal_with(&db, &id).await), GONE);
    let after = db
        .ai_suggestions()
        .await
        .expect("suggestions")
        .into_iter()
        .find(|s| s.id == seeded.id)
        .expect("still there");
    assert_eq!(after.status, "open", "not dismissed");
}

#[tokio::test]
async fn a_missing_payload_is_listed_unbookable() {
    let db = fresh_db();
    let id = enqueue_only(&db, &proposal(SLUG)).await;

    let groups = queue(&db).await;
    let p = &groups[0].proposals[0];
    assert_eq!(p.suggestion_id, id);
    assert_eq!(p.receipt, None);
    assert!(!p.bookable);
    assert_eq!(get_proposal_with(&db, &id).await.expect("get"), *p);
}

#[tokio::test]
async fn get_proposal_reads_one_open_proposal() {
    let db = fresh_db();
    let id = enqueue(&db, &proposal(SLUG)).await;
    let listed = queue(&db).await[0].proposals[0].clone();
    assert_eq!(get_proposal_with(&db, &id).await.expect("get"), listed);

    reject_proposal_with(&db, &id).await.expect("reject");
    assert_eq!(server_error(get_proposal_with(&db, &id).await), GONE);
}
