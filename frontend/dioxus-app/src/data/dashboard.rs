//! Dashboard read-model (F3) — the one page wired to REAL backend logic.
//!
//! The console hero + terminal grid in `pages/dashboard.rs` mirror React's
//! `DashFull`, which fetched a fan of endpoints. The three "current cycle" reads
//! (`totals`, `spend-series`, `top-shops`) have real services in `phosk_insights`
//! and are driven here against `MemoryDb::seeded()` on the server; the remaining
//! panels (categories, recent txns, recurring, alerts, AI insight) are seeded to
//! the design's shape until their services land.
//!
//! Money crosses as exact [`Money`] (centimes) via `phosk_model::money_centimes`;
//! the page renders it through [`crate::data::chf`].

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

// ── totals (REAL) ────────────────────────────────────────────────────────────

/// `GET /cycle/current/totals` — the headline cycle KPIs.
///
/// Field-for-field the `phosk_insights::TotalsDto`, but with money kept as exact
/// [`Money`] on the wire (centimes) instead of the HTTP edge's CHF float, so the
/// Dioxus client renders the real newtype. `savings_rate` is a 0–1 ratio;
/// `spent_pct`/`vs_last_cycle_pct` are integer percents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotalsDto {
    /// Cycle budget ceiling.
    #[serde(with = "phosk_model::money_centimes")]
    pub budget: Money,
    /// Spent so far this cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub spent: Money,
    /// `budget − spent` (negative if overspent).
    #[serde(with = "phosk_model::money_centimes")]
    pub remaining: Money,
    /// Sum of category caps.
    #[serde(with = "phosk_model::money_centimes")]
    pub allocated: Money,
    /// Savings target for the cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub savings_target: Money,
    /// Savings realised so far.
    #[serde(with = "phosk_model::money_centimes")]
    pub saved: Money,
    /// Projected savings at the current run-rate.
    #[serde(with = "phosk_model::money_centimes")]
    pub savings_projected: Money,
    /// `saved / budget`, 0–1 ratio.
    pub savings_rate: f64,
    /// Previous cycle's total spend.
    #[serde(with = "phosk_model::money_centimes")]
    pub last_cycle_spent: Money,
    /// `round(100·spent/budget)`, 0–100.
    pub spent_pct: i32,
    /// Signed percent vs last cycle.
    pub vs_last_cycle_pct: i32,
    /// `remaining / daysLeft` — daily spend to stay on budget.
    #[serde(with = "phosk_model::money_centimes")]
    pub per_day_to_stay_on_budget: Money,
}

/// The headline cycle KPIs (`GET /cycle/current/totals`).
///
/// REAL: composes `phosk_planning::totals` for the seeded June cycle.
#[server]
pub async fn get_totals() -> Result<TotalsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let t = phosk_insights::dashboard_totals(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        // `phosk_insights::TotalsDto` serializes money as CHF float; we want exact
        // centimes on this wire, so re-shape from its public Money fields.
        Ok(TotalsDto {
            budget: t.budget,
            spent: t.spent,
            remaining: t.remaining,
            allocated: t.allocated,
            savings_target: t.savings_target,
            saved: t.saved,
            savings_projected: t.savings_projected,
            savings_rate: t.savings_rate,
            last_cycle_spent: t.last_cycle_spent,
            spent_pct: t.spent_pct,
            vs_last_cycle_pct: t.vs_last_cycle_pct,
            per_day_to_stay_on_budget: t.per_day_to_stay_on_budget,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

// ── spend series (REAL) ──────────────────────────────────────────────────────

/// `GET /cycle/current/spend-series` — the spend-over-time arrays for the chart.
///
/// All arrays are the cycle length (30 for June). `daily` is per-day spend,
/// `cumulative` its running sum, `pace` the even-spend ideal,
/// `last_cycle_cumulative` the prior cycle aligned to this length, `today_index`
/// the 0-based position of `as_of`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendSeriesDto {
    /// Per-day spend.
    #[serde(with = "money_vec_centimes")]
    pub daily: Vec<Money>,
    /// Running cumulative spend.
    #[serde(with = "money_vec_centimes")]
    pub cumulative: Vec<Money>,
    /// Even-spend pace line.
    #[serde(with = "money_vec_centimes")]
    pub pace: Vec<Money>,
    /// Prior cycle cumulative, length-aligned (`None` unless comparing).
    #[serde(with = "opt_money_vec_centimes")]
    pub last_cycle_cumulative: Option<Vec<Money>>,
    /// 0-based index of `as_of`.
    pub today_index: usize,
}

/// The spend-over-time series (`GET /cycle/current/spend-series?compare=lastCycle`).
///
/// REAL: composes `phosk_insights::spend_series` with the prior-cycle comparison
/// on (the Dashboard always draws the "last cycle" trace).
#[server]
pub async fn get_spend_series() -> Result<SpendSeriesDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let s = phosk_insights::spend_series(session.db(), crate::data::today(), true)
            .await
            .map_err(crate::data::server_err)?;
        Ok(SpendSeriesDto {
            daily: s.daily,
            cumulative: s.cumulative,
            pace: s.pace,
            last_cycle_cumulative: s.last_cycle_cumulative,
            today_index: s.today_index,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

// ── top shops (REAL) ─────────────────────────────────────────────────────────

/// One shop's slice of the cycle (an element of [`TopShopsDto::shops`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopShareDto {
    /// Shop display name.
    pub shop: String,
    /// Total spent at this shop this cycle.
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
    /// `total / Σ totals`, 0–1.
    pub share: f64,
}

/// `GET /cycle/current/top-shops` — ranked shops + the bar-width denominator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopShopsDto {
    /// Ranked shops, highest total first.
    pub shops: Vec<ShopShareDto>,
    /// Largest shop total (bar-width denominator).
    #[serde(with = "phosk_model::money_centimes")]
    pub max_total: Money,
}

/// The cycle's top shops (`GET /cycle/current/top-shops`).
///
/// REAL: composes `phosk_insights::top_shops` (limit 8) for the seeded cycle.
#[server]
pub async fn get_top_shops() -> Result<TopShopsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let s = phosk_insights::top_shops(session.db(), crate::data::today(), 8)
            .await
            .map_err(crate::data::server_err)?;
        Ok(TopShopsDto {
            shops: s
                .shops
                .into_iter()
                .map(|sh| ShopShareDto {
                    shop: sh.shop,
                    total: sh.total,
                    share: sh.share,
                })
                .collect(),
            max_total: s.max_total,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

// ── recurring + alerts + insight (seeded) ────────────────────────────────────

/// One recurring charge surfaced on the dashboard (`GET /recurring` element).
///
/// `days_until` drives the hero "NEXT" dock sort; `next` is the due-date label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringDto {
    /// Stable id.
    pub id: String,
    /// Charge name, e.g. `"Spotify"`.
    pub name: String,
    /// Charge amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Next due date label, e.g. `"22 JUN"`.
    pub next: String,
    /// Whole days until the next charge (negative/0 = due/now).
    pub days_until: i32,
}

/// `GET /recurring` — the recurring charges + their monthly run-rate total.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringListDto {
    /// The recurring charges, soonest-due usable via `days_until`.
    pub recurring: Vec<RecurringDto>,
    /// Sum of monthly-equivalent charges.
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly_total: Money,
}

/// The dashboard recurring panel (`GET /recurring`).
///
/// REAL: composes `phosk_recurring::recurring_summary` for the seeded cycle.
#[server]
pub async fn get_recurring() -> Result<RecurringListDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let s =
            phosk_recurring::subscriptions::recurring_summary(session.db(), crate::data::today())
                .await
                .map_err(crate::data::server_err)?;
        Ok(RecurringListDto {
            recurring: s
                .recurring
                .into_iter()
                .map(|r| RecurringDto {
                    id: r.id,
                    name: r.name,
                    amount: r.amount,
                    next: r.next,
                    days_until: r.days_until,
                })
                .collect(),
            monthly_total: s.monthly_total,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// One action button on an [`AlertDto`]. Mirrors `phosk_planning::alerts::AlertActionDto`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertActionDto {
    /// Button label, e.g. `"VIEW"`, `"RAISE CAP"`, `"DISMISS"`, `"SNOOZE"`.
    pub label: String,
    /// The backend verb [`act_on_alert`] expects back for this button:
    /// `"navigate" | "dismiss" | "snooze" | "apply"`.
    pub kind: String,
}

/// One attention item (`GET /alerts` element). `tone` is `"alert"|"warn"|"info"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertDto {
    /// Stable id.
    pub id: String,
    /// Severity tone: `"alert"` (coral) / `"warn"` / `"info"`.
    pub tone: String,
    /// Short tag, e.g. `"BUDGET"`.
    pub tag: String,
    /// One-line headline.
    pub head: String,
    /// Supporting body line.
    pub body: String,
    /// Action buttons (first is the primary). Mirrors React `a.actions`
    /// (`comps.jsx` `AlertItem` renders one `<button class="btn[ p]">` each, e.g.
    /// VIEW / RAISE CAP / DISMISS / SNOOZE / MARK PAID); the page routes a press
    /// through [`act_on_alert`], sending the button's `kind`, never its label.
    pub actions: Vec<AlertActionDto>,
}

/// The dashboard alerts list (`GET /alerts`).
///
/// REAL: composes `phosk_planning::alerts` (persisted/rules-generated) for the
/// seeded cycle.
#[server]
pub async fn get_alerts() -> Result<Vec<AlertDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let alerts = phosk_planning::alerts::alerts(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(alerts
            .into_iter()
            .map(|a| AlertDto {
                id: a.id,
                tone: a.tone,
                tag: a.tag,
                head: a.head,
                body: a.body,
                actions: a
                    .actions
                    .into_iter()
                    .map(|act| AlertActionDto {
                        label: act.label,
                        kind: act.kind,
                    })
                    .collect(),
            })
            .collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// Act on a dashboard alert (`POST /alerts/{id}/{dismiss|snooze|apply}`).
///
/// `action` is the pressed button's `kind` (`AlertActionDto.kind`), never its
/// label: the server validates it against that alert's own actions and
/// rejects anything else. React's `AlertItem` POSTed each action label to the
/// dead REST layer and then re-fetched (`onChanged`); here that mutation is a
/// single seeded server fn that the page routes every non-navigation button
/// through (VIEW is handled client-side as a route push).
///
/// # Errors
/// [`ALERT_ACTION_FAILED`] on any rejection by the planning service (unknown
/// or dismissed alert, a kind this alert doesn't offer, an adapter failure),
/// at that [`phosk_core::error::PhoskError`]'s own HTTP status: its text is not
/// sent. A failure to build the session carries
/// [`crate::data::server_msg::UNAVAILABLE`] and a transport failure its own
/// message; the page shows [`ALERT_ACTION_FAILED`] for every error either way.
#[server]
pub async fn act_on_alert(id: String, action: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        act_on_alert_with(session.db(), &id, &action).await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = (id, action);
        Err(ServerFnError::new("server-only"))
    }
}

/// Shown for any [`act_on_alert`] rejection: a fixed, generic line so an
/// engine/store detail from `PhoskError` never reaches the page.
pub const ALERT_ACTION_FAILED: &str = "could not update this alert, try again";

/// [`act_on_alert`]'s logic over an explicit DB port (tests pass a fresh store).
/// `action` is the backend verb: `dismiss` | `snooze` | `apply`.
#[cfg(feature = "server-deps")]
pub(crate) async fn act_on_alert_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    id: &str,
    action: &str,
) -> Result<(), ServerFnError> {
    phosk_planning::alerts::act_on_alert(db, id, action, crate::data::today())
        .await
        .map_err(|e| ServerFnError::ServerError {
            message: ALERT_ACTION_FAILED.to_owned(),
            code: e.http_status(),
            details: None,
        })
}

/// `GET /insights/dashboard` — the GEMMA4 one-liner + estimated saving.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightDto {
    /// Model badge, e.g. `"GEMMA4"`.
    pub model: String,
    /// The insight sentence.
    pub text: String,
    /// Estimated CHF saving the suggestion unlocks.
    #[serde(with = "phosk_model::money_centimes")]
    pub estimated_savings: Money,
}

/// The dashboard AI insight (`GET /insights/dashboard`).
///
/// REAL: composes `phosk_ai::dashboard_insight` (canned GEMMA4 one-liner for now;
/// real local inference lands with the LLM adapter) for the seeded cycle.
#[server]
pub async fn get_insight() -> Result<InsightDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let i = phosk_ai::ai_features::dashboard_insight(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(InsightDto {
            model: i.model,
            text: i.text,
            estimated_savings: i.estimated_savings,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

// ── seed + serde helpers ──────────────────────────────────────────────────────

/// `serde` adapter for `Vec<Money>` round-tripping as a JSON array of i64 centimes.
pub(crate) mod money_vec_centimes {
    use phosk_core::money::Money;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serialize a `Vec<Money>` as an array of i64 centimes.
    pub fn serialize<S: Serializer>(v: &[Money], s: S) -> Result<S::Ok, S::Error> {
        let raw: Vec<i64> = v.iter().map(|m| m.centimes()).collect();
        raw.serialize(s)
    }

    /// Deserialize a `Vec<Money>` from an array of i64 centimes.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Money>, D::Error> {
        let raw = Vec::<i64>::deserialize(d)?;
        Ok(raw.into_iter().map(Money::from_centimes).collect())
    }
}

/// `serde` adapter for `Option<Vec<Money>>` (the spend-series comparison line).
pub(crate) mod opt_money_vec_centimes {
    use phosk_core::money::Money;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serialize `Option<Vec<Money>>` as `null` or an array of i64 centimes.
    #[allow(
        clippy::ref_option,
        reason = "serde serialize_with contract is fn(&T, S)"
    )]
    pub fn serialize<S: Serializer>(v: &Option<Vec<Money>>, s: S) -> Result<S::Ok, S::Error> {
        let raw: Option<Vec<i64>> = v.as_ref().map(|v| v.iter().map(|m| m.centimes()).collect());
        raw.serialize(s)
    }

    /// Deserialize `Option<Vec<Money>>` from `null` or an array of i64 centimes.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<Money>>, D::Error> {
        let raw = Option::<Vec<i64>>::deserialize(d)?;
        Ok(raw.map(|v| v.into_iter().map(Money::from_centimes).collect()))
    }
}
