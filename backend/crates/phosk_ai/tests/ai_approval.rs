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
async fn bulk_approve_applies_only_that_receipts_open_suggestion() {
    let db = empty_db();
    let (a, b) = (proposal("rcpt:m"), proposal("rcpt:n"));
    for p in [&a, &b] {
        enqueue(&db, p).await;
    }
    let outs = approve_receipt(&db, "rcpt:m").await.expect("bulk");
    assert_eq!(outs.len(), 1);
    assert!(outs[0].applied);
    assert_eq!(status(&db, a.suggestion_id).await, "accepted");
    assert_eq!(status(&db, b.suggestion_id).await, "open");
    assert_eq!(ledger_len(&db).await, 1);
    assert!(db.receipt_by_slug("rcpt:n").await.is_err());

    let again = approve_receipt(&db, "rcpt:m").await.expect("bulk again");
    assert!(again.is_empty(), "nothing left open for that receipt");
    let unknown = approve_receipt(&db, "rcpt:none").await;
    assert!(
        matches!(unknown, Err(PhoskError::NotFound(_))),
        "{unknown:?}"
    );
}

/// Two open proposals for one receipt would book only the last while marking
/// both applied — a false audit trail. Both entry points refuse instead.
#[tokio::test]
async fn conflicting_open_proposals_for_one_receipt_are_refused() {
    let db = empty_db();
    let (a1, a2) = (proposal("rcpt:m"), proposal("rcpt:m"));
    for p in [&a1, &a2] {
        enqueue(&db, p).await;
    }
    let res = approve_receipt(&db, "rcpt:m").await;
    assert!(matches!(res, Err(PhoskError::Invalid(_))), "{res:?}");
    let res = approve_suggestion(&db, a1.suggestion_id).await;
    assert!(matches!(res, Err(PhoskError::Invalid(_))), "{res:?}");
    assert_eq!(ledger_len(&db).await, 0);
    assert_eq!(status(&db, a1.suggestion_id).await, "open");
    assert_eq!(status(&db, a2.suggestion_id).await, "open");

    // Rejecting one resolves the conflict.
    reject_suggestion(&db, a1.suggestion_id)
        .await
        .expect("reject");
    let outs = approve_receipt(&db, "rcpt:m").await.expect("bulk");
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0].suggestion_id, a2.suggestion_id);
}

/// Validation runs before anything is written: the receipt's only open
/// proposal fails the schema, so nothing is booked or marked (a dismissed
/// proposal for the same slug is ignored, not applied instead).
#[tokio::test]
async fn bulk_approve_applies_nothing_when_one_open_proposal_is_invalid() {
    let db = empty_db();
    let (good, mut bad) = (proposal("rcpt:m"), proposal("rcpt:m"));
    enqueue(&db, &good).await;
    reject_suggestion(&db, good.suggestion_id)
        .await
        .expect("reject");
    enqueue(&db, &bad).await;
    bad.receipt.amount = Money::from_centimes(1);
    db.stage_receipt_proposal(bad.clone())
        .await
        .expect("restage");
    let res = approve_receipt(&db, "rcpt:m").await;
    assert!(matches!(res, Err(PhoskError::Invalid(_))), "{res:?}");
    assert_eq!(ledger_len(&db).await, 0);
    assert_eq!(status(&db, good.suggestion_id).await, "dismissed");
    assert_eq!(status(&db, bad.suggestion_id).await, "open");
}

/// Book `p` on a fresh ledger; the booked receipt id and its sorted line ids.
async fn booked_ids(p: &ReceiptProposal) -> (ReceiptId, Vec<String>) {
    let db = empty_db();
    enqueue(&db, p).await;
    approve_suggestion(&db, p.suggestion_id)
        .await
        .expect("approve");
    let r = db.receipt_by_slug(&p.receipt.slug).await.expect("booked");
    let mut lines: Vec<String> = db
        .line_items(r.id)
        .await
        .expect("lines")
        .iter()
        .map(|l| l.id.to_string())
        .collect();
    lines.sort_unstable();
    (r.id, lines)
}

/// Two applications of one proposal (e.g. a double-clicked APPROVE racing past
/// the booked-slug check) write the SAME rows, so the second overwrites the
/// first instead of booking a duplicate. The ids depend on the suggestion id
/// only — never on the staged (untrusted) ids.
#[tokio::test]
async fn the_same_proposal_always_books_the_same_ids() {
    let p = proposal("rcpt:same");
    let first = booked_ids(&p).await;
    assert_eq!(booked_ids(&p).await, first, "deterministic");

    let mut restaged = p.clone();
    restaged.receipt.id = ReceiptId::new();
    for l in &mut restaged.line_items {
        l.id = LineItemId::new();
        l.receipt_id = restaged.receipt.id;
    }
    assert_eq!(booked_ids(&restaged).await, first, "staged ids ignored");

    let other = proposal("rcpt:same");
    let (other_id, other_lines) = booked_ids(&other).await;
    assert_ne!(other_id, first.0, "another suggestion, another receipt id");
    assert!(other_lines.iter().all(|l| !first.1.contains(l)));
}

/// The losing call of a concurrent double-approve replays the same write:
/// one receipt, one dashboard projection, one set of lines.
#[tokio::test]
async fn a_replayed_approval_write_is_one_booking() {
    let db = empty_db();
    let p = proposal("rcpt:race");
    enqueue(&db, &p).await;
    approve_suggestion(&db, p.suggestion_id)
        .await
        .expect("approve");
    let r = db.receipt_by_slug("rcpt:race").await.expect("booked");
    let lines = db.line_items(r.id).await.expect("lines");

    let again = db
        .insert_receipt(r.clone(), lines.clone())
        .await
        .expect("replay");
    assert_eq!(again, r.id);
    assert_eq!(ledger_len(&db).await, 1);
    assert_eq!(
        db.transactions_between(day(), day())
            .await
            .expect("tx")
            .len(),
        1
    );
    assert_eq!(db.line_items(r.id).await.expect("lines").len(), lines.len());
}

/// Once a receipt is booked (and possibly user-corrected), no proposal ever
/// re-inserts it: a later proposal for the slug, or a re-approval after the
/// status write failed, is accepted with `applied == false`.
#[tokio::test]
async fn an_already_booked_receipt_is_never_overwritten() {
    let db = empty_db();
    let first = proposal("rcpt:k");
    enqueue(&db, &first).await;
    approve_suggestion(&db, first.suggestion_id)
        .await
        .expect("approve");
    let booked = db.receipt_by_slug("rcpt:k").await.expect("booked");

    // Re-approve after `insert_receipt` succeeded but the status write failed.
    db.update_suggestion_status(first.suggestion_id, "open")
        .await
        .expect("reopen");
    let out = approve_suggestion(&db, first.suggestion_id)
        .await
        .expect("re-approve");
    assert!(!out.applied);
    assert_eq!(status(&db, first.suggestion_id).await, "accepted");

    // A second, different proposal for the same slug.
    let mut second = proposal("rcpt:k");
    second.line_items.truncate(1);
    second.receipt.amount = second.line_items[0].line_total;
    enqueue(&db, &second).await;
    let outs = approve_receipt(&db, "rcpt:k").await.expect("bulk");
    assert_eq!(outs.len(), 1);
    assert!(!outs[0].applied, "booked receipt is never replaced");
    assert_eq!(status(&db, second.suggestion_id).await, "accepted");

    assert_eq!(db.receipt_by_slug("rcpt:k").await.expect("still"), booked);
    assert_eq!(db.line_items(booked.id).await.expect("lines").len(), 3);
    assert_eq!(ledger_len(&db).await, 1);
}

/// The `rcpt:` prefix guard: a proposal whose slug and target both name a
/// seeded receipt cannot replace it.
#[tokio::test]
async fn a_proposal_can_never_target_a_seeded_receipt() {
    let db = MemoryDb::seeded().expect("seed");
    let seeded = db.receipt_by_slug("t1").await.expect("t1 seeded");
    let seeded_lines = db.line_items(seeded.id).await.expect("lines");
    let p = proposal("t1");
    enqueue(&db, &p).await;
    let res = approve_suggestion(&db, p.suggestion_id).await;
    assert!(matches!(res, Err(PhoskError::Invalid(_))), "{res:?}");
    assert_eq!(db.receipt_by_slug("t1").await.expect("t1"), seeded);
    assert_eq!(db.line_items(seeded.id).await.expect("lines"), seeded_lines);
}

/// Staged ids and ledger flags are not trusted: staged ids aimed at a stored
/// receipt and line are ignored (the booked ids are derived from the
/// suggestion id), the stored rows stay untouched, and `fixed` / `signal_id`
/// are dropped.
#[tokio::test]
async fn model_supplied_ids_and_flags_are_not_trusted() {
    let db = MemoryDb::seeded().expect("seed");
    let seeded = db.receipt_by_slug("t1").await.expect("t1 seeded");
    let seeded_lines = db.line_items(seeded.id).await.expect("lines");
    let mut p = proposal("rcpt:ids");
    p.receipt.id = seeded.id;
    p.receipt.fixed = true;
    for l in &mut p.line_items {
        l.receipt_id = seeded.id;
    }
    p.line_items[0].id = seeded_lines[0].id;
    p.line_items[0].signal_id = Some(phosk_id::SignalId::new());
    enqueue(&db, &p).await;
    approve_suggestion(&db, p.suggestion_id)
        .await
        .expect("approve");

    let r = db.receipt_by_slug("rcpt:ids").await.expect("booked");
    assert_ne!(r.id, seeded.id);
    assert!(!r.fixed);
    let lines = db.line_items(r.id).await.expect("lines");
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|l| l.id != seeded_lines[0].id));
    assert!(lines.iter().all(|l| l.signal_id.is_none()));
    assert_eq!(db.line_items(seeded.id).await.expect("lines"), seeded_lines);
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
        ("bidi override in a line name", |p| {
            p.line_items[0].name = "Bananas \u{202E}FHC 01".to_owned();
        }),
        ("bidi isolate in category", |p| {
            p.receipt.category = "Groc\u{2066}eries".to_owned();
        }),
        ("zero-width space in shop", |p| {
            p.receipt.shop = "Synthetic\u{200B}Market".to_owned();
        }),
        ("zero-width joiner in a line name", |p| {
            p.line_items[1].name = "Br\u{200D}ead".to_owned();
        }),
        ("byte-order mark in shop", |p| {
            p.receipt.shop = "\u{FEFF}Synthetic Market".to_owned();
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
