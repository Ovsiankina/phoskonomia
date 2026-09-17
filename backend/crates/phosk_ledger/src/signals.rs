//! `signals` — tracked item-signals (the coral `⌁` pills) + candidates + movers.
//!
//! An item-signal is a specific product rolled up across receipts so its
//! quantity / spend / momentum can be watched. DTOs mirror
//! `dioxus-app/src/data/signals.rs` field-for-field (camelCase, money as exact
//! centimes). Momentum (`delta_pct`, `priorAvg`) comes from the trailing-N
//! baseline helper in `phosk_insights` (§6) — do NOT reimplement it here.

use chrono::Datelike;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::Period;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::{Signal, SignalOccurrence};
use serde::{Deserialize, Serialize};

/// One tracked item-signal, or (with `candidate`) an AI-proposed one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalDto {
    /// Stable id (the seed slug, e.g. `"coffee"`).
    pub id: String,
    /// Display label, e.g. `"Oat-milk flat white"`.
    pub label: String,
    /// Parent budget category, e.g. `"Coffee & snacks"`.
    pub parent: String,
    /// Since-label (when tracking began), e.g. `"MAR 2026"`.
    pub since: String,
    /// 12-point trend spark (unitless cycle spend points).
    pub series: Vec<f64>,
    /// Signed momentum percent vs the trailing-N baseline (e.g. `+28`).
    pub delta_pct: i32,
    /// Quantity consumed this cycle.
    pub cycle_qty: f64,
    /// Unit label for `cycle_qty`, e.g. `"cups"`.
    pub unit: String,
    /// Spent on this signal this cycle (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub cycle_spend: Money,
    /// Number of receipts this signal appears in this cycle.
    pub txns: u32,
    /// `true` for an AI candidate (`!tracked`).
    pub candidate: bool,
    /// One-line pitch shown for a candidate instead of `parent · since`.
    pub desc: String,
}

/// `movers` payload — the two extreme signals plus the full ranked list.
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

/// A single past occurrence of a signal (inspector "recent" row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalOccurrenceDto {
    /// Date label, e.g. `"16 JUN"`.
    pub date: String,
    /// Shop the occurrence came from.
    pub shop: String,
    /// Spend for this occurrence (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
}

/// `signal_detail` payload — the full inspector for one signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalDetailDto {
    /// The headline signal record (flattened into the parent object).
    #[serde(flatten)]
    pub signal: SignalDto,
    /// All-time spend on this signal (exact centimes).
    #[serde(with = "phosk_model::money_centimes")]
    pub all_time_spend: Money,
    /// All-time occurrence count.
    pub all_time_txns: u32,
    /// Recent occurrences, newest first.
    pub recent: Vec<SignalOccurrenceDto>,
    /// One-line AI read on the trend.
    pub guidance: String,
}

/// All tracked item-signals (momentum + cycle roll-ups derived per signal).
#[tracing::instrument(level = "debug", skip_all)]
pub async fn list_signals(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<Vec<SignalDto>, PhoskError> {
    let mut out = Vec::new();
    for signal in db.signals().await? {
        if signal.tracked {
            out.push(build_signal_dto(db, signal, as_of).await?);
        }
    }
    Ok(out)
}

/// Project a persisted [`Signal`] into its wire DTO: current-cycle spend and
/// quantity are summed over the signal's occurrences inside the resolved month
/// window; `txns`/`series`/`delta_pct` are the signal's curated attributes.
async fn build_signal_dto(
    db: &dyn DatabaseAdapter,
    signal: Signal,
    as_of: chrono::NaiveDate,
) -> Result<SignalDto, PhoskError> {
    let window = Period::Month.resolve(as_of)?;
    let occurrences = db.signal_occurrences(signal.id).await?;

    let in_cycle = occurrences
        .iter()
        .filter(|o| o.date >= window.start && o.date <= window.end);
    let cycle_spend = Money::sum(in_cycle.clone().map(|o| o.amount))?;
    let cycle_qty: f64 = in_cycle.map(|o| o.qty).sum();

    Ok(SignalDto {
        id: signal.slug,
        label: signal.label,
        parent: signal.parent,
        since: month_year_label(signal.since),
        series: signal.series,
        delta_pct: signal.delta_pct,
        cycle_qty,
        unit: signal.unit,
        cycle_spend,
        txns: signal.txns,
        candidate: !signal.tracked,
        desc: signal.desc,
    })
}

/// `"MAR 2026"` month-year label (the `since` presentation on the DTO).
fn month_year_label(date: chrono::NaiveDate) -> String {
    format!("{} {}", month_abbrev(date.month()), date.year())
}

/// `"16 JUN"` day label for an occurrence row.
fn day_label(date: chrono::NaiveDate) -> String {
    format!("{:02} {}", date.day(), month_abbrev(date.month()))
}

/// Uppercase three-letter month abbreviation.
const fn month_abbrev(month: u32) -> &'static str {
    match month {
        1 => "JAN",
        2 => "FEB",
        3 => "MAR",
        4 => "APR",
        5 => "MAY",
        6 => "JUN",
        7 => "JUL",
        8 => "AUG",
        9 => "SEP",
        10 => "OCT",
        11 => "NOV",
        _ => "DEC",
    }
}

/// AI-detected candidate item-signals not yet tracked (`!tracked`).
#[tracing::instrument(level = "debug", skip_all)]
pub async fn signal_candidates(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<Vec<SignalDto>, PhoskError> {
    let mut out = Vec::new();
    for signal in db.signals().await? {
        if !signal.tracked {
            out.push(build_signal_dto(db, signal, as_of).await?);
        }
    }
    Ok(out)
}

/// The fastest riser / faller pair + the momentum-ranked full list.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn movers(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<MoversDto, PhoskError> {
    let mut all = list_signals(db, as_of).await?;
    // Momentum-ranked descending (riser first, faller last); ties on id for a
    // deterministic order.
    all.sort_by(|a, b| b.delta_pct.cmp(&a.delta_pct).then_with(|| a.id.cmp(&b.id)));

    let riser = all
        .first()
        .cloned()
        .ok_or_else(|| PhoskError::NotFound("no tracked signals for movers".to_owned()))?;
    let faller = all
        .last()
        .cloned()
        .ok_or_else(|| PhoskError::NotFound("no tracked signals for movers".to_owned()))?;

    Ok(MoversDto { riser, faller, all })
}

/// One signal's full inspector payload, resolved by its seed `slug`.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn signal_detail(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
    slug: &str,
) -> Result<SignalDetailDto, PhoskError> {
    let signal = db.signal_by_slug(slug).await?;
    let label = signal.label.clone();
    let headline = build_signal_dto(db, signal.clone(), as_of).await?;

    let mut occurrences = db.signal_occurrences(signal.id).await?;
    let all_time_spend = Money::sum(occurrences.iter().map(|o| o.amount))?;
    // The current cycle is the lower bound on lifetime totals; the curated
    // `txns` count carries the receipt-level history the occurrence rollups
    // summarise.
    let occ_txns = u32::try_from(occurrences.len()).unwrap_or(u32::MAX);
    let all_time_txns = occ_txns.max(headline.txns);

    // Newest occurrence first for the inspector's "recent" rail.
    occurrences.sort_by(|a, b| b.date.cmp(&a.date));
    let recent: Vec<SignalOccurrenceDto> = occurrences.iter().map(occurrence_dto).collect();

    let guidance = if headline.delta_pct >= 0 {
        format!(
            "{label} is heating up this cycle. The AI keeps it on watch and flags any sustained drift."
        )
    } else {
        format!(
            "{label} is cooling this cycle. The AI keeps it on watch and flags any sustained drift."
        )
    };

    Ok(SignalDetailDto {
        signal: headline,
        all_time_spend,
        all_time_txns,
        recent,
        guidance,
    })
}

/// One occurrence projected to its inspector row (date label, shop, amount).
fn occurrence_dto(o: &SignalOccurrence) -> SignalOccurrenceDto {
    SignalOccurrenceDto {
        date: day_label(o.date),
        shop: o.shop.clone(),
        amount: o.amount,
    }
}

/// Promote a candidate signal to tracked. Write path.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn track_signal(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError> {
    let signal = db.signal_by_slug(slug).await?;
    db.set_signal_tracked(signal.id, true).await
}

/// Dismiss a candidate signal. Write path.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn dismiss_signal(db: &dyn DatabaseAdapter, slug: &str) -> Result<(), PhoskError> {
    let signal = db.signal_by_slug(slug).await?;
    db.delete_signal(signal.id).await
}
