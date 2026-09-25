//! Shared item-signal vocabulary (F3).
//!
//! "Item-signals" are tracked line-item trends (the coral `⌁` pills): a specific
//! product rolled up across receipts so its quantity / spend / momentum can be
//! watched. Dashboard, Transactions and Analytics all render them, so the shapes
//! and their `#[server]` reads live here once. Mirrors React `GET /signals`,
//! `/signals/candidates`, `/signals/movers`, `/signals/{id}`.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One tracked item-signal (an element of `GET /signals`) or, with `candidate`
/// set, an AI-proposed not-yet-tracked one (`GET /signals/candidates`).
///
/// `series` is the 12-cycle spark (unitless trend points the SVG draws);
/// `delta_pct` the signed momentum percent; `cycle_qty`/`unit` the quantity this
/// cycle; `cycle_spend` the CHF spent on it this cycle; `txns` the count of
/// receipts it appears in. Candidate rows carry a `desc` instead of a parent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalDto {
    /// Stable id (the React pill key), e.g. `"coffee"`.
    pub id: String,
    /// Display label, e.g. `"Oat-milk flat white"`.
    pub label: String,
    /// Parent budget category, e.g. `"Coffee & snacks"`.
    pub parent: String,
    /// Since-label (when tracking began), e.g. `"MAR 2026"`.
    pub since: String,
    /// 12-point trend spark (unitless cycle spend points).
    pub series: Vec<f64>,
    /// Signed momentum percent vs the prior window (e.g. `+28`).
    pub delta_pct: i32,
    /// Quantity consumed this cycle (e.g. `14`).
    pub cycle_qty: f64,
    /// Unit label for `cycle_qty`, e.g. `"cups"`.
    pub unit: String,
    /// CHF spent on this signal this cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub cycle_spend: Money,
    /// Number of receipts this signal appears in this cycle.
    pub txns: u32,
    /// `true` for an AI candidate (renders the `TRACK ▸` affordance).
    pub candidate: bool,
    /// One-line pitch shown for a candidate instead of `parent · since`.
    pub desc: String,
}

/// `GET /signals/movers` — the two extreme item-signals plus the full list.
///
/// `riser` is the fastest-climbing signal, `faller` the fastest-cooling; `all`
/// is every signal (the Analytics matrix can fall back to it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoversDto {
    /// Fastest riser (largest positive `delta_pct`).
    pub riser: SignalDto,
    /// Fastest faller (most negative `delta_pct`).
    pub faller: SignalDto,
    /// Every tracked signal, momentum-ranked.
    pub all: Vec<SignalDto>,
}

/// A single past occurrence of a signal (the inspector's "recent" rows).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalOccurrenceDto {
    /// Date label, e.g. `"16 JUN"`.
    pub date: String,
    /// Shop the occurrence came from.
    pub shop: String,
    /// CHF for this occurrence.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
}

/// `GET /signals/{id}` — the full inspector payload for one signal.
///
/// Extends [`SignalDto`]'s headline numbers with the inspector extras the
/// `SignalPanel` renders: the recent occurrences, an all-time spend, and an AI
/// guidance line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalDetailDto {
    /// The headline signal record.
    #[serde(flatten)]
    pub signal: SignalDto,
    /// All-time CHF spent on this signal.
    #[serde(with = "phosk_model::money_centimes")]
    pub all_time_spend: Money,
    /// All-time occurrence count.
    pub all_time_txns: u32,
    /// Recent occurrences, newest first.
    pub recent: Vec<SignalOccurrenceDto>,
    /// One-line AI read on the trend.
    pub guidance: String,
}

/// Tracked item-signals (`GET /signals`).
///
/// REAL: composes `phosk_ledger::signals::list_signals` for the seeded cycle.
#[server]
pub async fn get_signals() -> Result<Vec<SignalDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let sigs = phosk_ledger::signals::list_signals(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(sigs.into_iter().map(map_signal).collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// AI-detected candidate item-signals not yet tracked (`GET /signals/candidates`).
///
/// REAL: composes `phosk_ledger::signals::signal_candidates`.
#[server]
pub async fn get_signal_candidates() -> Result<Vec<SignalDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let sigs = phosk_ledger::signals::signal_candidates(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(sigs.into_iter().map(map_signal).collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The fastest mover pair + full list (`GET /signals/movers`).
///
/// REAL: composes `phosk_ledger::signals::movers`.
#[server]
pub async fn get_movers() -> Result<MoversDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let m = phosk_ledger::signals::movers(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(MoversDto {
            riser: map_signal(m.riser),
            faller: map_signal(m.faller),
            all: m.all.into_iter().map(map_signal).collect(),
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// One signal's full inspector payload (`GET /signals/{id}`).
///
/// REAL: composes `phosk_ledger::signals::signal_detail` for the slug.
#[server]
pub async fn get_signal(id: String) -> Result<SignalDetailDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let d = phosk_ledger::signals::signal_detail(session.db(), crate::data::today(), &id)
            .await
            .map_err(crate::data::server_err)?;
        Ok(SignalDetailDto {
            signal: map_signal(d.signal),
            all_time_spend: d.all_time_spend,
            all_time_txns: d.all_time_txns,
            recent: d
                .recent
                .into_iter()
                .map(|o| SignalOccurrenceDto {
                    date: o.date,
                    shop: o.shop,
                    amount: o.amount,
                })
                .collect(),
            guidance: d.guidance,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// Promote a candidate item-signal to tracked (`POST /signals/{id}/track`).
///
/// REAL: composes `phosk_ledger::signals::track_signal` (a write path) for the slug.
#[server]
pub async fn track_signal(id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        track_signal_with(session.db(), &id).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// [`track_signal`]'s logic over an explicit DB port (tests pass a fresh store).
#[cfg(feature = "server-deps")]
pub(crate) async fn track_signal_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    id: &str,
) -> Result<(), ServerFnError> {
    phosk_ledger::signals::track_signal(db, id)
        .await
        .map_err(crate::data::server_err)
}

/// Dismiss a candidate item-signal (`POST /signals/{id}/dismiss`).
///
/// REAL: composes `phosk_ledger::signals::dismiss_signal` for the slug.
#[server]
pub async fn dismiss_signal(id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        dismiss_signal_with(session.db(), &id).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// [`dismiss_signal`]'s logic over an explicit DB port (tests pass a fresh store).
#[cfg(feature = "server-deps")]
pub(crate) async fn dismiss_signal_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    id: &str,
) -> Result<(), ServerFnError> {
    phosk_ledger::signals::dismiss_signal(db, id)
        .await
        .map_err(crate::data::server_err)
}

// ── mappers (service DTO → wire DTO) ───────────────────────────────────────────

/// Map a `phosk_ledger` signal onto the wire [`SignalDto`].
#[cfg(feature = "server-deps")]
fn map_signal(s: phosk_ledger::signals::SignalDto) -> SignalDto {
    SignalDto {
        id: s.id,
        label: s.label,
        parent: s.parent,
        since: s.since,
        series: s.series,
        delta_pct: s.delta_pct,
        cycle_qty: s.cycle_qty,
        unit: s.unit,
        cycle_spend: s.cycle_spend,
        txns: s.txns,
        candidate: s.candidate,
        desc: s.desc,
    }
}
