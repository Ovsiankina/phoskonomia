//! Subscriptions read-model (F3).
//!
//! Backs the Subscriptions page: the billing-sweep impulse train, the KPI band,
//! the tunable card/row grid and the right-dock inspector. Mirrors React
//! `GET /subscriptions` (sort/group/amounts params), `/subscriptions/stats`,
//! `/subscriptions/billing-sweep`, `/subscriptions/{id}`.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One standing charge (`GET /subscriptions` element).
///
/// `cadence` is `"monthly"|"yearly"`; `status` `"ok"|"soon"|"due"|"watch"|
/// "paused"` (with a human `status_label`); `monthly_equiv`/`annual` are the
/// derived run-rates; `days_until`/`next_label` the cycle countdown; `source`
/// `"user"|"llm"` (auto-detected); `hist` the price-history bars; `glyph` the
/// card badge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionDto {
    /// Stable id.
    pub id: String,
    /// Service name, e.g. `"Netflix"`.
    pub name: String,
    /// Per-charge amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// `"monthly"` or `"yearly"`.
    pub cadence: String,
    /// Status key.
    pub status: String,
    /// Human status label, e.g. `"DUE SOON"`.
    pub status_label: String,
    /// Monthly-equivalent run-rate.
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly_equiv: Money,
    /// Annualized total.
    #[serde(with = "phosk_model::money_centimes")]
    pub annual: Money,
    /// Whole days until the next charge (monthly only; ≤0 = due).
    pub days_until: i32,
    /// Next-charge label, e.g. `"22 JUN"`.
    pub next_label: String,
    /// Day-of-month for monthly charges (0 for yearly).
    pub day: u32,
    /// Month label for yearly charges (empty for monthly).
    pub month: String,
    /// `"user"` or `"llm"` (auto-detected).
    pub source: String,
    /// Category, e.g. `"Entertainment"`.
    pub category: String,
    /// Card badge glyph, e.g. `"▶"`.
    pub glyph: String,
    /// Tracking-since label.
    pub since: String,
    /// `true` if the last charge rose vs the prior one.
    pub price_rose: bool,
    /// Price-history bars (raw chart numbers).
    pub hist: Vec<f64>,
    /// One-line note / AI guidance.
    pub note: String,
}

/// `GET /subscriptions/stats` — the KPI band roll-ups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubStatsDto {
    /// Active standing-charge count.
    pub count: u32,
    /// Monthly run-rate.
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly: Money,
    /// Annualized total.
    #[serde(with = "phosk_model::money_centimes")]
    pub annual: Money,
    /// Count auto-detected by the AI.
    pub auto_count: u32,
    /// Next-30-days roll-up.
    pub next30: Next30Dto,
    /// Needs-attention roll-up.
    pub flagged: FlaggedDto,
}

/// The "next 30 days" KPI roll-up (`SubStatsDto::next30`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Next30Dto {
    /// Number of charges due in the next 30 days.
    pub count: u32,
    /// Their combined total.
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
    /// The charges, soonest first.
    pub items: Vec<Next30ItemDto>,
}

/// One upcoming charge in the next-30 roll-up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Next30ItemDto {
    /// Service name.
    pub name: String,
    /// Days until it charges.
    pub days_until: i32,
}

/// The "needs attention" KPI roll-up (`SubStatsDto::flagged`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlaggedDto {
    /// Number flagged by the AI for review.
    pub count: u32,
    /// Supporting note.
    pub note: String,
}

/// One impulse on the billing sweep (`BillingSweepDto::impulses` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpulseDto {
    /// Subscription id (selects the inspector).
    pub id: String,
    /// Service name.
    pub name: String,
    /// Charge amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Day-of-cycle the charge lands on.
    pub day: u32,
    /// Status: `"paid"|"soon"|"due"|"watch"|"ok"`.
    pub status: String,
}

/// The sweep's cycle window (`BillingSweepDto::cycle`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepCycleDto {
    /// Current day-of-cycle (the TODAY marker).
    pub day: u32,
    /// Days in the cycle.
    pub days: u32,
    /// Short "today" label.
    pub as_of: String,
}

/// The next charge shown in the sweep footer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepNextDto {
    /// Service name.
    pub name: String,
    /// Next-charge label.
    pub next_label: String,
    /// Amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
}

/// The sweep footer roll-up (`BillingSweepDto::footer`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepFooterDto {
    /// CHF paid so far this cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub paid_this_cycle: Money,
    /// CHF still due this cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub still_due: Money,
    /// The next upcoming charge.
    pub next: SweepNextDto,
    /// A short footer note.
    pub note: String,
}

/// `GET /subscriptions/billing-sweep` — the periodic impulse train.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingSweepDto {
    /// Cycle window for the sweep axis.
    pub cycle: SweepCycleDto,
    /// The charge impulses.
    pub impulses: Vec<ImpulseDto>,
    /// Footer roll-up.
    pub footer: SweepFooterDto,
}

/// One recorded charge in the inspector (`SubscriptionDetailDto::recent` row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubChargeDto {
    /// Stable id.
    pub id: String,
    /// Date label.
    pub date: String,
    /// Note, e.g. `"confirmed"`.
    pub note: String,
    /// Amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
}

/// The inspector guidance line (`SubscriptionDetailDto::guidance`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubGuidanceDto {
    /// Guidance text.
    pub text: String,
    /// Severity, e.g. `"coral"` or empty.
    pub severity: String,
}

/// `GET /subscriptions/{id}` — the inspector payload (the list record + extras).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionDetailDto {
    /// The headline subscription record.
    #[serde(flatten)]
    pub subscription: SubscriptionDto,
    /// Recent recorded charges.
    pub recent: Vec<SubChargeDto>,
    /// AI guidance.
    pub guidance: SubGuidanceDto,
    /// `true` for an AI candidate (CONFIRM/DISMISS instead of cancel).
    pub candidate: bool,
}

/// Subscription list options for `GET /subscriptions`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubFilter {
    /// Sort key: `due|amount|name`.
    pub sort: String,
    /// `"cadence"` to group, else empty.
    pub group: String,
    /// `monthly|annual` amount display mode (display only here).
    pub amounts: String,
}

/// The standing charges (`GET /subscriptions`).
///
/// REAL: composes `phosk_recurring::subscriptions::list_subscriptions` (honours
/// the `sort`/`group` params) for the seeded cycle.
#[server]
pub async fn list_subscriptions(filter: SubFilter) -> Result<Vec<SubscriptionDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let svc_filter = phosk_recurring::subscriptions::SubFilter {
            sort: filter.sort,
            group: filter.group,
            amounts: filter.amounts,
        };
        let subs = phosk_recurring::subscriptions::list_subscriptions(
            session.db(),
            crate::data::today(),
            svc_filter,
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(subs.into_iter().map(map_sub).collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = filter;
        Err(ServerFnError::new("server-only"))
    }
}

/// The KPI band roll-ups (`GET /subscriptions/stats`).
///
/// REAL: composes `phosk_recurring::subscriptions::subscription_stats`.
#[server]
pub async fn get_subscription_stats() -> Result<SubStatsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let s =
            phosk_recurring::subscriptions::subscription_stats(session.db(), crate::data::today())
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(map_stats(s))
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The billing-sweep impulse train (`GET /subscriptions/billing-sweep`).
///
/// REAL: composes `phosk_recurring::subscriptions::billing_sweep`.
#[server]
pub async fn get_billing_sweep() -> Result<BillingSweepDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let b = phosk_recurring::subscriptions::billing_sweep(session.db(), crate::data::today())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(BillingSweepDto {
            cycle: SweepCycleDto {
                day: b.cycle.day,
                days: b.cycle.days,
                as_of: b.cycle.as_of,
            },
            impulses: b
                .impulses
                .into_iter()
                .map(|i| ImpulseDto {
                    id: i.id,
                    name: i.name,
                    amount: i.amount,
                    day: i.day,
                    status: i.status,
                })
                .collect(),
            footer: SweepFooterDto {
                paid_this_cycle: b.footer.paid_this_cycle,
                still_due: b.footer.still_due,
                next: SweepNextDto {
                    name: b.footer.next.name,
                    next_label: b.footer.next.next_label,
                    amount: b.footer.next.amount,
                },
                note: b.footer.note,
            },
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// One subscription's inspector payload (`GET /subscriptions/{id}`).
///
/// REAL: composes `phosk_recurring::subscriptions::subscription_detail` for the slug.
#[server]
pub async fn get_subscription(id: String) -> Result<SubscriptionDetailDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let d = phosk_recurring::subscriptions::subscription_detail(
            session.db(),
            crate::data::today(),
            &id,
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(SubscriptionDetailDto {
            subscription: map_sub(d.subscription),
            recent: d
                .recent
                .into_iter()
                .map(|c| SubChargeDto {
                    id: c.id,
                    date: c.date,
                    note: c.note,
                    amount: c.amount,
                })
                .collect(),
            guidance: SubGuidanceDto {
                text: d.guidance.text,
                severity: d.guidance.severity,
            },
            candidate: d.candidate,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

// ── mappers (service DTO → wire DTO) ───────────────────────────────────────────

/// Map a `phosk_recurring` subscription onto the wire [`SubscriptionDto`].
#[cfg(feature = "server-deps")]
fn map_sub(s: phosk_recurring::subscriptions::SubscriptionDto) -> SubscriptionDto {
    SubscriptionDto {
        id: s.id,
        name: s.name,
        amount: s.amount,
        cadence: s.cadence,
        status: s.status,
        status_label: s.status_label,
        monthly_equiv: s.monthly_equiv,
        annual: s.annual,
        days_until: s.days_until,
        next_label: s.next_label,
        day: s.day,
        month: s.month,
        source: s.source,
        category: s.category,
        glyph: s.glyph,
        since: s.since,
        price_rose: s.price_rose,
        hist: s.hist,
        note: s.note,
    }
}

/// Map the `phosk_recurring` stats roll-up onto the wire [`SubStatsDto`].
#[cfg(feature = "server-deps")]
fn map_stats(s: phosk_recurring::subscriptions::SubStatsDto) -> SubStatsDto {
    SubStatsDto {
        count: s.count,
        monthly: s.monthly,
        annual: s.annual,
        auto_count: s.auto_count,
        next30: Next30Dto {
            count: s.next30.count,
            total: s.next30.total,
            items: s
                .next30
                .items
                .into_iter()
                .map(|i| Next30ItemDto {
                    name: i.name,
                    days_until: i.days_until,
                })
                .collect(),
        },
        flagged: FlaggedDto {
            count: s.flagged.count,
            note: s.flagged.note,
        },
    }
}
