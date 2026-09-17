//! Budgets + categories read-model (F3).
//!
//! Shared by the Budgets page (envelopes, allocation console, KPI band, channel
//! inspector) and the Dashboard (the `c-channels` strip + category-budgets
//! matrix). Mirrors React `GET /categories`, `/budget/totals`,
//! `/budget/allocation`, `/categories/{name}`, `/categories/{name}/transactions`.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One budget envelope (`GET /categories` element; also the dashboard channels).
///
/// `budget` is the cap; `spent`/`proj`/`remaining` the cycle figures (proj =
/// projected end-of-cycle); `used_pct` the integer percent of cap used; `fixed`
/// marks an untunable standing charge; `items` the entry count; `spark` the
/// sparkline points; `hist` the per-cycle history bars; `next` the fixed-charge
/// due label; `note` the AI guidance line.
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// falls back to cap/domain; a present `Some(0.0)` renders 0% width — faithful
    /// to the JSX `seg.share != null` presence test (NOT a value `!= 0` test).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<f64>,
    /// `true` for fixed charges (rendered hatched).
    pub fixed: bool,
}

/// The GEMMA4 allocation advice line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
/// REAL: composes `phosk_planning::budgets::categories` for the seeded cycle.
#[server]
pub async fn get_categories() -> Result<Vec<CategoryDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let cats = phosk_planning::budgets::categories(session.db(), crate::data::today())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(cats.into_iter().map(map_category).collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The Budgets KPI band (`GET /budget/totals`).
///
/// REAL: composes `phosk_planning::budgets::budget_totals`.
#[server]
pub async fn get_budget_totals() -> Result<BudgetTotalsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let t = phosk_planning::budgets::budget_totals(session.db(), crate::data::today())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(BudgetTotalsDto {
            budget: t.budget,
            allocated: t.allocated,
            spent: t.spent,
            projected: t.projected,
            remaining: t.remaining,
            over_allocated: t.over_allocated,
            unallocated: t.unallocated,
            envelope_count: t.envelope_count,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The allocation console (`GET /budget/allocation`).
///
/// REAL: composes `phosk_planning::budgets::allocation`.
#[server]
pub async fn get_allocation() -> Result<AllocationDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let a = phosk_planning::budgets::allocation(session.db(), crate::data::today())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(AllocationDto {
            segments: a
                .segments
                .into_iter()
                .map(|s| AllocSegmentDto {
                    name: s.name,
                    cap: s.cap,
                    share: s.share,
                    fixed: s.fixed,
                })
                .collect(),
            ai_advice: AllocAdviceDto {
                model: a.ai_advice.model,
                text: a.ai_advice.text,
            },
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// One channel's inspector detail (`GET /categories/{name}`).
///
/// REAL: composes `phosk_planning::budgets::category_detail` for the named channel.
#[server]
pub async fn get_category_detail(name: String) -> Result<CategoryDetailDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let d = phosk_planning::budgets::category_detail(session.db(), crate::data::today(), &name)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(CategoryDetailDto {
            projected_spend: d.projected_spend,
            hist_avg: d.hist_avg,
            over_cap_amount: d.over_cap_amount,
            guidance: d.guidance,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = name;
        Err(ServerFnError::new("server-only"))
    }
}

/// A category's recent transactions (`GET /categories/{name}/transactions`).
///
/// REAL: composes `phosk_planning::budgets::category_transactions`.
#[server]
pub async fn get_category_transactions(name: String) -> Result<Vec<CategoryTxnDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let rows = phosk_planning::budgets::category_transactions(
            session.db(),
            crate::data::today(),
            &name,
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| CategoryTxnDto {
                id: r.id,
                date: r.date,
                shop: r.shop,
                amount: r.amount,
            })
            .collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = name;
        Err(ServerFnError::new("server-only"))
    }
}

// ── mappers (service DTO → wire DTO) ───────────────────────────────────────────

/// Map a `phosk_planning` envelope onto the wire [`CategoryDto`].
#[cfg(feature = "server-deps")]
fn map_category(c: phosk_planning::budgets::CategoryDto) -> CategoryDto {
    CategoryDto {
        name: c.name,
        budget: c.budget,
        spent: c.spent,
        proj: c.proj,
        remaining: c.remaining,
        used_pct: c.used_pct,
        fixed: c.fixed,
        items: c.items,
        spark: c.spark,
        hist: c.hist,
        next: c.next,
        note: c.note,
    }
}
