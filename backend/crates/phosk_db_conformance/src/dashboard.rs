//! Dashboard trio: `transactions_between`, `categories`, `budget_config`.
//! The port has no write path for these, so the checks read the shared seed.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_model::Transaction;

use crate::support::{Failure, Outcome, date, ensure, ensure_eq, ensure_invalid};

/// Sum the amounts of a window into centimes.
fn cents(txns: &[Transaction]) -> Result<i64, Failure> {
    Ok(Money::sum(txns.iter().map(|t| t.amount))?.centimes())
}

/// Both bounds of the window are inclusive; the seeded cycles add up exactly.
pub async fn transactions_between_is_inclusive_on_both_bounds(db: &dyn DatabaseAdapter) -> Outcome {
    let june1 = date(2026, 6, 1)?;
    let day1 = db.transactions_between(june1, june1).await?;
    ensure_eq(&day1.len(), &3, "June 1 line count")?;
    ensure_eq(&cents(&day1)?, &205_675, "June 1 total")?;
    ensure(
        day1.iter().all(|t| t.date == june1),
        "single-day window leaked",
    )?;

    let june = db.transactions_between(june1, date(2026, 6, 30)?).await?;
    ensure_eq(&june.len(), &28, "June line count")?;
    ensure_eq(&cents(&june)?, &322_245, "June total")?;

    let may = db
        .transactions_between(date(2026, 5, 1)?, date(2026, 5, 31)?)
        .await?;
    ensure_eq(&may.len(), &39, "May line count")?;
    ensure_eq(&cents(&may)?, &378_770, "May total")
}

/// A window with no data is an empty success; `from > to` is invalid input.
pub async fn transactions_between_empty_and_inverted_windows(db: &dyn DatabaseAdapter) -> Outcome {
    let (apr1, apr30) = (date(2026, 4, 1)?, date(2026, 4, 30)?);
    let april = db.transactions_between(apr1, apr30).await?;
    ensure(april.is_empty(), "April window must be empty")?;
    let inverted = db.transactions_between(apr30, apr1).await;
    ensure_invalid(inverted, "transactions_between(inverted)")
}

/// The eight seeded categories, each with a cap.
pub async fn categories_lists_the_seeded_caps(db: &dyn DatabaseAdapter) -> Outcome {
    let cats = db.categories().await?;
    ensure_eq(&cats.len(), &8, "category count")?;
    ensure(
        cats.iter().all(|c| c.cap.is_some()),
        "every category has a cap",
    )?;
    let groceries = cats.iter().find(|c| c.name == "GROCERIES");
    let cap = groceries.ok_or("GROCERIES missing")?.cap;
    ensure_eq(&cap.map(Money::centimes), &Some(80_000), "GROCERIES cap")
}

/// The singleton budget config.
pub async fn budget_config_returns_the_seeded_config(db: &dyn DatabaseAdapter) -> Outcome {
    let cfg = db.budget_config().await?;
    ensure_eq(&cfg.monthly_budget.centimes(), &420_000, "monthly budget")?;
    ensure_eq(&cfg.savings_target.centimes(), &90_000, "savings target")
}
