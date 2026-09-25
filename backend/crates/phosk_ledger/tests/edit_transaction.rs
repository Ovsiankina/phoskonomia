#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors the
    // other ledger integration tests).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::float_cmp,
    clippy::bool_assert_comparison
)]
//! Tests for `phosk_ledger::transactions::edit_transaction` and
//! `delete_transaction` — the correction and removal write paths (T11).
//!
//! The requirement: a human fixes a spend they already recorded (its category,
//! shop, fixed flag, date or — when there is nothing to derive it from — its
//! total), and the record becomes `UserModified` while keeping its identity and
//! its itemisation; or they remove the spend entirely and it stops counting
//! everywhere, ledger and dashboard aggregates alike.
//!
//! Ground truth for the pre-existing numbers is the deterministic Swiss seed in
//! `phosk_db_memory`, pinned at `as_of = 2026-06-18` (June cycle
//! `[2026-06-01, 2026-06-30]`). Money is asserted in exact i64 centimes.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::{CycleWindow, Period};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_ledger::transactions::{
    NewLineInput, NewTransaction, TxnEdit, TxnFilter, create_transaction, delete_transaction,
    edit_transaction, list_transactions,
};
use phosk_ledger::{daily_spend, top_shops};
use phosk_model::Source;

/// A calendar date for the fixtures.
fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
}

/// The seed clock: the spec's "today".
fn as_of() -> NaiveDate {
    day(2026, 6, 18)
}

/// The June 2026 cycle window `[2026-06-01, 2026-06-30]`.
fn june() -> CycleWindow {
    Period::Month.resolve(as_of()).expect("June resolves")
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// One manual line: no explicit category (inherits the receipt's), no signal.
fn line(name: &str, qty: f64, unit_cents: i64) -> NewLineInput {
    NewLineInput {
        name: name.to_owned(),
        qty,
        unit_price: Money::from_centimes(unit_cents),
        category: String::new(),
        signal_id: String::new(),
    }
}

/// A manual entry at a synthetic shop, dated June 10 2026 (day offset 9).
fn entry(lines: Vec<NewLineInput>) -> NewTransaction {
    NewTransaction {
        shop: "Bäckerei Fischer".to_owned(),
        date: day(2026, 6, 10),
        category: "Groceries".to_owned(),
        fixed: false,
        amount: None,
        lines,
    }
}

/// An itemised transaction to edit: 650 + 3×190 = 1220 centimes on June 10.
async fn itemised(db: &MemoryDb) -> String {
    create_transaction(
        db,
        entry(vec![line("Zopf", 1.0, 650), line("Gipfeli", 3.0, 190)]),
    )
    .await
    .expect("create ok")
    .id
}

/// A total-only transaction to edit: 4500 centimes on June 10, no lines.
async fn total_only(db: &MemoryDb) -> String {
    let mut input = entry(Vec::new());
    input.amount = Some(Money::from_centimes(4_500));
    create_transaction(db, input).await.expect("create ok").id
}

// ── Editing the descriptive fields ───────────────────────────────────────────

/// Editing shop / category / fixed / date rewrites exactly those fields, keeps
/// the receipt's identity (id and slug) and flips its provenance to
/// `UserModified` at full confidence.
#[tokio::test]
async fn an_edit_rewrites_the_named_fields_and_stamps_user_modified() {
    let db = seeded();
    let slug = itemised(&db).await;
    let before = db.receipt_by_slug(&slug).await.expect("stored");

    let edited = edit_transaction(
        &db,
        &slug,
        TxnEdit {
            shop: Some("Bäckerei Steiner".to_owned()),
            category: Some("Dining".to_owned()),
            fixed: Some(true),
            date: Some(day(2026, 6, 12)),
            ..TxnEdit::default()
        },
    )
    .await
    .expect("edit ok");

    assert_eq!(edited.id, slug, "the slug is the stable identity");
    assert_eq!(edited.amount.centimes(), 1_220, "untouched by a field edit");

    let after = db.receipt_by_slug(&slug).await.expect("still stored");
    assert_eq!(after.id, before.id, "the receipt keeps its typed id");
    assert_eq!(after.shop, "Bäckerei Steiner");
    assert_eq!(after.category, "Dining");
    assert_eq!(after.fixed, true);
    assert_eq!(after.date, day(2026, 6, 12));
    assert_eq!(after.provenance.source, Source::UserModified);
    assert_eq!(after.provenance.confidence, 1.0);
}

/// An edit names what it changed, so a caller can log or display it.
#[tokio::test]
async fn an_edit_reports_the_fields_it_changed() {
    let db = seeded();
    let slug = itemised(&db).await;

    let edited = edit_transaction(
        &db,
        &slug,
        TxnEdit {
            category: Some("Dining".to_owned()),
            fixed: Some(true),
            ..TxnEdit::default()
        },
    )
    .await
    .expect("edit ok");

    let mut changed = edited.changed;
    changed.sort();
    assert_eq!(changed, vec!["category".to_owned(), "fixed".to_owned()]);
}

/// A field set to the value it already holds is not a change: the record is
/// left exactly as it was, provenance included.
#[tokio::test]
async fn an_edit_that_changes_nothing_leaves_the_record_alone() {
    let db = seeded();
    let slug = itemised(&db).await;

    let edited = edit_transaction(
        &db,
        &slug,
        TxnEdit {
            shop: Some("Bäckerei Fischer".to_owned()),
            category: Some("Groceries".to_owned()),
            fixed: Some(false),
            date: Some(day(2026, 6, 10)),
            amount: Some(Money::from_centimes(1_220)),
        },
    )
    .await
    .expect("edit ok");

    assert!(edited.changed.is_empty(), "nothing actually changed");
    let after = db.receipt_by_slug(&slug).await.expect("stored");
    assert_eq!(
        after.provenance.source,
        Source::UserEntered,
        "a no-op edit does not rewrite provenance"
    );
}

/// The itemisation survives an edit untouched: same lines, same ids, same
/// provenance — editing the receipt is not re-entering it.
#[tokio::test]
async fn an_edit_keeps_the_line_items_intact() {
    let db = seeded();
    let slug = itemised(&db).await;
    let id = db.receipt_by_slug(&slug).await.expect("stored").id;
    let mut before = db.line_items(id).await.expect("lines");
    before.sort_by(|a, b| a.name.cmp(&b.name));

    edit_transaction(
        &db,
        &slug,
        TxnEdit {
            category: Some("Dining".to_owned()),
            ..TxnEdit::default()
        },
    )
    .await
    .expect("edit ok");

    let mut after = db.line_items(id).await.expect("lines");
    after.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(after, before, "the lines are carried over verbatim");
}

/// A receipt-level category edit does not re-tag the individual lines: item-level
/// categorisation is its own decision (`correct_line`).
#[tokio::test]
async fn an_edit_does_not_re_tag_the_line_categories() {
    let db = seeded();
    let slug = itemised(&db).await;
    let id = db.receipt_by_slug(&slug).await.expect("stored").id;

    edit_transaction(
        &db,
        &slug,
        TxnEdit {
            category: Some("Dining".to_owned()),
            ..TxnEdit::default()
        },
    )
    .await
    .expect("edit ok");

    let lines = db.line_items(id).await.expect("lines");
    assert!(
        lines.iter().all(|l| l.category == "Groceries"),
        "line categories are untouched by a receipt-level edit"
    );
}

// ── Editing the amount ───────────────────────────────────────────────────────

/// A total-only entry has nothing to derive its total from, so its amount is
/// editable — and the dashboard aggregates follow the correction.
#[tokio::test]
async fn editing_the_total_of_an_entry_without_lines_moves_the_aggregates() {
    let db = seeded();
    let before = daily_spend(&db, june()).await.expect("series");
    let june_10 = 9_usize; // 0-based day offset from June 1.
    let slug = total_only(&db).await;

    let edited = edit_transaction(
        &db,
        &slug,
        TxnEdit {
            amount: Some(Money::from_centimes(5_000)),
            ..TxnEdit::default()
        },
    )
    .await
    .expect("edit ok");
    assert_eq!(edited.amount.centimes(), 5_000);

    let after = daily_spend(&db, june()).await.expect("series");
    assert_eq!(
        after[june_10].centimes(),
        before[june_10].centimes() + 5_000,
        "June 10 carries the corrected amount once, not both versions"
    );
}

/// The itemisation owns the total: an amount that contradicts the lines is
/// refused rather than silently overruling them.
#[tokio::test]
async fn an_amount_that_contradicts_the_itemisation_is_rejected() {
    let db = seeded();
    let slug = itemised(&db).await;

    let err = edit_transaction(
        &db,
        &slug,
        TxnEdit {
            amount: Some(Money::from_centimes(9_900)),
            ..TxnEdit::default()
        },
    )
    .await
    .expect_err("an amount edit cannot overrule the lines");
    assert!(matches!(err, PhoskError::Invalid(_)), "got {err:?}");

    let after = db.receipt_by_slug(&slug).await.expect("stored");
    assert_eq!(after.amount.centimes(), 1_220, "the stored total is intact");
    assert_eq!(
        after.provenance.source,
        Source::UserEntered,
        "a rejected edit changes nothing at all"
    );
}

/// An amount equal to the derived sum is accepted (the UI may echo back what it
/// displayed) and counts as no change.
#[tokio::test]
async fn an_amount_matching_the_itemisation_is_accepted() {
    let db = seeded();
    let slug = itemised(&db).await;

    let edited = edit_transaction(
        &db,
        &slug,
        TxnEdit {
            amount: Some(Money::from_centimes(1_220)),
            ..TxnEdit::default()
        },
    )
    .await
    .expect("edit ok");
    assert_eq!(edited.amount.centimes(), 1_220);
    assert!(edited.changed.is_empty());
}

// ── Validation ───────────────────────────────────────────────────────────────

/// Blank required fields, a negative total and an unknown record are all refused
/// with the right error, and nothing is written.
#[tokio::test]
async fn an_edit_validates_its_input() {
    let db = seeded();
    let slug = total_only(&db).await;

    let blank_shop = edit_transaction(
        &db,
        &slug,
        TxnEdit {
            shop: Some("   ".to_owned()),
            ..TxnEdit::default()
        },
    )
    .await
    .expect_err("a blank shop is not a shop");
    assert!(matches!(blank_shop, PhoskError::Invalid(_)));

    let blank_category = edit_transaction(
        &db,
        &slug,
        TxnEdit {
            category: Some(String::new()),
            ..TxnEdit::default()
        },
    )
    .await
    .expect_err("a blank category is not a category");
    assert!(matches!(blank_category, PhoskError::Invalid(_)));

    let negative = edit_transaction(
        &db,
        &slug,
        TxnEdit {
            amount: Some(Money::from_centimes(-1)),
            ..TxnEdit::default()
        },
    )
    .await
    .expect_err("a spend is not negative");
    assert!(matches!(negative, PhoskError::Invalid(_)));

    let after = db.receipt_by_slug(&slug).await.expect("stored");
    assert_eq!(after.shop, "Bäckerei Fischer");
    assert_eq!(after.category, "Groceries");
    assert_eq!(after.amount.centimes(), 4_500);
}

/// Editing a record that does not exist is `NotFound`, not a silent create.
#[tokio::test]
async fn editing_an_unknown_transaction_is_not_found() {
    let db = seeded();
    let err = edit_transaction(
        &db,
        "manual:does-not-exist",
        TxnEdit {
            category: Some("Dining".to_owned()),
            ..TxnEdit::default()
        },
    )
    .await
    .expect_err("no such receipt");
    assert!(matches!(err, PhoskError::NotFound(_)), "got {err:?}");
}

/// Trimming is part of validation: surrounding whitespace never reaches the
/// ledger.
#[tokio::test]
async fn edited_text_fields_are_trimmed() {
    let db = seeded();
    let slug = total_only(&db).await;

    edit_transaction(
        &db,
        &slug,
        TxnEdit {
            shop: Some("  Coop  ".to_owned()),
            category: Some("  Dining  ".to_owned()),
            ..TxnEdit::default()
        },
    )
    .await
    .expect("edit ok");

    let after = db.receipt_by_slug(&slug).await.expect("stored");
    assert_eq!(after.shop, "Coop");
    assert_eq!(after.category, "Dining");
}

// ── The read models follow the edit ──────────────────────────────────────────

/// The Transactions page sees the corrected row — and a date moved out of the
/// cycle takes the spend with it.
#[tokio::test]
async fn the_transaction_list_follows_the_edit() {
    let db = seeded();
    let slug = itemised(&db).await;

    edit_transaction(
        &db,
        &slug,
        TxnEdit {
            shop: Some("Coop".to_owned()),
            date: Some(day(2026, 6, 12)),
            ..TxnEdit::default()
        },
    )
    .await
    .expect("edit ok");

    let list = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    let row = list
        .transactions
        .iter()
        .find(|t| t.id == slug)
        .expect("still listed");
    assert_eq!(row.shop, "Coop");
    assert_eq!(row.date, "12 JUN");
    assert_eq!(row.item_count, 2, "its lines came along");

    edit_transaction(
        &db,
        &slug,
        TxnEdit {
            date: Some(day(2026, 7, 2)),
            ..TxnEdit::default()
        },
    )
    .await
    .expect("edit ok");

    let list = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    assert!(
        !list.transactions.iter().any(|t| t.id == slug),
        "a July date leaves the June cycle"
    );
    let shops = top_shops(&db, june(), 100).await.expect("ranking");
    assert!(
        !shops
            .iter()
            .any(|s| s.shop == "Coop" && s.total.centimes() == 1_220),
        "the moved spend no longer counts towards June"
    );
}

// ── Deleting ─────────────────────────────────────────────────────────────────

/// A deleted transaction leaves the ledger, the read models and the dashboard
/// aggregates together — no orphan lines, no ghost spend.
#[tokio::test]
async fn deleting_a_transaction_removes_it_everywhere() {
    let db = seeded();
    let before_series = daily_spend(&db, june()).await.expect("series");
    let before_shops = top_shops(&db, june(), 100).await.expect("ranking").len();
    let before_receipts = db.all_receipts().await.expect("receipts").len();

    let slug = itemised(&db).await;
    let id = db.receipt_by_slug(&slug).await.expect("stored").id;

    delete_transaction(&db, &slug).await.expect("delete ok");

    assert!(
        matches!(
            db.receipt_by_slug(&slug).await,
            Err(PhoskError::NotFound(_))
        ),
        "the receipt is gone"
    );
    assert_eq!(
        db.all_receipts().await.expect("receipts").len(),
        before_receipts,
        "the store is back to its previous size"
    );
    assert!(
        db.line_items(id).await.expect("lines").is_empty(),
        "its lines went with it"
    );

    let after_series = daily_spend(&db, june()).await.expect("series");
    assert_eq!(
        Money::sum(after_series).expect("no overflow").centimes(),
        Money::sum(before_series).expect("no overflow").centimes(),
        "the cycle total is back to what it was"
    );
    let after_shops = top_shops(&db, june(), 100).await.expect("ranking");
    assert_eq!(
        after_shops.len(),
        before_shops,
        "the shop is unranked again"
    );

    let list = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    assert!(
        !list.transactions.iter().any(|t| t.id == slug),
        "the list no longer shows it"
    );
}

/// Deleting touches only the named record.
#[tokio::test]
async fn deleting_one_transaction_leaves_its_neighbours_alone() {
    let db = seeded();
    let keep = itemised(&db).await;
    let drop = total_only(&db).await;

    delete_transaction(&db, &drop).await.expect("delete ok");

    let kept = db.receipt_by_slug(&keep).await.expect("still stored");
    assert_eq!(kept.amount.centimes(), 1_220);
    assert_eq!(
        db.line_items(kept.id).await.expect("lines").len(),
        2,
        "the neighbour keeps its itemisation"
    );
}

/// Deleting an unknown record — or the same one twice — is `NotFound`, never a
/// silent success.
#[tokio::test]
async fn deleting_an_unknown_transaction_is_not_found() {
    let db = seeded();
    let err = delete_transaction(&db, "manual:does-not-exist")
        .await
        .expect_err("no such receipt");
    assert!(matches!(err, PhoskError::NotFound(_)), "got {err:?}");

    let slug = total_only(&db).await;
    delete_transaction(&db, &slug).await.expect("delete ok");
    let again = delete_transaction(&db, &slug)
        .await
        .expect_err("already deleted");
    assert!(matches!(again, PhoskError::NotFound(_)), "got {again:?}");
}
