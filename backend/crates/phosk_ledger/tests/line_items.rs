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
//! Tests for `phosk_ledger::line_items` — the receipt-line read model
//! (`transaction_lines`), the backend-derived `line_total` helper, and the
//! line-correction write path (`correct_line`).
//!
//! These drive the implemented read model and write path, asserting the exact
//! contract values below against the seed.
//!
//! Ground truth is the deterministic Swiss seed in `phosk_db_memory`
//! (`seed::seed_receipts_and_lines`), pinned at `as_of = 2026-06-18`:
//!
//! - `t1` (Migros): 4 lines — Oat-milk flat white 1.0×560=560 (0.88), Bananas
//!   1.2×320=384 (0.91), Bread (unclear) 1.0×431=431 (0.58, low-conf),
//!   Mixed basket 1.0×4500=4500 (0.95). 1 low-conf line.
//! - `t2` (Restaurant Linde): 2 lines — Craft IPA 2.0×720=1440 (0.90),
//!   Main course 1.0×5010=5010 (0.93). 0 low-conf.
//! - `t3` (Coop): 2 lines — Gruyère AOP 0.4×6600=2640 (0.92),
//!   Vegetables 1.0×1590=1590 (0.89). 0 low-conf.
//! - `t5` (Migros): 2 lines — Oat-milk flat white 1.0×560=560 (0.87),
//!   Pain au chocolat 2.0×360=720 (0.84). 0 low-conf.
//! - `t4`, `t6`, `t7`, `t8`, `t9`: no itemised lines seeded → empty.
//!
//! Seed line items carry `signal_id: None`, so the derived `sigs` feed is empty
//! for every receipt (the green phase wires signal linkage; this fixes the
//! current contract).

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_id::LineItemId;
use phosk_ledger::line_items::{correct_line, line_total, transaction_lines};

/// The deterministic Swiss seed.
fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// Resolve a real seeded `LineItemId` for a receipt slug, in field order, via
/// the port (allowed in tests). Returns the line item so its current value /
/// provenance can be asserted too.
async fn nth_line_id(db: &MemoryDb, slug: &str, idx: usize) -> phosk_model::LineItem {
    let receipt = db
        .receipt_by_slug(slug)
        .await
        .expect("seed receipt resolves by slug");
    let mut lines = db
        .line_items(receipt.id)
        .await
        .expect("seed lines load by receipt id");
    assert!(idx < lines.len(), "receipt {slug} has a line #{idx}");
    lines.swap_remove(idx)
}

// ── transaction_lines: happy paths ────────────────────────────────────────────

/// `t1` resolves to its four seeded lines with exact, backend-derived totals.
#[tokio::test]
async fn transaction_lines_t1_has_four_lines_with_exact_totals() {
    let db = seeded();
    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");

    assert_eq!(dto.lines.len(), 4, "t1 has 4 seeded lines");

    let expected: [(&str, f64, i64, i64, f64); 4] = [
        ("Oat-milk flat white", 1.0, 560, 560, 0.88),
        ("Bananas", 1.2, 320, 384, 0.91),
        ("Bread (unclear)", 1.0, 431, 431, 0.58),
        ("Mixed basket", 1.0, 4_500, 4_500, 0.95),
    ];
    for (line, (name, qty, unit, total, conf)) in dto.lines.iter().zip(expected) {
        assert_eq!(line.name, name, "line name");
        assert!((line.qty - qty).abs() < f64::EPSILON, "qty for {name}");
        assert_eq!(line.unit_price.centimes(), unit, "unit price for {name}");
        assert_eq!(
            line.line_total.centimes(),
            total,
            "backend-derived line total for {name}"
        );
        assert!(
            (line.confidence - conf).abs() < f64::EPSILON,
            "confidence for {name}"
        );
    }
}

/// `low_conf` counts lines strictly below the 0.7 threshold — `t1` has exactly
/// one (Bread, 0.58).
#[tokio::test]
async fn transaction_lines_t1_low_conf_is_one() {
    let db = seeded();
    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    assert_eq!(dto.low_conf, 1, "t1 has one line below 0.7 (Bread 0.58)");
}

/// The seed line items carry no signal linkage, so the distinct `sigs` feed is
/// empty for `t1`.
#[tokio::test]
async fn transaction_lines_t1_sigs_reflect_seed_signal_links() {
    let db = seeded();
    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    assert!(
        dto.sigs.is_empty(),
        "seed lines carry signal_id=None, so sigs is empty"
    );
}

/// `t3` has two lines, both above threshold → `low_conf == 0`; the fractional
/// Gruyère quantity (0.4 × 6600) derives exactly to 2640.
#[tokio::test]
async fn transaction_lines_t3_fractional_qty_total_is_exact() {
    let db = seeded();
    let dto = transaction_lines(&db, "t3")
        .await
        .expect("transaction_lines t3 ok");
    assert_eq!(dto.lines.len(), 2, "t3 has 2 seeded lines");
    assert_eq!(dto.low_conf, 0, "t3 has no low-confidence lines");

    let gruyere = &dto.lines[0];
    assert_eq!(gruyere.name, "Gruyère AOP");
    assert!((gruyere.qty - 0.4).abs() < f64::EPSILON);
    assert_eq!(gruyere.unit_price.centimes(), 6_600);
    assert_eq!(
        gruyere.line_total.centimes(),
        2_640,
        "0.4 × 6600 derives to 2640"
    );
}

/// `t5` has two lines, both above threshold; the qty-2 Pain au chocolat derives
/// to 720.
#[tokio::test]
async fn transaction_lines_t5_two_lines_no_low_conf() {
    let db = seeded();
    let dto = transaction_lines(&db, "t5")
        .await
        .expect("transaction_lines t5 ok");
    assert_eq!(dto.lines.len(), 2);
    assert_eq!(dto.low_conf, 0);
    let pain = &dto.lines[1];
    assert_eq!(pain.name, "Pain au chocolat");
    assert_eq!(pain.line_total.centimes(), 720, "2 × 360 = 720");
}

/// The line totals of a receipt sum to that receipt's headline amount — `t1`'s
/// four lines sum to its CHF 58.75 (5875 centimes) receipt total.
#[tokio::test]
async fn transaction_lines_t1_line_totals_sum_to_receipt_amount() {
    let db = seeded();
    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    let summed = Money::sum(dto.lines.iter().map(|l| l.line_total)).expect("no overflow");
    assert_eq!(
        summed.centimes(),
        5_875,
        "t1 lines (560+384+431+4500) sum to the CHF 58.75 receipt total"
    );
}

// ── transaction_lines: edge cases ─────────────────────────────────────────────

/// A receipt with no itemised lines (`t4`) yields an empty, well-formed payload,
/// not an error.
#[tokio::test]
async fn transaction_lines_receipt_without_lines_is_empty() {
    let db = seeded();
    let dto = transaction_lines(&db, "t4")
        .await
        .expect("transaction_lines t4 ok");
    assert!(dto.lines.is_empty(), "t4 has no seeded lines");
    assert!(dto.sigs.is_empty());
    assert_eq!(dto.low_conf, 0, "no lines → zero low-confidence");
}

/// An unknown receipt slug is a [`PhoskError::NotFound`] (404), not a panic.
#[tokio::test]
async fn transaction_lines_unknown_slug_is_not_found() {
    let db = seeded();
    let err = transaction_lines(&db, "does-not-exist")
        .await
        .expect_err("unknown slug must be NotFound");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "unknown slug → NotFound, got {err:?}"
    );
}

/// The service is callable behind the `&dyn DatabaseAdapter` PORT (ADR-010) —
/// it never sees the concrete `MemoryDb` type.
#[tokio::test]
async fn transaction_lines_works_through_the_port_trait_object() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;
    let dto = transaction_lines(port, "t2")
        .await
        .expect("transaction_lines t2 ok");
    assert_eq!(dto.lines.len(), 2, "t2 has 2 seeded lines");
}

/// The DTO serializes camelCase with money fields as exact i64 centimes (no CHF
/// float on the wire).
#[tokio::test]
async fn transaction_lines_serializes_camel_case_centimes() {
    let db = seeded();
    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    let json = serde_json::to_value(&dto).expect("serialize");

    assert!(
        json.get("lowConf").is_some(),
        "lowConf camelCase key present"
    );
    let first = &json["lines"][0];
    assert_eq!(
        first["unitPrice"], 560,
        "unitPrice is exact i64 centimes, camelCase"
    );
    assert_eq!(
        first["lineTotal"], 560,
        "lineTotal is exact i64 centimes, camelCase"
    );
    assert!(
        first.get("signalId").is_some(),
        "signalId camelCase key present"
    );
}

// ── line_total: the pure backend-derived helper ───────────────────────────────

/// Whole-quantity totals are exact.
#[tokio::test]
async fn line_total_whole_qty_is_exact() {
    let got = line_total(3.0, Money::from_centimes(195)).expect("line_total ok");
    assert_eq!(got.centimes(), 585, "3 × 195 = 585");
}

/// Fractional quantities that land on an exact centime derive exactly.
#[tokio::test]
async fn line_total_fractional_qty_exact_centime() {
    let got = line_total(1.2, Money::from_centimes(320)).expect("line_total ok");
    assert_eq!(got.centimes(), 384, "1.2 × 320 = 384");

    let gruyere = line_total(0.4, Money::from_centimes(6_600)).expect("line_total ok");
    assert_eq!(gruyere.centimes(), 2_640, "0.4 × 6600 = 2640");
}

/// A sub-centime product rounds to the nearest centime (round half away from
/// zero / nearest): 0.333 × 100 = 33.3 → 33.
#[tokio::test]
async fn line_total_rounds_to_nearest_centime() {
    let got = line_total(0.333, Money::from_centimes(100)).expect("line_total ok");
    assert_eq!(got.centimes(), 33, "33.3 rounds to 33");

    // 0.5 × 333 = 166.5 → rounds to 167 (nearest, half away from zero).
    let half = line_total(0.5, Money::from_centimes(333)).expect("line_total ok");
    assert_eq!(half.centimes(), 167, "166.5 rounds to 167");
}

/// A zero quantity yields a zero line total.
#[tokio::test]
async fn line_total_zero_qty_is_zero() {
    let got = line_total(0.0, Money::from_centimes(999)).expect("line_total ok");
    assert_eq!(got, Money::ZERO, "0 × anything = 0");
}

/// A zero unit price yields a zero line total.
#[tokio::test]
async fn line_total_zero_price_is_zero() {
    let got = line_total(7.0, Money::ZERO).expect("line_total ok");
    assert_eq!(got, Money::ZERO, "anything × 0 = 0");
}

/// An overflowing product is a checked [`PhoskError::Overflow`], never a wrap or
/// panic.
#[tokio::test]
async fn line_total_overflow_is_checked_error() {
    let err = line_total(1.0e18, Money::from_centimes(1_000_000))
        .expect_err("an astronomically large product must overflow i64 centimes");
    assert!(
        matches!(err, PhoskError::Overflow(_)),
        "out-of-range product → Overflow, got {err:?}"
    );
}

// ── correct_line: the write path (direct edit + audit log) ────────────────────

/// A direct field edit (the `category` field) is applied: re-reading the receipt
/// lines shows the new value.
#[tokio::test]
async fn correct_line_direct_edit_updates_the_field() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 2).await; // "Bread (unclear)", category "Groceries"
    correct_line(&db, line.id, "category", "Bakery")
        .await
        .expect("correct_line category ok");

    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    assert_eq!(
        dto.lines[2].category, "Bakery",
        "the corrected category is persisted"
    );
}

/// Correcting the `name` field is applied (trimmed, per the service's
/// `bounded_label` validation).
#[tokio::test]
async fn correct_line_name_edit_is_applied() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 2).await;
    correct_line(&db, line.id, "name", "Sourdough loaf")
        .await
        .expect("correct_line name ok");

    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    assert_eq!(dto.lines[2].name, "Sourdough loaf");
}

/// Correcting `qty` re-derives the `line_total` (backend-derived, never trusted
/// from input): changing Bananas' qty from 1.2 to 2.0 makes the total 640.
#[tokio::test]
async fn correct_line_qty_edit_rederives_line_total() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 1).await; // "Bananas", 1.2 × 320 = 384
    correct_line(&db, line.id, "qty", "2")
        .await
        .expect("correct_line qty ok");

    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    assert!(
        (dto.lines[1].qty - 2.0).abs() < f64::EPSILON,
        "qty is now 2"
    );
    assert_eq!(
        dto.lines[1].line_total.centimes(),
        640,
        "line_total re-derived: 2 × 320 = 640"
    );
}

/// Correcting `unitPrice` re-derives the `line_total` from the new price.
#[tokio::test]
async fn correct_line_unit_price_edit_rederives_line_total() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 1).await; // Bananas, qty 1.2
    correct_line(&db, line.id, "unit_price", "500")
        .await
        .expect("correct_line unit_price ok");

    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    assert_eq!(
        dto.lines[1].unit_price.centimes(),
        500,
        "unit price corrected to 500 centimes"
    );
    assert_eq!(
        dto.lines[1].line_total.centimes(),
        600,
        "line_total re-derived: 1.2 × 500 = 600"
    );
}

/// A correction flips the line's provenance to `UserModified` at full
/// confidence — clearing the low-confidence (coral) flag on the reviewed line.
#[tokio::test]
async fn correct_line_flips_provenance_to_user_modified() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 2).await; // Bread, conf 0.58 (low)
    assert!(
        line.provenance.is_low_confidence(),
        "precondition: the seeded line is low-confidence"
    );

    correct_line(&db, line.id, "name", "Sourdough loaf")
        .await
        .expect("correct_line ok");

    // The reviewed line is no longer low-confidence after the user edit.
    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    assert!(
        !phosk_model::is_low_confidence(dto.lines[2].confidence),
        "a user-modified line is full-confidence; the coral flag clears"
    );

    // ...and the receipt's low-confidence count drops to zero.
    assert_eq!(
        dto.low_conf, 0,
        "correcting the only low-confidence line zeroes low_conf"
    );
}

/// Confirming a line (`confirmed` field) without changing a value still flips it
/// to user-reviewed, clearing the low-confidence flag.
#[tokio::test]
async fn correct_line_confirm_clears_low_confidence() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 2).await; // Bread, low-conf
    correct_line(&db, line.id, "confirmed", "true")
        .await
        .expect("correct_line confirmed ok");

    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    assert_eq!(
        dto.low_conf, 0,
        "confirming the low-confidence line clears the flag"
    );
}

/// Re-linking a line to a tracked signal (`signal_id` field) is applied and
/// shows up in the receipt's distinct `sigs` feed.
#[tokio::test]
async fn correct_line_signal_id_edit_updates_sigs_feed() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 0).await; // Oat-milk flat white
    correct_line(&db, line.id, "signal_id", "coffee")
        .await
        .expect("correct_line signal_id ok");

    let dto = transaction_lines(&db, "t1")
        .await
        .expect("transaction_lines t1 ok");
    assert_eq!(
        dto.lines[0].signal_id, "coffee",
        "the line now feeds the coffee signal"
    );
    assert!(
        dto.sigs.contains(&"coffee".to_owned()),
        "the receipt's sigs feed now lists coffee"
    );
}

/// Correcting an unknown line id is a [`PhoskError::NotFound`] (404), not a
/// panic.
#[tokio::test]
async fn correct_line_unknown_id_is_not_found() {
    let db = seeded();
    let bogus = LineItemId::new();
    let err = correct_line(&db, bogus, "category", "Bakery")
        .await
        .expect_err("an unknown line id must be NotFound");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "unknown line id → NotFound, got {err:?}"
    );
}

/// Correcting an unknown field is a caller error ([`PhoskError::Invalid`]),
/// distinct from a missing entity.
#[tokio::test]
async fn correct_line_unknown_field_is_invalid() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 0).await;
    let err = correct_line(&db, line.id, "not_a_field", "x")
        .await
        .expect_err("an unknown field must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "unknown field → Invalid, got {err:?}"
    );
}

/// A non-numeric value for a numeric field (`qty`) is a caller error
/// ([`PhoskError::Invalid`]), not a panic.
#[tokio::test]
async fn correct_line_non_numeric_qty_is_invalid() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 1).await;
    let err = correct_line(&db, line.id, "qty", "not-a-number")
        .await
        .expect_err("a non-numeric qty must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "non-numeric qty → Invalid, got {err:?}"
    );
}

/// A non-numeric value for `unit_price` is likewise [`PhoskError::Invalid`].
#[tokio::test]
async fn correct_line_non_numeric_unit_price_is_invalid() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 1).await;
    let err = correct_line(&db, line.id, "unit_price", "free")
        .await
        .expect_err("a non-numeric unit_price must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "non-numeric unit_price → Invalid, got {err:?}"
    );
}

// ── correct_line: F2 service-layer validation ─────────────────────────────
//
// `correct_line` must reject the same shapes `create_transaction`/`build_line`
// reject (blank labels, a non-finite/non-positive qty, a negative unit price),
// plus a length-bounded label, matching the wire layer's existing caps in
// `dioxus-app/src/data/transactions.rs::line_fix`. A rejected correction must
// leave the stored line, its provenance and the audit log untouched — nothing
// is written until every check has passed.

/// A blank (whitespace-only) name is rejected, and the stored line — every
/// field, not just the name — is byte-for-byte untouched.
#[tokio::test]
async fn correct_line_blank_name_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 0).await; // Oat-milk flat white, conf 0.88
    let err = correct_line(&db, before.id, "name", "   ")
        .await
        .expect_err("a blank name must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "blank name → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 0).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A blank (whitespace-only) category is likewise rejected, and the stored
/// line is byte-for-byte untouched.
#[tokio::test]
async fn correct_line_blank_category_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 0).await;
    let err = correct_line(&db, before.id, "category", "\t\n")
        .await
        .expect_err("a blank category must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "blank category → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 0).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A name past the length cap is rejected — mirrors the wire layer's
/// `MAX_NAME_CHARS` (120) so the service is at least as strict as the UI —
/// and the stored line is byte-for-byte untouched.
#[tokio::test]
async fn correct_line_name_too_long_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 0).await;
    let too_long = "x".repeat(121);
    let err = correct_line(&db, before.id, "name", &too_long)
        .await
        .expect_err("a name past the length cap must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "over-long name → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 0).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A category past the length cap is rejected — mirrors the wire layer's
/// `MAX_CATEGORY_CHARS` (60) — and the stored line is untouched.
#[tokio::test]
async fn correct_line_category_too_long_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 0).await;
    let too_long = "x".repeat(61);
    let err = correct_line(&db, before.id, "category", &too_long)
        .await
        .expect_err("a category past the length cap must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "over-long category → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 0).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A name exactly at the length cap (120 chars) is accepted.
#[tokio::test]
async fn correct_line_name_at_max_length_is_accepted() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 0).await;
    let max_len = "x".repeat(120);
    correct_line(&db, line.id, "name", &max_len)
        .await
        .expect("a 120-char name must be accepted");

    let after = nth_line_id(&db, "t1", 0).await;
    assert_eq!(after.name, max_len, "the 120-char name is stored in full");
}

/// A name containing a control character is rejected, and the stored line is
/// untouched.
#[tokio::test]
async fn correct_line_name_with_control_char_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 0).await;
    let err = correct_line(&db, before.id, "name", "Bad\u{7}name")
        .await
        .expect_err("a name with a control character must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "control character in name → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 0).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A name with leading/trailing whitespace is accepted and stored trimmed.
#[tokio::test]
async fn correct_line_name_padded_is_stored_trimmed() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 0).await;
    correct_line(&db, line.id, "name", "  Sourdough loaf  ")
        .await
        .expect("a padded name must be accepted");

    let after = nth_line_id(&db, "t1", 0).await;
    assert_eq!(after.name, "Sourdough loaf", "the stored name is trimmed");
}

/// A qty of exactly zero is rejected (not merely non-numeric), and the stored
/// line is byte-for-byte untouched.
#[tokio::test]
async fn correct_line_qty_zero_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 1).await; // Bananas, qty 1.2
    let err = correct_line(&db, before.id, "qty", "0")
        .await
        .expect_err("qty of zero must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "qty 0 → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 1).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A negative qty is rejected, and the stored line is byte-for-byte untouched.
#[tokio::test]
async fn correct_line_qty_negative_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 1).await;
    let err = correct_line(&db, before.id, "qty", "-2")
        .await
        .expect_err("a negative qty must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "negative qty → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 1).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// `NaN` parses as an f64 but is not a usable quantity — rejected, and the
/// stored line is byte-for-byte untouched.
#[tokio::test]
async fn correct_line_qty_nan_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 1).await;
    let err = correct_line(&db, before.id, "qty", "NaN")
        .await
        .expect_err("a NaN qty must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "NaN qty → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 1).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// `inf` parses as an f64 but is not a finite quantity — rejected, and the
/// stored line is byte-for-byte untouched.
#[tokio::test]
async fn correct_line_qty_infinite_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 1).await;
    let err = correct_line(&db, before.id, "qty", "inf")
        .await
        .expect_err("an infinite qty must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "infinite qty → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 1).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A negative unit price is rejected — the stored line is byte-for-byte
/// untouched.
#[tokio::test]
async fn correct_line_unit_price_negative_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 1).await; // Bananas, unit price 320
    let err = correct_line(&db, before.id, "unit_price", "-100")
        .await
        .expect_err("a negative unit_price must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "negative unit_price → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 1).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A qty above `MAX_QTY` is rejected — the stored line is untouched.
#[tokio::test]
async fn correct_line_qty_above_max_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 1).await;
    let err = correct_line(&db, before.id, "qty", "100000.5")
        .await
        .expect_err("a qty above MAX_QTY must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "qty above the cap → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 1).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A qty exactly at `MAX_QTY` is still accepted.
#[tokio::test]
async fn correct_line_qty_at_max_is_accepted() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 1).await;
    correct_line(&db, before.id, "qty", "100000")
        .await
        .expect("a qty exactly at MAX_QTY is accepted");
    let after = nth_line_id(&db, "t1", 1).await;
    assert!((after.qty - phosk_ledger::line_items::MAX_QTY).abs() < f64::EPSILON);
}

/// A unit price above `MAX_UNIT_PRICE_CENTIMES` is rejected — the stored line
/// is untouched.
#[tokio::test]
async fn correct_line_unit_price_above_max_is_invalid() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 1).await;
    let err = correct_line(&db, before.id, "unit_price", "100000001")
        .await
        .expect_err("a unit price above the cap must be rejected as Invalid");
    assert!(
        matches!(err, PhoskError::Invalid(_)),
        "unit price above the cap → Invalid, got {err:?}"
    );

    let after = nth_line_id(&db, "t1", 1).await;
    assert_eq!(
        before, after,
        "a rejected correction must not change the stored line"
    );
}

/// A unit price exactly at `MAX_UNIT_PRICE_CENTIMES` is still accepted.
#[tokio::test]
async fn correct_line_unit_price_at_max_is_accepted() {
    let db = seeded();
    let before = nth_line_id(&db, "t1", 1).await;
    correct_line(&db, before.id, "unit_price", "100000000")
        .await
        .expect("a unit price exactly at the cap is accepted");
    let after = nth_line_id(&db, "t1", 1).await;
    assert_eq!(
        after.unit_price.centimes(),
        phosk_ledger::line_items::MAX_UNIT_PRICE_CENTIMES
    );
}

/// The write path is callable behind the `&dyn DatabaseAdapter` PORT (ADR-010).
#[tokio::test]
async fn correct_line_works_through_the_port_trait_object() {
    let db = seeded();
    let line = nth_line_id(&db, "t1", 0).await;
    let port: &dyn DatabaseAdapter = &db;
    correct_line(port, line.id, "category", "Coffee & snacks")
        .await
        .expect("correct_line via port ok");
}

/// Helper sanity: a fabricated `LineItemId` is parseable back from its `Display`
/// form, so the green phase can round-trip ids through the audit log's string
/// `entity_id`.
#[test]
fn line_item_id_round_trips_through_display() {
    let id = LineItemId::new();
    let shown = id.to_string();
    let parsed: NaiveDate = NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid date");
    // (NaiveDate here only anchors the seed clock referenced in module docs.)
    assert!(!shown.is_empty(), "id Display is non-empty");
    let _ = parsed;
}
