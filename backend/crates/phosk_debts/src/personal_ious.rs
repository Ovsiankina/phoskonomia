//! Personal IOUs: the informal "owe / owed" list and its net-position beam.
//! Mirrors React `GET /personal-ious`, `/personal-ious/stats`.
//!
//! The DTOs are field-for-field projections of the `PersonalIou*` structs in
//! `frontend/dioxus-app/src/data/debts.rs`: same camelCase keys, money as exact
//! i64 centimes. The derivations (`repaidPct`, the net-position sums) compute
//! from the port rows — no hardcoded constants.

use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;

/// One personal IOU (`GET /personal-ious` element).
///
/// `dir` is `"in"` (owed to you) or `"out"` (you owe); `repaidPct` 0–1; `of` the
/// original amount (current `amount` is what's left).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalIouDto {
    /// Stable id (seed slug, e.g. `"i1"`).
    pub id: String,
    /// `"in"` or `"out"`.
    pub dir: String,
    /// Person's name.
    pub person: String,
    /// Initials for the avatar.
    pub initials: String,
    /// Amount still outstanding, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Original amount, exact i64 centimes.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IouStatsDto {
    /// Total owed to you (`Σ` dir=="in"), exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub owed_to_you: Money,
    /// Total you owe (`Σ` dir=="out"), exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub you_owe: Money,
    /// Net position (`owedToYou − youOwe`), exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub net: Money,
    /// Inbound IOU count.
    pub count_in: u32,
    /// Outbound IOU count.
    pub count_out: u32,
}

/// The personal IOUs (`GET /personal-ious`).
///
/// Reads every [`phosk_model::PersonalIou`] from the port and projects it,
/// computing `repaidPct = (of − amount) / of`.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter read.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn list_personal_ious(
    db: &dyn DatabaseAdapter,
) -> Result<Vec<PersonalIouDto>, PhoskError> {
    let ious = db.personal_ious().await?;
    Ok(ious
        .into_iter()
        .map(|i| PersonalIouDto {
            id: i.slug,
            dir: i.dir,
            person: i.person,
            initials: i.initials,
            repaid_pct: repaid_pct(i.of, i.amount),
            amount: i.amount,
            of: i.of,
            reason: i.reason,
            since: i.since.format("%d %b %Y").to_string().to_uppercase(),
        })
        .collect())
}

/// The IOU net-position figures (`GET /personal-ious/stats`).
///
/// `owedToYou = Σ` dir=="in"; `youOwe = Σ` dir=="out"; `net = owedToYou − youOwe`;
/// plus the inbound/outbound counts.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter read or the checked sums.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn iou_stats(db: &dyn DatabaseAdapter) -> Result<IouStatsDto, PhoskError> {
    let ious = db.personal_ious().await?;

    let mut owed_to_you = Money::ZERO;
    let mut you_owe = Money::ZERO;
    let mut count_in: u32 = 0;
    let mut count_out: u32 = 0;

    for i in &ious {
        if i.dir == "in" {
            owed_to_you = owed_to_you.checked_add(i.amount)?;
            count_in = count_in.saturating_add(1);
        } else if i.dir == "out" {
            you_owe = you_owe.checked_add(i.amount)?;
            count_out = count_out.saturating_add(1);
        }
    }

    let net = owed_to_you.checked_sub(you_owe)?;

    Ok(IouStatsDto {
        owed_to_you,
        you_owe,
        net,
        count_in,
        count_out,
    })
}

/// `(of − amount) / of` as a 0–1 fraction; 0 when `of` is 0.
fn repaid_pct(of: Money, amount: Money) -> f64 {
    let of_c = of.centimes();
    if of_c == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    {
        (of_c - amount.centimes()) as f64 / of_c as f64
    }
}
