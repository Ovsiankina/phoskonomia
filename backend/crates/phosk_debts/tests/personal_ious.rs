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
//! RED integration tests for `phosk_debts::personal_ious` — the informal
//! "owe / owed" list and its net-position beam.
//!
//! These drive the service layer (`list_personal_ious`, `iou_stats`) through the
//! `&dyn DatabaseAdapter` PORT against the deterministic Swiss seed
//! ([`MemoryDb::seeded`]). They assert EXACT values: money in i64 centimes,
//! `repaidPct` via the contract formula `(of − amount) / of` (0 when `of == 0`),
//! and the net-position sums.
//!
//! The seed IOUs (`frontend/dioxus-app/src/data/debts.rs` / `seed.rs`) are:
//!
//! | slug | dir | person | amount | of    | repaidPct           |
//! |------|-----|--------|--------|-------|---------------------|
//! | i1   | in  | Léa    | 12000  | 12000 | 0.0                 |
//! | i2   | in  | Marco  |  4500  |  9000 | 0.5                 |
//! | i3   | out | Sophie |  6000  |  6000 | 0.0                 |
//! | i4   | out | Dad    | 20000  | 50000 | 0.6                 |
//!
//! Stats: owedToYou = 12000+4500 = 16500; youOwe = 6000+20000 = 26000;
//! net = 16500 − 26000 = −9500; countIn = 2; countOut = 2.
//!
//! Every service body is `todo!()` in the skeleton, so these compile and FAIL at
//! runtime (the RED bar). No production logic lives here.

use std::sync::Arc;

use phosk_adapter_db::DatabaseAdapter;
use phosk_db_memory::MemoryDb;
use phosk_debts::personal_ious::{iou_stats, list_personal_ious};

/// The seeded adapter behind the `&dyn DatabaseAdapter` PORT feature code holds.
fn seeded() -> Arc<dyn DatabaseAdapter> {
    Arc::new(MemoryDb::seeded().expect("seed is valid"))
}

/// Fetch the IOU list, indexed by slug (`id`), for per-row assertions.
async fn ious_by_slug(
    db: &dyn DatabaseAdapter,
) -> std::collections::HashMap<String, phosk_debts::personal_ious::PersonalIouDto> {
    list_personal_ious(db)
        .await
        .expect("list_personal_ious ok")
        .into_iter()
        .map(|i| (i.id.clone(), i))
        .collect()
}

// ── list_personal_ious: CRUD / projection ──────────────────────────────────────

/// The seed has exactly four IOUs (`i1..i4`).
#[tokio::test]
async fn list_returns_four_seeded_ious() {
    let db = seeded();
    let ious = list_personal_ious(db.as_ref())
        .await
        .expect("list_personal_ious ok");
    assert_eq!(ious.len(), 4, "seed defines four personal IOUs");
    let mut slugs: Vec<&str> = ious.iter().map(|i| i.id.as_str()).collect();
    slugs.sort_unstable();
    assert_eq!(slugs, ["i1", "i2", "i3", "i4"]);
}

/// Every projected field of `i1` (a fresh, unrepaid inbound IOU) is exact.
#[tokio::test]
async fn i1_projects_every_field_exactly() {
    let db = seeded();
    let by = ious_by_slug(db.as_ref()).await;
    let i1 = by.get("i1").expect("i1 present");
    assert_eq!(i1.id, "i1");
    assert_eq!(i1.dir, "in");
    assert_eq!(i1.person, "Léa");
    assert_eq!(i1.initials, "L");
    assert_eq!(i1.amount.centimes(), 12_000, "amount CHF 120.00");
    assert_eq!(i1.of.centimes(), 12_000, "original CHF 120.00");
    assert_eq!(i1.reason, "Concert tickets");
    assert!(
        (i1.repaid_pct - 0.0).abs() < f64::EPSILON,
        "nothing repaid yet ⇒ 0.0, got {}",
        i1.repaid_pct
    );
}

// ── dir(in|out) ─────────────────────────────────────────────────────────────────

/// Two inbound (`in`) and two outbound (`out`); every `dir` is one of the two.
#[tokio::test]
async fn directions_split_two_in_two_out() {
    let db = seeded();
    let ious = list_personal_ious(db.as_ref())
        .await
        .expect("list_personal_ious ok");
    let n_in = ious.iter().filter(|i| i.dir == "in").count();
    let n_out = ious.iter().filter(|i| i.dir == "out").count();
    assert_eq!(n_in, 2, "i1 + i2 owed to you");
    assert_eq!(n_out, 2, "i3 + i4 you owe");
    assert!(
        ious.iter().all(|i| i.dir == "in" || i.dir == "out"),
        "dir is strictly in|out"
    );
}

#[tokio::test]
async fn inbound_ious_are_lea_and_marco() {
    let db = seeded();
    let by = ious_by_slug(db.as_ref()).await;
    assert_eq!(by.get("i1").expect("i1").dir, "in");
    assert_eq!(by.get("i2").expect("i2").dir, "in");
    assert_eq!(by.get("i2").expect("i2").person, "Marco");
}

#[tokio::test]
async fn outbound_ious_are_sophie_and_dad() {
    let db = seeded();
    let by = ious_by_slug(db.as_ref()).await;
    assert_eq!(by.get("i3").expect("i3").dir, "out");
    assert_eq!(by.get("i4").expect("i4").dir, "out");
    assert_eq!(by.get("i4").expect("i4").person, "Dad");
}

// ── partial repayment (of -> repaidPct) ─────────────────────────────────────────

/// A half-repaid inbound IOU: amount 4500 of 9000 ⇒ repaidPct 0.5.
#[tokio::test]
async fn i2_half_repaid_is_point_five() {
    let db = seeded();
    let by = ious_by_slug(db.as_ref()).await;
    let i2 = by.get("i2").expect("i2 present");
    assert_eq!(i2.amount.centimes(), 4_500);
    assert_eq!(i2.of.centimes(), 9_000);
    assert!(
        (i2.repaid_pct - 0.5).abs() < 1e-9,
        "(9000-4500)/9000 = 0.5, got {}",
        i2.repaid_pct
    );
}

/// A mostly-repaid outbound IOU: amount 20000 of 50000 ⇒ repaidPct 0.6.
#[tokio::test]
async fn i4_sixty_percent_repaid() {
    let db = seeded();
    let by = ious_by_slug(db.as_ref()).await;
    let i4 = by.get("i4").expect("i4 present");
    assert_eq!(i4.amount.centimes(), 20_000);
    assert_eq!(i4.of.centimes(), 50_000);
    assert!(
        (i4.repaid_pct - 0.6).abs() < 1e-9,
        "(50000-20000)/50000 = 0.6, got {}",
        i4.repaid_pct
    );
}

/// `repaidPct` is `(of − amount) / of` for EVERY seeded row, computed independently.
#[tokio::test]
async fn repaid_pct_matches_formula_for_all_rows() {
    let db = seeded();
    let ious = list_personal_ious(db.as_ref())
        .await
        .expect("list_personal_ious ok");
    for i in &ious {
        let of = i.of.centimes();
        let amount = i.amount.centimes();
        let expected = if of == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            {
                (of - amount) as f64 / of as f64
            }
        };
        assert!(
            (i.repaid_pct - expected).abs() < 1e-9,
            "{}: repaidPct {} != (of-amount)/of {}",
            i.id,
            i.repaid_pct,
            expected
        );
    }
}

/// `repaidPct` is bounded to the 0..=1 fraction the UI meter expects.
#[tokio::test]
async fn repaid_pct_is_a_zero_to_one_fraction() {
    let db = seeded();
    let ious = list_personal_ious(db.as_ref())
        .await
        .expect("list_personal_ious ok");
    for i in &ious {
        assert!(
            (0.0..=1.0).contains(&i.repaid_pct),
            "{}: repaidPct {} out of 0..=1",
            i.id,
            i.repaid_pct
        );
    }
}

/// Fully-outstanding rows (`amount == of`) have repaidPct 0.0 (the div-guard's
/// boundary: a present `of`, zero progress).
#[tokio::test]
async fn fully_outstanding_rows_are_zero_repaid() {
    let db = seeded();
    let by = ious_by_slug(db.as_ref()).await;
    for slug in ["i1", "i3"] {
        let iou = by.get(slug).expect("present");
        assert_eq!(iou.amount, iou.of, "{slug} is fully outstanding");
        assert!(
            iou.repaid_pct.abs() < f64::EPSILON,
            "{slug}: nothing repaid ⇒ 0.0"
        );
    }
}

// ── stats (owedToYou, youOwe, net, countIn, countOut, maxSingle) ────────────────

#[tokio::test]
async fn stats_owed_to_you_sums_inbound() {
    let db = seeded();
    let stats = iou_stats(db.as_ref()).await.expect("iou_stats ok");
    assert_eq!(
        stats.owed_to_you.centimes(),
        16_500,
        "Σ dir==in = 12000 + 4500"
    );
}

#[tokio::test]
async fn stats_you_owe_sums_outbound() {
    let db = seeded();
    let stats = iou_stats(db.as_ref()).await.expect("iou_stats ok");
    assert_eq!(
        stats.you_owe.centimes(),
        26_000,
        "Σ dir==out = 6000 + 20000"
    );
}

/// Net = owedToYou − youOwe = 16500 − 26000 = −9500 (you owe more; negative).
#[tokio::test]
async fn stats_net_is_negative_nine_thousand_five_hundred() {
    let db = seeded();
    let stats = iou_stats(db.as_ref()).await.expect("iou_stats ok");
    assert_eq!(
        stats.net.centimes(),
        -9_500,
        "net = owedToYou − youOwe = 16500 − 26000"
    );
}

/// Net is internally consistent with the two component sums.
#[tokio::test]
async fn stats_net_equals_owed_minus_owe() {
    let db = seeded();
    let stats = iou_stats(db.as_ref()).await.expect("iou_stats ok");
    assert_eq!(
        stats.net.centimes(),
        stats.owed_to_you.centimes() - stats.you_owe.centimes(),
        "net is owedToYou − youOwe exactly"
    );
}

#[tokio::test]
async fn stats_counts_are_two_and_two() {
    let db = seeded();
    let stats = iou_stats(db.as_ref()).await.expect("iou_stats ok");
    assert_eq!(stats.count_in, 2, "two inbound IOUs");
    assert_eq!(stats.count_out, 2, "two outbound IOUs");
}

/// The largest single outstanding balance across all IOUs is Dad's CHF 200.00
/// (amount 20000). (Covers the `maxSingle` figure from the feature scope; the
/// stats DTO carries no `maxSingle` field, so it is derived from the list — if
/// the green phase needs it on `IouStatsDto`, note the gap.)
#[tokio::test]
async fn max_single_outstanding_is_dad_twenty_thousand() {
    let db = seeded();
    let ious = list_personal_ious(db.as_ref())
        .await
        .expect("list_personal_ious ok");
    let max = ious
        .iter()
        .map(|i| i.amount.centimes())
        .max()
        .expect("non-empty");
    assert_eq!(
        max, 20_000,
        "Dad's CHF 200.00 is the largest single balance"
    );
}

/// Stats sums tie back to the list: owedToYou + youOwe equals the total of every
/// outstanding `amount`, and the counts equal the list partition.
#[tokio::test]
async fn stats_are_consistent_with_the_list() {
    let db = seeded();
    let ious = list_personal_ious(db.as_ref())
        .await
        .expect("list_personal_ious ok");
    let stats = iou_stats(db.as_ref()).await.expect("iou_stats ok");

    let total_amount: i64 = ious.iter().map(|i| i.amount.centimes()).sum();
    assert_eq!(
        stats.owed_to_you.centimes() + stats.you_owe.centimes(),
        total_amount,
        "in + out sums cover every outstanding amount"
    );
    assert_eq!(
        u32::try_from(ious.iter().filter(|i| i.dir == "in").count()).expect("fits"),
        stats.count_in
    );
    assert_eq!(
        u32::try_from(ious.iter().filter(|i| i.dir == "out").count()).expect("fits"),
        stats.count_out
    );
}

// ── edge cases: empty, div-by-zero guard ────────────────────────────────────────

/// An empty store yields an empty list (no panic).
#[tokio::test]
async fn empty_db_list_is_empty() {
    let db: Arc<dyn DatabaseAdapter> = Arc::new(MemoryDb::new(
        Vec::new(),
        Vec::new(),
        phosk_model::BudgetConfig {
            monthly_budget: phosk_core::money::Money::ZERO,
            savings_target: phosk_core::money::Money::ZERO,
        },
    ));
    let ious = list_personal_ious(db.as_ref())
        .await
        .expect("list ok on empty");
    assert!(ious.is_empty(), "no IOUs ⇒ empty list");
}

/// An empty store yields all-zero stats (the sums/net are `Money::ZERO`, counts 0)
/// — the div-by-zero / no-rows guard.
#[tokio::test]
async fn empty_db_stats_are_all_zero() {
    let db: Arc<dyn DatabaseAdapter> = Arc::new(MemoryDb::new(
        Vec::new(),
        Vec::new(),
        phosk_model::BudgetConfig {
            monthly_budget: phosk_core::money::Money::ZERO,
            savings_target: phosk_core::money::Money::ZERO,
        },
    ));
    let stats = iou_stats(db.as_ref()).await.expect("stats ok on empty");
    assert_eq!(stats.owed_to_you.centimes(), 0);
    assert_eq!(stats.you_owe.centimes(), 0);
    assert_eq!(stats.net.centimes(), 0);
    assert_eq!(stats.count_in, 0);
    assert_eq!(stats.count_out, 0);
}

// ── wire shape (camelCase / centimes) ───────────────────────────────────────────

/// A `PersonalIouDto` serializes with camelCase keys and money as exact i64
/// centimes (never CHF f64).
#[tokio::test]
async fn iou_dto_serializes_camel_case_centimes() {
    let db = seeded();
    let by = ious_by_slug(db.as_ref()).await;
    let i2 = by.get("i2").expect("i2 present");
    let v = serde_json::to_value(i2).expect("serialize");
    assert_eq!(v["amount"], 4_500, "amount is i64 centimes");
    assert_eq!(v["of"], 9_000, "of is i64 centimes");
    assert_eq!(v["repaidPct"], 0.5, "repaidPct is camelCase");
    assert_eq!(v["initials"], "M");
    assert!(
        v.get("repaid_pct").is_none(),
        "no snake_case key leaks onto the wire"
    );
}

/// `IouStatsDto` serializes camelCase, money as centimes, and round-trips the
/// negative net exactly.
#[tokio::test]
async fn stats_dto_serializes_camel_case_centimes() {
    let db = seeded();
    let stats = iou_stats(db.as_ref()).await.expect("iou_stats ok");
    let v = serde_json::to_value(&stats).expect("serialize");
    assert_eq!(v["owedToYou"], 16_500);
    assert_eq!(v["youOwe"], 26_000);
    assert_eq!(
        v["net"], -9_500,
        "negative net survives the i64-centime wire form"
    );
    assert_eq!(v["countIn"], 2);
    assert_eq!(v["countOut"], 2);

    let back: phosk_debts::personal_ious::IouStatsDto =
        serde_json::from_value(v).expect("round-trips");
    assert_eq!(back.net.centimes(), -9_500);
}
