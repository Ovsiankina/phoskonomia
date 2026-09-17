//! `/cycle/current` — the current budgeting cycle window (one calendar month).
//!
//! Thin HTTP edge (ADR-001/010): the period logic lives in
//! [`phosk_core::cycle`]; here we resolve "today", then format the frontend's
//! JSON. The `label`/`asOf` strings are *presentation*, so they are built at the
//! edge (ADR-010 — formatting lives at the boundary, never in the domain).
use axum::{Json, Router, routing::get};
use chrono::{Datelike, Local};
use phosk_core::cycle::{CycleWindow, Period};
use serde_json::json;

use crate::routes::ApiError;

pub fn cycle() -> Router {
    Router::new().route("/cycle/current", get(current_cycle))
}

/// GET /cycle/current — the month window for today.
#[tracing::instrument(level = "debug", skip_all)]
async fn current_cycle() -> Result<Json<serde_json::Value>, ApiError> {
    let today = Local::now().date_naive();
    let window = Period::Month.resolve(today)?;
    tracing::debug!(label = %window.as_of.format("%b %Y"), day = window.day_index(), "serving current cycle");
    Ok(Json(cycle_window_json(&window)))
}

/// Domain window → the frontend's `{label, day, days, daysLeft, asOf}` shape.
fn cycle_window_json(w: &CycleWindow) -> serde_json::Value {
    let month_abbr = w.as_of.format("%b").to_string().to_uppercase(); // "JUN"
    json!({
        "label": format!("{month_abbr} {}", w.as_of.year()),  // "JUN 2026"
        "day": w.day_index(),
        "days": w.len_days(),
        "daysLeft": w.days_left(),
        "asOf": format!("{} {month_abbr}", w.as_of.day()),    // "19 JUN"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn june_19_2026_matches_the_frontend_contract() {
        let window = Period::Month
            .resolve(NaiveDate::from_ymd_opt(2026, 6, 19).expect("valid date"))
            .expect("a month always resolves");
        let v = cycle_window_json(&window);
        assert_eq!(v["label"], "JUN 2026");
        assert_eq!(v["day"], 19);
        assert_eq!(v["days"], 30);
        assert_eq!(v["daysLeft"], 11);
        assert_eq!(v["asOf"], "19 JUN");
    }
}
