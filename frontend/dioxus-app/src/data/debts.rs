//! Debts + personal-IOU read-model (F3).
//!
//! Backs the Debts page: the payoff-trajectory hero, KPI band, debt card/row
//! grid with payoff meters, the right-dock inspector (decay series + payments),
//! and the personal-IOU net-position beam. Mirrors React `GET /debts`,
//! `/debts/stats`, `/debts/trajectory`, `/debts/{id}`, `/debts/{id}/payments`,
//! `/personal-ious`, `/personal-ious/stats`.

use dioxus::prelude::*;
use phosk_core::money::Money;
use serde::{Deserialize, Serialize};

/// One open balance (`GET /debts` element).
///
/// `apr` is a 0–1 rate; `paid_off_pct` 0–1; `months_to_payoff` the amortization
/// horizon (≥600 ⇒ revolving/unknown, rendered `—`); `status` `"high"|"due"|
/// "watch"|"ok"`; `src` `"user"|"llm"`. `hist` is the balance spark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtDto {
    /// Stable id.
    pub id: String,
    /// Debt name, e.g. `"VW lease"`.
    pub name: String,
    /// Lender, e.g. `"AMAG Leasing"`.
    pub lender: String,
    /// Type tag, e.g. `"LEASE"|"CARD"|"LOAN"|"TAX"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Current outstanding balance.
    #[serde(with = "phosk_model::money_centimes")]
    pub balance: Money,
    /// Original borrowed amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub orig: Money,
    /// Scheduled monthly payment.
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly: Money,
    /// Annual interest run-rate.
    #[serde(with = "phosk_model::money_centimes")]
    pub annual_interest: Money,
    /// Interest remaining over the life (≥0; large = revolving).
    #[serde(with = "phosk_model::money_centimes")]
    pub interest_remaining: Money,
    /// APR as a 0–1 rate.
    pub apr: f64,
    /// Fraction paid off, 0–1.
    pub paid_off_pct: f64,
    /// Months to payoff (≥600 = revolving/unknown).
    pub months_to_payoff: i32,
    /// Status key.
    pub status: String,
    /// Human status label.
    pub status_label: String,
    /// Next-payment label, e.g. `"01 JUL"`.
    pub next_label: String,
    /// Payment day-of-month.
    pub day: u32,
    /// Term in months (0 = revolving).
    pub term: u32,
    /// `"user"` or `"llm"`.
    pub src: String,
    /// Card glyph.
    pub glyph: String,
    /// Tracking-since label.
    pub since: String,
    /// One-line AI note.
    pub note: String,
    /// Balance spark (raw chart numbers).
    pub hist: Vec<f64>,
    /// Group bucket label (e.g. `"LEASES & LOANS"`).
    pub group_label: String,
}

/// `GET /debts/stats` — the KPI band + strategy targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtStatsDto {
    /// Open-balance count.
    pub count: u32,
    /// Count auto-detected.
    pub auto_count: u32,
    /// Total outstanding.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_owed: Money,
    /// Total originally borrowed.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_orig: Money,
    /// Total scheduled monthly outflow.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_monthly: Money,
    /// Annual interest run-rate across all debts.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_interest_yr: Money,
    /// Balance-weighted APR, 0–1.
    pub weighted_apr: f64,
    /// Fraction of total borrowed that's been repaid, 0–1.
    pub paid_off_total_pct: f64,
    /// Months to debt-free at the current pace.
    pub horizon: u32,
    /// Projected debt-free label, e.g. `"NOV 2028"`.
    pub debt_free_label: String,
    /// Avalanche target debt id (highest APR).
    pub avalanche_target: String,
    /// Snowball target debt id (smallest balance).
    pub snowball_target: String,
}

/// One point on the combined-balance trajectory (`m` months from now, `total`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajPointDto {
    /// Months offset from now (negative = history, 0 = today).
    pub m: i32,
    /// Combined balance at that month.
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
}

/// `GET /debts/trajectory` — the combined-balance decay curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryDto {
    /// The curve points (history through projection).
    pub points: Vec<TrajPointDto>,
    /// X-axis tick month offsets.
    pub x_ticks: Vec<i32>,
    /// Debt-free label, e.g. `"NOV 2028"`.
    pub debt_free_label: String,
}

/// The inspector decay series (`DebtDetailDto::decay_series`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecaySeriesDto {
    /// Historical balances (oldest → today).
    #[serde(with = "crate::data::dashboard::money_vec_centimes")]
    pub hist: Vec<Money>,
    /// Projected balances (today → payoff).
    #[serde(with = "crate::data::dashboard::money_vec_centimes")]
    pub forward: Vec<Money>,
    /// Index of "today" within `hist`.
    pub today_index: usize,
}

/// `GET /debts/{id}` — the inspector payload (decay series + guidance).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtDetailDto {
    /// The balance decay over time.
    pub decay_series: DecaySeriesDto,
    /// AI payoff guidance.
    pub guidance: String,
}

/// One recorded payment (`GET /debts/{id}/payments` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtPaymentDto {
    /// Stable id.
    pub id: String,
    /// Date label.
    pub date: String,
    /// Note, e.g. `"− CHF 450 paid"`.
    pub note: String,
    /// Amount paid.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Balance after the payment.
    #[serde(with = "phosk_model::money_centimes")]
    pub balance: Money,
}

/// One personal IOU (`GET /personal-ious` element).
///
/// `dir` is `"in"` (owed to you) or `"out"` (you owe); `repaid_pct` 0–1; `of` the
/// original amount (current `amount` is what's left).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalIouDto {
    /// Stable id.
    pub id: String,
    /// `"in"` or `"out"`.
    pub dir: String,
    /// Person's name.
    pub person: String,
    /// Initials for the avatar.
    pub initials: String,
    /// Amount still outstanding.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Original amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub of: Money,
    /// Reason note.
    pub reason: String,
    /// Since label.
    pub since: String,
    /// Fraction repaid, 0–1.
    pub repaid_pct: f64,
}

/// `GET /personal-ious/stats` — the net-position beam figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IouStatsDto {
    /// Total owed to you.
    #[serde(with = "phosk_model::money_centimes")]
    pub owed_to_you: Money,
    /// Total you owe.
    #[serde(with = "phosk_model::money_centimes")]
    pub you_owe: Money,
    /// Net position (owed − owe).
    #[serde(with = "phosk_model::money_centimes")]
    pub net: Money,
    /// Inbound IOU count.
    pub count_in: u32,
    /// Outbound IOU count.
    pub count_out: u32,
}

/// The open balances (`GET /debts`).
///
/// REAL: composes `phosk_debts::debts::list_debts` for the seeded cycle.
#[server]
pub async fn list_debts() -> Result<Vec<DebtDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let debts = phosk_debts::debts::list_debts(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(debts.into_iter().map(map_debt).collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The KPI band + strategy targets (`GET /debts/stats`).
///
/// REAL: composes `phosk_debts::debts::debt_stats`.
#[server]
pub async fn get_debt_stats() -> Result<DebtStatsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let s = phosk_debts::debts::debt_stats(session.db(), crate::data::today())
            .await
            .map_err(crate::data::server_err)?;
        Ok(DebtStatsDto {
            count: s.count,
            auto_count: s.auto_count,
            total_owed: s.total_owed,
            total_orig: s.total_orig,
            total_monthly: s.total_monthly,
            total_interest_yr: s.total_interest_yr,
            weighted_apr: s.weighted_apr,
            paid_off_total_pct: s.paid_off_total_pct,
            horizon: s.horizon,
            debt_free_label: s.debt_free_label,
            avalanche_target: s.avalanche_target,
            snowball_target: s.snowball_target,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The combined-balance trajectory (`GET /debts/trajectory`).
///
/// REAL: composes `phosk_debts::debts::trajectory` (the `strategy`
/// `avalanche|snowball|none` is an ordering hint forwarded to the service).
#[server]
pub async fn get_trajectory(strategy: String) -> Result<TrajectoryDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let t = phosk_debts::debts::trajectory(session.db(), crate::data::today(), &strategy)
            .await
            .map_err(crate::data::server_err)?;
        Ok(TrajectoryDto {
            points: t
                .points
                .into_iter()
                .map(|p| TrajPointDto {
                    m: p.m,
                    total: p.total,
                })
                .collect(),
            x_ticks: t.x_ticks,
            debt_free_label: t.debt_free_label,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = strategy;
        Err(ServerFnError::new("server-only"))
    }
}

/// One debt's inspector payload (`GET /debts/{id}`).
///
/// REAL: composes `phosk_debts::debts::debt_detail` for the slug.
#[server]
pub async fn get_debt(id: String) -> Result<DebtDetailDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let d = phosk_debts::debts::debt_detail(session.db(), &id)
            .await
            .map_err(crate::data::server_err)?;
        Ok(DebtDetailDto {
            decay_series: DecaySeriesDto {
                hist: d.decay_series.hist,
                forward: d.decay_series.forward,
                today_index: d.decay_series.today_index,
            },
            guidance: d.guidance,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// One debt's recent payments (`GET /debts/{id}/payments`).
///
/// REAL: composes `phosk_debts::debts::debt_payments` for the slug.
#[server]
pub async fn get_debt_payments(id: String) -> Result<Vec<DebtPaymentDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let ps = phosk_debts::debts::debt_payments(session.db(), &id)
            .await
            .map_err(crate::data::server_err)?;
        Ok(ps
            .into_iter()
            .map(|p| DebtPaymentDto {
                id: p.id,
                date: p.date,
                note: p.note,
                amount: p.amount,
                balance: p.balance,
            })
            .collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = id;
        Err(ServerFnError::new("server-only"))
    }
}

/// The personal IOUs (`GET /personal-ious`).
///
/// REAL: composes `phosk_debts::personal_ious::list_personal_ious`.
#[server]
pub async fn list_personal_ious() -> Result<Vec<PersonalIouDto>, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let ious = phosk_debts::personal_ious::list_personal_ious(session.db())
            .await
            .map_err(crate::data::server_err)?;
        Ok(ious
            .into_iter()
            .map(|p| PersonalIouDto {
                id: p.id,
                dir: p.dir,
                person: p.person,
                initials: p.initials,
                amount: p.amount,
                of: p.of,
                reason: p.reason,
                since: p.since,
                repaid_pct: p.repaid_pct,
            })
            .collect())
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

/// The IOU net-position figures (`GET /personal-ious/stats`).
///
/// REAL: composes `phosk_debts::personal_ious::iou_stats`.
#[server]
pub async fn get_iou_stats() -> Result<IouStatsDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let session = crate::data::build_session().await?;
        let s = phosk_debts::personal_ious::iou_stats(session.db())
            .await
            .map_err(crate::data::server_err)?;
        Ok(IouStatsDto {
            owed_to_you: s.owed_to_you,
            you_owe: s.you_owe,
            net: s.net,
            count_in: s.count_in,
            count_out: s.count_out,
        })
    }
    #[cfg(not(feature = "server-deps"))]
    {
        Err(ServerFnError::new("server-only"))
    }
}

// ── mappers (service DTO → wire DTO) ───────────────────────────────────────────

/// Map a `phosk_debts` open balance onto the wire [`DebtDto`].
#[cfg(feature = "server-deps")]
fn map_debt(d: phosk_debts::debts::DebtDto) -> DebtDto {
    DebtDto {
        id: d.id,
        name: d.name,
        lender: d.lender,
        kind: d.kind,
        balance: d.balance,
        orig: d.orig,
        monthly: d.monthly,
        annual_interest: d.annual_interest,
        interest_remaining: d.interest_remaining,
        apr: d.apr,
        paid_off_pct: d.paid_off_pct,
        months_to_payoff: d.months_to_payoff,
        status: d.status,
        status_label: d.status_label,
        next_label: d.next_label,
        day: d.day,
        term: d.term,
        src: d.src,
        glyph: d.glyph,
        since: d.since,
        note: d.note,
        hist: d.hist,
        group_label: d.group_label,
    }
}
