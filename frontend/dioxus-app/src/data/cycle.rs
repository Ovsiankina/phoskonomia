//! Shared current-cycle window (F3).
//!
//! Every page's top-bar + section labels read the current budgeting cycle. This
//! mirrors React `GET /cycle/current`: a small window descriptor with the
//! presentation labels resolved at the edge (the `phosk_core::cycle` engine deals
//! only in dates/counts — labels like `"JUN 2026"` / `"18 JUN"` are formatted
//! here, per ADR-010).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// `GET /cycle/current` — the active cycle window the whole UI frames itself in.
///
/// `label` is the human cycle name (`"JUN 2026"`); `day`/`days` are 1-based
/// position / total length; `days_left` the whole days remaining; `as_of` the
/// short "today" label (`"18 JUN"`); `start_date`/`end_date` ISO bounds the
/// Debts page parses to anchor its month axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleDto {
    /// Human cycle label, e.g. `"JUN 2026"`.
    pub label: String,
    /// 1-based day index of `as_of` within the cycle (e.g. 18).
    pub day: u32,
    /// Total days in the cycle (e.g. 30 for June).
    pub days: u32,
    /// Whole days remaining after `as_of` (inclusive of `end`).
    pub days_left: u32,
    /// Short "today" label, e.g. `"18 JUN"`.
    pub as_of: String,
    /// ISO start date of the cycle (`"2026-06-01"`).
    pub start_date: String,
    /// ISO end date of the cycle (`"2026-06-30"`).
    pub end_date: String,
}

/// The current budgeting cycle (`GET /cycle/current`).
///
/// On the server this resolves [`Period::Month`] for the seeded "today"
/// (2026-06-18) and formats the labels; the client gets the finished
/// [`CycleDto`]. The June seed yields `JUN 2026 · day 18/30 · 12 left`.
#[server]
pub async fn get_cycle() -> Result<CycleDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        use phosk_core::cycle::Period;
        let as_of = crate::data::today();
        let w = Period::Month
            .resolve(as_of)
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(CycleDto {
            label: month_year_label(w.start),
            day: w.day_index(),
            days: w.len_days(),
            days_left: w.days_left(),
            as_of: day_month_label(as_of),
            start_date: w.start.to_string(),
            end_date: w.end.to_string(),
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        // Client target never executes the body (the server fn is transported);
        // this arm exists only so the crate type-checks without server-deps.
        Err(ServerFnError::new("server-only"))
    }
}

/// `"JUN 2026"` from a date (uppercase 3-letter month + year). Edge formatting.
#[cfg(feature = "server-deps")]
fn month_year_label(d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    format!("{} {}", month_abbr(d.month()), d.year())
}

/// `"18 JUN"` from a date (day + uppercase 3-letter month). Edge formatting.
#[cfg(feature = "server-deps")]
fn day_month_label(d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    format!("{} {}", d.day(), month_abbr(d.month()))
}

/// 1-based month number → uppercase 3-letter abbreviation.
#[cfg(feature = "server-deps")]
fn month_abbr(m: u32) -> &'static str {
    const M: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    M.get((m as usize).wrapping_sub(1))
        .copied()
        .unwrap_or("JAN")
}
