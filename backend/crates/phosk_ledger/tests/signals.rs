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
//! Tests for `phosk_ledger::signals` — tracked item-signals, candidates,
//! movers, the inspector detail, and the track/dismiss write paths.
//!
//! These assert the EXACT wire-spec shapes (`dioxus-app/src/data/signals.rs`)
//! and the deterministic Swiss seed (`phosk_db_memory`, clock `2026-06-18`,
//! June cycle `[2026-06-01, 2026-06-30]`). The seeded item-signals are
//! `coffee` / `pain` / `beer` / `gruyere` (tracked) and `energy-drink`
//! (candidate). Every money field is exact i64 centimes; momentum (`deltaPct`)
//! is the trailing-N=3 baseline; `series` is the 12-point spark.
//!
//! All six services are implemented against `&dyn DatabaseAdapter`, so each test
//! exercises the real read/write paths against the seed.

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_db_memory::MemoryDb;
use phosk_ledger::signals::{
    SignalDto, dismiss_signal, list_signals, movers, signal_candidates, signal_detail, track_signal,
};

/// The seeded demo clock: day 18 of the June 2026 cycle.
fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid as_of date")
}

fn seeded() -> MemoryDb {
    MemoryDb::seeded().expect("seed is valid")
}

/// Find a signal by its stable slug id in a list (panics in-test if absent —
/// `expect` is allowed in tests).
fn by_id<'a>(list: &'a [SignalDto], id: &str) -> &'a SignalDto {
    list.iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("signal {id} present in list"))
}

// ── list_signals (tracked) ──────────────────────────────────────────────────

/// `list_signals` returns exactly the four TRACKED signals, none of the
/// candidate (`energy-drink` is `tracked == false`, so excluded).
#[tokio::test]
async fn list_signals_returns_four_tracked_only() {
    let db = seeded();
    let signals = list_signals(&db, as_of()).await.expect("list_signals ok");

    assert_eq!(signals.len(), 4, "four tracked signals");
    let mut ids: Vec<&str> = signals.iter().map(|s| s.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["beer", "coffee", "gruyere", "pain"]);
    assert!(
        signals.iter().all(|s| !s.candidate),
        "tracked signals are not candidates"
    );
    assert!(
        signals.iter().all(|s| s.id != "energy-drink"),
        "candidate excluded from tracked list"
    );
}

/// The `coffee` signal carries every headline field exactly as the wire spec:
/// label/parent/since strings, 12-point series, +28 momentum, 16 cups,
/// CHF 89.60 (8_960 centimes) this cycle, 12 receipts, not a candidate.
#[tokio::test]
async fn list_signals_coffee_headline_is_exact() {
    let db = seeded();
    let signals = list_signals(&db, as_of()).await.expect("list_signals ok");
    let coffee = by_id(&signals, "coffee");

    assert_eq!(coffee.label, "Oat-milk flat white");
    assert_eq!(coffee.parent, "Coffee & snacks");
    assert_eq!(coffee.since, "MAR 2026");
    assert_eq!(
        coffee.series,
        vec![
            6.0, 7.0, 9.0, 8.0, 11.0, 10.0, 12.0, 11.0, 13.0, 12.0, 14.0, 16.0
        ],
        "12-point spark"
    );
    assert_eq!(coffee.delta_pct, 28, "coffee momentum +28");
    assert_eq!(coffee.cycle_qty, 16.0);
    assert_eq!(coffee.unit, "cups");
    assert_eq!(
        coffee.cycle_spend.centimes(),
        8_960,
        "coffee cycle spend CHF 89.60"
    );
    assert_eq!(coffee.txns, 12, "coffee appears on 12 receipts");
    assert!(!coffee.candidate);
    assert!(coffee.desc.is_empty(), "tracked signals have no pitch");
}

/// The remaining three tracked signals match the seed exactly (spend in
/// centimes, qty, unit, txns, and the pinned momentum percents).
#[tokio::test]
async fn list_signals_all_tracked_values_exact() {
    let db = seeded();
    let signals = list_signals(&db, as_of()).await.expect("list_signals ok");

    let pain = by_id(&signals, "pain");
    assert_eq!(pain.label, "Pain au chocolat");
    assert_eq!(pain.parent, "Coffee & snacks");
    assert_eq!(pain.since, "JAN 2026");
    assert_eq!(pain.cycle_spend.centimes(), 3_150);
    assert_eq!(pain.cycle_qty, 9.0);
    assert_eq!(pain.unit, "pcs");
    assert_eq!(pain.txns, 7);
    assert_eq!(pain.delta_pct, 12);

    let beer = by_id(&signals, "beer");
    assert_eq!(beer.label, "Craft IPA");
    assert_eq!(beer.parent, "Going out");
    assert_eq!(beer.since, "FEB 2026");
    assert_eq!(beer.cycle_spend.centimes(), 2_880);
    assert_eq!(beer.cycle_qty, 4.0);
    assert_eq!(beer.unit, "bottles");
    assert_eq!(beer.txns, 3);
    assert_eq!(beer.delta_pct, -22, "beer is cooling −22");

    let gruyere = by_id(&signals, "gruyere");
    assert_eq!(gruyere.label, "Gruyère AOP");
    assert_eq!(gruyere.parent, "Groceries");
    assert_eq!(gruyere.since, "DEC 2025");
    assert_eq!(gruyere.cycle_spend.centimes(), 2_640);
    assert_eq!(gruyere.cycle_qty, 1.0);
    assert_eq!(gruyere.unit, "kg");
    assert_eq!(gruyere.txns, 4);
    assert_eq!(gruyere.delta_pct, 7);
}

/// Every tracked signal's `series` is exactly 12 points (the spark the SVG draws).
#[tokio::test]
async fn list_signals_series_is_twelve_points() {
    let db = seeded();
    let signals = list_signals(&db, as_of()).await.expect("list_signals ok");
    for s in &signals {
        assert_eq!(s.series.len(), 12, "{} has a 12-point spark", s.id);
    }
}

/// `deltaPct` is the trailing-N=3 momentum: for each tracked signal, the sign
/// matches the seed's direction (coffee/pain/gruyere rising, beer falling).
#[tokio::test]
async fn list_signals_momentum_signs_match_trend() {
    let db = seeded();
    let signals = list_signals(&db, as_of()).await.expect("list_signals ok");
    assert!(by_id(&signals, "coffee").delta_pct > 0);
    assert!(by_id(&signals, "pain").delta_pct > 0);
    assert!(by_id(&signals, "gruyere").delta_pct > 0);
    assert!(
        by_id(&signals, "beer").delta_pct < 0,
        "the cooling signal is negative"
    );
}

/// The wire DTO serializes camelCase with money as exact centimes (`cycleSpend`
/// is an integer, NOT a CHF float) and a boolean `candidate`.
#[tokio::test]
async fn list_signals_json_is_camelcase_centimes() {
    let db = seeded();
    let signals = list_signals(&db, as_of()).await.expect("list_signals ok");
    let coffee = by_id(&signals, "coffee");
    let v = serde_json::to_value(coffee).expect("serialize SignalDto");

    assert_eq!(v["id"], "coffee");
    assert_eq!(v["cycleSpend"], 8_960, "money as exact centime integer");
    assert_eq!(v["cycleQty"], 16.0);
    assert_eq!(v["deltaPct"], 28);
    assert_eq!(v["candidate"], false);
    assert!(v.get("cycle_spend").is_none(), "snake_case key absent");
    assert!(
        v["cycleSpend"].is_i64(),
        "cycleSpend is an integer, not a float"
    );
}

// ── signal_candidates ───────────────────────────────────────────────────────

/// `signal_candidates` returns exactly the one untracked `energy-drink`,
/// flagged `candidate: true` with its AI pitch and current-cycle rollup.
#[tokio::test]
async fn signal_candidates_returns_energy_drink_only() {
    let db = seeded();
    let cands = signal_candidates(&db, as_of())
        .await
        .expect("signal_candidates ok");

    assert_eq!(cands.len(), 1, "one candidate seeded");
    let ed = &cands[0];
    assert_eq!(ed.id, "energy-drink");
    assert_eq!(ed.label, "Energy drinks");
    assert!(ed.candidate, "candidate flag set");
    assert!(!ed.desc.is_empty(), "candidate carries a pitch");
    assert_eq!(ed.unit, "cans");
    assert_eq!(ed.cycle_qty, 6.0);
    assert_eq!(
        ed.cycle_spend.centimes(),
        2_340,
        "candidate spend CHF 23.40"
    );
    assert_eq!(ed.txns, 3);
}

/// Candidates and tracked signals are disjoint: nothing tracked appears among
/// the candidates.
#[tokio::test]
async fn signal_candidates_excludes_tracked() {
    let db = seeded();
    let cands = signal_candidates(&db, as_of())
        .await
        .expect("signal_candidates ok");
    assert!(
        cands.iter().all(|s| s.candidate),
        "every candidate is untracked"
    );
    for tracked in ["coffee", "pain", "beer", "gruyere"] {
        assert!(
            cands.iter().all(|s| s.id != tracked),
            "{tracked} is tracked, not a candidate"
        );
    }
}

/// The candidate serializes with `candidate: true` and centime money.
#[tokio::test]
async fn signal_candidates_json_flags_candidate() {
    let db = seeded();
    let cands = signal_candidates(&db, as_of())
        .await
        .expect("signal_candidates ok");
    let v = serde_json::to_value(&cands[0]).expect("serialize candidate");
    assert_eq!(v["candidate"], true);
    assert_eq!(v["cycleSpend"], 2_340);
    assert_eq!(v["id"], "energy-drink");
}

// ── movers (riser / faller) ─────────────────────────────────────────────────

/// `movers.riser` is the largest positive momentum (coffee, +28); `faller` is
/// the most negative (beer, −22). `all` ranks every TRACKED signal.
#[tokio::test]
async fn movers_picks_max_riser_and_min_faller() {
    let db = seeded();
    let m = movers(&db, as_of()).await.expect("movers ok");

    assert_eq!(m.riser.id, "coffee", "fastest riser is coffee");
    assert_eq!(m.riser.delta_pct, 28);
    assert_eq!(m.faller.id, "beer", "fastest faller is beer");
    assert_eq!(m.faller.delta_pct, -22);
}

/// `movers.all` contains every tracked signal (4) and excludes the candidate.
#[tokio::test]
async fn movers_all_is_every_tracked_signal() {
    let db = seeded();
    let m = movers(&db, as_of()).await.expect("movers ok");
    assert_eq!(m.all.len(), 4, "all four tracked signals in the list");
    assert!(
        m.all.iter().all(|s| s.id != "energy-drink"),
        "candidate excluded from movers list"
    );
}

/// `movers.all` is momentum-ranked descending (riser first, faller last), so the
/// list itself starts with `riser` and ends with `faller`.
#[tokio::test]
async fn movers_all_is_ranked_descending_by_momentum() {
    let db = seeded();
    let m = movers(&db, as_of()).await.expect("movers ok");
    assert!(
        m.all.windows(2).all(|w| w[0].delta_pct >= w[1].delta_pct),
        "momentum is non-increasing down the list"
    );
    assert_eq!(m.all.first().expect("non-empty").id, "coffee");
    assert_eq!(m.all.last().expect("non-empty").id, "beer");
}

/// The riser carries the full headline record (it IS a `SignalDto`), not just an
/// id — its cycle spend is the coffee value.
#[tokio::test]
async fn movers_riser_is_full_signal_record() {
    let db = seeded();
    let m = movers(&db, as_of()).await.expect("movers ok");
    assert_eq!(m.riser.cycle_spend.centimes(), 8_960);
    assert_eq!(m.riser.cycle_qty, 16.0);
    assert_eq!(m.riser.series.len(), 12);
}

// ── signal_detail (inspector) ───────────────────────────────────────────────

/// `signal_detail("coffee")` flattens the headline `SignalDto` into the detail
/// payload (so `id`/`cycleSpend`/`deltaPct` appear at the top level), then adds
/// the all-time totals, the recent occurrences, and a guidance line.
#[tokio::test]
async fn signal_detail_flattens_headline_and_adds_inspector() {
    let db = seeded();
    let d = signal_detail(&db, as_of(), "coffee")
        .await
        .expect("signal_detail ok");

    // Flattened headline.
    assert_eq!(d.signal.id, "coffee");
    assert_eq!(d.signal.cycle_spend.centimes(), 8_960);
    assert_eq!(d.signal.delta_pct, 28);

    // All-time totals are >= the single current cycle.
    assert!(
        d.all_time_spend.centimes() >= 8_960,
        "all-time spend covers more than one cycle"
    );
    assert!(d.all_time_txns >= 12, "all-time txns >= this cycle's 12");

    // Recent occurrences are present, newest-first.
    assert!(!d.recent.is_empty(), "recent occurrences listed");
    assert!(!d.guidance.is_empty(), "AI guidance line present");
}

/// The flattened detail serializes the headline keys at the TOP level (no nested
/// `signal` object), alongside the inspector keys, all camelCase with centime
/// money.
#[tokio::test]
async fn signal_detail_json_is_flattened_camelcase_centimes() {
    let db = seeded();
    let d = signal_detail(&db, as_of(), "coffee")
        .await
        .expect("signal_detail ok");
    let v = serde_json::to_value(&d).expect("serialize SignalDetailDto");

    // Flattened: headline keys at the top level.
    assert_eq!(v["id"], "coffee");
    assert_eq!(v["cycleSpend"], 8_960);
    assert!(
        v.get("signal").is_none(),
        "headline is flattened, not nested under `signal`"
    );

    // Inspector keys, camelCase + centimes.
    assert!(
        v["allTimeSpend"].is_i64(),
        "allTimeSpend is centime integer"
    );
    assert!(v["allTimeTxns"].is_u64());
    assert!(v["recent"].is_array());
    assert!(v["guidance"].is_string());
    assert!(v.get("all_time_spend").is_none(), "snake_case key absent");
}

/// Each recent occurrence carries a date label, a shop, and an exact centime
/// amount (camelCase serialized).
#[tokio::test]
async fn signal_detail_recent_occurrences_are_exact() {
    let db = seeded();
    let d = signal_detail(&db, as_of(), "coffee")
        .await
        .expect("signal_detail ok");
    let first = d.recent.first().expect("at least one recent occurrence");
    assert!(!first.date.is_empty(), "occurrence has a date label");
    assert!(!first.shop.is_empty(), "occurrence has a shop");

    let v = serde_json::to_value(first).expect("serialize occurrence");
    assert!(v["amount"].is_i64(), "occurrence amount is centime integer");
    assert!(v["date"].is_string());
    assert!(v["shop"].is_string());
}

/// The guidance line reflects momentum direction: a rising signal "heats up",
/// a cooling one does not.
#[tokio::test]
async fn signal_detail_guidance_tracks_direction() {
    let db = seeded();
    let coffee = signal_detail(&db, as_of(), "coffee")
        .await
        .expect("coffee detail ok");
    let beer = signal_detail(&db, as_of(), "beer")
        .await
        .expect("beer detail ok");
    assert!(coffee.signal.delta_pct >= 0);
    assert!(beer.signal.delta_pct < 0);
    assert!(!coffee.guidance.is_empty());
    assert!(!beer.guidance.is_empty());
    assert_ne!(
        coffee.guidance, beer.guidance,
        "rising vs cooling read differently"
    );
}

/// A candidate slug also resolves to a detail (the inspector opens on candidates
/// too): `energy-drink` flattens with `candidate: true`.
#[tokio::test]
async fn signal_detail_resolves_candidate_slug() {
    let db = seeded();
    let d = signal_detail(&db, as_of(), "energy-drink")
        .await
        .expect("candidate detail ok");
    assert_eq!(d.signal.id, "energy-drink");
    assert!(d.signal.candidate);
    assert_eq!(d.signal.cycle_spend.centimes(), 2_340);
}

/// An unknown slug is a `NotFound`, not a panic or a silent empty payload.
#[tokio::test]
async fn signal_detail_unknown_slug_is_not_found() {
    use phosk_core::error::PhoskError;
    let db = seeded();
    let err = signal_detail(&db, as_of(), "does-not-exist")
        .await
        .expect_err("unknown slug errors");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "unknown signal slug is NotFound, got {err:?}"
    );
}

// ── track_signal / dismiss_signal (write paths) ─────────────────────────────

/// Tracking the `energy-drink` candidate succeeds (it exists & is untracked).
#[tokio::test]
async fn track_signal_promotes_candidate() {
    let db = seeded();
    track_signal(&db, "energy-drink")
        .await
        .expect("track candidate ok");

    // After tracking, it leaves the candidate list...
    let cands = signal_candidates(&db, as_of())
        .await
        .expect("candidates ok");
    assert!(
        cands.iter().all(|s| s.id != "energy-drink"),
        "tracked candidate no longer a candidate"
    );
    // ...and joins the tracked list.
    let tracked = list_signals(&db, as_of()).await.expect("list_signals ok");
    assert!(
        tracked.iter().any(|s| s.id == "energy-drink"),
        "promoted candidate now tracked"
    );
}

/// Dismissing the `energy-drink` candidate succeeds and removes it from the
/// candidate list without promoting it to tracked.
#[tokio::test]
async fn dismiss_signal_drops_candidate() {
    let db = seeded();
    dismiss_signal(&db, "energy-drink")
        .await
        .expect("dismiss candidate ok");

    let cands = signal_candidates(&db, as_of())
        .await
        .expect("candidates ok");
    assert!(
        cands.iter().all(|s| s.id != "energy-drink"),
        "dismissed candidate gone from candidates"
    );
    let tracked = list_signals(&db, as_of()).await.expect("list_signals ok");
    assert!(
        tracked.iter().all(|s| s.id != "energy-drink"),
        "dismissed candidate is NOT promoted to tracked"
    );
}

/// Tracking an unknown slug is a `NotFound`, not a panic.
#[tokio::test]
async fn track_signal_unknown_slug_is_not_found() {
    use phosk_core::error::PhoskError;
    let db = seeded();
    let err = track_signal(&db, "nope")
        .await
        .expect_err("unknown track errors");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "unknown track slug is NotFound, got {err:?}"
    );
}

/// Dismissing an unknown slug is a `NotFound`, not a panic.
#[tokio::test]
async fn dismiss_signal_unknown_slug_is_not_found() {
    use phosk_core::error::PhoskError;
    let db = seeded();
    let err = dismiss_signal(&db, "nope")
        .await
        .expect_err("unknown dismiss errors");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "unknown dismiss slug is NotFound, got {err:?}"
    );
}

// ── port-object-safety smoke ────────────────────────────────────────────────

/// Every read service is callable behind the `&dyn DatabaseAdapter` PORT handle
/// (ADR-010) — it never sees the concrete `MemoryDb` type.
#[tokio::test]
async fn services_work_through_the_port_trait_object() {
    let db = seeded();
    let port: &dyn DatabaseAdapter = &db;
    let signals = list_signals(port, as_of()).await.expect("list ok");
    assert_eq!(signals.len(), 4);
    let m = movers(port, as_of()).await.expect("movers ok");
    assert_eq!(m.all.len(), 4);
}
