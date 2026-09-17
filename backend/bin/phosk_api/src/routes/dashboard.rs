//! Dashboard read-model endpoints — the `phosk_insights` bounded context
//! (ADR-005). These are data-driven rollups: budget-vs-spent KPIs, the daily
//! spend trace, top shops this cycle, and an AI narrative.
//!
//! Split out of `cycle` on purpose: the cycle *window* (`/cycle/current`) is a
//! shared foundation primitive (`phosk_core::cycle`) used on every page, whereas
//! these four are one screen's aggregation. Different responsibility, different
//! home.
//!
//! Three of the four are now wired (ADR-010 HTTP→service→repo→port→adapter): the
//! handlers extract the injected `Arc<dyn DatabaseAdapter>` (see `main.rs`),
//! resolve "today", and delegate to the `phosk_insights` read-model gateway,
//! which already returns the frontend's exact JSON shape (camelCase keys, money
//! as CHF numbers) — so the handlers return its serde-`Serialize` DTOs and let
//! axum serialize them at the response boundary. `/insights/dashboard` stays an
//! honest `501`: it needs the `phosk_ai` LLM spine (ADR-006/007), tracked in
//! backend-features-todo.md §5.
use std::sync::Arc;

use axum::extract::{Extension, Query};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Local;
use phosk_adapter_db::DatabaseAdapter;
use phosk_insights::{SpendSeriesDto, TopShopsDto, TotalsDto};
use serde::Deserialize;

use crate::routes::ApiError;

/// How many shops the top-shops bar chart shows.
const TOP_SHOPS_LIMIT: usize = 5;

pub fn dashboard() -> Router {
    Router::new()
        .route("/cycle/current/totals", get(totals))
        .route("/cycle/current/spend-series", get(spend_series))
        .route("/cycle/current/top-shops", get(top_shops))
        .route(
            "/insights/dashboard",
            ni!(get, "ai: dashboard narrative insight (GEMMA4)"),
        )
}

/// Query params for `/cycle/current/spend-series`. `compare=lastCycle` turns on
/// the prior-cycle comparison line; any other value (or absent) leaves it off.
#[derive(Debug, Default, Deserialize)]
struct SpendSeriesQuery {
    #[serde(default)]
    compare: Option<String>,
}

/// GET /cycle/current/totals — the headline budget-vs-spent KPIs for today's
/// cycle (`phosk_insights::dashboard_totals`).
#[tracing::instrument(level = "debug", skip_all)]
async fn totals(
    Extension(db): Extension<Arc<dyn DatabaseAdapter>>,
) -> Result<Json<TotalsDto>, ApiError> {
    let as_of = Local::now().date_naive();
    let dto = phosk_insights::dashboard_totals(db.as_ref(), as_of).await?;
    tracing::debug!("serving cycle totals");
    Ok(Json(dto))
}

/// GET /cycle/current/spend-series — the daily/cumulative/pace spend trace for
/// today's cycle, with the prior-cycle line when `compare=lastCycle`
/// (`phosk_insights::spend_series`).
#[tracing::instrument(level = "debug", skip_all, fields(compare))]
async fn spend_series(
    Extension(db): Extension<Arc<dyn DatabaseAdapter>>,
    Query(q): Query<SpendSeriesQuery>,
) -> Result<Json<SpendSeriesDto>, ApiError> {
    let compare_last_cycle = q.compare.as_deref() == Some("lastCycle");
    tracing::Span::current().record("compare", compare_last_cycle);
    let as_of = Local::now().date_naive();
    let dto = phosk_insights::spend_series(db.as_ref(), as_of, compare_last_cycle).await?;
    tracing::debug!(compare_last_cycle, "serving spend series");
    Ok(Json(dto))
}

/// GET /cycle/current/top-shops — the top [`TOP_SHOPS_LIMIT`] shops this cycle
/// with their shares (`phosk_insights::top_shops`).
#[tracing::instrument(level = "debug", skip_all)]
async fn top_shops(
    Extension(db): Extension<Arc<dyn DatabaseAdapter>>,
) -> Result<Json<TopShopsDto>, ApiError> {
    let as_of = Local::now().date_naive();
    let dto = phosk_insights::top_shops(db.as_ref(), as_of, TOP_SHOPS_LIMIT).await?;
    tracing::debug!("serving top shops");
    Ok(Json(dto))
}
