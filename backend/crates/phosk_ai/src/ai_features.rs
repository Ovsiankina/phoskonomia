//! `ai_features` — the AI feature surfaces over the panel spine.
//!
//! The activity-feed write (dismiss) and the dashboard insight line +
//! estimated saving (`/insights/dashboard`).
//!
//! The dashboard insight is **computed, not generated**: it is derived
//! deterministically from the cycle's receipts, the category caps and the
//! monthly budget, and labelled [`COMPUTED_SOURCE`] so the screen never passes
//! it off as model output. Dashboards load often and the local model is slow,
//! so a model call here would cost seconds for a sentence arithmetic can write.

use serde::{Deserialize, Serialize};

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::{CycleWindow, Period};
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::{CategoryCap, Receipt};

/// The `source` label of an insight computed from the user's data by plain
/// arithmetic (no language model involved).
pub const COMPUTED_SOURCE: &str = "COMPUTED";

/// The dashboard insight (`/insights/dashboard`): one sentence + the estimated
/// CHF saving it points at. Mirrors `dioxus-app/src/data/dashboard.rs::InsightDto`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightDto {
    /// Who wrote `text`: [`COMPUTED_SOURCE`] for a deterministic insight, or
    /// the real model id when a language model wrote it.
    pub source: String,
    /// The insight sentence.
    pub text: String,
    /// Estimated saving the insight points at, exact i64 centimes. Zero means
    /// "no estimate" (nothing to save, or not computable).
    #[serde(with = "phosk_model::money_centimes")]
    pub estimated_savings: Money,
}

/// Dismiss an AI activity-feed item by its stable id.
///
/// # Errors
/// Propagates any [`PhoskError`] from the adapter writes ([`PhoskError::NotFound`]
/// when the id is unknown).
#[tracing::instrument(level = "debug", skip_all, fields(id))]
pub async fn dismiss_feed_item(db: &dyn DatabaseAdapter, id: &str) -> Result<(), PhoskError> {
    db.dismiss_feed_item(id).await
}

/// The dashboard insight for the cycle (calendar month) containing `as_of`,
/// computed from the data, in this order of priority:
///
/// 1. no spend recorded this cycle → says so;
/// 2. a capped category whose run-rate projection overshoots its cap → the
///    worst one, with the avoidable overshoot as the estimated saving;
/// 3. a monthly budget is set → the cycle's pace against it (an overshoot
///    is the estimated saving);
/// 4. otherwise → the cycle's spend so far and its biggest category.
///
/// Spend is "to date" (`[start, as_of]`), projected linearly to the cycle end.
///
/// # Errors
/// Propagates any [`PhoskError`] from cycle resolution, the adapter reads, or
/// checked centime arithmetic.
#[tracing::instrument(level = "debug", skip_all, fields(as_of = %as_of))]
pub async fn dashboard_insight(
    db: &dyn DatabaseAdapter,
    as_of: chrono::NaiveDate,
) -> Result<InsightDto, PhoskError> {
    let window = Period::Month.resolve(as_of)?;
    let receipts = db.receipts_between(window.start, window.as_of).await?;
    let caps = db.category_caps().await?;
    let budget = db.budget_config().await?.monthly_budget;
    compose_insight(&receipts, &caps, budget, window)
}

/// The pure core of [`dashboard_insight`] (see there for the rules).
fn compose_insight(
    receipts: &[Receipt],
    caps: &[CategoryCap],
    budget: Money,
    window: CycleWindow,
) -> Result<InsightDto, PhoskError> {
    let insight = |text: String, saving: Money| InsightDto {
        source: COMPUTED_SOURCE.to_owned(),
        text,
        estimated_savings: saving.max(Money::ZERO),
    };

    let spent = Money::sum(receipts.iter().map(|r| r.amount))?;
    if receipts.is_empty() || spent.centimes() <= 0 {
        return Ok(insight(
            "No spending recorded this cycle yet. Add a receipt or a transaction and this line will track your pace.".to_owned(),
            Money::ZERO,
        ));
    }
    let days_left = window.days_left();

    // (2) The capped category heading furthest past its cap.
    let mut worst: Option<(String, Money, Money, Money)> = None; // name, spent, cap, projected
    for cap in caps {
        let Some(limit) = cap.cap.filter(|c| c.centimes() > 0) else {
            continue;
        };
        let cat_spent = Money::sum(
            receipts
                .iter()
                .filter(|r| r.category == cap.name)
                .map(|r| r.amount),
        )?;
        let projected = project(cat_spent, window)?;
        let over = projected.checked_sub(limit)?;
        if over.centimes() > 0
            && worst
                .as_ref()
                .is_none_or(|(_, _, l, p)| p.checked_sub(*l).is_ok_and(|o| over > o))
        {
            worst = Some((cap.name.clone(), cat_spent, limit, projected));
        }
    }
    if let Some((name, cat_spent, limit, projected)) = worst {
        // What is still avoidable this cycle: the projection above the cap,
        // or above what is already spent once the cap is blown.
        let saving = projected.checked_sub(cat_spent.max(limit))?;
        let text = if cat_spent > limit {
            let over = cat_spent.checked_sub(limit)?;
            format!(
                "{name} is already {over} over its {limit} cap, with {days_left} {} left in the cycle.",
                plural_days(days_left)
            )
        } else {
            let over = projected.checked_sub(limit)?;
            format!(
                "At this pace {name} ends the cycle at {projected}, {over} over its {limit} cap."
            )
        };
        return Ok(insight(text, saving));
    }

    // (3) Pace against the monthly budget.
    let projected = project(spent, window)?;
    if budget.centimes() > 0 {
        if projected > budget {
            let over = projected.checked_sub(budget)?;
            let saving = projected.checked_sub(spent.max(budget))?;
            return Ok(insight(
                format!(
                    "At this pace you spend {projected} this cycle, {over} over your {budget} budget."
                ),
                saving,
            ));
        }
        let headroom = budget.checked_sub(projected)?;
        return Ok(insight(
            format!(
                "On pace: {spent} spent so far, heading for {projected} of your {budget} budget ({headroom} to spare)."
            ),
            Money::ZERO,
        ));
    }

    // (4) No budget: what the money went on.
    let (top, top_spent) = biggest_category(receipts)?;
    let share = percent_of(top_spent, spent);
    Ok(insight(
        format!(
            "{spent} spent so far this cycle; {top} is the biggest share at {share}%. Set a monthly budget to see your pace."
        ),
        Money::ZERO,
    ))
}

/// Linear run-rate projection of `spent` (to date) to the end of the window:
/// `spent · len_days / day_index`, in exact centimes.
fn project(spent: Money, window: CycleWindow) -> Result<Money, PhoskError> {
    let day_index = i64::from(window.day_index().max(1));
    let len_days = i64::from(window.len_days());
    let scaled = spent
        .centimes()
        .checked_mul(len_days)
        .ok_or_else(|| PhoskError::Overflow(format!("projecting {spent} over {len_days} days")))?;
    Ok(Money::from_centimes(scaled / day_index))
}

/// The category with the most spend (ties broken by name, ascending).
fn biggest_category(receipts: &[Receipt]) -> Result<(String, Money), PhoskError> {
    let mut totals: Vec<(String, Money)> = Vec::new();
    for r in receipts {
        match totals.iter_mut().find(|(n, _)| *n == r.category) {
            Some((_, t)) => *t = t.checked_add(r.amount)?,
            None => totals.push((r.category.clone(), r.amount)),
        }
    }
    totals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(totals
        .into_iter()
        .next()
        .unwrap_or_else(|| (String::new(), Money::ZERO)))
}

/// `part / whole` as a rounded whole percent (0 when `whole` is not positive).
fn percent_of(part: Money, whole: Money) -> i64 {
    let w = i128::from(whole.centimes());
    if w <= 0 {
        return 0;
    }
    let p = i128::from(part.centimes()) * 100;
    i64::try_from((p + w / 2) / w).unwrap_or(0)
}

/// `"day"` / `"days"`.
const fn plural_days(n: u32) -> &'static str {
    if n == 1 { "day" } else { "days" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("date")
    }

    fn window(day: u32) -> CycleWindow {
        Period::Month.resolve(d(2026, 6, day)).expect("window")
    }

    fn receipt(category: &str, chf: i64) -> Receipt {
        Receipt {
            id: phosk_id::ReceiptId::default(),
            slug: String::new(),
            shop: "Shop".to_owned(),
            date: d(2026, 6, 1),
            category: category.to_owned(),
            amount: Money::from_centimes(chf * 100),
            fixed: false,
            provenance: phosk_model::Provenance::user_entered(),
            source_kind: "MANUAL".to_owned(),
            ocr_engine: String::new(),
            ocr_regions: 0,
        }
    }

    fn cap(name: &str, chf: Option<i64>) -> CategoryCap {
        CategoryCap {
            id: phosk_id::CategoryId::default(),
            slug: String::new(),
            name: name.to_owned(),
            cap: chf.map(|c| Money::from_centimes(c * 100)),
            fixed: false,
            glyph: String::new(),
            note: String::new(),
            provenance: phosk_model::Provenance::user_entered(),
        }
    }

    #[test]
    fn no_spend_says_so_and_estimates_nothing() {
        let i = compose_insight(&[], &[], Money::ZERO, window(10)).expect("insight");
        assert_eq!(i.source, COMPUTED_SOURCE);
        assert!(i.text.starts_with("No spending recorded"), "{}", i.text);
        assert_eq!(i.estimated_savings, Money::ZERO);
    }

    #[test]
    fn worst_projected_cap_overshoot_wins() {
        // Day 10 of 30: projections are ×3.
        let receipts = [receipt("Coffee", 30), receipt("Food", 100)];
        let caps = [cap("Coffee", Some(50)), cap("Food", Some(200))];
        let i = compose_insight(&receipts, &caps, Money::ZERO, window(10)).expect("insight");
        // Coffee → 90 vs 50 (40 over); Food → 300 vs 200 (100 over).
        assert_eq!(
            i.text,
            "At this pace Food ends the cycle at CHF 300.00, CHF 100.00 over its CHF 200.00 cap."
        );
        assert_eq!(i.estimated_savings, Money::from_centimes(10_000));
    }

    #[test]
    fn a_blown_cap_counts_only_the_still_avoidable_spend() {
        let receipts = [receipt("Coffee", 60)];
        let caps = [cap("Coffee", Some(50))];
        let i = compose_insight(&receipts, &caps, Money::ZERO, window(10)).expect("insight");
        assert_eq!(
            i.text,
            "Coffee is already CHF 10.00 over its CHF 50.00 cap, with 20 days left in the cycle."
        );
        // Projected 180, already spent 60 → 120 still avoidable.
        assert_eq!(i.estimated_savings, Money::from_centimes(12_000));
    }

    #[test]
    fn uncapped_categories_fall_back_to_budget_pace() {
        let receipts = [receipt("Food", 100)];
        let caps = [cap("Food", None)];
        let budget = Money::from_centimes(20_000);
        let i = compose_insight(&receipts, &caps, budget, window(10)).expect("insight");
        assert_eq!(
            i.text,
            "At this pace you spend CHF 300.00 this cycle, CHF 100.00 over your CHF 200.00 budget."
        );
        assert_eq!(i.estimated_savings, Money::from_centimes(10_000));

        let roomy = Money::from_centimes(50_000);
        let i = compose_insight(&receipts, &caps, roomy, window(10)).expect("insight");
        assert!(
            i.text.starts_with("On pace: CHF 100.00 spent"),
            "{}",
            i.text
        );
        assert_eq!(i.estimated_savings, Money::ZERO);
    }

    #[test]
    fn no_budget_names_the_biggest_category() {
        let receipts = [receipt("Food", 75), receipt("Coffee", 25)];
        let i = compose_insight(&receipts, &[], Money::ZERO, window(10)).expect("insight");
        assert_eq!(
            i.text,
            "CHF 100.00 spent so far this cycle; Food is the biggest share at 75%. Set a monthly budget to see your pace."
        );
        assert_eq!(i.estimated_savings, Money::ZERO);
    }
}
