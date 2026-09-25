//! Integration tests for the approval service — the ONLY path from a model
//! proposal to the ledger. Driven through the PORT against an empty
//! `MemoryDb`, so every ledger count below is absolute.
#![allow(
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::missing_const_for_fn
)]

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_ai::ai_approval::{
    approve_receipt, approve_suggestion, pending_suggestions, reject_suggestion,
};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_id::{LineItemId, ReceiptId, SuggestionId};
use phosk_model::{
    AiSuggestion, BudgetConfig, LineItem, Provenance, Receipt, ReceiptProposal, Source,
};

fn empty_db() -> MemoryDb {
    MemoryDb::new(
        Vec::new(),
        Vec::new(),
        BudgetConfig {
            monthly_budget: Money::from_centimes(420_000),
            savings_target: Money::from_centimes(90_000),
        },
    )
}

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid date")
}

fn line(receipt_id: ReceiptId, name: &str, cents: i64, conf: f64) -> LineItem {
    LineItem {
        id: LineItemId::new(),
        receipt_id,
        name: name.to_owned(),
        qty: 1.0,
        unit_price: Money::from_centimes(cents),
        line_total: Money::from_centimes(cents),
        category: "Groceries".to_owned(),
        signal_id: None,
        provenance: Provenance {
            source: Source::LlmInferred,
            confidence: conf,
        },
    }
}

/// A model-shaped proposal: Ocr receipt, LlmInferred lines (one low-confidence).
fn proposal(slug: &str) -> ReceiptProposal {
    let rid = ReceiptId::new();
    ReceiptProposal {
        suggestion_id: SuggestionId::new(),
        receipt: Receipt {
            id: rid,
            slug: slug.to_owned(),
            shop: "Synthetic Market".to_owned(),
            date: day(),
            category: "Groceries".to_owned(),
            amount: Money::from_centimes(865),
            fixed: false,
            provenance: Provenance {
                source: Source::Ocr,
                confidence: 0.82,
            },
            source_kind: "PHOTO".to_owned(),
            ocr_engine: "OCR".to_owned(),
            ocr_regions: 3,
        },
        line_items: vec![
            line(rid, "Bananas", 245, 0.95),
            line(rid, "Bread", 320, 0.91),
            line(rid, "Illegible", 300, 0.40),
        ],
    }
}

/// Stage `p` and enqueue its open `receipt` suggestion, as intake does.
async fn enqueue(db: &MemoryDb, p: &ReceiptProposal) {
    db.stage_receipt_proposal(p.clone()).await.expect("stage");
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
}

async fn status(db: &MemoryDb, id: SuggestionId) -> String {
    db.ai_suggestions()
        .await
        .expect("suggestions")
        .into_iter()
        .find(|s| s.id == id)
        .expect("suggestion present")
        .status
}

async fn ledger_len(db: &MemoryDb) -> usize {
    db.all_receipts().await.expect("receipts").len()
}

#[tokio::test]
async fn approve_applies_the_proposal_with_model_provenance_preserved() {
    let db = empty_db();
    let p = proposal("rcpt:a");
    enqueue(&db, &p).await;
    assert_eq!(ledger_len(&db).await, 0, "enqueued ≠ booked");

    let out = approve_suggestion(&db, p.suggestion_id)
        .await
        .expect("approve");
    assert!(out.applied);
    assert_eq!(out.receipt_slug, "rcpt:a");
    assert_eq!(status(&db, p.suggestion_id).await, "accepted");

    let r = db.receipt_by_slug("rcpt:a").await.expect("booked");
    assert_eq!(r.amount.centimes(), 865);
    assert_eq!(
        r.provenance, p.receipt.provenance,
        "Ocr kept, not relabelled"
    );
    let lines = db.line_items(r.id).await.expect("lines");
    assert_eq!(lines.len(), 3);
    assert!(
        lines
            .iter()
            .all(|l| l.provenance.source == Source::LlmInferred),
        "line provenance stays LlmInferred"
    );
    assert!(lines.iter().any(|l| l.provenance.is_low_confidence()));
    let txs = db.transactions_between(day(), day()).await.expect("txs");
    assert_eq!(txs.len(), 1, "the spend is projected once");
}

#[tokio::test]
async fn approving_twice_never_double_books() {
    let db = empty_db();
    let p = proposal("rcpt:b");
    enqueue(&db, &p).await;
    approve_suggestion(&db, p.suggestion_id).await.expect("1st");
    let again = approve_suggestion(&db, p.suggestion_id).await.expect("2nd");
    assert!(!again.applied, "second approval is a no-op");
    assert_eq!(ledger_len(&db).await, 1);
    let txs = db.transactions_between(day(), day()).await.expect("txs");
    assert_eq!(txs.len(), 1);
}

#[tokio::test]
async fn reject_dismisses_and_never_touches_the_ledger() {
    let db = empty_db();
    let p = proposal("rcpt:c");
    enqueue(&db, &p).await;
    reject_suggestion(&db, p.suggestion_id)
        .await
        .expect("reject");
    assert_eq!(status(&db, p.suggestion_id).await, "dismissed");
    reject_suggestion(&db, p.suggestion_id)
        .await
        .expect("rejecting twice is a no-op");
    assert_eq!(ledger_len(&db).await, 0);
    let res = approve_suggestion(&db, p.suggestion_id).await;
    assert!(matches!(res, Err(PhoskError::Invalid(_))), "{res:?}");
    assert_eq!(ledger_len(&db).await, 0);
}

#[tokio::test]
async fn an_applied_suggestion_cannot_be_rejected() {
    let db = empty_db();
    let p = proposal("rcpt:d");
    enqueue(&db, &p).await;
    approve_suggestion(&db, p.suggestion_id)
        .await
        .expect("approve");
    let res = reject_suggestion(&db, p.suggestion_id).await;
    assert!(matches!(res, Err(PhoskError::Invalid(_))), "{res:?}");
    assert_eq!(status(&db, p.suggestion_id).await, "accepted");
}

#[tokio::test]
async fn pending_lists_open_suggestions_grouped_per_receipt() {
    let db = empty_db();
    let (a1, a2, b) = (proposal("rcpt:x"), proposal("rcpt:x"), proposal("rcpt:y"));
    for p in [&a1, &a2, &b] {
        enqueue(&db, p).await;
    }
    let done = proposal("rcpt:z");
    enqueue(&db, &done).await;
    reject_suggestion(&db, done.suggestion_id)
        .await
        .expect("reject");

    let groups = pending_suggestions(&db).await.expect("pending");
    let slugs: Vec<_> = groups.iter().map(|g| g.receipt_slug.clone()).collect();
    assert_eq!(
        slugs,
        vec![Some("rcpt:x".to_owned()), Some("rcpt:y".to_owned())]
    );
    assert_eq!(groups[0].suggestions.len(), 2);
    let first = &groups[0].suggestions[0];
    assert_eq!(first.suggestion.id, a1.suggestion_id);
    assert_eq!(
        first.proposal.as_ref(),
        Some(&a1),
        "payload for the review UI"
    );
}

#[tokio::test]
async fn bulk_approve_applies_only_that_receipts_open_suggestions() {
    let db = empty_db();
    let (a1, a2, b) = (proposal("rcpt:m"), proposal("rcpt:m"), proposal("rcpt:n"));
    for p in [&a1, &a2, &b] {
        enqueue(&db, p).await;
    }
    let outs = approve_receipt(&db, "rcpt:m").await.expect("bulk");
    assert_eq!(outs.len(), 2);
    assert_eq!(status(&db, a1.suggestion_id).await, "accepted");
    assert_eq!(status(&db, a2.suggestion_id).await, "accepted");
    assert_eq!(status(&db, b.suggestion_id).await, "open");
    assert_eq!(ledger_len(&db).await, 1, "same slug books one receipt");
    assert!(db.receipt_by_slug("rcpt:n").await.is_err());

    let again = approve_receipt(&db, "rcpt:m").await.expect("bulk again");
    assert!(again.is_empty(), "nothing left open for that receipt");
    let unknown = approve_receipt(&db, "rcpt:none").await;
    assert!(
        matches!(unknown, Err(PhoskError::NotFound(_))),
        "{unknown:?}"
    );
}

#[tokio::test]
async fn unknown_or_unstaged_suggestions_are_not_found() {
    let db = empty_db();
    let res = approve_suggestion(&db, SuggestionId::new()).await;
    assert!(matches!(res, Err(PhoskError::NotFound(_))), "{res:?}");
    let res = reject_suggestion(&db, SuggestionId::new()).await;
    assert!(matches!(res, Err(PhoskError::NotFound(_))), "{res:?}");

    // An open receipt suggestion whose payload was never staged.
    let p = proposal("rcpt:ghost");
    db.enqueue_suggestion(AiSuggestion {
        id: p.suggestion_id,
        kind: "receipt".to_owned(),
        text: "ghost".to_owned(),
        confidence: 0.9,
        target: Some("rcpt:ghost".to_owned()),
        estimated_savings: None,
        status: "open".to_owned(),
    })
    .await
    .expect("enqueue");
    let res = approve_suggestion(&db, p.suggestion_id).await;
    assert!(matches!(res, Err(PhoskError::NotFound(_))), "{res:?}");
    assert_eq!(status(&db, p.suggestion_id).await, "open");
}

#[tokio::test]
async fn non_receipt_suggestions_cannot_be_applied_to_the_ledger() {
    let db = MemoryDb::seeded().expect("seed");
    let seeded = db.ai_suggestions().await.expect("s");
    let s = seeded.first().expect("a seeded suggestion");
    assert_ne!(s.kind, "receipt");
    let before = ledger_len(&db).await;
    let res = approve_suggestion(&db, s.id).await;
    assert!(matches!(res, Err(PhoskError::Invalid(_))), "{res:?}");
    assert_eq!(ledger_len(&db).await, before);
}

/// Proposal content is hostile: each tampered payload is refused before the
/// ledger is touched, and the suggestion stays open for the human.
#[tokio::test]
async fn hostile_proposals_are_rejected_before_the_ledger() {
    type Tamper = fn(&mut ReceiptProposal);
    let cases: Vec<(&str, Tamper)> = vec![
        ("claims user provenance", |p| {
            p.receipt.provenance = Provenance {
                source: Source::UserEntered,
                confidence: 1.0,
            };
        }),
        ("line claims user provenance", |p| {
            p.line_items[0].provenance.source = Source::UserModified;
        }),
        ("total ≠ Σ lines", |p| {
            p.receipt.amount = Money::from_centimes(1);
        }),
        ("negative price", |p| {
            p.line_items[0].unit_price = Money::from_centimes(-245);
            p.line_items[0].line_total = Money::from_centimes(-245);
            p.receipt.amount = Money::from_centimes(375);
        }),
        ("absurd amount", |p| {
            p.line_items[0].line_total = Money::from_centimes(i64::MAX / 2);
            p.receipt.amount = Money::from_centimes(i64::MAX / 2 + 620);
        }),
        ("oversize name", |p| {
            p.line_items[0].name = "x".repeat(10_000);
        }),
        ("control chars in shop", |p| {
            p.receipt.shop = "Shop\u{1b}[2J".to_owned();
        }),
        ("empty shop", |p| p.receipt.shop = "  ".to_owned()),
        ("date in 1970", |p| {
            p.receipt.date = NaiveDate::from_ymd_opt(1970, 1, 1).expect("date");
        }),
        ("non-finite qty", |p| p.line_items[0].qty = f64::NAN),
        ("confidence out of range", |p| {
            p.line_items[1].provenance.confidence = 7.0;
        }),
        ("line bound to another receipt", |p| {
            p.line_items[0].receipt_id = ReceiptId::new();
        }),
        ("no lines", |p| {
            p.line_items.clear();
            p.receipt.amount = Money::ZERO;
        }),
        ("slug differs from the suggestion target", |p| {
            p.receipt.slug = "t1".to_owned();
        }),
    ];
    for (what, tamper) in cases {
        let db = empty_db();
        let mut p = proposal("rcpt:h");
        enqueue(&db, &p).await;
        tamper(&mut p);
        db.stage_receipt_proposal(p.clone()).await.expect("restage");
        let res = approve_suggestion(&db, p.suggestion_id).await;
        assert!(
            matches!(res, Err(PhoskError::Invalid(_))),
            "{what}: {res:?}"
        );
        assert_eq!(ledger_len(&db).await, 0, "{what}: ledger untouched");
        assert_eq!(status(&db, p.suggestion_id).await, "open", "{what}");
    }
}
