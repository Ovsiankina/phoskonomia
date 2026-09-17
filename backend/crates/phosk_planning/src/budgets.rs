//! Budgets + categories service slice (F3).
//!
//! Mirrors the Dioxus view structs in `frontend/dioxus-app/src/data/budgets.rs`
//! (`GET /categories`, `/budget/totals`, `/budget/allocation`,
//! `/categories/{name}`, `/categories/{name}/transactions`). Every DTO here is
//! field-for-field identical to that wire truth: `#[serde(rename_all =
//! "camelCase")]`, money via `phosk_model::money_centimes` (exact i64 centimes).
//!
//! The service fn *signatures* and DTO *shapes* are fixed (the test agents pin
//! them verbatim). The bodies and derived-field helpers compute the spend
//! roll-ups and projections described below.
//!
//! ## Derived formulas
//!
//! Let `S` = current-cycle spend-to-date `[start, as_of]`, `cap` = category cap,
//! `N` = cycle length (days), `d` = day index. Per the build contract §5.2:
//! - `spent` = Σ current-cycle receipts in this category.
//! - `proj` (`projectedSpend`) = `S · N / d` (run-rate; checked centime math).
//! - `remaining` = `cap − spent` (signed).
//! - `usedPct` = `round(100 · spent / cap)` (0 when cap is 0/unlimited).
//! - `overCapAmount` = `max(0, proj − cap)`.
//! - `histAvg` = trailing-N-cycle average spend (§6 momentum helper).
//! - Totals: `allocated` = Σ caps; `spent` = Σ all spend-to-date;
//!   `projected` = Σ per-cat proj; `remaining` = `budget − spent`;
//!   `overAllocated` = `max(0, allocated − budget)`;
//!   `unallocated` = `max(0, budget − allocated)`; `envelopeCount` = count of caps.
//! - Allocation segment: `cap` (width), `share = cap / Σcaps`, `fixed`.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::{CycleWindow, Period};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::{CategoryCap, Receipt};

/// One budget envelope (`GET /categories` element; also the dashboard channels).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    /// Category name (its identity), e.g. `"Groceries"`.
    pub name: String,
    /// Cap / budget for the cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
    /// Spent so far this cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub spent: Money,
    /// Projected end-of-cycle spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub proj: Money,
    /// `budget − spent` (negative if over).
    #[serde(with = "phosk_model::money_centimes")]
    pub remaining: Money,
    /// Integer percent of cap used (0–999).
    pub used_pct: i32,
    /// `true` for a fixed/standing charge (untunable).
    pub fixed: bool,
    /// Entry count this cycle.
    pub items: u32,
    /// Sparkline points (unitless daily spend).
    pub spark: Vec<f64>,
    /// Per-cycle history bars (CHF as raw chart numbers — presentation series).
    pub hist: Vec<f64>,
    /// Due label for a fixed charge (empty otherwise).
    pub next: String,
    /// One-line AI guidance for this channel.
    pub note: String,
}

/// `GET /budget/totals` — the Budgets KPI band figures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetTotalsDto {
    /// Monthly budget ceiling.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
    /// Sum of all caps.
    #[serde(with = "phosk_model::money_centimes")]
    pub allocated: Money,
    /// Spent so far.
    #[serde(with = "phosk_model::money_centimes")]
    pub spent: Money,
    /// Projected end-of-cycle spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub projected: Money,
    /// Budget left.
    #[serde(with = "phosk_model::money_centimes")]
    pub remaining: Money,
    /// CHF allocated beyond budget (0 if none).
    #[serde(with = "phosk_model::money_centimes")]
    pub over_allocated: Money,
    /// CHF budget not yet allocated to a cap.
    #[serde(with = "phosk_model::money_centimes")]
    pub unallocated: Money,
    /// Number of envelopes.
    pub envelope_count: u32,
}

/// One segment of the allocation bar (`AllocationDto::segments` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocSegmentDto {
    /// Segment / category name.
    pub name: String,
    /// Its cap (drives the segment width).
    #[serde(with = "phosk_model::money_centimes")]
    pub cap: Money,
    /// Share of the bar (0–1). `None` (absent in the wire payload) → the page
    /// falls back to cap/domain; a present `Some(0.0)` renders 0% width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<f64>,
    /// `true` for fixed charges (rendered hatched).
    pub fixed: bool,
}

/// The GEMMA4 allocation advice line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocAdviceDto {
    /// Model badge.
    pub model: String,
    /// Advice sentence.
    pub text: String,
}

/// `GET /budget/allocation` — the channel-mix bar segments + AI advice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationDto {
    /// Ordered cap segments.
    pub segments: Vec<AllocSegmentDto>,
    /// AI advice on the mix.
    pub ai_advice: AllocAdviceDto,
}

/// `GET /categories/{name}` — the channel inspector detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDetailDto {
    /// Projected end-of-cycle spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub projected_spend: Money,
    /// N-cycle average spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub hist_avg: Money,
    /// CHF over cap (0 if under).
    #[serde(with = "phosk_model::money_centimes")]
    pub over_cap_amount: Money,
    /// AI guidance paragraph.
    pub guidance: String,
}

/// A category's recent transaction (`GET /categories/{name}/transactions` row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTxnDto {
    /// Stable id.
    pub id: String,
    /// Date label, e.g. `"16 JUN"`.
    pub date: String,
    /// Shop.
    pub shop: String,
    /// Amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
}

/// The budget envelopes (`GET /categories`).
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter; returns
/// [`PhoskError::Overflow`] on any checked centime overflow.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn categories(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<Vec<CategoryDto>, PhoskError> {
    let window = Period::Month.resolve(as_of)?;
    let caps = db.category_caps().await?;
    let receipts = receipts_to_date(db, window).await?;
    let mut out = Vec::with_capacity(caps.len());
    for cap in &caps {
        out.push(envelope_for(db, cap, &receipts, window).await?);
    }
    Ok(out)
}

/// Current-cycle receipts up to (and including) `as_of`.
async fn receipts_to_date(
    db: &dyn DatabaseAdapter,
    window: CycleWindow,
) -> Result<Vec<Receipt>, PhoskError> {
    db.receipts_between(window.start, window.as_of).await
}

/// Build one envelope DTO from its cap and the current-cycle receipts.
async fn envelope_for(
    db: &dyn DatabaseAdapter,
    cap: &CategoryCap,
    receipts: &[Receipt],
    window: CycleWindow,
) -> Result<CategoryDto, PhoskError> {
    let in_cat: Vec<&Receipt> = receipts.iter().filter(|r| r.category == cap.name).collect();
    let spent = Money::sum(in_cat.iter().map(|r| r.amount))?;
    let cap_money = cap.cap.unwrap_or(Money::ZERO);
    let proj = project_run_rate(spent, window)?;
    let remaining = cap_money.checked_sub(spent)?;
    let used = used_pct(spent, cap_money);
    let hist = db.budget_history(&cap.name).await?;
    let hist_series: Vec<f64> = hist.iter().map(|h| h.spent.as_chf_f64()).collect();
    let spark: Vec<f64> = in_cat.iter().map(|r| r.amount.as_chf_f64()).collect();
    Ok(CategoryDto {
        name: cap.name.clone(),
        budget: cap_money,
        spent,
        proj,
        remaining,
        used_pct: used,
        fixed: cap.fixed,
        items: u32::try_from(in_cat.len()).unwrap_or(u32::MAX),
        spark,
        hist: hist_series,
        next: String::new(),
        note: cap.note.clone(),
    })
}

/// The Budgets KPI band (`GET /budget/totals`).
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter; [`PhoskError::Overflow`] on
/// any checked centime overflow.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn budget_totals(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<BudgetTotalsDto, PhoskError> {
    let config = db.budget_config().await?;
    let budget = config.monthly_budget;
    let envelopes = categories(db, as_of).await?;

    // An unlimited cap renders as a zero `budget` in the DTO, so summing the
    // envelope budgets already excludes unlimited channels from `allocated`.
    let allocated = Money::sum(envelopes.iter().map(|c| c.budget))?;
    let spent = Money::sum(envelopes.iter().map(|c| c.spent))?;
    let projected = Money::sum(envelopes.iter().map(|c| c.proj))?;
    let remaining = budget.checked_sub(spent)?;
    let over_allocated = allocated.checked_sub(budget)?.max(Money::ZERO);
    let unallocated = budget.checked_sub(allocated)?.max(Money::ZERO);
    let envelope_count = u32::try_from(envelopes.len()).unwrap_or(u32::MAX);

    Ok(BudgetTotalsDto {
        budget,
        allocated,
        spent,
        projected,
        remaining,
        over_allocated,
        unallocated,
        envelope_count,
    })
}

/// The allocation console (`GET /budget/allocation`).
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter; [`PhoskError::Overflow`] on
/// any checked centime overflow.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn allocation(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<AllocationDto, PhoskError> {
    let _ = as_of;
    let caps = db.category_caps().await?;
    let total = Money::sum(caps.iter().filter_map(|c| c.cap))?;
    let total_c = total.centimes();
    let segments = caps
        .iter()
        .map(|c| {
            let cap = c.cap.unwrap_or(Money::ZERO);
            AllocSegmentDto {
                name: c.name.clone(),
                cap,
                share: Some(share_of(cap.centimes(), total_c)),
                fixed: c.fixed,
            }
        })
        .collect();
    Ok(AllocationDto {
        segments,
        ai_advice: AllocAdviceDto {
            model: "GEMMA4".to_owned(),
            text: "Rent and insurance dominate the mix; the discretionary channels have room."
                .to_owned(),
        },
    })
}

/// One channel's inspector detail (`GET /categories/{name}`).
///
/// # Errors
/// Returns [`PhoskError::NotFound`] if no category matches `name`; propagates
/// any other [`PhoskError`]; [`PhoskError::Overflow`] on checked centime overflow.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of, name = %name))]
pub async fn category_detail(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
    name: &str,
) -> Result<CategoryDetailDto, PhoskError> {
    // `category_cap_by_name` surfaces `NotFound` for an unknown channel.
    let cap = db.category_cap_by_name(name).await?;
    let window = Period::Month.resolve(as_of)?;
    let receipts = receipts_to_date(db, window).await?;
    let spent = Money::sum(
        receipts
            .iter()
            .filter(|r| r.category == cap.name)
            .map(|r| r.amount),
    )?;
    let projected_spend = project_run_rate(spent, window)?;
    let cap_money = cap.cap.unwrap_or(Money::ZERO);
    let over_cap_amount = projected_spend.checked_sub(cap_money)?.max(Money::ZERO);

    let hist = db.budget_history(&cap.name).await?;
    let prior: Vec<Money> = hist.iter().map(|h| h.spent).collect();
    let n = momentum_baseline_cycles(db).await?;
    let hist_avg = trailing_avg(&prior, n)?;

    Ok(CategoryDetailDto {
        projected_spend,
        hist_avg,
        over_cap_amount,
        guidance: cap.note.clone(),
    })
}

/// A category's recent transactions (`GET /categories/{name}/transactions`).
///
/// # Errors
/// Returns [`PhoskError::NotFound`] if no category matches `name`; propagates
/// any other [`PhoskError`].
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of, name = %name))]
pub async fn category_transactions(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
    name: &str,
) -> Result<Vec<CategoryTxnDto>, PhoskError> {
    // Surface `NotFound` for an unknown channel before listing anything.
    let cap = db.category_cap_by_name(name).await?;
    let window = Period::Month.resolve(as_of)?;
    let mut rows: Vec<&Receipt> = Vec::new();
    let receipts = receipts_to_date(db, window).await?;
    for r in &receipts {
        if r.category == cap.name {
            rows.push(r);
        }
    }
    // Newest first; ties broken by slug for a stable order.
    rows.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));
    Ok(rows
        .into_iter()
        .map(|r| CategoryTxnDto {
            id: r.slug.clone(),
            date: date_label(r.date),
            shop: r.shop.clone(),
            amount: r.amount,
        })
        .collect())
}

/// Set (or clear, with `None`) a category's cap.
///
/// # Errors
/// Returns [`PhoskError::NotFound`] if no category matches `name`; propagates
/// any adapter [`PhoskError`].
#[tracing::instrument(level = "debug", skip_all, fields(name = %name))]
pub async fn set_cap(
    db: &dyn DatabaseAdapter,
    name: &str,
    cap: Option<Money>,
) -> Result<(), PhoskError> {
    // `set_category_cap` resolves the envelope by name and surfaces `NotFound`
    // for an unknown channel.
    db.set_category_cap(name, cap).await
}

// ── derived-field helpers ───────────────────────────────────────────────────────

/// Run-rate projection: `spent · N / d` (checked integer-centime math). Returns
/// `spent` unchanged when the window's day index is `0` (never divides by zero).
///
/// # Errors
/// [`PhoskError::Overflow`] if the scaled multiplication overflows i64.
fn project_run_rate(spent: Money, window: CycleWindow) -> Result<Money, PhoskError> {
    let day_index = i64::from(window.day_index());
    if day_index == 0 {
        return Ok(spent);
    }
    let len_days = i64::from(window.len_days());
    let scaled = spent
        .centimes()
        .checked_mul(len_days)
        .ok_or_else(|| PhoskError::Overflow(format!("projecting {spent} over {len_days} days")))?;
    Ok(Money::from_centimes(scaled / day_index))
}

/// `round(100 · spent / cap)` as an integer percent; `0` when `cap` is `0`.
// The cast computes a small unitless percentage (never money) and saturates
// rather than wrapping; no money value is reconstructed from the float.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "computing a small integer percentage, not an amount; saturating cast"
)]
fn used_pct(spent: Money, cap: Money) -> i32 {
    let cap_c = cap.centimes();
    if cap_c == 0 {
        return 0;
    }
    let pct = 100.0 * spent.centimes() as f64 / cap_c as f64;
    pct.round() as i32
}

/// `cap / total` as a `0–1` share; `0.0` when `total` is `0`.
// A unitless ratio for segment width, never reused as a money value.
#[allow(
    clippy::cast_precision_loss,
    reason = "computing a unitless 0–1 ratio, not an amount"
)]
fn share_of(cap: i64, total: i64) -> f64 {
    if total == 0 {
        0.0
    } else {
        cap as f64 / total as f64
    }
}

/// Trailing-N-cycle average of a per-cycle [`Money`] series (oldest → newest),
/// taking the last `n` (or all if fewer) and integer-dividing the checked sum by
/// the count. [`Money::ZERO`] when there are no prior cycles.
///
/// # Errors
/// [`PhoskError::Overflow`] if the checked sum overflows i64 centimes.
fn trailing_avg(prior_cycles: &[Money], n: u32) -> Result<Money, PhoskError> {
    let take = usize::try_from(n)
        .unwrap_or(usize::MAX)
        .min(prior_cycles.len());
    if take == 0 {
        return Ok(Money::ZERO);
    }
    let window = &prior_cycles[prior_cycles.len() - take..];
    let total = Money::sum(window.iter().copied())?;
    let count = i64::try_from(take).unwrap_or(i64::MAX);
    Ok(Money::from_centimes(total.centimes() / count))
}

/// The trailing-N baseline length from settings; defaults to `3` on a missing or
/// unparseable `momentum_baseline_cycles` preference.
async fn momentum_baseline_cycles(db: &dyn DatabaseAdapter) -> Result<u32, PhoskError> {
    match db.preference("momentum_baseline_cycles").await {
        Ok(p) => Ok(p.value.parse::<u32>().unwrap_or(3)),
        Err(PhoskError::NotFound(_)) => Ok(3),
        Err(e) => Err(e),
    }
}

/// `"DD MON"` upper-case date label, e.g. `"16 JUN"`.
fn date_label(d: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    let idx = (d.month() as usize).saturating_sub(1).min(11);
    format!("{:02} {}", d.day(), MONTHS[idx])
}
