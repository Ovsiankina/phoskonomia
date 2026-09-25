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
//! RED tests for `phosk_recurring::recurring_detect`.
//!
//! These pin the recurring-detection AI slice against the deterministic Swiss
//! seed (`phosk_db_memory::MemoryDb::seeded`) at `as_of = 2026-06-18`:
//!
//! - `detect` scans the seed receipts and surfaces standing-charge CANDIDATES
//!   (`source == LlmInferred` ⇒ wire `"llm"`), most-confident first, money in
//!   exact i64 centimes, with a `scanned` receipt count.
//! - `confirm_candidate(slug)` flips the matching subscription's provenance to
//!   `UserEntered` (wire `source == "user"`) and persists it; `NotFound` for an
//!   unknown slug.
//! - `dismiss_candidate(slug)` drops the candidate; `NotFound` for an unknown
//!   slug.
//! - Usage-based review flags: the `gym` sub (status `watch`) stays flagged.
//!
//! Every body under test is `todo!()`, so each test COMPILES and then panics at
//! runtime — i.e. RED. No production logic lives here. `expect("msg")` is used
//! for fallible setup; never a bare `unwrap()`.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_db_memory::MemoryDb;
use phosk_recurring::lifecycle::resume_subscription;
use phosk_recurring::recurring_detect::{
    DetectionDto, RecurringCandidateDto, confirm_candidate, detect, dismiss_candidate,
};
use phosk_recurring::subscriptions::{SubFilter, list_subscriptions, subscription_detail};

/// The seeded demo clock: day 18 of the June 2026 cycle.
fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid as_of date")
}

/// A fresh seeded in-memory adapter.
fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

// ── detect: happy path ────────────────────────────────────────────────────────

/// `detect` returns a `DetectionDto`; it never errors against the seed.
#[tokio::test]
async fn detect_succeeds_against_seed() {
    let db = seeded();
    let out: DetectionDto = detect(&db, as_of()).await.expect("detect ok");
    // The seed has receipts to scan, so there is a non-zero scanned count.
    assert!(out.scanned > 0, "detect scans the seeded receipts");
}

/// `scanned` equals the number of receipts the port exposes (the seed's t1..t9).
#[tokio::test]
async fn detect_scanned_count_matches_all_receipts() {
    let db = seeded();
    let receipts = db.all_receipts().await.expect("all_receipts ok");
    let expected = u32::try_from(receipts.len()).expect("receipt count fits u32");
    assert_eq!(expected, 9, "the seed exposes nine receipts t1..t9");

    let out = detect(&db, as_of()).await.expect("detect ok");
    assert_eq!(
        out.scanned, expected,
        "scanned == number of receipts examined"
    );
}

/// Candidates are LLM-inferred ⇒ confidence strictly below 1.0 and within 0..1.
#[tokio::test]
async fn detect_candidates_are_llm_confidence_band() {
    let db = seeded();
    let out = detect(&db, as_of()).await.expect("detect ok");
    assert!(
        !out.candidates.is_empty(),
        "the seed surfaces at least one recurring candidate"
    );
    for c in &out.candidates {
        assert!(
            (0.0..1.0).contains(&c.confidence),
            "LLM-inferred candidate {} confidence {} is in [0.0, 1.0)",
            c.id,
            c.confidence
        );
    }
}

/// Candidates are ordered most-confident first.
#[tokio::test]
async fn detect_candidates_sorted_most_confident_first() {
    let db = seeded();
    let out = detect(&db, as_of()).await.expect("detect ok");
    let confs: Vec<f64> = out.candidates.iter().map(|c| c.confidence).collect();
    let mut sorted = confs.clone();
    sorted.sort_by(|a, b| b.partial_cmp(a).expect("no NaN confidences"));
    assert_eq!(
        confs, sorted,
        "candidates are returned most-confident first"
    );
}

/// The seeded LLM subscription `icloud` (auto-detected from receipts) surfaces
/// as a candidate.
#[tokio::test]
async fn detect_surfaces_icloud_candidate() {
    let db = seeded();
    let out = detect(&db, as_of()).await.expect("detect ok");
    let icloud: &RecurringCandidateDto = out
        .candidates
        .iter()
        .find(|c| c.id == "icloud")
        .expect("icloud surfaces as a recurring candidate");
    // iCloud+ 2TB seed amount is CHF 9.99 = 999 centimes, monthly on day 15.
    assert_eq!(
        icloud.amount.centimes(),
        999,
        "icloud amount is 999 centimes"
    );
    assert_eq!(icloud.cadence, "monthly", "icloud is a monthly charge");
    assert_eq!(icloud.day, 15, "icloud bills on day 15");
    assert!(
        !icloud.name.is_empty(),
        "candidate carries an inferred service name"
    );
    assert!(
        !icloud.rationale.is_empty(),
        "candidate carries a rationale line for the AI feed"
    );
    assert!(
        icloud.occurrences >= 1,
        "candidate cites at least one observed charge"
    );
}

/// Candidates expose money as exact i64 centimes (camelCase JSON wire form).
#[tokio::test]
async fn detect_candidate_serializes_amount_as_centimes() {
    let db = seeded();
    let out = detect(&db, as_of()).await.expect("detect ok");
    let icloud = out
        .candidates
        .iter()
        .find(|c| c.id == "icloud")
        .expect("icloud candidate present");
    let v = serde_json::to_value(icloud).expect("candidate serializes");
    assert_eq!(
        v.get("amount").and_then(serde_json::Value::as_i64),
        Some(999),
        "amount serializes as exact i64 centimes, not CHF float"
    );
    // camelCase keys present.
    assert!(v.get("rationale").is_some(), "rationale key present");
    assert!(
        v.get("occurrences").is_some(),
        "occurrences key present (camelCase)"
    );
}

// ── confirm_candidate ─────────────────────────────────────────────────────────

/// Confirming a candidate flips its subscription provenance to user-entered.
#[tokio::test]
async fn confirm_candidate_flips_source_to_user() {
    let db = seeded();

    // Pre-condition: icloud is auto-detected (wire source "llm").
    let before = list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    let icloud_before = before
        .iter()
        .find(|s| s.id == "icloud")
        .expect("icloud present before confirm");
    assert_eq!(icloud_before.source, "llm", "icloud starts auto-detected");

    confirm_candidate(&db, "icloud").await.expect("confirm ok");

    // Post-condition: icloud is now user-entered (wire source "user").
    let after = list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    let icloud_after = after
        .iter()
        .find(|s| s.id == "icloud")
        .expect("icloud present after confirm");
    assert_eq!(
        icloud_after.source, "user",
        "confirming flips source to user-entered"
    );
}

/// Confirming an unknown candidate is a `NotFound`.
#[tokio::test]
async fn confirm_unknown_candidate_is_not_found() {
    let db = seeded();
    let err = confirm_candidate(&db, "does-not-exist")
        .await
        .expect_err("unknown slug must error");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "unknown candidate ⇒ NotFound, got {err:?}"
    );
    assert_eq!(err.http_status(), 404, "NotFound is HTTP 404");
    assert_eq!(err.code(), "not_found");
}

/// Confirming reduces the auto-detected count in the next `detect` sweep.
#[tokio::test]
async fn confirm_removes_candidate_from_next_detection() {
    let db = seeded();
    let before = detect(&db, as_of()).await.expect("detect ok");
    assert!(
        before.candidates.iter().any(|c| c.id == "icloud"),
        "icloud is a candidate before confirmation"
    );

    confirm_candidate(&db, "icloud").await.expect("confirm ok");

    let after = detect(&db, as_of()).await.expect("detect ok");
    assert!(
        !after.candidates.iter().any(|c| c.id == "icloud"),
        "a confirmed charge is no longer an open candidate"
    );
}

// ── dismiss_candidate ─────────────────────────────────────────────────────────

/// Dismissing a candidate drops it from the next detection sweep.
#[tokio::test]
async fn dismiss_removes_candidate_from_next_detection() {
    let db = seeded();
    let before = detect(&db, as_of()).await.expect("detect ok");
    assert!(
        before.candidates.iter().any(|c| c.id == "icloud"),
        "icloud is a candidate before dismissal"
    );

    dismiss_candidate(&db, "icloud").await.expect("dismiss ok");

    let after = detect(&db, as_of()).await.expect("detect ok");
    assert!(
        !after.candidates.iter().any(|c| c.id == "icloud"),
        "a dismissed candidate no longer surfaces"
    );
}

/// Dismissing must NOT confirm: the sub does not become user-entered.
#[tokio::test]
async fn dismiss_does_not_confirm_subscription() {
    let db = seeded();
    dismiss_candidate(&db, "icloud").await.expect("dismiss ok");

    let subs = list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    // Either icloud is gone, or it is still NOT user-entered. It must never be
    // silently promoted to "user" by a dismissal.
    if let Some(icloud) = subs.iter().find(|s| s.id == "icloud") {
        assert_ne!(
            icloud.source, "user",
            "dismiss must not promote a candidate to user-entered"
        );
    }
}

/// Dismissing an unknown candidate is a `NotFound`.
#[tokio::test]
async fn dismiss_unknown_candidate_is_not_found() {
    let db = seeded();
    let err = dismiss_candidate(&db, "nope")
        .await
        .expect_err("unknown slug must error");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "unknown candidate ⇒ NotFound, got {err:?}"
    );
    assert_eq!(err.http_status(), 404);
}

/// Dismissing then resuming a candidate un-dismisses it: it is an open
/// candidate again in the inspector, per [`recurring_detect::is_open_candidate`]
/// (a resumed record's status is no longer `"paused"`).
#[tokio::test]
async fn dismiss_then_resume_makes_it_an_open_candidate_again() {
    let db = seeded();
    dismiss_candidate(&db, "icloud").await.expect("dismiss ok");

    resume_subscription(&db, "icloud", as_of())
        .await
        .expect("resume ok");

    let detail = subscription_detail(&db, as_of(), "icloud")
        .await
        .expect("detail ok");
    assert!(
        detail.candidate,
        "a resumed candidate is open again, not stuck dismissed"
    );
}

// ── usage-based review flags ──────────────────────────────────────────────────

/// `detect` never proposes a charge that is ALREADY a user-entered subscription
/// (no duplicate of netflix/spotify/gym/domain — those are user-confirmed).
#[tokio::test]
async fn detect_excludes_already_confirmed_subscriptions() {
    let db = seeded();
    let out = detect(&db, as_of()).await.expect("detect ok");
    for confirmed in ["netflix", "spotify", "gym", "domain"] {
        assert!(
            !out.candidates.iter().any(|c| c.id == confirmed),
            "{confirmed} is user-confirmed and must not be re-proposed as a candidate"
        );
    }
}

/// Detection on an empty database yields zero candidates and zero scanned, with
/// no panic / no divide-by-zero.
#[tokio::test]
async fn detect_on_empty_db_is_empty() {
    use phosk_core::money::Money;
    use phosk_model::BudgetConfig;
    let db = MemoryDb::new(
        Vec::new(),
        Vec::new(),
        BudgetConfig {
            monthly_budget: Money::ZERO,
            savings_target: Money::ZERO,
        },
    );
    let out = detect(&db, as_of()).await.expect("detect ok on empty db");
    assert_eq!(out.scanned, 0, "no receipts scanned on an empty db");
    assert!(
        out.candidates.is_empty(),
        "no candidates surface on an empty db"
    );
}

/// The detection sweep is deterministic: the same seed + `as_of` yields the
/// identical `DetectionDto` (stable ordering, stable confidences).
#[tokio::test]
async fn detect_is_deterministic() {
    let a = detect(&seeded(), as_of()).await.expect("detect ok");
    let b = detect(&seeded(), as_of()).await.expect("detect ok");
    assert_eq!(a, b, "detection is deterministic for a fixed seed + as_of");
}
