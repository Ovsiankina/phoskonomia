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
    clippy::suboptimal_flops,
    clippy::bool_assert_comparison,
    clippy::needless_collect,
    clippy::comparison_chain,
    clippy::redundant_closure_for_method_calls,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_possible_truncation
)]
//! Tests for `phosk_ledger::transactions::create_transaction` — the manual
//! entry write path (T10).
//!
//! The requirement: a human types a dated spend at a shop, optionally itemised,
//! and it becomes a real ledger record — stamped `UserEntered` at full
//! confidence, with every money value derived by the backend, and visible to
//! the cycle aggregates the rest of the app reads.
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
    NewLineInput, NewTransaction, TxnFilter, create_transaction, list_transactions,
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

// ── The happy path ───────────────────────────────────────────────────────────

/// A manual entry is persisted as a receipt plus its lines, all stamped
/// `UserEntered` at full confidence, with `MANUAL` source and no OCR metadata.
#[tokio::test]
async fn create_transaction_persists_a_user_entered_receipt_with_its_lines() {
    let db = seeded();
    let created = create_transaction(
        &db,
        entry(vec![line("Zopf", 1.0, 650), line("Gipfeli", 3.0, 190)]),
    )
    .await
    .expect("create ok");

    assert_eq!(created.amount.centimes(), 650 + 570, "650 + 3×190");
    assert_eq!(created.item_count, 2);

    let receipt = db
        .receipt_by_slug(&created.id)
        .await
        .expect("the new receipt is readable by its slug");
    assert_eq!(receipt.shop, "Bäckerei Fischer");
    assert_eq!(receipt.category, "Groceries");
    assert_eq!(receipt.date, day(2026, 6, 10));
    assert_eq!(receipt.amount.centimes(), 1_220);
    assert_eq!(receipt.fixed, false);
    assert_eq!(receipt.provenance.source, Source::UserEntered);
    assert_eq!(receipt.provenance.confidence, 1.0);
    assert_eq!(receipt.source_kind, "MANUAL");
    assert!(
        receipt.ocr_engine.is_empty(),
        "no OCR engine for manual entry"
    );
    assert_eq!(receipt.ocr_regions, 0);

    let lines = db.line_items(receipt.id).await.expect("lines readable");
    assert_eq!(lines.len(), 2);
    assert!(
        lines.iter().all(|l| l.receipt_id == receipt.id),
        "every line is bound to the new receipt"
    );
    assert!(
        lines.iter().all(
            |l| l.provenance.source == Source::UserEntered && !l.provenance.is_low_confidence()
        ),
        "typed-in lines are authoritative, never review-flagged"
    );
}

/// Line totals and the receipt total are backend-derived from `qty × unit_price`
/// (rounded half away from zero), never taken from the caller.
#[tokio::test]
async fn line_totals_and_the_receipt_total_are_backend_derived() {
    let db = seeded();
    // 0.4 × 6600 = 2640 exactly; 2.5 × 349 = 872.5 → 873.
    let created = create_transaction(
        &db,
        entry(vec![
            line("Gruyère AOP", 0.4, 6_600),
            line("Trauben", 2.5, 349),
        ]),
    )
    .await
    .expect("create ok");
    assert_eq!(created.amount.centimes(), 2_640 + 873);

    let receipt = db.receipt_by_slug(&created.id).await.expect("stored");
    let lines = db.line_items(receipt.id).await.expect("lines");
    let total_of = |name: &str| {
        lines
            .iter()
            .find(|l| l.name == name)
            .expect("line present")
            .line_total
            .centimes()
    };
    assert_eq!(total_of("Gruyère AOP"), 2_640);
    assert_eq!(total_of("Trauben"), 873, "872.5 rounds away from zero");
}

/// A total-only entry (no itemisation) is a valid manual record.
#[tokio::test]
async fn a_total_only_entry_needs_no_line_items() {
    let db = seeded();
    let mut input = entry(Vec::new());
    input.amount = Some(Money::from_centimes(4_500));

    let created = create_transaction(&db, input).await.expect("create ok");
    assert_eq!(created.amount.centimes(), 4_500);
    assert_eq!(created.item_count, 0);

    let receipt = db.receipt_by_slug(&created.id).await.expect("stored");
    assert_eq!(receipt.amount.centimes(), 4_500);
    assert!(
        db.line_items(receipt.id).await.expect("lines").is_empty(),
        "no lines were invented"
    );
}

/// A stated total that agrees with the itemisation is accepted (the UI may send
/// what it displayed; the backend still stores its own derivation).
#[tokio::test]
async fn a_stated_total_matching_the_lines_is_accepted() {
    let db = seeded();
    let mut input = entry(vec![line("Zopf", 1.0, 650)]);
    input.amount = Some(Money::from_centimes(650));

    let created = create_transaction(&db, input).await.expect("create ok");
    assert_eq!(created.amount.centimes(), 650);
}

/// Every entry gets its own identity: two identical manual entries are two
/// receipts, not one overwriting the other.
#[tokio::test]
async fn two_identical_entries_are_two_distinct_receipts() {
    let db = seeded();
    let before = db.all_receipts().await.expect("receipts").len();

    let first = create_transaction(&db, entry(vec![line("Zopf", 1.0, 650)]))
        .await
        .expect("create ok");
    let second = create_transaction(&db, entry(vec![line("Zopf", 1.0, 650)]))
        .await
        .expect("create ok");

    assert_ne!(first.id, second.id, "each entry gets a distinct slug");
    assert_eq!(
        db.all_receipts().await.expect("receipts").len(),
        before + 2,
        "both entries are stored"
    );
}

// ── Line details ─────────────────────────────────────────────────────────────

/// A line with no category of its own inherits the receipt's; an explicit one is
/// kept.
#[tokio::test]
async fn a_line_without_a_category_inherits_the_receipt_category() {
    let db = seeded();
    let mut itemised = line("Waschmittel", 1.0, 1_290);
    itemised.category = "Household".to_owned();
    let created = create_transaction(&db, entry(vec![line("Zopf", 1.0, 650), itemised]))
        .await
        .expect("create ok");

    let receipt = db.receipt_by_slug(&created.id).await.expect("stored");
    let lines = db.line_items(receipt.id).await.expect("lines");
    let category_of = |name: &str| {
        lines
            .iter()
            .find(|l| l.name == name)
            .expect("line present")
            .category
            .clone()
    };
    assert_eq!(category_of("Zopf"), "Groceries", "inherited");
    assert_eq!(category_of("Waschmittel"), "Household", "kept as given");
}

/// A line may be attached to a tracked item-signal by its slug.
#[tokio::test]
async fn a_line_can_be_linked_to_a_tracked_signal_by_slug() {
    let db = seeded();
    let mut coffee = line("Flat white", 1.0, 560);
    coffee.signal_id = "coffee".to_owned();
    let created = create_transaction(&db, entry(vec![coffee]))
        .await
        .expect("create ok");

    let signal = db.signal_by_slug("coffee").await.expect("seeded signal");
    let receipt = db.receipt_by_slug(&created.id).await.expect("stored");
    let lines = db.line_items(receipt.id).await.expect("lines");
    assert_eq!(lines[0].signal_id, Some(signal.id));
}

/// An unknown signal slug is `NotFound`, and nothing is written.
#[tokio::test]
async fn an_unknown_signal_slug_is_not_found_and_writes_nothing() {
    let db = seeded();
    let before = db.all_receipts().await.expect("receipts").len();
    let mut ghost = line("Flat white", 1.0, 560);
    ghost.signal_id = "no-such-signal".to_owned();

    let err = create_transaction(&db, entry(vec![ghost]))
        .await
        .expect_err("rejected");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
    assert_eq!(
        db.all_receipts().await.expect("receipts").len(),
        before,
        "a rejected entry leaves no partial receipt"
    );
}

// ── Validation ───────────────────────────────────────────────────────────────

/// A stated total that contradicts the itemisation is refused rather than
/// silently overridden — a mis-keyed number must not enter the ledger.
#[tokio::test]
async fn a_stated_total_that_contradicts_the_lines_is_rejected() {
    let db = seeded();
    let before = db.all_receipts().await.expect("receipts").len();
    let mut input = entry(vec![line("Zopf", 1.0, 650)]);
    input.amount = Some(Money::from_centimes(700));

    let err = create_transaction(&db, input).await.expect_err("rejected");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "expected Invalid, got {err:?}"
    );
    assert_eq!(
        db.all_receipts().await.expect("receipts").len(),
        before,
        "nothing was written"
    );
}

/// An entry with neither lines nor a total is not a spend record.
#[tokio::test]
async fn an_entry_without_lines_or_a_total_is_rejected() {
    let db = seeded();
    let err = create_transaction(&db, entry(Vec::new()))
        .await
        .expect_err("rejected");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "expected Invalid, got {err:?}"
    );
}

/// Shop and category are required; surrounding whitespace is trimmed off the
/// stored value.
#[tokio::test]
async fn shop_and_category_are_required_and_trimmed() {
    let db = seeded();

    let mut blank_shop = entry(vec![line("Zopf", 1.0, 650)]);
    blank_shop.shop = "   ".to_owned();
    assert!(
        matches!(
            create_transaction(&db, blank_shop).await,
            Err(PhoskError::Invalid(_))
        ),
        "a blank shop is invalid"
    );

    let mut blank_category = entry(vec![line("Zopf", 1.0, 650)]);
    blank_category.category = String::new();
    assert!(
        matches!(
            create_transaction(&db, blank_category).await,
            Err(PhoskError::Invalid(_))
        ),
        "a blank category is invalid"
    );

    let mut padded = entry(vec![line("  Zopf  ", 1.0, 650)]);
    padded.shop = "  Coop  ".to_owned();
    padded.category = " Groceries ".to_owned();
    let created = create_transaction(&db, padded).await.expect("create ok");
    let receipt = db.receipt_by_slug(&created.id).await.expect("stored");
    assert_eq!(receipt.shop, "Coop");
    assert_eq!(receipt.category, "Groceries");
    let lines = db.line_items(receipt.id).await.expect("lines");
    assert_eq!(lines[0].name, "Zopf");
}

/// A line needs a name and a quantity that is a positive, finite number.
#[tokio::test]
async fn a_line_needs_a_name_and_a_positive_finite_quantity() {
    let db = seeded();

    for bad_qty in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let input = entry(vec![line("Zopf", bad_qty, 650)]);
        assert!(
            matches!(
                create_transaction(&db, input).await,
                Err(PhoskError::Invalid(_))
            ),
            "qty {bad_qty} must be rejected"
        );
    }

    let input = entry(vec![line("   ", 1.0, 650)]);
    assert!(
        matches!(
            create_transaction(&db, input).await,
            Err(PhoskError::Invalid(_))
        ),
        "a nameless line is invalid"
    );
}

/// Negative money is not a manual spend: neither a unit price nor a total.
#[tokio::test]
async fn negative_amounts_are_rejected() {
    let db = seeded();

    let input = entry(vec![line("Rückerstattung", 1.0, -650)]);
    assert!(
        matches!(
            create_transaction(&db, input).await,
            Err(PhoskError::Invalid(_))
        ),
        "a negative unit price is invalid"
    );

    let mut refund = entry(Vec::new());
    refund.amount = Some(Money::from_centimes(-4_500));
    assert!(
        matches!(
            create_transaction(&db, refund).await,
            Err(PhoskError::Invalid(_))
        ),
        "a negative total is invalid"
    );
}

// ── The new entry is part of the ledger ──────────────────────────────────────

/// The whole point: a created transaction counts towards the cycle aggregates
/// every other page reads (daily spend series and top shops).
#[tokio::test]
async fn a_created_transaction_joins_the_cycle_aggregates() {
    let db = seeded();
    let before = daily_spend(&db, june()).await.expect("series");
    let june_10 = 9_usize; // 0-based day offset from June 1.

    let mut input = entry(vec![line("Zopf", 1.0, 650), line("Gipfeli", 3.0, 190)]);
    input.shop = "Bäckerei Fischer".to_owned();
    create_transaction(&db, input).await.expect("create ok");

    let after = daily_spend(&db, june()).await.expect("series");
    assert_eq!(
        after[june_10].centimes(),
        before[june_10].centimes() + 1_220,
        "June 10 carries the new spend"
    );
    assert_eq!(
        Money::sum(after.clone()).expect("no overflow").centimes(),
        Money::sum(before).expect("no overflow").centimes() + 1_220,
        "the cycle total grows by exactly the new amount"
    );

    let shops = top_shops(&db, june(), 100).await.expect("ranking");
    let fischer = shops
        .iter()
        .find(|s| s.shop == "Bäckerei Fischer")
        .expect("the new shop is ranked");
    assert_eq!(fischer.total.centimes(), 1_220);
}

/// It also shows up in the Transactions page read model, with its derived
/// item count.
#[tokio::test]
async fn a_created_transaction_shows_up_in_the_transaction_list() {
    let db = seeded();
    let created = create_transaction(
        &db,
        entry(vec![line("Zopf", 1.0, 650), line("Gipfeli", 3.0, 190)]),
    )
    .await
    .expect("create ok");

    let list = list_transactions(&db, as_of(), TxnFilter::default())
        .await
        .expect("list ok");
    let row = list
        .transactions
        .iter()
        .find(|t| t.id == created.id)
        .expect("the new entry is listed");
    assert_eq!(row.shop, "Bäckerei Fischer");
    assert_eq!(row.date, "10 JUN");
    assert_eq!(row.amount.centimes(), 1_220);
    assert_eq!(row.item_count, 2);
    assert_eq!(row.low_conf_count, 0, "typed-in lines need no review");
}
