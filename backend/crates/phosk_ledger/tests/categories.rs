#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors the
    // dashboard integration test).
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
//! Tests for `phosk_ledger::categories` — the ledger-side per-category spend
//! distribution (`category_spend`), the filter-dropdown option list
//! (`available_categories`), and the category write side (`create_category`,
//! `rename_category`, `delete_category`, `category_usage`).
//!
//! The read tests pin EXACT centime totals + receipt counts against the
//! deterministic Swiss seed at `as_of = 2026-06-18` (June cycle
//! `[2026-06-01, 2026-06-30]`).
//!
//! Source of truth for expected values (the June receipts `t1..t9`, see
//! `phosk_db_memory::seed::seed_receipts_and_lines`):
//!
//! | slug | shop            | category          | amount (cents) |
//! |------|-----------------|-------------------|----------------|
//! | t1   | Migros          | Groceries         |  5_875         |
//! | t2   | Restaurant Linde| Going out         |  6_450         |
//! | t3   | Coop            | Groceries         |  4_230         |
//! | t4   | Galaxus         | Shopping          | 12_990         |
//! | t5   | Migros          | Coffee & snacks   |  1_280         |
//! | t6   | Denner          | Groceries         |  2_990         |
//! | t7   | SBB             | Transport         |  3_400         |
//! | t8   | Landlord        | Rent              |168_000         |
//! | t9   | Helsana         | Health insurance  | 31_800         |
//!
//! Per-category roll-up (centimes / receipt count), ranked by total descending,
//! ties broken on name ascending:
//!   Rent             168_000 / 1
//!   Health insurance  31_800 / 1
//!   Groceries         13_095 / 3   (5_875 + 4_230 + 2_990)
//!   Shopping          12_990 / 1
//!   Going out          6_450 / 1
//!   Transport          3_400 / 1
//!   Coffee & snacks    1_280 / 1
//!   ─────────────────────────────
//!   total            237_015 / 9   (7 distinct categories)

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_id::{LineItemId, ReceiptId};
use phosk_ledger::categories::{
    CategorySpendDto, NewCategory, available_categories, category_spend, category_usage,
    create_category, delete_category, rename_category,
};
use phosk_model::{LineItem, Provenance, Receipt, Source};

/// The seeded demo clock: day 18 of the June 2026 cycle.
fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid as_of date")
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// Look up one category's roll-up by name, or fail the test with a clear message.
fn find<'a>(rows: &'a [CategorySpendDto], name: &str) -> &'a CategorySpendDto {
    rows.iter()
        .find(|r| r.category == name)
        .unwrap_or_else(|| panic!("category {name} present in roll-up"))
}

// ── category_spend ─────────────────────────────────────────────────────────────

/// The June cycle has exactly seven distinct categories.
#[tokio::test]
async fn category_spend_has_seven_distinct_categories() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    assert_eq!(rows.len(), 7, "seven distinct categories in the June cycle");
}

/// The full ranking — category, total (centimes), receipt count — is exact and
/// ordered by total descending (ties on name ascending). This is the spine test.
#[tokio::test]
async fn category_spend_full_ranking_is_exact() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");

    // (category, total_centimes, txns) in the expected rank order.
    let expected = [
        ("Rent", 168_000_i64, 1_u32),
        ("Health insurance", 31_800, 1),
        ("Groceries", 13_095, 3),
        ("Shopping", 12_990, 1),
        ("Going out", 6_450, 1),
        ("Transport", 3_400, 1),
        ("Coffee & snacks", 1_280, 1),
    ];
    assert_eq!(
        rows.len(),
        expected.len(),
        "row count matches expected ranking"
    );
    for (got, (name, cents, txns)) in rows.iter().zip(expected) {
        assert_eq!(got.category, name, "category name in rank order");
        assert_eq!(got.total.centimes(), cents, "exact total for {name}");
        assert_eq!(got.txns, txns, "receipt count for {name}");
    }
}

/// Totals are non-increasing down the ranking — the core ordering invariant.
#[tokio::test]
async fn category_spend_is_sorted_non_increasing() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    assert!(
        rows.windows(2).all(|w| w[0].total >= w[1].total),
        "each total is >= the next"
    );
}

/// Groceries spans three receipts (t1 5_875 + t3 4_230 + t6 2_990); they are
/// aggregated into ONE row, not listed separately, with a count of 3.
#[tokio::test]
async fn category_spend_aggregates_repeat_category_receipts() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    assert_eq!(
        rows.iter().filter(|r| r.category == "Groceries").count(),
        1,
        "Groceries appears once, aggregated"
    );
    let groceries = find(&rows, "Groceries");
    assert_eq!(
        groceries.total.centimes(),
        13_095,
        "3 grocery receipts summed"
    );
    assert_eq!(groceries.txns, 3, "three grocery receipts counted");
}

/// A single-receipt category (Coffee & snacks: only t5) carries txns == 1 and the
/// receipt's exact amount.
#[tokio::test]
async fn category_spend_single_receipt_category_is_exact() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    let coffee = find(&rows, "Coffee & snacks");
    assert_eq!(coffee.total.centimes(), 1_280, "lone t5 amount");
    assert_eq!(coffee.txns, 1, "one Coffee & snacks receipt");
}

/// The per-category totals sum to the June receipt total (CHF 2370.15), and the
/// counts sum to the nine seeded receipts — the roll-up is exhaustive and lossless.
#[tokio::test]
async fn category_spend_totals_and_counts_are_exhaustive() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");

    let summed = Money::sum(rows.iter().map(|r| r.total)).expect("no overflow");
    assert_eq!(
        summed.centimes(),
        237_015,
        "category totals sum to CHF 2370.15"
    );

    let receipts: u32 = rows.iter().map(|r| r.txns).sum();
    assert_eq!(receipts, 9, "counts sum to the nine June receipts");
}

/// The fixed standing charges (Rent, Health insurance) are present in the spend
/// roll-up just like variable categories — the ledger view does not exclude them.
#[tokio::test]
async fn category_spend_includes_fixed_charges() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    assert_eq!(find(&rows, "Rent").total.centimes(), 168_000, "Rent total");
    assert_eq!(
        find(&rows, "Health insurance").total.centimes(),
        31_800,
        "Health insurance total"
    );
}

/// An `as_of` whose cycle has no receipts (April 2026) yields an empty roll-up,
/// not an error.
#[tokio::test]
async fn category_spend_empty_cycle_is_empty() {
    let db = seeded();
    let april = NaiveDate::from_ymd_opt(2026, 4, 15).expect("valid date");
    let rows = category_spend(&db, april).await.expect("category_spend ok");
    assert!(
        rows.is_empty(),
        "no seeded receipts in April → empty roll-up"
    );
}

/// The May cycle holds no seeded *receipts* (receipts are all dated June), so its
/// roll-up is empty — confirming `category_spend` is scoped to the resolved cycle
/// window, not the whole receipt table.
#[tokio::test]
async fn category_spend_is_scoped_to_the_resolved_cycle() {
    let db = seeded();
    let may = NaiveDate::from_ymd_opt(2026, 5, 18).expect("valid date");
    let rows = category_spend(&db, may).await.expect("category_spend ok");
    assert!(
        rows.is_empty(),
        "seeded receipts are June-only → May cycle roll-up is empty"
    );
}

/// The DTO serializes to the camelCase wire form with money as exact i64 centimes
/// (`total` is a raw integer, NOT a CHF float).
#[tokio::test]
async fn category_spend_dto_serializes_centimes_camel_case() {
    let db = seeded();
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    let rent = find(&rows, "Rent");
    let json = serde_json::to_value(rent).expect("serialize");
    assert_eq!(json["category"], serde_json::json!("Rent"));
    assert_eq!(
        json["total"],
        serde_json::json!(168_000),
        "exact centimes, not CHF"
    );
    assert_eq!(json["txns"], serde_json::json!(1));
}

/// The service is callable behind the `&dyn DatabaseAdapter` PORT handle (ADR-010)
/// — it never sees the concrete `MemoryDb` type.
#[tokio::test]
async fn category_spend_works_through_the_port_trait_object() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;
    let rows = category_spend(port, as_of())
        .await
        .expect("category_spend ok");
    assert_eq!(rows.len(), 7);
}

// ── available_categories ───────────────────────────────────────────────────────

/// The distinct category names across all receipts, sorted ascending — the exact
/// filter-dropdown option list.
#[tokio::test]
async fn available_categories_are_distinct_and_sorted() {
    let db = seeded();
    let cats = available_categories(&db)
        .await
        .expect("available_categories ok");
    let expected = vec![
        "Coffee & snacks".to_owned(),
        "Going out".to_owned(),
        "Groceries".to_owned(),
        "Health insurance".to_owned(),
        "Rent".to_owned(),
        "Shopping".to_owned(),
        "Transport".to_owned(),
    ];
    assert_eq!(cats, expected, "ascending, de-duplicated category names");
}

/// There are exactly seven distinct categories (no duplicates from the three
/// Groceries receipts).
#[tokio::test]
async fn available_categories_has_seven_entries() {
    let db = seeded();
    let cats = available_categories(&db)
        .await
        .expect("available_categories ok");
    assert_eq!(
        cats.len(),
        7,
        "seven distinct categories across all receipts"
    );
}

/// The list is strictly ascending (sorted, no dupes) — a structural invariant the
/// filter UI relies on.
#[tokio::test]
async fn available_categories_is_strictly_ascending() {
    let db = seeded();
    let cats = available_categories(&db)
        .await
        .expect("available_categories ok");
    assert!(
        cats.windows(2).all(|w| w[0] < w[1]),
        "strictly ascending: sorted and de-duplicated"
    );
}

/// `available_categories` spans the WHOLE receipt table (it is the unfiltered
/// option list), so it is independent of any cycle window — its set of names is a
/// superset of (here equal to) the current cycle's `category_spend` names.
#[tokio::test]
async fn available_categories_covers_category_spend_names() {
    let db = seeded();
    let cats = available_categories(&db)
        .await
        .expect("available_categories ok");
    let rows = category_spend(&db, as_of())
        .await
        .expect("category_spend ok");
    for row in &rows {
        assert!(
            cats.contains(&row.category),
            "{} from the cycle roll-up is a known filter option",
            row.category
        );
    }
}

// ── Write side: create ─────────────────────────────────────────────────────────

/// A `NewCategory` request with everything but the name held fixed.
fn new_category(name: &str) -> NewCategory {
    NewCategory {
        name: name.to_owned(),
        cap: Some(Money::from_centimes(15_000)),
        fixed: false,
        glyph: "☂".to_owned(),
        note: "rainy-day envelope".to_owned(),
    }
}

/// A created category is stored, readable by name, and stamped `UserEntered`
/// with a slug derived from its name.
#[tokio::test]
async fn create_category_stores_a_user_entered_record() {
    let db = seeded();
    let before = db.category_caps().await.expect("caps ok").len();

    let made = create_category(&db, new_category("Rainy day"))
        .await
        .expect("create ok");
    assert_eq!(made.name, "Rainy day");
    assert_eq!(made.slug, "rainy-day", "slug derived from the name");
    assert_eq!(made.provenance.source, Source::UserEntered);
    assert_eq!(made.provenance.confidence, 1.0);
    assert_eq!(made.cap.map(|m| m.centimes()), Some(15_000));

    let stored = db
        .category_cap_by_name("Rainy day")
        .await
        .expect("readable by name");
    assert_eq!(stored, made, "what was returned is what was stored");
    assert_eq!(
        db.category_caps().await.expect("caps ok").len(),
        before + 1,
        "exactly one category was added"
    );
}

/// The name is trimmed, and a blank / overlong / control-character name is
/// refused before the store is touched.
#[tokio::test]
async fn create_category_validates_the_name() {
    let db = seeded();

    let made = create_category(&db, new_category("  Padded  "))
        .await
        .expect("create ok");
    assert_eq!(made.name, "Padded", "the name is trimmed");

    for bad in ["", "   ", "Side\nsplit", "Tab\tbed"] {
        let res = create_category(&db, new_category(bad)).await;
        assert!(
            matches!(res, Err(PhoskError::Invalid(_))),
            "{bad:?} is not a category name, got {res:?}"
        );
    }
    let too_long = "x".repeat(49);
    let res = create_category(&db, new_category(&too_long)).await;
    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "a 49-character name is refused, got {res:?}"
    );
    let at_limit = "y".repeat(48);
    create_category(&db, new_category(&at_limit))
        .await
        .expect("48 characters is still a name");
}

/// Two categories may not carry the same name, whatever the casing — they are
/// the same envelope to a human.
#[tokio::test]
async fn create_category_rejects_a_duplicate_name_case_insensitively() {
    let db = seeded();
    for taken in ["Groceries", "groceries", "GROCERIES", " Groceries "] {
        let res = create_category(&db, new_category(taken)).await;
        assert!(
            matches!(res, Err(PhoskError::Invalid(_))),
            "{taken:?} collides with the seeded Groceries, got {res:?}"
        );
    }
    assert_eq!(
        db.category_caps().await.expect("caps ok").len(),
        8,
        "no rejected create reached the store"
    );
}

/// A negative cap is not a budget; money stays exact and non-negative here.
#[tokio::test]
async fn create_category_rejects_a_negative_cap() {
    let db = seeded();
    let req = NewCategory {
        cap: Some(Money::from_centimes(-1)),
        ..new_category("Overdraft")
    };
    let res = create_category(&db, req).await;
    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "a negative cap is refused, got {res:?}"
    );
    let unlimited = NewCategory {
        cap: None,
        ..new_category("Unlimited")
    };
    let made = create_category(&db, unlimited).await.expect("create ok");
    assert_eq!(made.cap, None, "no cap means unlimited, not zero");
}

/// Slugs stay unique even when two different names slugify the same way.
#[tokio::test]
async fn create_category_derives_a_unique_slug() {
    let db = seeded();
    let first = create_category(&db, new_category("Fun stuff"))
        .await
        .expect("create ok");
    let second = create_category(&db, new_category("Fun & stuff"))
        .await
        .expect("create ok");
    assert_eq!(first.slug, "fun-stuff");
    assert_eq!(second.slug, "fun-stuff-2", "the colliding slug is suffixed");
    assert_ne!(first.id, second.id);

    let symbols = create_category(&db, new_category("✦✦"))
        .await
        .expect("create ok");
    assert_eq!(
        symbols.slug, "category",
        "a name with no ASCII falls back to a usable slug"
    );
}

/// An empty glyph falls back to the default one; the note is optional.
#[tokio::test]
async fn create_category_defaults_the_glyph_and_allows_no_note() {
    let db = seeded();
    let req = NewCategory {
        glyph: String::new(),
        note: "   ".to_owned(),
        ..new_category("Spartan")
    };
    let made = create_category(&db, req).await.expect("create ok");
    assert_eq!(made.glyph, "◆", "the default glyph is used");
    assert_eq!(made.note, "", "a blank note is stored as empty");
}

// ── Write side: rename ─────────────────────────────────────────────────────────

/// A rename carries the receipts with it: the spend roll-up reports the new
/// name with the same totals, and the old name disappears everywhere.
#[tokio::test]
async fn rename_category_carries_the_spend_history() {
    let db = seeded();
    let rows = category_spend(&db, as_of()).await.expect("spend ok");
    let before = find(&rows, "Groceries").clone();

    rename_category(&db, "Groceries", "Food & drink")
        .await
        .expect("rename ok");

    let rows = category_spend(&db, as_of()).await.expect("spend ok");
    assert!(
        !rows.iter().any(|r| r.category == "Groceries"),
        "the old name is gone from the roll-up"
    );
    let after = find(&rows, "Food & drink");
    assert_eq!(after.total, before.total, "the total moved unchanged");
    assert_eq!(after.txns, before.txns, "the receipt count moved unchanged");
    assert_eq!(rows.len(), 7, "renaming does not create a category");

    let cats = available_categories(&db).await.expect("options ok");
    assert!(cats.contains(&"Food & drink".to_owned()));
    assert!(!cats.contains(&"Groceries".to_owned()));
}

/// The record keeps its id and slug (the UI and the budget history key on
/// them), and its provenance becomes `UserModified`.
#[tokio::test]
async fn rename_category_keeps_the_id_and_slug_and_stamps_user_modified() {
    let db = seeded();
    let before = db
        .category_cap_by_name("Transport")
        .await
        .expect("seeded cap");
    let history = db.budget_history("Transport").await.expect("history ok");
    assert!(!history.is_empty(), "Transport has seeded history");

    rename_category(&db, "Transport", "Mobility")
        .await
        .expect("rename ok");

    let after = db.category_cap_by_name("Mobility").await.expect("renamed");
    assert_eq!(after.id, before.id, "the id is stable");
    assert_eq!(after.slug, before.slug, "the slug is stable");
    assert_eq!(after.cap, before.cap, "the cap is untouched");
    assert_eq!(after.fixed, before.fixed);
    assert_eq!(after.provenance.source, Source::UserModified);
    assert_eq!(
        db.budget_history("Mobility").await.expect("history ok"),
        history,
        "the history follows the renamed category (it keys on the id)"
    );
}

/// Renaming something that is not there is `NotFound`; renaming onto an
/// existing name is `Invalid` — and neither touches the store.
#[tokio::test]
async fn rename_category_rejects_unknown_and_colliding_names() {
    let db = seeded();
    let res = rename_category(&db, "Nonexistent", "Whatever").await;
    assert!(
        matches!(res, Err(PhoskError::NotFound(_))),
        "unknown source category, got {res:?}"
    );

    for taken in ["Transport", "transport"] {
        let res = rename_category(&db, "Groceries", taken).await;
        assert!(
            matches!(res, Err(PhoskError::Invalid(_))),
            "renaming onto {taken:?} collides, got {res:?}"
        );
    }
    let res = rename_category(&db, "Groceries", "  ").await;
    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "a blank new name is refused, got {res:?}"
    );
    assert!(
        db.category_cap_by_name("Groceries").await.is_ok(),
        "no rejected rename changed anything"
    );
}

/// Renaming a category to the name it already has is a harmless no-op; a
/// case-only change is a real rename.
#[tokio::test]
async fn rename_category_handles_same_name_and_case_only_changes() {
    let db = seeded();
    let before = db.category_cap_by_name("Rent").await.expect("seeded cap");
    rename_category(&db, "Rent", "Rent").await.expect("no-op ok");
    assert_eq!(
        db.category_cap_by_name("Rent").await.expect("still there"),
        before,
        "a same-name rename changes nothing at all"
    );

    rename_category(&db, "Rent", "RENT")
        .await
        .expect("case-only rename ok");
    let after = db.category_cap_by_name("RENT").await.expect("renamed");
    assert_eq!(after.id, before.id);
    assert_eq!(after.provenance.source, Source::UserModified);
    let receipts = db.all_receipts().await.expect("receipts ok");
    assert!(
        receipts.iter().any(|r| r.category == "RENT"),
        "the receipts followed the case change"
    );
}

// ── Write side: usage + delete-if-empty ────────────────────────────────────────

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

/// `category_usage` counts the receipt and each of its lines that carry the
/// name; an unused name counts zero.
#[tokio::test]
async fn category_usage_counts_receipts_and_their_lines() {
    let db = seeded();
    create_category(&db, new_category("Hobby"))
        .await
        .expect("create ok");
    assert_eq!(
        category_usage(&db, "Hobby").await.expect("usage ok"),
        0,
        "a fresh category is unused"
    );

    let id = ReceiptId::new();
    let lines = vec![hobby_line(id, "Brush"), hobby_line(id, "Canvas")];
    db.insert_receipt(hobby_receipt(id), lines)
        .await
        .expect("insert ok");

    assert_eq!(
        category_usage(&db, "Hobby").await.expect("usage ok"),
        3,
        "one receipt plus its two lines"
    );
    assert_eq!(
        category_usage(&db, "Nonexistent").await.expect("usage ok"),
        0
    );
}

/// A category that still carries history cannot be deleted, and the error says
/// which category held it back.
#[tokio::test]
async fn delete_category_refuses_while_the_category_is_used() {
    let db = seeded();
    match delete_category(&db, "Groceries").await {
        Err(PhoskError::Invalid(msg)) => assert!(
            msg.contains("Groceries"),
            "the message names the category: {msg}"
        ),
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert!(
        db.category_cap_by_name("Groceries").await.is_ok(),
        "the category survived the refused delete"
    );
    let rows = category_spend(&db, as_of()).await.expect("spend ok");
    assert_eq!(
        find(&rows, "Groceries").total.centimes(),
        13_095,
        "its spend is intact"
    );
}

/// An unused category is deletable; an unknown one is `NotFound`.
#[tokio::test]
async fn delete_category_removes_an_unused_one() {
    let db = seeded();
    create_category(&db, new_category("Temporary"))
        .await
        .expect("create ok");
    let before = db.category_caps().await.expect("caps ok").len();

    delete_category(&db, "Temporary").await.expect("delete ok");

    assert!(
        matches!(
            db.category_cap_by_name("Temporary").await,
            Err(PhoskError::NotFound(_))
        ),
        "the category is gone"
    );
    assert_eq!(
        db.category_caps().await.expect("caps ok").len(),
        before - 1,
        "exactly one category was removed"
    );

    let res = delete_category(&db, "Temporary").await;
    assert!(
        matches!(res, Err(PhoskError::NotFound(_))),
        "deleting it twice is NotFound, got {res:?}"
    );
}

/// The two write paths compose: after a rename the old name holds nothing, and
/// the new one carries the history that blocks its deletion.
#[tokio::test]
async fn a_category_emptied_by_a_rename_becomes_deletable() {
    let db = seeded();
    rename_category(&db, "Groceries", "Sundry stuff")
        .await
        .expect("rename ok");

    assert_eq!(
        category_usage(&db, "Groceries").await.expect("usage ok"),
        0,
        "nothing references the vacated name"
    );
    let res = delete_category(&db, "Groceries").await;
    assert!(
        matches!(res, Err(PhoskError::NotFound(_))),
        "the vacated name has no record left to delete, got {res:?}"
    );
    let res = delete_category(&db, "Sundry stuff").await;
    assert!(
        matches!(res, Err(PhoskError::Invalid(_))),
        "the renamed category now carries the history, got {res:?}"
    );
}
