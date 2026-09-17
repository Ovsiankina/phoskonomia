//! Integration tests for the wired dashboard read endpoints.
//!
//! Drives the full API [`build_app`] router in-process (no socket) via
//! [`tower::ServiceExt::oneshot`], with the deterministic Swiss seed injected as
//! the `Arc<dyn DatabaseAdapter>` `Extension` — exactly how `main.rs` wires it.
//! Asserts each of the three now-live endpoints returns `200` with its contract
//! fields present and sane, and that the still-unbuilt narrative endpoint stays
//! an honest `501`.
//!
//! The seed is pinned to May/June 2026, but the handlers resolve the cycle from
//! the real "today", so these assertions deliberately check *shape and units*
//! (key presence, value ranges, invariants), never the seed's exact figures —
//! the figures themselves are pinned in `phosk_insights`/`phosk_planning` unit
//! tests against an explicit `as_of`. That keeps the integration test stable on
//! any calendar date.
//!
//! Test-only: `unwrap`/`expect` are fine here (the workspace denies them in
//! production). `clippy.toml`'s allow-in-tests only covers `#[test]`-bodies, not
//! these integration-test helpers, so the exemption is made explicit crate-wide.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use phosk_adapter_db::DatabaseAdapter;
use phosk_api::build_app;
use phosk_db_memory::MemoryDb;
use serde_json::Value;
use tower::ServiceExt;

/// Build the app with the seeded in-memory adapter behind the PORT.
fn seeded_app() -> axum::Router {
    let db: Arc<dyn DatabaseAdapter> = Arc::new(MemoryDb::seeded().expect("seed is valid"));
    build_app(db)
}

/// GET `path` through the router, returning the status and parsed JSON body.
async fn get_json(path: &str) -> (StatusCode, Value) {
    let response = seeded_app()
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes();
    let json: Value = serde_json::from_slice(&bytes).expect("body is JSON");
    (status, json)
}

/// `/cycle/current/totals` returns 200 with every camelCase contract key, money
/// fields as JSON numbers, and the unit-pinned non-money fields in range.
#[tokio::test]
async fn totals_returns_200_with_contract_fields() {
    let (status, v) = get_json("/api/v1/cycle/current/totals").await;
    assert_eq!(status, StatusCode::OK, "totals is live, got {status}");

    let obj = v.as_object().expect("totals is a JSON object");
    for key in [
        "budget",
        "spent",
        "remaining",
        "allocated",
        "savingsTarget",
        "saved",
        "savingsProjected",
        "savingsRate",
        "lastCycleSpent",
        "spentPct",
        "vsLastCyclePct",
        "perDayToStayOnBudget",
    ] {
        assert!(obj.contains_key(key), "totals missing contract key `{key}`");
        assert!(v[key].is_number(), "`{key}` must be a JSON number");
    }

    // Unit conventions (load-bearing for the frontend formatters).
    let spent_pct = v["spentPct"].as_i64().expect("spentPct integer");
    assert!(
        (0..=100).contains(&spent_pct),
        "spentPct is a 0–100 integer, got {spent_pct}"
    );
    let savings_rate = v["savingsRate"].as_f64().expect("savingsRate number");
    assert!(
        (0.0..=1.0).contains(&savings_rate),
        "savingsRate is a 0–1 ratio, got {savings_rate}"
    );
}

/// `/cycle/current/spend-series` (default) returns 200 with equal-length
/// `daily`/`cumulative`/`pace` arrays, a `null` `lastCycleCumulative`, and a
/// `todayIndex` that indexes into those arrays.
#[tokio::test]
async fn spend_series_default_returns_200_aligned_arrays() {
    let (status, v) = get_json("/api/v1/cycle/current/spend-series").await;
    assert_eq!(status, StatusCode::OK, "spend-series is live, got {status}");

    let daily = v["daily"].as_array().expect("daily array");
    let cumulative = v["cumulative"].as_array().expect("cumulative array");
    let pace = v["pace"].as_array().expect("pace array");
    let len = daily.len();
    assert!(len > 0, "a cycle has at least one day");
    assert_eq!(cumulative.len(), len, "cumulative length == days");
    assert_eq!(pace.len(), len, "pace length == days");

    // No comparison line unless ?compare=lastCycle.
    assert_eq!(
        v["lastCycleCumulative"],
        Value::Null,
        "no comparison without compare=lastCycle"
    );

    let today_index = v["todayIndex"].as_u64().expect("todayIndex integer") as usize;
    assert!(
        today_index < len,
        "todayIndex {today_index} must index into the {len}-day series"
    );

    // cumulative is a non-decreasing running sum.
    let nums: Vec<f64> = cumulative
        .iter()
        .map(|x| x.as_f64().expect("number"))
        .collect();
    assert!(
        nums.windows(2).all(|w| w[0] <= w[1] + 1e-9),
        "cumulative never decreases"
    );
}

/// `/cycle/current/spend-series?compare=lastCycle` adds the prior-cycle line,
/// length-aligned to the current cycle.
#[tokio::test]
async fn spend_series_compare_includes_last_cycle() {
    let (status, v) = get_json("/api/v1/cycle/current/spend-series?compare=lastCycle").await;
    assert_eq!(status, StatusCode::OK, "spend-series is live, got {status}");

    let len = v["daily"].as_array().expect("daily array").len();
    let last = v["lastCycleCumulative"]
        .as_array()
        .expect("lastCycleCumulative present when comparing");
    assert_eq!(
        last.len(),
        len,
        "comparison line is aligned to the current cycle length"
    );
}

/// `/cycle/current/top-shops` returns 200 with a `shops[]` of `{shop,total,share}`
/// (at most the limit of 5), a `maxTotal` number, and shares in `0..=1`.
#[tokio::test]
async fn top_shops_returns_200_with_shares() {
    let (status, v) = get_json("/api/v1/cycle/current/top-shops").await;
    assert_eq!(status, StatusCode::OK, "top-shops is live, got {status}");

    assert!(v["maxTotal"].is_number(), "maxTotal is a CHF number");
    let shops = v["shops"].as_array().expect("shops array");
    assert!(shops.len() <= 5, "limited to 5 shops, got {}", shops.len());

    for shop in shops {
        assert!(shop["shop"].is_string(), "shop name is a string");
        assert!(shop["total"].is_number(), "shop total is a CHF number");
        let share = shop["share"].as_f64().expect("share number");
        assert!(
            (0.0..=1.0).contains(&share),
            "share is a 0–1 proportion, got {share}"
        );
    }
}

/// The AI narrative endpoint is still an honest `501` (needs the LLM spine).
#[tokio::test]
async fn insights_dashboard_is_still_not_implemented() {
    let (status, v) = get_json("/api/v1/insights/dashboard").await;
    assert_eq!(
        status,
        StatusCode::NOT_IMPLEMENTED,
        "narrative insight is not wired yet"
    );
    assert_eq!(v["error"], "not_implemented");
}
