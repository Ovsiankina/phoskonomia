//! Alerts service slice (F3).
//!
//! Mirrors the `AlertDto` view struct in
//! `frontend/dioxus-app/src/data/dashboard.rs` (`GET /alerts`,
//! `POST /alerts/{id}/{action}`). The DTO is field-for-field identical to that
//! wire truth: `#[serde(rename_all = "camelCase")]`.
//!
//! The service fn *signatures* and DTO *shapes* are fixed (the test agents pin
//! them verbatim). The bodies and the alert-rules helper run the rules engine
//! described below.
//!
//! ## Alert-engine rules (build contract §5.2)
//!
//! Generated from the cap/spend data at `as_of`:
//! - `over_budget`: category `spent > cap` ⇒ tone `"alert"`.
//! - `at_risk`: `usedPct > 80` OR `proj > cap` ⇒ tone `"warn"`.
//! - `savings`: savings on track vs target ⇒ tone `"info"`.
//! - `recurring_missing`: an expected charge not seen this cycle.
//!
//! `actions` carry labels (`VIEW` / `RAISE CAP` / `DISMISS` / `SNOOZE` /
//! `MARK PAID`); the page routes a non-navigation press through [`act_on_alert`].
//!
//! ## Snooze and re-trigger
//!
//! [`snooze_alert`] hides an alert until a date, or until the next cycle
//! starts (the SNOOZE action's term). It records the rules-engine *level* of
//! the alert's target category at that moment — `0` no rule fires, `1`
//! `at_risk`, `2` `over_budget` — and the alert resurfaces in [`alerts`] on
//! the first day that either:
//! - the term has ended (`as_of >= until`), or
//! - its target's level is now strictly higher than the recorded one (an
//!   at-risk alert whose category goes over its cap comes back at once).
//!
//! More spend at the same level does not re-trigger: that is exactly what the
//! user chose to stop hearing about. An alert without a category target (the
//! savings nudge) can only come back when its term ends. Re-triggering is
//! derived on read, never written back, so the stored status stays what the
//! user chose. A snoozed alert without a term (stored before snoozes carried
//! one) stays hidden.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::{CycleWindow, Period};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::{Alert, AlertSnooze};

/// One attention item (`GET /alerts` element).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Action button labels (first is the primary), e.g. `VIEW` / `RAISE CAP` /
    /// `DISMISS` / `SNOOZE` / `MARK PAID`.
    pub actions: Vec<String>,
}

/// The dashboard alerts list (`GET /alerts`), generated from cap/spend data.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter; [`PhoskError::Overflow`] on
/// any checked centime overflow inside the rules engine.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn alerts(
    db: &dyn DatabaseAdapter,
    as_of: NaiveDate,
) -> Result<Vec<AlertDto>, PhoskError> {
    // Prefer the persisted alert log: the seeded a1/a2/a3 list (and any prior
    // lifecycle state) is authoritative. Only `status == "active"` surfaces.
    // A snoozed alert rejoins them once its snooze has run out or its
    // condition has worsened (module docs).
    let mut persisted: Vec<AlertDto> = Vec::new();
    for a in db.alerts().await? {
        let shown = match a.status.as_str() {
            "active" => true,
            "snoozed" => resurfaces(db, &a, as_of).await?,
            _ => false,
        };
        if shown {
            persisted.push(AlertDto {
                id: a.slug,
                tone: a.tone,
                tag: a.tag,
                head: a.head,
                body: a.body,
                actions: a.actions.into_iter().map(|act| act.label).collect(),
            });
        }
    }
    if !persisted.is_empty() {
        return Ok(persisted);
    }

    // No persisted alerts ⇒ generate from cap/spend data via the rules engine
    // (build-contract §5.2). Prefer the richer CategoryCap envelopes; fall back
    // to the dashboard `Category` seed when no caps are configured.
    let window = Period::Month.resolve(as_of)?;
    let mut out = Vec::new();
    let caps = db.category_caps().await?;
    if caps.is_empty() {
        let categories = db.categories().await?;
        for cat in categories {
            let Some(cap) = cat.cap else { continue };
            let spent = category_spend(db, &cat.name, window).await?;
            eval_category_rules(&mut out, &cat.name, spent, cap, window);
        }
    } else {
        for cap in caps {
            let Some(cap_money) = cap.cap else { continue };
            let spent = category_spend_receipts(db, &cap.name, window).await?;
            eval_category_rules(&mut out, &cap.name, spent, cap_money, window);
        }
    }
    Ok(out)
}

/// Sum of dashboard `Transaction` rows for one category in the spend-to-date
/// window `[start, as_of]`.
async fn category_spend(
    db: &dyn DatabaseAdapter,
    name: &str,
    window: CycleWindow,
) -> Result<Money, PhoskError> {
    let txns = db.transactions_between(window.start, window.as_of).await?;
    Money::sum(
        txns.into_iter()
            .filter(|t| t.category == name)
            .map(|t| t.amount),
    )
}

/// Sum of `Receipt` rows for one category in the spend-to-date window.
async fn category_spend_receipts(
    db: &dyn DatabaseAdapter,
    name: &str,
    window: CycleWindow,
) -> Result<Money, PhoskError> {
    let receipts = db.receipts_between(window.start, window.as_of).await?;
    Money::sum(
        receipts
            .into_iter()
            .filter(|r| r.category == name)
            .map(|r| r.amount),
    )
}

/// Act on a dashboard alert (`POST /alerts/{id}/{dismiss|snooze|apply}`).
///
/// `as_of` is the caller's "today", the same day it reads [`alerts`] with:
/// the SNOOZE action's term (the next cycle) and recorded level are measured
/// against it, so the snooze compares like with like on the next read.
///
/// # Errors
/// Returns [`PhoskError::NotFound`] if no alert matches `alert`; returns
/// [`PhoskError::Invalid`] for an unknown `action`; propagates any adapter
/// [`PhoskError`].
#[tracing::instrument(level = "debug", skip_all)]
pub async fn act_on_alert(
    db: &dyn DatabaseAdapter,
    alert: &str,
    action: &str,
    as_of: NaiveDate,
) -> Result<(), PhoskError> {
    // Resolve the alert (by slug) to its typed id + target; NotFound if absent.
    let entity = db
        .alerts()
        .await?
        .into_iter()
        .find(|a| a.slug == alert)
        .ok_or_else(|| PhoskError::NotFound(format!("alert {alert}")))?;

    match action {
        "dismiss" => db.update_alert_status(entity.id, "dismissed").await,
        "snooze" => snooze_alert(db, alert, SnoozeUntil::NextCycle, as_of)
            .await
            .map(|_| ()),
        "apply" => raise_targeted_cap(db, entity.target.as_deref()).await,
        other => Err(PhoskError::Invalid(format!("unknown alert action {other}"))),
    }
}

/// How long [`snooze_alert`] hides an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnoozeUntil {
    /// Until this day (it resurfaces on it); must be after `as_of`.
    Date(NaiveDate),
    /// Until the first day of the next monthly cycle.
    NextCycle,
}

/// Snooze an alert (by slug) and return the recorded term. Write path.
///
/// Snoozing an already-snoozed alert replaces its term and re-records its
/// level. See the module docs for when it comes back.
///
/// # Errors
/// [`PhoskError::NotFound`] if no alert matches `alert`;
/// [`PhoskError::Invalid`] if the alert is dismissed or the term does not end
/// after `as_of`; otherwise any port error.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn snooze_alert(
    db: &dyn DatabaseAdapter,
    alert: &str,
    until: SnoozeUntil,
    as_of: NaiveDate,
) -> Result<AlertSnooze, PhoskError> {
    let entity = db
        .alerts()
        .await?
        .into_iter()
        .find(|a| a.slug == alert)
        .ok_or_else(|| PhoskError::NotFound(format!("alert {alert}")))?;
    if entity.status == "dismissed" {
        return Err(PhoskError::Invalid(format!(
            "alert {alert} is dismissed and cannot be snoozed"
        )));
    }
    let until = match until {
        SnoozeUntil::Date(day) => day,
        SnoozeUntil::NextCycle => Period::Month
            .resolve(as_of)?
            .end
            .succ_opt()
            .ok_or_else(|| PhoskError::Overflow(format!("the cycle after {as_of}")))?,
    };
    if until <= as_of {
        return Err(PhoskError::Invalid(format!(
            "a snooze must end after {as_of}, not on {until}"
        )));
    }
    let level = target_level(db, entity.target.as_deref(), as_of).await?;
    let snooze = AlertSnooze { until, level };
    db.snooze_alert(entity.id, snooze).await?;
    Ok(snooze)
}

/// Whether a snoozed alert is back on the list at `as_of`.
async fn resurfaces(
    db: &dyn DatabaseAdapter,
    alert: &Alert,
    as_of: NaiveDate,
) -> Result<bool, PhoskError> {
    let Some(snooze) = alert.snooze else {
        return Ok(false);
    };
    if as_of >= snooze.until {
        return Ok(true);
    }
    Ok(target_level(db, alert.target.as_deref(), as_of).await? > snooze.level)
}

/// The rules-engine level of an alert's target category at `as_of`; `0` when
/// the alert has no target, the target is not a category, or it is uncapped.
async fn target_level(
    db: &dyn DatabaseAdapter,
    target: Option<&str>,
    as_of: NaiveDate,
) -> Result<u8, PhoskError> {
    let Some(target) = target else {
        return Ok(0);
    };
    let caps = db.category_caps().await?;
    let Some(cap) = caps.iter().find(|c| c.slug == target || c.name == target) else {
        return Ok(0);
    };
    let Some(cap_money) = cap.cap else {
        return Ok(0);
    };
    let window = Period::Month.resolve(as_of)?;
    let spent = category_spend_receipts(db, &cap.name, window).await?;
    Ok(rule_level(spent, cap_money, window))
}

/// Raise the cap of the alert's targeted envelope by 10% (the RAISE CAP action).
/// The target is a [`CategoryCap`](phosk_model::CategoryCap) slug; an unlimited
/// or missing target is a no-op-safe error path.
async fn raise_targeted_cap(
    db: &dyn DatabaseAdapter,
    target: Option<&str>,
) -> Result<(), PhoskError> {
    let target = target
        .ok_or_else(|| PhoskError::Invalid("alert has no target to raise a cap on".to_owned()))?;
    let cap = db
        .category_caps()
        .await?
        .into_iter()
        .find(|c| c.slug == target || c.name == target)
        .ok_or_else(|| PhoskError::NotFound(format!("category {target}")))?;
    // Raise an already-capped envelope by 10% (rounded up by a centime so even a
    // tiny cap strictly increases); leave an unlimited envelope unbounded.
    if let Some(current) = cap.cap {
        let raised = current
            .centimes()
            .checked_add(current.centimes() / 10 + 1)
            .ok_or_else(|| PhoskError::Overflow(format!("raising cap for {target}")))?;
        db.set_category_cap(&cap.name, Some(Money::from_centimes(raised)))
            .await?;
    }
    Ok(())
}

// ── rules-engine helper ─────────────────────────────────────────────────────────

/// Evaluate the alert rules for one category's cycle figures, pushing any
/// triggered [`AlertDto`]. `spent`/`cap` are exact [`Money`]; `proj` and
/// `usedPct` are derived here from the run-rate formula (`proj = spent·N/d`).
///
/// - `over_budget`: `spent > cap` ⇒ tone `"alert"` (coral).
/// - `at_risk`: `usedPct > 80` OR `proj > cap` (and not already over) ⇒ tone `"warn"`.
fn eval_category_rules(
    out: &mut Vec<AlertDto>,
    name: &str,
    spent: Money,
    cap: Money,
    window: CycleWindow,
) {
    let tag = name.to_uppercase();
    let level = rule_level(spent, cap, window);

    if level == LEVEL_OVER_BUDGET {
        out.push(AlertDto {
            id: format!("gen-over-{name}"),
            tone: "alert".to_owned(),
            tag,
            head: format!("{name} over budget"),
            body: format!("{name} spend has crossed its cap this cycle."),
            actions: vec![
                "VIEW".to_owned(),
                "RAISE CAP".to_owned(),
                "DISMISS".to_owned(),
            ],
        });
    } else if level == LEVEL_AT_RISK {
        out.push(AlertDto {
            id: format!("gen-risk-{name}"),
            tone: "warn".to_owned(),
            tag,
            head: format!("{name} run-rate above cap"),
            body: format!("{name} is on pace to exceed its cap by month-end."),
            actions: vec!["VIEW".to_owned(), "DISMISS".to_owned()],
        });
    }
}

/// Rules-engine level: `at_risk`.
const LEVEL_AT_RISK: u8 = 1;
/// Rules-engine level: `over_budget`.
const LEVEL_OVER_BUDGET: u8 = 2;

/// Which category rule fires for these figures: [`LEVEL_OVER_BUDGET`] when
/// `spent > cap`, else [`LEVEL_AT_RISK`] when `usedPct > 80` or `proj > cap`,
/// else `0`. An unlimited / zero cap fires nothing (and never divides by zero).
fn rule_level(spent: Money, cap: Money, window: CycleWindow) -> u8 {
    let cap_c = cap.centimes();
    if cap_c <= 0 {
        return 0;
    }
    let spent_c = spent.centimes();
    if spent_c > cap_c {
        LEVEL_OVER_BUDGET
    } else if used_pct(spent_c, cap_c) > 80 || project_centimes(spent_c, window) > cap_c {
        LEVEL_AT_RISK
    } else {
        0
    }
}

/// `spent · N / d` run-rate in centimes; returns `spent` when day index is `0`.
fn project_centimes(spent_c: i64, window: CycleWindow) -> i64 {
    let day_index = i64::from(window.day_index());
    if day_index == 0 {
        return spent_c;
    }
    let len_days = i64::from(window.len_days());
    // An overflowing projection is unambiguously "over cap".
    spent_c
        .checked_mul(len_days)
        .map_or(i64::MAX, |scaled| scaled / day_index)
}

/// `round(100 · spent / cap)` integer percent; `0` when `cap <= 0`.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "computing a small integer percentage, not an amount; saturating cast"
)]
fn used_pct(spent_c: i64, cap_c: i64) -> i32 {
    if cap_c <= 0 {
        return 0;
    }
    let pct = 100.0 * spent_c as f64 / cap_c as f64;
    pct.round() as i32
}
