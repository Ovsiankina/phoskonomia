#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors the
    // sibling `categories` integration test).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
//! Tests for `phosk_ledger::categories` merge + split — the two ways history
//! moves *between* categories.
//!
//! A **merge** folds one category into another: every row that named the source
//! (receipts, their line items, subscriptions, signals) ends up naming the
//! target, and the source record disappears. A **split** carves a *new*
//! category out of an existing one by re-pointing a hand-picked set of line
//! items; the receipts those lines hang off keep their own category.
//!
//! Both are all-or-nothing: a rejected call must leave the store byte-identical
//! to what it was. That is the property most of these tests are really about,
//! so each rejection case re-reads the affected rows afterwards.
//!
//! Seed facts used below (June 2026 cycle, `phosk_db_memory::seed`):
//! Groceries spans receipts t1, t3 and t6 — `5_875 + 4_230 + 2_990 = 13_095`
//! centimes over three receipts; "Coffee & snacks" spans t5 alone, `1_280`.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_id::{LineItemId, ReceiptId};
use phosk_ledger::categories::{
    CategorySplit, NewCategory, category_spend, category_usage, create_category, merge_categories,
    split_category,
};
use phosk_model::{LineItem, Provenance, Receipt};

/// The seeded demo clock: day 18 of the June 2026 cycle.
const fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid as_of date")
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// A minimal `NewCategory` with the given name.
fn new_category(name: &str) -> NewCategory {
    NewCategory {
        name: name.to_owned(),
        cap: Some(Money::from_centimes(10_000)),
        fixed: false,
        glyph: String::new(),
        note: String::new(),
    }
}

/// The exact centime total a category carries in the June cycle, or zero.
async fn spend(db: &dyn DatabaseAdapter, category: &str) -> i64 {
    category_spend(db, as_of())
        .await
        .expect("category_spend ok")
        .iter()
        .find(|r| r.category == category)
        .map_or(0, |r| r.total.centimes())
}

/// Every stored line item, across every receipt.
async fn all_lines(db: &dyn DatabaseAdapter) -> Vec<LineItem> {
    let mut out = Vec::new();
    for r in db.all_receipts().await.expect("receipts ok") {
        out.extend(db.line_items(r.id).await.expect("lines ok"));
    }
    out
}

/// How many line items name `category`.
async fn lines_in(db: &dyn DatabaseAdapter, category: &str) -> usize {
    all_lines(db)
        .await
        .into_iter()
        .filter(|l| l.category == category)
        .count()
}

/// A synthetic receipt in the "Hobby" category (not part of the seed).
fn hobby_receipt(id: ReceiptId) -> Receipt {
    Receipt {
        id,
        slug: "hobby-1".to_owned(),
        shop: "Atelier".to_owned(),
        date: as_of(),
        category: "Hobby".to_owned(),
        amount: Money::from_centimes(4_000),
        fixed: false,
        provenance: Provenance::user_entered(),
        source_kind: "MANUAL".to_owned(),
        ocr_engine: String::new(),
        ocr_regions: 0,
    }
}

/// A synthetic "Hobby" line bound to `receipt_id`.
fn hobby_line(receipt_id: ReceiptId, name: &str) -> LineItem {
    LineItem {
        id: LineItemId::new(),
        receipt_id,
        name: name.to_owned(),
        qty: 1.0,
        unit_price: Money::from_centimes(2_000),
        line_total: Money::from_centimes(2_000),
        category: "Hobby".to_owned(),
        signal_id: None,
        provenance: Provenance::user_entered(),
    }
}

// ── merge ──────────────────────────────────────────────────────────────────────

/// The spine test: merging "Coffee & snacks" into "Going out" moves that
/// category's whole spend onto the target, empties the source of references and
/// removes its record. No centime is created or lost.
#[tokio::test]
async fn merge_moves_all_spend_and_removes_the_source() {
    let db = seeded();
    let source_before = spend(&db, "Coffee & snacks").await;
    let target_before = spend(&db, "Going out").await;
    assert_eq!(
        source_before, 1_280,
        "t5 is the lone Coffee & snacks receipt"
    );
    assert_eq!(target_before, 6_450, "t2 is the lone Going out receipt");
    let caps_before = db.category_caps().await.expect("caps ok").len();

    let moved = merge_categories(&db, "Coffee & snacks", "Going out")
        .await
        .expect("merge ok");

    assert!(moved > 0, "the merge reported the rows it moved");
    assert_eq!(
        spend(&db, "Coffee & snacks").await,
        0,
        "the source carries no spend any more"
    );
    assert_eq!(
        spend(&db, "Going out").await,
        source_before + target_before,
        "the target carries both categories' spend, exactly"
    );
    assert_eq!(
        category_usage(&db, "Coffee & snacks")
            .await
            .expect("usage ok"),
        0,
        "no row still names the source"
    );
    assert!(
        matches!(
            db.category_cap_by_name("Coffee & snacks").await,
            Err(PhoskError::NotFound(_))
        ),
        "the source record is gone"
    );
    assert_eq!(
        db.category_caps().await.expect("caps ok").len(),
        caps_before - 1,
        "exactly one category disappeared"
    );
}

/// A merge carries line items, not just receipts: every line that named the
/// source names the target afterwards, and the count adds up.
#[tokio::test]
async fn merge_repoints_historical_line_items() {
    let db = seeded();
    let source_lines = lines_in(&db, "Coffee & snacks").await;
    let target_lines = lines_in(&db, "Going out").await;
    assert!(source_lines > 0, "the seed has Coffee & snacks line items");

    merge_categories(&db, "Coffee & snacks", "Going out")
        .await
        .expect("merge ok");

    assert_eq!(
        lines_in(&db, "Coffee & snacks").await,
        0,
        "no line still names the source"
    );
    assert_eq!(
        lines_in(&db, "Going out").await,
        source_lines + target_lines,
        "every source line moved to the target"
    );
}

/// The reported count is the number of rows actually re-pointed — the same
/// number `category_usage` saw on the source just before the merge.
#[tokio::test]
async fn merge_reports_the_number_of_rows_it_moved() {
    let db = seeded();
    let expected = category_usage(&db, "Coffee & snacks")
        .await
        .expect("usage ok");

    let moved = merge_categories(&db, "Coffee & snacks", "Going out")
        .await
        .expect("merge ok");

    assert_eq!(moved, expected, "every reference was accounted for");
}

/// The target keeps its own id, slug and cap: a merge changes what points at
/// it, never what it *is* (its cap is not summed with the source's — that is a
/// budgeting decision the user makes afterwards on the Budgets page).
#[tokio::test]
async fn merge_leaves_the_target_record_intact() {
    let db = seeded();
    let before = db
        .category_cap_by_name("Going out")
        .await
        .expect("target exists");

    merge_categories(&db, "Coffee & snacks", "Going out")
        .await
        .expect("merge ok");

    let after = db
        .category_cap_by_name("Going out")
        .await
        .expect("still ok");
    assert_eq!(after.id, before.id, "the target's id is stable");
    assert_eq!(after.slug, before.slug, "the target's slug is stable");
    assert_eq!(after.cap, before.cap, "the caps are not summed");
    assert_eq!(after.glyph, before.glyph, "the glyph is untouched");
}

/// An unknown source or target is `NotFound`, and nothing moves.
#[tokio::test]
async fn merge_rejects_unknown_categories() {
    let db = seeded();
    let before = spend(&db, "Groceries").await;

    let res = merge_categories(&db, "Nonexistent", "Groceries").await;
    assert!(
        matches!(res, Err(PhoskError::NotFound(_))),
        "unknown source is NotFound, got {res:?}"
    );
    let res = merge_categories(&db, "Groceries", "Nonexistent").await;
    assert!(
        matches!(res, Err(PhoskError::NotFound(_))),
        "unknown target is NotFound, got {res:?}"
    );
    assert_eq!(
        spend(&db, "Groceries").await,
        before,
        "the refused merges moved nothing"
    );
}

/// Merging a category into itself is refused — including by a case-only
/// difference, because names are the same envelope to a human. Refusing it
/// matters: folding a category into itself would delete the record its own
/// history still points at.
#[tokio::test]
async fn merge_refuses_a_category_into_itself() {
    let db = seeded();

    let res = merge_categories(&db, "Groceries", "Groceries").await;
    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "self-merge is Invalid, got {res:?}"
    );
    let res = merge_categories(&db, "Groceries", "groceries").await;
    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "case-only self-merge is Invalid, got {res:?}"
    );

    assert!(
        db.category_cap_by_name("Groceries").await.is_ok(),
        "the category survived"
    );
    assert_eq!(
        spend(&db, "Groceries").await,
        13_095,
        "its spend is untouched"
    );
}

/// A merged-away category is free again: its name can be created anew and
/// starts empty.
#[tokio::test]
async fn a_merged_away_name_can_be_created_again() {
    let db = seeded();
    merge_categories(&db, "Coffee & snacks", "Going out")
        .await
        .expect("merge ok");

    let fresh = create_category(&db, new_category("Coffee & snacks"))
        .await
        .expect("the freed name is available");

    assert_eq!(fresh.name, "Coffee & snacks");
    assert_eq!(
        category_usage(&db, "Coffee & snacks")
            .await
            .expect("usage ok"),
        0,
        "the re-created category starts empty"
    );
}

// ── split ──────────────────────────────────────────────────────────────────────

/// The spine test: splitting two hand-picked lines out of "Hobby" creates the
/// new category and moves exactly those lines — the third line and the
/// receipt's own category stay put.
#[tokio::test]
async fn split_moves_only_the_named_lines_into_a_new_category() {
    let db = seeded();
    create_category(&db, new_category("Hobby"))
        .await
        .expect("create ok");
    let receipt = ReceiptId::new();
    let lines = vec![
        hobby_line(receipt, "Brush"),
        hobby_line(receipt, "Canvas"),
        hobby_line(receipt, "Coffee"),
    ];
    let picked: Vec<LineItemId> = lines.iter().take(2).map(|l| l.id).collect();
    db.insert_receipt(hobby_receipt(receipt), lines)
        .await
        .expect("insert ok");

    let created = split_category(
        &db,
        CategorySplit {
            from: "Hobby".to_owned(),
            into: new_category("Art supplies"),
            lines: picked.clone(),
        },
    )
    .await
    .expect("split ok");

    assert_eq!(created.name, "Art supplies");
    assert_eq!(created.slug, "art-supplies", "the slug is derived");
    assert!(
        db.category_cap_by_name("Art supplies").await.is_ok(),
        "the new category is stored"
    );
    assert_eq!(
        lines_in(&db, "Art supplies").await,
        2,
        "exactly the two picked lines moved"
    );
    assert_eq!(lines_in(&db, "Hobby").await, 1, "the third line stayed");

    let moved: Vec<LineItem> = all_lines(&db)
        .await
        .into_iter()
        .filter(|l| picked.contains(&l.id))
        .collect();
    assert_eq!(moved.len(), 2, "both picked lines are still stored");
    assert!(
        moved.iter().all(|l| l.category == "Art supplies"),
        "the picked lines carry the new category"
    );
    assert!(
        moved.iter().all(|l| l.receipt_id == receipt),
        "a split does not move lines between receipts"
    );

    let parent = db.receipt(receipt).await.expect("receipt ok");
    assert_eq!(
        parent.category, "Hobby",
        "the receipt keeps its own category"
    );
    assert_eq!(
        parent.amount.centimes(),
        4_000,
        "the receipt total is untouched"
    );
}

/// A split leaves the amounts alone: the two categories' line totals add up to
/// what the source carried before.
#[tokio::test]
async fn split_conserves_the_line_totals() {
    let db = seeded();
    create_category(&db, new_category("Hobby"))
        .await
        .expect("create ok");
    let receipt = ReceiptId::new();
    let lines = vec![hobby_line(receipt, "Brush"), hobby_line(receipt, "Canvas")];
    let picked = vec![lines[0].id];
    db.insert_receipt(hobby_receipt(receipt), lines)
        .await
        .expect("insert ok");

    let total_before: i64 = all_lines(&db)
        .await
        .iter()
        .filter(|l| l.category == "Hobby")
        .map(|l| l.line_total.centimes())
        .sum();

    split_category(
        &db,
        CategorySplit {
            from: "Hobby".to_owned(),
            into: new_category("Art supplies"),
            lines: picked,
        },
    )
    .await
    .expect("split ok");

    let after: Vec<LineItem> = all_lines(&db).await;
    let hobby: i64 = after
        .iter()
        .filter(|l| l.category == "Hobby")
        .map(|l| l.line_total.centimes())
        .sum();
    let art: i64 = after
        .iter()
        .filter(|l| l.category == "Art supplies")
        .map(|l| l.line_total.centimes())
        .sum();
    assert_eq!(hobby + art, total_before, "no centime was created or lost");
    assert_eq!(art, 2_000, "the picked line's exact total moved");
}

/// A line that belongs to a *different* category cannot be dragged into the
/// split: the call is refused and not one line moves.
#[tokio::test]
async fn split_refuses_lines_outside_the_source_category() {
    let db = seeded();
    create_category(&db, new_category("Hobby"))
        .await
        .expect("create ok");
    let receipt = ReceiptId::new();
    let lines = vec![hobby_line(receipt, "Brush")];
    let hobby_id = lines[0].id;
    db.insert_receipt(hobby_receipt(receipt), lines)
        .await
        .expect("insert ok");
    let foreign = all_lines(&db)
        .await
        .into_iter()
        .find(|l| l.category == "Groceries")
        .expect("the seed has Groceries lines");

    let res = split_category(
        &db,
        CategorySplit {
            from: "Hobby".to_owned(),
            into: new_category("Art supplies"),
            lines: vec![hobby_id, foreign.id],
        },
    )
    .await;

    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "a foreign line is Invalid, got {res:?}"
    );
    assert!(
        matches!(
            db.category_cap_by_name("Art supplies").await,
            Err(PhoskError::NotFound(_))
        ),
        "the rejected split created no category"
    );
    assert_eq!(
        lines_in(&db, "Hobby").await,
        1,
        "the source line did not move"
    );
    let unchanged = all_lines(&db)
        .await
        .into_iter()
        .find(|l| l.id == foreign.id)
        .expect("the foreign line is still stored");
    assert_eq!(
        unchanged.category, "Groceries",
        "the foreign line is intact"
    );
}

/// An unknown line id, an unknown source category, an empty selection and a
/// name that is already taken are all refused before anything is written.
#[tokio::test]
async fn split_rejects_bad_input_without_writing() {
    let db = seeded();
    create_category(&db, new_category("Hobby"))
        .await
        .expect("create ok");
    let receipt = ReceiptId::new();
    let lines = vec![hobby_line(receipt, "Brush")];
    let hobby_id = lines[0].id;
    db.insert_receipt(hobby_receipt(receipt), lines)
        .await
        .expect("insert ok");
    let caps_before = db.category_caps().await.expect("caps ok").len();

    let split = |from: &str, into: &str, ids: Vec<LineItemId>| CategorySplit {
        from: from.to_owned(),
        into: new_category(into),
        lines: ids,
    };

    let res = split_category(&db, split("Hobby", "Art supplies", vec![LineItemId::new()])).await;
    assert!(
        matches!(res, Err(PhoskError::NotFound(_))),
        "an unknown line is NotFound, got {res:?}"
    );
    let res = split_category(&db, split("Nonexistent", "Art supplies", vec![hobby_id])).await;
    assert!(
        matches!(res, Err(PhoskError::NotFound(_))),
        "an unknown source category is NotFound, got {res:?}"
    );
    let res = split_category(&db, split("Hobby", "Art supplies", Vec::new())).await;
    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "an empty selection is Invalid, got {res:?}"
    );
    let res = split_category(&db, split("Hobby", "groceries", vec![hobby_id])).await;
    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "a taken name (any case) is Invalid, got {res:?}"
    );
    let res = split_category(&db, split("Hobby", "   ", vec![hobby_id])).await;
    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "a blank name is Invalid, got {res:?}"
    );

    assert_eq!(
        db.category_caps().await.expect("caps ok").len(),
        caps_before,
        "not one rejected split created a category"
    );
    assert_eq!(
        lines_in(&db, "Hobby").await,
        1,
        "not one rejected split moved a line"
    );
}

/// The same line id listed twice is one line, not two: the selection is
/// de-duplicated rather than refused.
#[tokio::test]
async fn split_deduplicates_a_repeated_line_id() {
    let db = seeded();
    create_category(&db, new_category("Hobby"))
        .await
        .expect("create ok");
    let receipt = ReceiptId::new();
    let lines = vec![hobby_line(receipt, "Brush"), hobby_line(receipt, "Canvas")];
    let picked = lines[0].id;
    db.insert_receipt(hobby_receipt(receipt), lines)
        .await
        .expect("insert ok");

    split_category(
        &db,
        CategorySplit {
            from: "Hobby".to_owned(),
            into: new_category("Art supplies"),
            lines: vec![picked, picked],
        },
    )
    .await
    .expect("split ok");

    assert_eq!(
        lines_in(&db, "Art supplies").await,
        1,
        "the repeated id moved one line"
    );
    assert_eq!(lines_in(&db, "Hobby").await, 1, "the other line stayed");
}

/// Split then merge is a round trip: carving lines out and folding them back
/// restores the source's line count.
#[tokio::test]
async fn split_then_merge_back_restores_the_source() {
    let db = seeded();
    create_category(&db, new_category("Hobby"))
        .await
        .expect("create ok");
    let receipt = ReceiptId::new();
    let lines = vec![
        hobby_line(receipt, "Brush"),
        hobby_line(receipt, "Canvas"),
        hobby_line(receipt, "Coffee"),
    ];
    let picked: Vec<LineItemId> = lines.iter().take(2).map(|l| l.id).collect();
    db.insert_receipt(hobby_receipt(receipt), lines)
        .await
        .expect("insert ok");
    let before = lines_in(&db, "Hobby").await;

    split_category(
        &db,
        CategorySplit {
            from: "Hobby".to_owned(),
            into: new_category("Art supplies"),
            lines: picked,
        },
    )
    .await
    .expect("split ok");
    merge_categories(&db, "Art supplies", "Hobby")
        .await
        .expect("merge ok");

    assert_eq!(lines_in(&db, "Hobby").await, before, "every line came home");
    assert!(
        matches!(
            db.category_cap_by_name("Art supplies").await,
            Err(PhoskError::NotFound(_))
        ),
        "the carved-out category is gone again"
    );
}
