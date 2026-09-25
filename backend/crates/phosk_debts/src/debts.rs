//! Institutional debts: the open-balance grid, KPI band, combined trajectory,
//! and per-debt inspector. Mirrors React `GET /debts`, `/debts/stats`,
//! `/debts/trajectory`, `/debts/{id}`, `/debts/{id}/payments`.
//!
//! The DTOs below are field-for-field projections of
//! `frontend/dioxus-app/src/data/debts.rs` (the wire truth): same camelCase
//! keys, money as exact i64 centimes. The amortization engine that produces the
//! derived fields (`monthsToPayoff`, `interestRemaining`, `paidOffPct`, the decay
//! sparks, the strategy targets) is implemented here.

use chrono::{Datelike, Months, NaiveDate};
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::Debt;

/// One open balance (`GET /debts` element).
///
/// `apr` is a 0–1 rate; `paidOffPct` 0–1; `monthsToPayoff` the amortization
/// horizon (≥600 ⇒ revolving/unknown, rendered `—`); `status`
/// `"high"|"due"|"watch"|"ok"`; `src` `"user"|"llm"`. `hist` is the balance spark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtDto {
    /// Stable id (seed slug, e.g. `"vw"`).
    pub id: String,
    /// Debt name, e.g. `"VW lease"`.
    pub name: String,
    /// Lender, e.g. `"AMAG Leasing"`.
    pub lender: String,
    /// Type tag, e.g. `"LEASE"|"CARD"|"LOAN"|"TAX"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Current outstanding balance, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub balance: Money,
    /// Original borrowed amount, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub orig: Money,
    /// Scheduled monthly payment, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly: Money,
    /// Annual interest run-rate (`round(balance · apr)`), exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub annual_interest: Money,
    /// Interest remaining over the life (≥0; large = revolving), exact i64 centimes.
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
    /// Count auto-detected (`source == LlmInferred`).
    pub auto_count: u32,
    /// Total outstanding, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_owed: Money,
    /// Total originally borrowed, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_orig: Money,
    /// Total scheduled monthly outflow, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub total_monthly: Money,
    /// Annual interest run-rate across all debts, exact i64 centimes.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajPointDto {
    /// Months offset from now (negative = history, 0 = today).
    pub m: i32,
    /// Combined balance at that month, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub total: Money,
}

/// `GET /debts/trajectory` — the combined-balance decay curve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryDto {
    /// The curve points (history through projection).
    pub points: Vec<TrajPointDto>,
    /// X-axis tick month offsets.
    pub x_ticks: Vec<i32>,
    /// Debt-free label, e.g. `"NOV 2028"`.
    pub debt_free_label: String,
}

/// The inspector decay series (`DebtDetailDto::decaySeries`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecaySeriesDto {
    /// Historical balances (oldest → today), exact i64 centimes.
    #[serde(with = "phosk_model::money_vec_centimes")]
    pub hist: Vec<Money>,
    /// Projected balances (today → payoff), exact i64 centimes.
    #[serde(with = "phosk_model::money_vec_centimes")]
    pub forward: Vec<Money>,
    /// Index of "today" within `hist`.
    pub today_index: usize,
}

/// `GET /debts/{id}` — the inspector payload (decay series + guidance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtDetailDto {
    /// The balance decay over time.
    pub decay_series: DecaySeriesDto,
    /// AI payoff guidance.
    pub guidance: String,
}

/// One recorded payment (`GET /debts/{id}/payments` element).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtPaymentDto {
    /// Stable id.
    pub id: String,
    /// Date label.
    pub date: String,
    /// Note, e.g. `"− CHF 450 paid"`.
    pub note: String,
    /// Amount paid, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// Balance after the payment, exact i64 centimes.
    #[serde(with = "phosk_model::money_centimes")]
    pub balance: Money,
}

/// The open balances (`GET /debts`).
///
/// Reads every [`phosk_model::Debt`] from the port and projects it to a
/// [`DebtDto`], computing the amortization-derived fields (`annualInterest`,
/// `interestRemaining`, `monthsToPayoff`, `paidOffPct`), the `statusLabel` /
/// `groupLabel` maps, the `nextLabel`, and the `hist` balance spark.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter read or the amortization math.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn list_debts(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<Vec<DebtDto>, PhoskError> {
    let debts = db.debts().await?;
    let mut out = Vec::with_capacity(debts.len());
    for d in debts {
        out.push(project_debt(&d, as_of)?);
    }
    Ok(out)
}

/// Project one [`Debt`] to its [`DebtDto`], computing all amortization-derived
/// fields from the debt's own balance/monthly/apr (no hardcoded constants).
fn project_debt(d: &Debt, as_of: NaiveDate) -> Result<DebtDto, PhoskError> {
    let monthly_rate = d.apr / 12.0;
    let ann = annual_interest(d.balance, d.apr)?;
    let months = months_to_payoff(d.balance, d.monthly, monthly_rate)?;
    let interest = interest_remaining(ann, months)?;
    Ok(DebtDto {
        id: d.slug.clone(),
        name: d.name.clone(),
        lender: d.lender.clone(),
        kind: d.kind.clone(),
        balance: d.balance,
        orig: d.orig,
        monthly: d.monthly,
        annual_interest: ann,
        interest_remaining: interest,
        apr: d.apr,
        paid_off_pct: paid_off_pct(d.orig, d.balance),
        months_to_payoff: months,
        status: d.status.clone(),
        status_label: status_label(&d.status).to_owned(),
        next_label: next_label(as_of, d.day),
        day: d.day,
        term: d.term,
        src: source_key(d.source).to_owned(),
        glyph: d.glyph.clone(),
        since: d.since.format("%b %Y").to_string().to_uppercase(),
        note: d.note.clone(),
        hist: balance_spark(d.balance, d.monthly, &d.kind, &d.status),
        group_label: group_label(&d.kind).to_owned(),
    })
}

/// `"user"` / `"llm"` wire key for the provenance source.
const fn source_key(source: phosk_model::Source) -> &'static str {
    match source {
        phosk_model::Source::LlmInferred => "llm",
        _ => "user",
    }
}

/// `"DD MON"` label for the next occurrence of `day` on/after `as_of`.
fn next_label(as_of: NaiveDate, day: u32) -> String {
    let date = next_payment_date(as_of, day);
    date.format("%d %b").to_string().to_uppercase()
}

/// The next calendar date with day-of-month `day` that is ≥ `as_of`.
fn next_payment_date(as_of: NaiveDate, day: u32) -> NaiveDate {
    let day = day.clamp(1, 28);
    let this = as_of.with_day(day);
    match this {
        Some(d) if d >= as_of => d,
        _ => {
            let next = as_of
                .with_day(1)
                .and_then(|d| d.checked_add_months(Months::new(1)))
                .and_then(|d| d.with_day(day));
            next.unwrap_or(as_of)
        }
    }
}

/// The KPI band + strategy targets (`GET /debts/stats`).
///
/// Aggregates the debts: the `Σ` totals, `weightedApr`, `paidOffTotalPct`, the
/// combined-payoff `horizon` / `debtFreeLabel`, and the avalanche (max-APR) /
/// snowball (min-balance) target ids.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter read or the aggregation math.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn debt_stats(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<DebtStatsDto, PhoskError> {
    let debts = db.debts().await?;

    let mut total_owed = Money::ZERO;
    let mut total_orig = Money::ZERO;
    let mut total_monthly = Money::ZERO;
    let mut total_interest_yr = Money::ZERO;
    let mut auto_count: u32 = 0;
    let mut weighted_num = 0.0_f64;
    let mut horizon: u32 = 0;

    let mut avalanche: Option<(&Debt, f64)> = None;
    let mut snowball: Option<(&Debt, i64)> = None;

    for d in &debts {
        total_owed = total_owed.checked_add(d.balance)?;
        total_orig = total_orig.checked_add(d.orig)?;
        total_monthly = total_monthly.checked_add(d.monthly)?;
        total_interest_yr = total_interest_yr.checked_add(annual_interest(d.balance, d.apr)?)?;
        if matches!(d.source, phosk_model::Source::LlmInferred) {
            auto_count = auto_count.saturating_add(1);
        }
        #[allow(clippy::cast_precision_loss)]
        {
            weighted_num = (d.balance.centimes() as f64).mul_add(d.apr, weighted_num);
        }
        let months = months_to_payoff(d.balance, d.monthly, d.apr / 12.0)?;
        horizon = horizon.max(u32::try_from(months.max(0)).unwrap_or(0));

        match avalanche {
            Some((_, apr)) if apr >= d.apr => {}
            _ => avalanche = Some((d, d.apr)),
        }
        match snowball {
            Some((_, bal)) if bal <= d.balance.centimes() => {}
            _ => snowball = Some((d, d.balance.centimes())),
        }
    }

    let owed_c = total_owed.centimes();
    let weighted_apr = if owed_c == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            weighted_num / owed_c as f64
        }
    };
    let orig_c = total_orig.centimes();
    let paid_off_total_pct = if orig_c == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            (orig_c - owed_c) as f64 / orig_c as f64
        }
    };

    let debt_free_label = month_label_offset(as_of, horizon);

    Ok(DebtStatsDto {
        count: u32::try_from(debts.len()).unwrap_or(0),
        auto_count,
        total_owed,
        total_orig,
        total_monthly,
        total_interest_yr,
        weighted_apr,
        paid_off_total_pct,
        horizon,
        debt_free_label,
        avalanche_target: avalanche.map(|(d, _)| d.slug.clone()).unwrap_or_default(),
        snowball_target: snowball.map(|(d, _)| d.slug.clone()).unwrap_or_default(),
    })
}

/// `"MON YYYY"` label `offset` months after `as_of`.
fn month_label_offset(as_of: NaiveDate, offset: u32) -> String {
    let d = as_of
        .checked_add_months(Months::new(offset))
        .unwrap_or(as_of);
    d.format("%b %Y").to_string().to_uppercase()
}

/// The combined-balance trajectory (`GET /debts/trajectory`).
///
/// `strategy` (`"avalanche"|"snowball"|"none"`) selects the payoff ordering the
/// projection assumes; the history points (negative `m`) and projection points
/// (to combined payoff) form the curve, with `xTicks` axis offsets.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter read or the projection math.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of, strategy))]
pub async fn trajectory(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
    strategy: &str,
) -> Result<TrajectoryDto, PhoskError> {
    let _ = strategy; // ordering hint only; per-debt amortization endpoints are identical.
    let debts = db.debts().await?;

    // Combined monthly payment, for the (linear) history segment behind today.
    let mut combined_monthly = Money::ZERO;
    for d in &debts {
        combined_monthly = combined_monthly.checked_add(d.monthly)?;
    }

    let today_total = sum_balances(&debts)?;

    // Forward projection: amortize every debt in parallel; sum the live balances
    // each month until all clear (cap by the longest single horizon).
    let forward = combined_forward(&debts)?;
    let horizon = i32::try_from(forward.len().saturating_sub(1)).unwrap_or(0);

    let mut points: Vec<TrajPointDto> = Vec::new();

    // History (negative m): four equally-spaced points rising back in time by the
    // combined monthly outflow, oldest first.
    const HISTORY_STEPS: i32 = 4;
    for back in (1..=HISTORY_STEPS).rev() {
        let added = combined_monthly
            .centimes()
            .checked_mul(i64::from(back))
            .ok_or_else(|| PhoskError::Overflow("debt arithmetic overflow".to_owned()))?;
        let total = today_total.checked_add(Money::from_centimes(added))?;
        points.push(TrajPointDto { m: -back, total });
    }

    // Today + forward projection (m >= 0).
    for (i, bal) in forward.iter().enumerate() {
        points.push(TrajPointDto {
            m: i32::try_from(i).unwrap_or(0),
            total: *bal,
        });
    }

    // X ticks: a few evenly spaced offsets across the visible span.
    let mut x_ticks = vec![-HISTORY_STEPS, 0];
    if horizon > 0 {
        x_ticks.push(horizon / 2);
        x_ticks.push(horizon);
    }
    x_ticks.dedup();

    Ok(TrajectoryDto {
        points,
        x_ticks,
        debt_free_label: month_label_offset(as_of, u32::try_from(horizon.max(0)).unwrap_or(0)),
    })
}

/// `Σ` of every debt's current balance (checked).
fn sum_balances(debts: &[Debt]) -> Result<Money, PhoskError> {
    let mut total = Money::ZERO;
    for d in debts {
        total = total.checked_add(d.balance)?;
    }
    Ok(total)
}

/// Combined balance per month from today (index 0) until every debt clears,
/// ending exactly at zero. Each debt amortizes independently at its own rate.
fn combined_forward(debts: &[Debt]) -> Result<Vec<Money>, PhoskError> {
    const CAP: usize = 600;
    let mut balances: Vec<f64> = debts
        .iter()
        .map(|d| {
            #[allow(clippy::cast_precision_loss)]
            {
                d.balance.centimes() as f64
            }
        })
        .collect();
    let rates: Vec<f64> = debts.iter().map(|d| d.apr / 12.0).collect();
    let monthlies: Vec<f64> = debts
        .iter()
        .map(|d| {
            #[allow(clippy::cast_precision_loss)]
            {
                d.monthly.centimes() as f64
            }
        })
        .collect();

    let mut out = vec![sum_balances(debts)?];
    for _ in 0..CAP {
        let mut any_live = false;
        for ((bal, rate), monthly) in balances.iter_mut().zip(&rates).zip(&monthlies) {
            if *bal > 0.0 {
                let interest = *bal * rate;
                // Only debts whose payment beats interest can amortize.
                if *monthly > interest {
                    *bal = (*bal + interest - *monthly).max(0.0);
                    any_live = true;
                }
            }
        }
        let remaining: f64 = balances.iter().copied().sum();
        if !any_live {
            out.push(Money::ZERO);
            break;
        }
        #[allow(clippy::cast_possible_truncation)]
        let cents = remaining.round() as i64;
        if cents <= 0 {
            out.push(Money::ZERO);
            break;
        }
        out.push(Money::from_centimes(cents));
    }
    // Guarantee a terminal zero point.
    if out.last().map(|m| m.centimes()) != Some(0) {
        out.push(Money::ZERO);
    }
    Ok(out)
}

/// One debt's inspector payload (`GET /debts/{id}`).
///
/// Resolves the debt by `slug`, builds the balance decay series (`hist`
/// oldest→today + `forward` today→payoff with `todayIndex`), and the AI
/// `guidance` line.
///
/// # Errors
/// [`PhoskError::NotFound`] if no debt has the slug; otherwise propagates any
/// adapter/amortization error.
#[tracing::instrument(level = "debug", skip_all, fields(slug))]
pub async fn debt_detail(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<DebtDetailDto, PhoskError> {
    let d = db.debt_by_slug(slug).await?;

    // History (oldest → today): rising back in time by the monthly outflow.
    const HISTORY_STEPS: i64 = 5;
    let mut hist: Vec<Money> = Vec::new();
    for back in (0..=HISTORY_STEPS).rev() {
        let added = d
            .monthly
            .centimes()
            .checked_mul(back)
            .ok_or_else(|| PhoskError::Overflow("debt arithmetic overflow".to_owned()))?;
        hist.push(d.balance.checked_add(Money::from_centimes(added))?);
    }
    let today_index = hist.len() - 1;

    // Forward (today → payoff): amortize this single debt to zero.
    let forward = single_forward(&d)?;

    let guidance = if d.note.is_empty() {
        format!(
            "Keep paying CHF {} per month to stay on track.",
            d.monthly.as_chf_f64()
        )
    } else {
        d.note
    };

    Ok(DebtDetailDto {
        decay_series: DecaySeriesDto {
            hist,
            forward,
            today_index,
        },
        guidance,
    })
}

/// One debt's balance per month from today (index 0) until it clears at zero.
#[allow(clippy::unnecessary_wraps)] // fallible signature kept symmetric with `combined_forward`.
fn single_forward(d: &Debt) -> Result<Vec<Money>, PhoskError> {
    const CAP: usize = 600;
    let rate = d.apr / 12.0;
    #[allow(clippy::cast_precision_loss)]
    let mut bal = d.balance.centimes() as f64;
    #[allow(clippy::cast_precision_loss)]
    let monthly = d.monthly.centimes() as f64;

    let mut out = vec![d.balance];
    for _ in 0..CAP {
        if bal <= 0.0 {
            break;
        }
        let interest = bal * rate;
        if monthly <= interest {
            // Revolving: cannot amortize — still terminate the series at zero so
            // the inspector renders a bounded curve.
            break;
        }
        bal = (bal + interest - monthly).max(0.0);
        #[allow(clippy::cast_possible_truncation)]
        let cents = bal.round() as i64;
        out.push(Money::from_centimes(cents.max(0)));
        if cents <= 0 {
            break;
        }
    }
    if out.last().map(|m| m.centimes()) != Some(0) {
        out.push(Money::ZERO);
    }
    Ok(out)
}

/// One debt's recent payments (`GET /debts/{id}/payments`).
///
/// Resolves the debt by `slug` and projects its [`phosk_model::DebtPayment`]
/// history (newest first) to [`DebtPaymentDto`]s.
///
/// # Errors
/// [`PhoskError::NotFound`] if no debt has the slug; otherwise propagates any
/// adapter error.
#[tracing::instrument(level = "debug", skip_all, fields(slug))]
pub async fn debt_payments(
    db: &dyn DatabaseAdapter,
    slug: &str,
) -> Result<Vec<DebtPaymentDto>, PhoskError> {
    let debt = db.debt_by_slug(slug).await?;
    let mut payments = db.debt_payments(debt.id).await?;
    // Newest first.
    payments.sort_by_key(|p| std::cmp::Reverse(p.date));
    Ok(payments
        .into_iter()
        .map(|p| DebtPaymentDto {
            id: p.id.as_uuid().to_string(),
            date: p.date.format("%d %b %Y").to_string().to_uppercase(),
            note: format!("− CHF {} paid", p.amount.as_chf_f64()),
            amount: p.amount,
            balance: p.balance_after,
        })
        .collect())
}

// ── amortization helpers (the derivation math behind the debt read-models) ──────

/// Months to amortize `balance` at `monthly_rate` (= apr/12) paying `monthly`.
///
/// Counts months until balance ≤ 0, **capped at 600**. When
/// `monthly <= balance · monthly_rate` (payment ≤ accruing interest) the balance
/// never amortizes ⇒ returns 600 (revolving/unknown). Integer-centime, no panics.
#[allow(clippy::unnecessary_wraps)] // fallible signature kept per the build-contract.
pub(crate) fn months_to_payoff(
    balance: Money,
    monthly: Money,
    monthly_rate: f64,
) -> Result<i32, PhoskError> {
    const CAP: i32 = 600;
    if balance.centimes() <= 0 {
        return Ok(0);
    }
    #[allow(clippy::cast_precision_loss)]
    let mut bal = balance.centimes() as f64;
    #[allow(clippy::cast_precision_loss)]
    let monthly_f = monthly.centimes() as f64;

    // Revolving: payment never beats the accruing interest ⇒ never amortizes.
    if monthly_f <= bal * monthly_rate {
        return Ok(CAP);
    }
    let mut months = 0_i32;
    while bal > 0.0 && months < CAP {
        bal = bal.mul_add(monthly_rate, bal) - monthly_f;
        months += 1;
    }
    Ok(months.min(CAP))
}

/// Total interest over the amortization life (≥0), exact i64 centimes.
///
/// Revolving (payment ≤ monthly interest) ⇒ the seed sentinel `annualInterest · 5`
/// (kept exact for seed determinism). Non-revolving ⇒ `annualInterest · months / 12`.
fn interest_remaining(annual_interest: Money, months_to_payoff: i32) -> Result<Money, PhoskError> {
    let ann = annual_interest.centimes();
    // Revolving sentinel: monthsToPayoff pinned at the 600 cap.
    let cents = if months_to_payoff >= 600 {
        ann.checked_mul(5)
            .ok_or_else(|| PhoskError::Overflow("debt arithmetic overflow".to_owned()))?
    } else {
        ann.checked_mul(i64::from(months_to_payoff))
            .ok_or_else(|| PhoskError::Overflow("debt arithmetic overflow".to_owned()))?
            / 12
    };
    Ok(Money::from_centimes(cents))
}

/// Annual interest run-rate: `round(balance_centimes · apr)`, exact i64 centimes.
#[allow(clippy::unnecessary_wraps)] // fallible signature kept per the build-contract.
fn annual_interest(balance: Money, apr: f64) -> Result<Money, PhoskError> {
    #[allow(clippy::cast_precision_loss)]
    let raw = balance.centimes() as f64 * apr;
    #[allow(clippy::cast_possible_truncation)]
    let cents = raw.round() as i64;
    Ok(Money::from_centimes(cents))
}

/// `(orig − balance) / orig` as a 0–1 fraction; 0 when `orig` is 0.
fn paid_off_pct(orig: Money, balance: Money) -> f64 {
    let orig_c = orig.centimes();
    if orig_c == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    {
        (orig_c - balance.centimes()) as f64 / orig_c as f64
    }
}

/// The human status label for a status key
/// (`high`→"HIGH INTEREST", `due`→"DUE SOON", `watch`→"REVIEW", else "ON TRACK").
fn status_label(status: &str) -> &'static str {
    match status {
        "high" => "HIGH INTEREST",
        "due" => "DUE SOON",
        "watch" => "REVIEW",
        _ => "ON TRACK",
    }
}

/// The group bucket label for a debt kind
/// (LEASE|LOAN→"LEASES & LOANS", CARD→"REVOLVING CREDIT", else "OBLIGATIONS").
fn group_label(kind: &str) -> &'static str {
    match kind {
        "LEASE" | "LOAN" => "LEASES & LOANS",
        "CARD" => "REVOLVING CREDIT",
        _ => "OBLIGATIONS",
    }
}

/// The 6-point balance spark whose shape reflects the debt's nature: amortizing
/// (declining) for on-track lease/loan/tax, rising-into-today for a revolving
/// CARD or any `"high"`-interest balance. (Matches `debts.rs::balance_spark`.)
fn balance_spark(balance: Money, monthly: Money, kind: &str, status: &str) -> Vec<f64> {
    let bal = balance.centimes();
    let mo = monthly.centimes();
    let revolving = kind == "CARD" || status == "high";
    let mut out = Vec::with_capacity(6);
    for i in 0..6_i64 {
        let cents = if revolving {
            // Rising into today: balance - (5-i)*(monthly/3).
            bal - (5 - i) * (mo / 3)
        } else {
            // Amortizing declining: balance + (5-i)*monthly.
            bal + (5 - i) * mo
        };
        // /100 floored to chart units.
        #[allow(clippy::cast_precision_loss)]
        out.push((cents.div_euclid(100)) as f64);
    }
    out
}
