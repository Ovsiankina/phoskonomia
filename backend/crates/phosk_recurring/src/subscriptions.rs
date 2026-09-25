//! Subscriptions read-model (F3) — service over the `DatabaseAdapter` PORT.
//!
//! Backs the Subscriptions page (billing-sweep impulse train, KPI band, the
//! card/row grid, the right-dock inspector) and the dashboard recurring panel.
//! DTO shapes mirror `frontend/dioxus-app/src/data/subscriptions.rs` and the
//! dashboard `RecurringDto`/`RecurringListDto` field-for-field (camelCase keys,
//! money as exact i64 centimes).

use chrono::{Datelike, NaiveDate};
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::{Charge, Subscription};
use serde::{Deserialize, Serialize};

/// Upper-case three-letter month abbreviations, 1-indexed via `MONTHS[m - 1]`.
const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// Days in a given (year, 1-based month), accounting for leap years.
const fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `(year, month)` advanced by one month (wrapping December → January).
const fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month >= 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

/// `(year, month)` moved back one month (wrapping January → December).
const fn prev_month(year: i32, month: u32) -> (i32, u32) {
    if month <= 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

/// The 1-based month number for an upper-case abbreviation (`"FEB"` ⇒ 2).
pub(crate) fn month_from_abbr(abbr: &str) -> Option<u32> {
    MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(abbr))
        .map(|i| u32::try_from(i).unwrap_or(0) + 1)
}

/// A valid date for `(year, month, day)`, clamping `day` to the month length.
fn clamped_date(year: i32, month: u32, day: u32) -> Result<NaiveDate, PhoskError> {
    let last = days_in_month(year, month);
    let d = day.clamp(1, last);
    NaiveDate::from_ymd_opt(year, month, d)
        .ok_or_else(|| PhoskError::InvalidDate(format!("{year}-{month}-{d}")))
}

/// The next charge date on or after `as_of` for a subscription's cadence.
fn next_charge_date(
    as_of: NaiveDate,
    day: u32,
    cadence: &str,
    month: &str,
) -> Result<NaiveDate, PhoskError> {
    if cadence == "yearly" {
        let m = month_from_abbr(month).unwrap_or(1);
        // Yearly charges land on the 1st of the labelled month.
        let candidate = clamped_date(as_of.year(), m, 1)?;
        if candidate >= as_of {
            return Ok(candidate);
        }
        return clamped_date(as_of.year() + 1, m, 1);
    }
    // Monthly: the next occurrence of day-of-month `day` on/after `as_of`.
    let this = clamped_date(as_of.year(), as_of.month(), day)?;
    if this >= as_of {
        return Ok(this);
    }
    let (ny, nm) = next_month(as_of.year(), as_of.month());
    clamped_date(ny, nm, day)
}

/// The billing day of the cycle in progress: the last charge date on or before
/// `as_of`.
pub(crate) fn last_charge_date(
    as_of: NaiveDate,
    day: u32,
    cadence: &str,
    month: &str,
) -> Result<NaiveDate, PhoskError> {
    if cadence == "yearly" {
        // Precondition: `month` is a validated abbreviation (the write path
        // rejects anything else); January is a last-resort fallback only.
        let m = month_from_abbr(month).unwrap_or(1);
        // Yearly charges land on the 1st of the labelled month.
        let candidate = clamped_date(as_of.year(), m, 1)?;
        if candidate <= as_of {
            return Ok(candidate);
        }
        return clamped_date(as_of.year() - 1, m, 1);
    }
    // Monthly: the last occurrence of day-of-month `day` on/before `as_of`.
    let this = clamped_date(as_of.year(), as_of.month(), day)?;
    if this <= as_of {
        return Ok(this);
    }
    let (py, pm) = prev_month(as_of.year(), as_of.month());
    clamped_date(py, pm, day)
}

// ── DTOs ────────────────────────────────────────────────────────────────────

/// One standing charge (the `list_subscriptions` element).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionDto {
    /// Stable id (the seed slug, e.g. `"netflix"`).
    pub id: String,
    /// Service name, e.g. `"Netflix"`.
    pub name: String,
    /// Per-charge amount.
    #[serde(with = "phosk_model::money_centimes")]
    pub amount: Money,
    /// `"monthly"` or `"yearly"`.
    pub cadence: String,
    /// Status key: `"ok"|"soon"|"due"|"watch"|"paused"`.
    pub status: String,
    /// Human status label, e.g. `"DUE SOON"`.
    pub status_label: String,
    /// Monthly-equivalent run-rate.
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly_equiv: Money,
    /// Annualized total.
    #[serde(with = "phosk_model::money_centimes")]
    pub annual: Money,
    /// Whole days until the next charge (≤0 = due).
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

/// The KPI band roll-ups (`subscription_stats`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubStatsDto {
    /// Active standing-charge count.
    pub count: u32,
    /// Monthly run-rate (Σ monthlyEquiv).
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly: Money,
    /// Annualized total (monthly * 12).
    #[serde(with = "phosk_model::money_centimes")]
    pub annual: Money,
    /// Count auto-detected by the AI (`source == llm`).
    pub auto_count: u32,
    /// Next-30-days roll-up.
    pub next30: Next30Dto,
    /// Needs-attention roll-up.
    pub flagged: FlaggedDto,
}

/// The "next 30 days" KPI roll-up (`SubStatsDto::next30`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Next30ItemDto {
    /// Service name.
    pub name: String,
    /// Days until it charges.
    pub days_until: i32,
}

/// The "needs attention" KPI roll-up (`SubStatsDto::flagged`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlaggedDto {
    /// Number flagged for review (subs in `watch`/`due`).
    pub count: u32,
    /// Supporting note.
    pub note: String,
}

/// One impulse on the billing sweep (`BillingSweepDto::impulses` element).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepFooterDto {
    /// Paid so far this cycle (Σ paid impulse amounts).
    #[serde(with = "phosk_model::money_centimes")]
    pub paid_this_cycle: Money,
    /// Still due this cycle (Σ non-paid impulse amounts).
    #[serde(with = "phosk_model::money_centimes")]
    pub still_due: Money,
    /// The next upcoming charge.
    pub next: SweepNextDto,
    /// A short footer note.
    pub note: String,
}

/// The periodic impulse train (`billing_sweep`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubGuidanceDto {
    /// Guidance text.
    pub text: String,
    /// Severity, e.g. `"coral"` or empty.
    pub severity: String,
}

/// The inspector payload (`subscription_detail`): the list record + extras.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionDetailDto {
    /// The headline subscription record.
    #[serde(flatten)]
    pub subscription: SubscriptionDto,
    /// Recent recorded charges (newest first).
    pub recent: Vec<SubChargeDto>,
    /// AI guidance.
    pub guidance: SubGuidanceDto,
    /// `true` for an OPEN AI candidate, not yet dismissed or confirmed
    /// (CONFIRM/DISMISS instead of cancel).
    pub candidate: bool,
}

/// Subscription list options for `list_subscriptions`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubFilter {
    /// Sort key: `due|amount|name`.
    pub sort: String,
    /// `"cadence"` to group, else empty.
    pub group: String,
    /// `monthly|annual` amount display mode (display only here).
    pub amounts: String,
}

/// One recurring charge surfaced on the dashboard (`RecurringListDto` element).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// The dashboard recurring panel (`recurring_summary`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringListDto {
    /// The recurring charges, soonest-due usable via `days_until`.
    pub recurring: Vec<RecurringDto>,
    /// Sum of monthly-equivalent charges.
    #[serde(with = "phosk_model::money_centimes")]
    pub monthly_total: Money,
}

// ── Services ──────────────────────────────────────────────────────────────────

/// The standing charges, honouring the `filter` sort/group.
///
/// # Errors
/// Propagates any [`PhoskError`] from the port or the derivations.
#[tracing::instrument(skip_all, fields(as_of = %as_of))]
pub async fn list_subscriptions(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
    filter: SubFilter,
) -> Result<Vec<SubscriptionDto>, PhoskError> {
    let subs = db.subscriptions().await?;
    let mut out = Vec::with_capacity(subs.len());
    for sub in &subs {
        let charges = db.subscription_charges(sub.id).await?;
        out.push(build_dto(sub, &charges, as_of)?);
    }
    match filter.sort.as_str() {
        "due" => out.sort_by_key(|s| s.days_until),
        "amount" => out.sort_by_key(|s| std::cmp::Reverse(s.monthly_equiv.centimes())),
        "name" => out.sort_by(|a, b| a.name.cmp(&b.name)),
        _ => {}
    }
    Ok(out)
}

/// The KPI band roll-ups.
///
/// # Errors
/// Propagates any [`PhoskError`] from the port or the derivations.
#[tracing::instrument(skip_all, fields(as_of = %as_of))]
pub async fn subscription_stats(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<SubStatsDto, PhoskError> {
    let all = list_subscriptions(db, as_of, SubFilter::default()).await?;
    // A paused or cancelled charge stays on the page but costs nothing, so it
    // is out of every roll-up (ADR: the KPI band counts ACTIVE charges).
    let subs: Vec<SubscriptionDto> = all
        .into_iter()
        .filter(|s| crate::lifecycle::is_active(&s.status))
        .collect();
    let count = u32::try_from(subs.len()).unwrap_or(u32::MAX);
    let monthly = Money::sum(subs.iter().map(|s| s.monthly_equiv))?;
    let annual_total = Money::from_centimes(
        monthly
            .centimes()
            .checked_mul(12)
            .ok_or_else(|| PhoskError::Overflow("stats annual".to_owned()))?,
    );
    let auto_count =
        u32::try_from(subs.iter().filter(|s| s.source == "llm").count()).unwrap_or(u32::MAX);

    // next30: monthly charges due within the next 30 days, soonest first.
    let mut upcoming: Vec<&SubscriptionDto> = subs
        .iter()
        .filter(|s| s.cadence == "monthly" && (0..=30).contains(&s.days_until))
        .collect();
    upcoming.sort_by_key(|s| s.days_until);
    let next30 = Next30Dto {
        count: u32::try_from(upcoming.len()).unwrap_or(u32::MAX),
        total: Money::sum(upcoming.iter().map(|s| s.amount))?,
        items: upcoming
            .iter()
            .map(|s| Next30ItemDto {
                name: s.name.clone(),
                days_until: s.days_until,
            })
            .collect(),
    };

    let flagged_count = u32::try_from(
        subs.iter()
            .filter(|s| s.status == "watch" || s.status == "due")
            .count(),
    )
    .unwrap_or(u32::MAX);
    let flagged = FlaggedDto {
        count: flagged_count,
        note: if flagged_count == 0 {
            "All charges look healthy.".to_owned()
        } else {
            "Review flagged charges on the Subs page.".to_owned()
        },
    };

    Ok(SubStatsDto {
        count,
        monthly,
        annual: annual_total,
        auto_count,
        next30,
        flagged,
    })
}

/// The billing-sweep impulse train + footer.
///
/// # Errors
/// Propagates any [`PhoskError`] from the port or the derivations.
#[tracing::instrument(skip_all, fields(as_of = %as_of))]
pub async fn billing_sweep(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<BillingSweepDto, PhoskError> {
    let day_index = as_of.day();
    let cycle_days = days_in_month(as_of.year(), as_of.month());
    let as_of_label = {
        let abbr = MONTHS
            .get((as_of.month() as usize).saturating_sub(1))
            .copied()
            .unwrap_or("");
        format!("{day_index:02} {abbr}")
    };

    let subs = db.subscriptions().await?;
    let mut impulses = Vec::new();
    let mut paid_total = Money::ZERO;
    let mut due_total = Money::ZERO;
    for sub in &subs {
        // Paused/cancelled charges do not bill, so they emit no impulse.
        if sub.cadence != "monthly" || !crate::lifecycle::is_active(&sub.status) {
            continue;
        }
        let paid = sub.day < day_index;
        let status = if paid {
            "paid".to_owned()
        } else {
            sub.status.clone()
        };
        if paid {
            paid_total = paid_total.checked_add(sub.amount)?;
        } else {
            due_total = due_total.checked_add(sub.amount)?;
        }
        impulses.push(ImpulseDto {
            id: sub.slug.clone(),
            name: sub.name.clone(),
            amount: sub.amount,
            day: sub.day,
            status,
        });
    }
    impulses.sort_by_key(|i| i.day);

    // The next upcoming (not-yet-paid) charge, soonest first.
    let mut upcoming: Vec<&Subscription> = subs
        .iter()
        .filter(|s| {
            s.cadence == "monthly" && s.day >= day_index && crate::lifecycle::is_active(&s.status)
        })
        .collect();
    upcoming.sort_by_key(|s| s.day);
    let next = match upcoming.first() {
        Some(s) => SweepNextDto {
            name: s.name.clone(),
            next_label: next_label(as_of, s.day, &s.cadence, &s.month)?,
            amount: s.amount,
        },
        None => SweepNextDto {
            name: String::new(),
            next_label: String::new(),
            amount: Money::ZERO,
        },
    };

    Ok(BillingSweepDto {
        cycle: SweepCycleDto {
            day: day_index,
            days: cycle_days,
            as_of: as_of_label,
        },
        impulses,
        footer: SweepFooterDto {
            paid_this_cycle: paid_total,
            still_due: due_total,
            next,
            note: "Paid charges are settled for this cycle.".to_owned(),
        },
    })
}

/// One subscription's inspector payload.
///
/// # Errors
/// Returns [`PhoskError::NotFound`] if `slug` resolves to no subscription;
/// otherwise propagates any port/derivation error.
#[tracing::instrument(skip_all, fields(as_of = %as_of))]
pub async fn subscription_detail(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
    slug: &str,
) -> Result<SubscriptionDetailDto, PhoskError> {
    let sub = db.subscription_by_slug(slug).await?;
    let charges = db.subscription_charges(sub.id).await?;
    let dto = build_dto(&sub, &charges, as_of)?;

    let mut recent: Vec<&Charge> = charges.iter().collect();
    recent.sort_by_key(|c| std::cmp::Reverse(c.date));
    let recent: Vec<SubChargeDto> = recent
        .iter()
        .map(|c| SubChargeDto {
            id: c.id.as_uuid().to_string(),
            date: c.date.to_string(),
            note: c.note.clone(),
            amount: c.amount,
        })
        .collect();

    let guidance = match sub.status.as_str() {
        "watch" => SubGuidanceDto {
            text: "Flagged for review — usage looks low for the price.".to_owned(),
            severity: "coral".to_owned(),
        },
        "due" => SubGuidanceDto {
            text: "This charge has not been seen this cycle.".to_owned(),
            severity: "coral".to_owned(),
        },
        _ => SubGuidanceDto {
            text: "On track — no action needed.".to_owned(),
            severity: String::new(),
        },
    };

    // Reuses the detector's own openness test so this flag and the
    // detection feed (`recurring_detect::detect`) can never disagree about
    // which records are still-open candidates.
    let candidate = crate::recurring_detect::is_open_candidate(&sub);

    Ok(SubscriptionDetailDto {
        subscription: dto,
        recent,
        guidance,
        candidate,
    })
}

/// The dashboard recurring panel.
///
/// # Errors
/// Propagates any [`PhoskError`] from the port or the derivations.
#[tracing::instrument(skip_all, fields(as_of = %as_of))]
pub async fn recurring_summary(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<RecurringListDto, PhoskError> {
    let subs = list_subscriptions(db, as_of, SubFilter::default()).await?;
    let monthly_total = Money::sum(subs.iter().map(|s| s.monthly_equiv))?;
    let mut recurring: Vec<RecurringDto> = subs
        .iter()
        .map(|s| RecurringDto {
            id: s.id.clone(),
            name: s.name.clone(),
            amount: s.amount,
            next: s.next_label.clone(),
            days_until: s.days_until,
        })
        .collect();
    recurring.sort_by_key(|r| r.days_until);
    Ok(RecurringListDto {
        recurring,
        monthly_total,
    })
}

// ── Derived-field helpers ─────────────────────────────────────────────────────

/// Monthly-equivalent run-rate: monthly ⇒ `amount`; yearly ⇒ `amount / 12`
/// (integer-centime truncating division).
fn monthly_equiv(amount: Money, cadence: &str) -> Money {
    if cadence == "yearly" {
        Money::from_centimes(amount.centimes() / 12)
    } else {
        amount
    }
}

/// Annualized total: monthly ⇒ `amount * 12`; yearly ⇒ `amount`.
fn annual(amount: Money, cadence: &str) -> Result<Money, PhoskError> {
    if cadence == "yearly" {
        return Ok(amount);
    }
    let cents = amount
        .centimes()
        .checked_mul(12)
        .ok_or_else(|| PhoskError::Overflow("subscription annual".to_owned()))?;
    Ok(Money::from_centimes(cents))
}

/// Whole days from `as_of` to the next occurrence of the charge date. `≤0` = due.
pub(crate) fn days_until(
    as_of: NaiveDate,
    day: u32,
    cadence: &str,
    month: &str,
) -> Result<i32, PhoskError> {
    let next = next_charge_date(as_of, day, cadence, month)?;
    let days = (next - as_of).num_days();
    i32::try_from(days).map_err(|_| PhoskError::Overflow("days_until".to_owned()))
}

/// `"DD MON"` label for the next charge date.
fn next_label(
    as_of: NaiveDate,
    day: u32,
    cadence: &str,
    month: &str,
) -> Result<String, PhoskError> {
    let next = next_charge_date(as_of, day, cadence, month)?;
    let m = next.month();
    let abbr = MONTHS
        .get((m as usize).saturating_sub(1))
        .copied()
        .unwrap_or("");
    Ok(format!("{:02} {}", next.day(), abbr))
}

/// Human status label for a status key.
fn status_label(status: &str) -> String {
    match status {
        crate::lifecycle::STATUS_DUE => "NOT SEEN",
        crate::lifecycle::STATUS_SOON => "DUE SOON",
        crate::lifecycle::STATUS_WATCH => "REVIEW",
        crate::lifecycle::STATUS_PAUSED => "PAUSED",
        crate::lifecycle::STATUS_CANCELLED => "CANCELLED",
        _ => "ACTIVE",
    }
    .to_owned()
}

/// Map the internal [`phosk_model::Source`] to the wire string (`"user"`/`"llm"`).
fn source_str(source: phosk_model::Source) -> String {
    match source {
        phosk_model::Source::LlmInferred | phosk_model::Source::RuleGenerated => "llm",
        _ => "user",
    }
    .to_owned()
}

/// `priceRose`: the last recorded charge exceeds the prior one.
fn price_rose(charges: &[Charge]) -> bool {
    let mut sorted: Vec<&Charge> = charges.iter().collect();
    sorted.sort_by_key(|c| c.date);
    // The current (latest) charge rose vs. the cheapest earlier one: the seed
    // models a price rise as a discounted first charge followed by full price.
    match sorted.split_last() {
        Some((last, earlier)) if !earlier.is_empty() => earlier
            .iter()
            .map(|c| c.amount.centimes())
            .min()
            .is_some_and(|cheapest| last.amount.centimes() > cheapest),
        _ => false,
    }
}

/// Build the full [`SubscriptionDto`] for a subscription + its charge history.
fn build_dto(
    sub: &Subscription,
    charges: &[Charge],
    as_of: NaiveDate,
) -> Result<SubscriptionDto, PhoskError> {
    let monthly = monthly_equiv(sub.amount, &sub.cadence);
    let annual_total = annual(sub.amount, &sub.cadence)?;
    let days = days_until(as_of, sub.day, &sub.cadence, &sub.month)?;
    let label = next_label(as_of, sub.day, &sub.cadence, &sub.month)?;
    let hist: Vec<f64> = {
        let mut sorted: Vec<&Charge> = charges.iter().collect();
        sorted.sort_by_key(|c| c.date);
        sorted.iter().map(|c| c.amount.as_chf_f64()).collect()
    };
    Ok(SubscriptionDto {
        id: sub.slug.clone(),
        name: sub.name.clone(),
        amount: sub.amount,
        cadence: sub.cadence.clone(),
        status: sub.status.clone(),
        status_label: status_label(&sub.status),
        monthly_equiv: monthly,
        annual: annual_total,
        days_until: days,
        next_label: label,
        day: sub.day,
        month: sub.month.clone(),
        source: source_str(sub.source),
        category: sub.category.clone(),
        glyph: sub.glyph.clone(),
        since: sub.since.to_string(),
        price_rose: price_rose(charges),
        hist,
        note: sub.note.clone(),
    })
}
