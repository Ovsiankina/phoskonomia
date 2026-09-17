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
//! RED integration tests for `phosk_recurring::subscriptions` (+ the dashboard
//! recurring panel and the AI recurring-detection skeleton).
//!
//! These pin the service layer against the deterministic Swiss seed
//! ([`phosk_db_memory::MemoryDb::seeded`]) and the wire DTO shapes mirrored from
//! `frontend/dioxus-app/src/data/subscriptions.rs` /
//! `frontend/dioxus-app/src/data/dashboard.rs`.
//!
//! They are deliberately RED: every service body is `todo!()`, so each test
//! COMPILES and then panics at runtime. No production logic lives here.
//!
//! ## The cycle anchor
//!
//! Every test uses `as_of = 2026-06-18` (the seed's billing-cycle "TODAY",
//! matching the dioxus `get_billing_sweep` `cycle.day = 18`). At that anchor the
//! monthly subscriptions' derived `days_until` line up with the dioxus seed's
//! hand-filled values:
//!
//! | sub      | day | next charge | days_until |
//! |----------|-----|-------------|-----------:|
//! | netflix  |  22 | 22 JUN      |          4 |
//! | spotify  |  28 | 28 JUN      |         10 |
//! | icloud   |  15 | 15 JUL      |         27 |
//! | gym      |   1 | 01 JUL      |         13 |
//!
//! Money is asserted in exact i64 centimes; the seed amounts are
//! netflix 1990, spotify 1595, icloud 999, gym 8900, nyt 1700, domain 4200.

use chrono::NaiveDate;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_recurring::recurring_detect;
use phosk_recurring::subscriptions::{self, SubFilter};

// ── helpers ───────────────────────────────────────────────────────────────────

/// The seed's billing-cycle anchor ("18 JUN 2026").
fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid anchor date")
}

/// A seeded in-memory adapter.
fn db() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

// The six seeded subscription slugs (== their wire ids).
const ALL_SLUGS: [&str; 6] = ["netflix", "spotify", "icloud", "gym", "nyt", "domain"];

// ── list_subscriptions ─────────────────────────────────────────────────────────

#[tokio::test]
async fn list_returns_all_six_seeded_subscriptions() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    assert_eq!(subs.len(), 6, "the seed defines six standing charges");
    let ids: Vec<&str> = subs.iter().map(|s| s.id.as_str()).collect();
    for slug in ALL_SLUGS {
        assert!(ids.contains(&slug), "missing subscription {slug}");
    }
}

#[tokio::test]
async fn list_carries_the_exact_seed_amounts_in_centimes() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    let amount = |id: &str| -> i64 {
        subs.iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("sub {id} present"))
            .amount
            .centimes()
    };
    assert_eq!(amount("netflix"), 1_990);
    assert_eq!(amount("spotify"), 1_595);
    assert_eq!(amount("icloud"), 999);
    assert_eq!(amount("gym"), 8_900);
    assert_eq!(amount("nyt"), 1_700);
    assert_eq!(amount("domain"), 4_200);
}

#[tokio::test]
async fn list_maps_source_enum_to_the_wire_string() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    let source = |id: &str| -> String {
        subs.iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("sub {id} present"))
            .source
            .clone()
    };
    // user-entered subs ⇒ "user"; LLM-inferred ⇒ "llm".
    assert_eq!(source("netflix"), "user");
    assert_eq!(source("spotify"), "user");
    assert_eq!(source("gym"), "user");
    assert_eq!(source("domain"), "user");
    assert_eq!(source("icloud"), "llm");
    assert_eq!(source("nyt"), "llm");
}

#[tokio::test]
async fn list_derives_monthly_run_rates_for_monthly_subs() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    let netflix = subs.iter().find(|s| s.id == "netflix").expect("netflix");
    // monthly ⇒ monthlyEquiv = amount, annual = amount * 12.
    assert_eq!(netflix.cadence, "monthly");
    assert_eq!(netflix.monthly_equiv.centimes(), 1_990);
    assert_eq!(netflix.annual.centimes(), 1_990 * 12);

    let gym = subs.iter().find(|s| s.id == "gym").expect("gym");
    assert_eq!(gym.monthly_equiv.centimes(), 8_900);
    assert_eq!(gym.annual.centimes(), 8_900 * 12);
}

#[tokio::test]
async fn list_derives_monthly_equiv_for_yearly_subs_via_integer_division() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    // yearly ⇒ annual = amount, monthlyEquiv = amount / 12 (integer centimes).
    let nyt = subs.iter().find(|s| s.id == "nyt").expect("nyt");
    assert_eq!(nyt.cadence, "yearly");
    assert_eq!(nyt.annual.centimes(), 1_700);
    assert_eq!(
        nyt.monthly_equiv.centimes(),
        1_700 / 12,
        "1700/12 = 141 cmt"
    );

    let domain = subs.iter().find(|s| s.id == "domain").expect("domain");
    assert_eq!(domain.annual.centimes(), 4_200);
    assert_eq!(domain.monthly_equiv.centimes(), 4_200 / 12, "4200/12 = 350");
}

#[tokio::test]
async fn list_derives_days_until_for_monthly_subs_at_the_anchor() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    let days = |id: &str| -> i32 {
        subs.iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("sub {id} present"))
            .days_until
    };
    // At as_of = 18 JUN 2026 (see module docs).
    assert_eq!(days("netflix"), 4, "day 22 − 18");
    assert_eq!(days("spotify"), 10, "day 28 − 18");
    assert_eq!(days("icloud"), 27, "15 already past ⇒ 15 JUL");
    assert_eq!(days("gym"), 13, "1 already past ⇒ 01 JUL");
}

#[tokio::test]
async fn list_derives_next_label_for_monthly_subs() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    let label = |id: &str| -> String {
        subs.iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("sub {id} present"))
            .next_label
            .clone()
    };
    assert_eq!(label("netflix"), "22 JUN");
    assert_eq!(label("spotify"), "28 JUN");
    assert_eq!(label("icloud"), "15 JUL");
    assert_eq!(label("gym"), "01 JUL");
}

#[tokio::test]
async fn list_derives_status_label_from_status_key() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    // netflix is "soon" ⇒ "DUE SOON"; gym is "watch" ⇒ "REVIEW";
    // spotify/icloud are "ok" ⇒ "ACTIVE".
    let netflix = subs.iter().find(|s| s.id == "netflix").expect("netflix");
    assert_eq!(netflix.status, "soon");
    assert_eq!(netflix.status_label, "DUE SOON");

    let gym = subs.iter().find(|s| s.id == "gym").expect("gym");
    assert_eq!(gym.status, "watch");
    assert_eq!(gym.status_label, "REVIEW");

    let spotify = subs.iter().find(|s| s.id == "spotify").expect("spotify");
    assert_eq!(spotify.status, "ok");
    assert_eq!(spotify.status_label, "ACTIVE");
}

#[tokio::test]
async fn list_flags_price_rose_from_charge_history() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    // seed_charges sets the FIRST charge (amount−200) below the later two, so the
    // last charge rose vs. its predecessor: priceRose = true for every sub.
    let netflix = subs.iter().find(|s| s.id == "netflix").expect("netflix");
    assert!(
        netflix.price_rose,
        "last charge rose vs. the prior one in the seed history"
    );
}

#[tokio::test]
async fn list_carries_day_and_month_fields_per_cadence() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    // monthly ⇒ day set, month empty.
    let netflix = subs.iter().find(|s| s.id == "netflix").expect("netflix");
    assert_eq!(netflix.day, 22);
    assert_eq!(netflix.month, "");
    // yearly ⇒ day 0, month label set.
    let nyt = subs.iter().find(|s| s.id == "nyt").expect("nyt");
    assert_eq!(nyt.day, 0);
    assert_eq!(nyt.month, "FEB");
}

#[tokio::test]
async fn list_carries_category_and_glyph_metadata() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    let netflix = subs.iter().find(|s| s.id == "netflix").expect("netflix");
    assert_eq!(netflix.category, "Entertainment");
    assert_eq!(netflix.glyph, "▶");
    assert_eq!(netflix.name, "Netflix");
}

#[tokio::test]
async fn list_sorts_by_days_until_when_sort_is_due() {
    let db = db();
    let filter = SubFilter {
        sort: "due".to_owned(),
        ..SubFilter::default()
    };
    let subs = subscriptions::list_subscriptions(&db, as_of(), filter)
        .await
        .expect("list ok");
    // Soonest non-negative days_until first: netflix(4) before spotify(10).
    let pos = |id: &str| subs.iter().position(|s| s.id == id).expect("present");
    assert!(pos("netflix") < pos("spotify"), "due sort: 4 before 10");
    assert!(pos("spotify") < pos("gym"), "due sort: 10 before 13");
}

#[tokio::test]
async fn list_sorts_by_monthly_equiv_descending_when_sort_is_amount() {
    let db = db();
    let filter = SubFilter {
        sort: "amount".to_owned(),
        ..SubFilter::default()
    };
    let subs = subscriptions::list_subscriptions(&db, as_of(), filter)
        .await
        .expect("list ok");
    // gym (8900 monthlyEquiv) is the largest run-rate ⇒ first.
    assert_eq!(subs.first().expect("non-empty").id, "gym");
    // Monotonically non-increasing monthlyEquiv.
    let rates: Vec<i64> = subs.iter().map(|s| s.monthly_equiv.centimes()).collect();
    assert!(
        rates.windows(2).all(|w| w[0] >= w[1]),
        "amount sort is monthlyEquiv-descending: {rates:?}"
    );
}

#[tokio::test]
async fn list_sorts_alphabetically_by_name_when_sort_is_name() {
    let db = db();
    let filter = SubFilter {
        sort: "name".to_owned(),
        ..SubFilter::default()
    };
    let subs = subscriptions::list_subscriptions(&db, as_of(), filter)
        .await
        .expect("list ok");
    let names: Vec<&str> = subs.iter().map(|s| s.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "name sort is alphabetical");
}

// ── subscription_stats ─────────────────────────────────────────────────────────

#[tokio::test]
async fn stats_count_is_six() {
    let db = db();
    let stats = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    assert_eq!(stats.count, 6);
}

#[tokio::test]
async fn stats_monthly_is_sum_of_monthly_equiv() {
    let db = db();
    let stats = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    // monthlyEquiv: netflix 1990 + spotify 1595 + icloud 999 + gym 8900
    //             + nyt (1700/12 = 141) + domain (4200/12 = 350) = 13_975.
    let expected = 1_990 + 1_595 + 999 + 8_900 + (1_700 / 12) + (4_200 / 12);
    assert_eq!(stats.monthly.centimes(), expected, "Σ monthlyEquiv");
}

#[tokio::test]
async fn stats_annual_is_monthly_times_twelve() {
    let db = db();
    let stats = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    assert_eq!(
        stats.annual.centimes(),
        stats.monthly.centimes() * 12,
        "annual = monthly * 12"
    );
}

#[tokio::test]
async fn stats_auto_count_is_the_llm_inferred_subs() {
    let db = db();
    let stats = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    // icloud + nyt are source == llm.
    assert_eq!(stats.auto_count, 2);
}

#[tokio::test]
async fn stats_next30_counts_charges_within_thirty_days() {
    let db = db();
    let stats = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    // All four monthly subs charge within 30 days (4,10,27,13); the yearly subs
    // do not. So next30.count = 4.
    assert_eq!(
        stats.next30.count, 4,
        "four monthly charges in the next 30 days"
    );
    assert_eq!(stats.next30.items.len(), 4);
}

#[tokio::test]
async fn stats_next30_items_are_sorted_soonest_first() {
    let db = db();
    let stats = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    let days: Vec<i32> = stats.next30.items.iter().map(|i| i.days_until).collect();
    let mut sorted = days.clone();
    sorted.sort_unstable();
    assert_eq!(days, sorted, "next30 items soonest-first");
    assert_eq!(
        stats.next30.items.first().expect("non-empty").name,
        "Netflix",
        "Netflix (4 days) is soonest"
    );
}

#[tokio::test]
async fn stats_next30_total_sums_the_upcoming_charge_amounts() {
    let db = db();
    let stats = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    // The four monthly charge amounts: 1990 + 1595 + 999 + 8900 = 13_484.
    assert_eq!(stats.next30.total.centimes(), 1_990 + 1_595 + 999 + 8_900);
}

#[tokio::test]
async fn stats_flagged_counts_watch_and_due_subs() {
    let db = db();
    let stats = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    // Only gym is "watch" in the seed; none are "due". flagged.count = 1.
    assert_eq!(stats.flagged.count, 1, "gym is the only flagged sub");
    assert!(!stats.flagged.note.is_empty(), "flagged carries a note");
}

// ── billing_sweep ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn sweep_cycle_window_is_the_anchor_day_in_a_thirty_day_cycle() {
    let db = db();
    let sweep = subscriptions::billing_sweep(&db, as_of())
        .await
        .expect("sweep ok");
    assert_eq!(sweep.cycle.day, 18, "TODAY marker at day 18");
    assert_eq!(sweep.cycle.days, 30, "30-day cycle");
    assert_eq!(sweep.cycle.as_of, "18 JUN");
}

#[tokio::test]
async fn sweep_has_one_impulse_per_monthly_sub() {
    let db = db();
    let sweep = subscriptions::billing_sweep(&db, as_of())
        .await
        .expect("sweep ok");
    // Only the four monthly subs land on the cycle axis.
    assert_eq!(sweep.impulses.len(), 4, "four monthly impulses");
    let ids: Vec<&str> = sweep.impulses.iter().map(|i| i.id.as_str()).collect();
    for id in ["netflix", "spotify", "icloud", "gym"] {
        assert!(ids.contains(&id), "impulse for {id}");
    }
}

#[tokio::test]
async fn sweep_marks_charges_before_today_as_paid() {
    let db = db();
    let sweep = subscriptions::billing_sweep(&db, as_of())
        .await
        .expect("sweep ok");
    // Day-of-cycle < 18 ⇒ "paid". gym (day 1) and icloud (day 15) are paid.
    let status = |id: &str| -> String {
        sweep
            .impulses
            .iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("impulse {id}"))
            .status
            .clone()
    };
    assert_eq!(status("gym"), "paid", "day 1 < 18 ⇒ paid");
    assert_eq!(status("icloud"), "paid", "day 15 < 18 ⇒ paid");
    // netflix (22) and spotify (28) are still upcoming ⇒ not paid.
    assert_ne!(status("netflix"), "paid", "day 22 ≥ 18 ⇒ not paid");
    assert_ne!(status("spotify"), "paid", "day 28 ≥ 18 ⇒ not paid");
}

#[tokio::test]
async fn sweep_footer_splits_paid_and_due_totals() {
    let db = db();
    let sweep = subscriptions::billing_sweep(&db, as_of())
        .await
        .expect("sweep ok");
    // paid: gym 8900 + icloud 999 = 9899.
    assert_eq!(sweep.footer.paid_this_cycle.centimes(), 8_900 + 999);
    // still due: netflix 1990 + spotify 1595 = 3585.
    assert_eq!(sweep.footer.still_due.centimes(), 1_990 + 1_595);
}

#[tokio::test]
async fn sweep_footer_next_charge_is_the_soonest_upcoming() {
    let db = db();
    let sweep = subscriptions::billing_sweep(&db, as_of())
        .await
        .expect("sweep ok");
    // The soonest still-upcoming charge is Netflix (22 JUN, 1990).
    assert_eq!(sweep.footer.next.name, "Netflix");
    assert_eq!(sweep.footer.next.next_label, "22 JUN");
    assert_eq!(sweep.footer.next.amount.centimes(), 1_990);
}

// ── subscription_detail ────────────────────────────────────────────────────────

#[tokio::test]
async fn detail_flattens_the_subscription_record() {
    let db = db();
    let detail = subscriptions::subscription_detail(&db, as_of(), "netflix")
        .await
        .expect("detail ok");
    assert_eq!(detail.subscription.id, "netflix");
    assert_eq!(detail.subscription.name, "Netflix");
    assert_eq!(detail.subscription.amount.centimes(), 1_990);
    assert_eq!(detail.subscription.monthly_equiv.centimes(), 1_990);
}

#[tokio::test]
async fn detail_lists_recent_charges_from_the_seed_history() {
    let db = db();
    let detail = subscriptions::subscription_detail(&db, as_of(), "netflix")
        .await
        .expect("detail ok");
    // seed_charges seeds three charges per subscription.
    assert_eq!(detail.recent.len(), 3, "three seeded charges");
    // The price-rise model: the oldest charge is amount−200, the later two equal.
    let amounts: Vec<i64> = detail.recent.iter().map(|c| c.amount.centimes()).collect();
    assert!(
        amounts.contains(&(1_990 - 200)),
        "the discounted first charge (1790) is present: {amounts:?}"
    );
    assert!(
        amounts.iter().filter(|&&a| a == 1_990).count() >= 2,
        "two full-price charges present: {amounts:?}"
    );
}

#[tokio::test]
async fn detail_guidance_is_coral_for_watch_subs() {
    let db = db();
    // gym is "watch" ⇒ guidance severity "coral".
    let detail = subscriptions::subscription_detail(&db, as_of(), "gym")
        .await
        .expect("detail ok");
    assert_eq!(detail.guidance.severity, "coral");
    assert!(!detail.guidance.text.is_empty());
}

#[tokio::test]
async fn detail_guidance_is_plain_for_ok_subs() {
    let db = db();
    // spotify is "ok" ⇒ no coral severity.
    let detail = subscriptions::subscription_detail(&db, as_of(), "spotify")
        .await
        .expect("detail ok");
    assert_eq!(detail.guidance.severity, "", "ok subs are not coral");
}

#[tokio::test]
async fn detail_marks_user_subscriptions_as_non_candidate() {
    let db = db();
    let detail = subscriptions::subscription_detail(&db, as_of(), "netflix")
        .await
        .expect("detail ok");
    assert!(!detail.candidate, "a confirmed sub is not an AI candidate");
}

#[tokio::test]
async fn detail_unknown_slug_is_not_found() {
    let db = db();
    let err = subscriptions::subscription_detail(&db, as_of(), "does-not-exist")
        .await
        .expect_err("unknown slug must be NotFound");
    assert_eq!(err.http_status(), 404);
    assert_eq!(err.code(), "not_found");
    assert!(matches!(err, PhoskError::NotFound(_)));
}

// ── recurring_summary (dashboard panel) ─────────────────────────────────────────

#[tokio::test]
async fn recurring_summary_lists_recurring_charges() {
    let db = db();
    let summary = subscriptions::recurring_summary(&db, as_of())
        .await
        .expect("summary ok");
    assert!(
        !summary.recurring.is_empty(),
        "the dashboard recurring panel is non-empty"
    );
    // Netflix is one of the recurring charges surfaced on the dashboard.
    assert!(
        summary.recurring.iter().any(|r| r.id == "netflix"),
        "Netflix is a recurring charge"
    );
}

#[tokio::test]
async fn recurring_summary_carries_amount_and_days_until() {
    let db = db();
    let summary = subscriptions::recurring_summary(&db, as_of())
        .await
        .expect("summary ok");
    let netflix = summary
        .recurring
        .iter()
        .find(|r| r.id == "netflix")
        .expect("netflix recurring");
    assert_eq!(netflix.amount.centimes(), 1_990);
    assert_eq!(netflix.days_until, 4, "22 JUN − 18 JUN");
    assert_eq!(netflix.next, "22 JUN");
}

#[tokio::test]
async fn recurring_summary_monthly_total_is_sum_of_monthly_equiv() {
    let db = db();
    let summary = subscriptions::recurring_summary(&db, as_of())
        .await
        .expect("summary ok");
    // Σ monthlyEquiv over all six subs (same figure as subscription_stats.monthly).
    let expected = 1_990 + 1_595 + 999 + 8_900 + (1_700 / 12) + (4_200 / 12);
    assert_eq!(summary.monthly_total.centimes(), expected);
}

#[tokio::test]
async fn recurring_summary_is_sorted_soonest_due_first() {
    let db = db();
    let summary = subscriptions::recurring_summary(&db, as_of())
        .await
        .expect("summary ok");
    let days: Vec<i32> = summary.recurring.iter().map(|r| r.days_until).collect();
    let mut sorted = days.clone();
    sorted.sort_unstable();
    assert_eq!(days, sorted, "recurring panel is soonest-due first");
}

// ── recurring_detect (AI candidate skeleton) ───────────────────────────────────

#[tokio::test]
async fn detect_scans_receipts_and_reports_the_count() {
    let db = db();
    let detection = recurring_detect::detect(&db, as_of())
        .await
        .expect("detect ok");
    // The sweep scans some receipts; the count is reported and non-zero against
    // the seed's receipt set.
    assert!(detection.scanned > 0, "scanned a non-empty receipt set");
}

#[tokio::test]
async fn detect_candidates_are_sorted_most_confident_first() {
    let db = db();
    let detection = recurring_detect::detect(&db, as_of())
        .await
        .expect("detect ok");
    let conf: Vec<f64> = detection.candidates.iter().map(|c| c.confidence).collect();
    assert!(
        conf.windows(2).all(|w| w[0] >= w[1]),
        "candidates are most-confident first: {conf:?}"
    );
}

#[tokio::test]
async fn detect_candidate_confidence_is_in_unit_range() {
    let db = db();
    let detection = recurring_detect::detect(&db, as_of())
        .await
        .expect("detect ok");
    for c in &detection.candidates {
        assert!(
            (0.0..=1.0).contains(&c.confidence),
            "confidence in 0..=1: {}",
            c.confidence
        );
        assert!(c.amount.centimes() >= 0, "non-negative candidate amount");
        assert!(c.occurrences >= 1, "at least one observed occurrence");
    }
}

#[tokio::test]
async fn confirm_unknown_candidate_is_not_found() {
    let db = db();
    let err = recurring_detect::confirm_candidate(&db, "no-such-candidate")
        .await
        .expect_err("unknown candidate must be NotFound");
    assert_eq!(err.code(), "not_found");
    assert!(matches!(err, PhoskError::NotFound(_)));
}

#[tokio::test]
async fn dismiss_unknown_candidate_is_not_found() {
    let db = db();
    let err = recurring_detect::dismiss_candidate(&db, "no-such-candidate")
        .await
        .expect_err("unknown candidate must be NotFound");
    assert_eq!(err.code(), "not_found");
    assert!(matches!(err, PhoskError::NotFound(_)));
}

// ── DTO wire-shape guards (serde camelCase + money centimes) ────────────────────

#[tokio::test]
async fn subscription_dto_serializes_camel_case_with_centime_money() {
    let db = db();
    let subs = subscriptions::list_subscriptions(&db, as_of(), SubFilter::default())
        .await
        .expect("list ok");
    let netflix = subs.iter().find(|s| s.id == "netflix").expect("netflix");
    let json = serde_json::to_value(netflix).expect("serialize");
    // Money is an exact i64 centime integer on the wire (never an f64 CHF).
    assert_eq!(json["amount"], serde_json::json!(1_990));
    assert_eq!(json["monthlyEquiv"], serde_json::json!(1_990));
    assert_eq!(json["annual"], serde_json::json!(1_990 * 12));
    // camelCase keys are present.
    assert!(json.get("statusLabel").is_some(), "statusLabel key present");
    assert!(json.get("daysUntil").is_some(), "daysUntil key present");
    assert!(json.get("nextLabel").is_some(), "nextLabel key present");
    assert!(json.get("priceRose").is_some(), "priceRose key present");
}

#[tokio::test]
async fn detail_dto_flattens_subscription_fields_to_top_level() {
    let db = db();
    let detail = subscriptions::subscription_detail(&db, as_of(), "netflix")
        .await
        .expect("detail ok");
    let json = serde_json::to_value(&detail).expect("serialize");
    // #[serde(flatten)] hoists the SubscriptionDto fields to the top level.
    assert_eq!(json["id"], serde_json::json!("netflix"));
    assert_eq!(json["amount"], serde_json::json!(1_990));
    assert!(
        json.get("recent").is_some(),
        "recent present alongside flattened fields"
    );
    assert!(json.get("guidance").is_some(), "guidance present");
    assert!(json.get("candidate").is_some(), "candidate present");
}

#[tokio::test]
async fn stats_dto_serializes_money_in_centimes_and_camel_case() {
    let db = db();
    let stats = subscriptions::subscription_stats(&db, as_of())
        .await
        .expect("stats ok");
    let json = serde_json::to_value(&stats).expect("serialize");
    let monthly = 1_990 + 1_595 + 999 + 8_900 + (1_700 / 12) + (4_200 / 12);
    assert_eq!(json["monthly"], serde_json::json!(monthly));
    assert_eq!(json["annual"], serde_json::json!(monthly * 12));
    assert!(json.get("autoCount").is_some(), "autoCount key present");
    assert!(
        json["next30"].get("items").is_some(),
        "next30.items present"
    );
}

// A compile-time guard that `Money` is the centime newtype these DTOs expect;
// keeps the import meaningful even if every assertion above regresses to todo!().
#[test]
fn money_centimes_constructor_is_available() {
    assert_eq!(Money::from_centimes(1_990).centimes(), 1_990);
}
