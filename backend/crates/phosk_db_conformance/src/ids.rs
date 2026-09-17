//! Typed-id round trips.
//!
//! Regression guard for the historical `SurrealDB` failure where a storage
//! record id (`Thing`) leaked into deserialisation and the read path answered
//! with a server error instead of the entity. Ids written as typed `phosk_id`
//! values must come back through every read path as the same typed values,
//! and must work as lookup keys.

use std::collections::HashSet;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_id::{CategoryId, LineItemId, ReceiptId};

use crate::support::{Outcome, date, ensure, ensure_eq, line, receipt};

/// Category ids: the caps list, the by-name lookup, the history rows and the
/// spend history all agree on one typed [`CategoryId`] per category, and a
/// cap write keeps it.
pub async fn category_ids_round_trip_as_typed_ids(db: &dyn DatabaseAdapter) -> Outcome {
    let caps = db.category_caps().await?;
    ensure(!caps.is_empty(), "seeded store has category caps")?;
    let ids: HashSet<CategoryId> = caps.iter().map(|c| c.id).collect();
    ensure_eq(&ids.len(), &caps.len(), "category ids are distinct")?;

    for cap in &caps {
        let by_name = db.category_cap_by_name(&cap.name).await?;
        ensure_eq(&by_name.id, &cap.id, "category_cap_by_name id")?;
        let history = db.budget_history(&cap.name).await?;
        let linked = history.iter().all(|h| h.category_id == cap.id);
        ensure(linked, "budget_history rows carry their category's id")?;
    }

    let spend = db.spend_history(6, date(2026, 6, 19)?).await?;
    ensure(!spend.is_empty(), "seeded spend history is not empty")?;
    let known = spend.iter().all(|h| ids.contains(&h.category_id));
    ensure(known, "spend_history rows reference known category ids")?;

    let first = caps.first().ok_or("no caps")?;
    let cap = Some(Money::from_centimes(12_300));
    db.set_category_cap(&first.name, cap).await?;
    let after = db.category_cap_by_name(&first.name).await?;
    ensure_eq(&after.id, &first.id, "a cap write keeps the category id")
}

/// Receipt and line ids: every seeded receipt resolves by its own id and slug
/// to the same id, its lines point back at it, and a freshly minted id survives
/// every read path unchanged.
pub async fn receipt_ids_round_trip_as_typed_ids(db: &dyn DatabaseAdapter) -> Outcome {
    let seeded = db.all_receipts().await?;
    ensure(!seeded.is_empty(), "seeded store has receipts")?;
    for r in &seeded {
        ensure_eq(&db.receipt(r.id).await?.id, &r.id, "receipt(id).id")?;
        let by_slug = db.receipt_by_slug(&r.slug).await?;
        ensure_eq(&by_slug.id, &r.id, "receipt_by_slug id")?;
        let lines = db.line_items(r.id).await?;
        let linked = lines.iter().all(|l| l.receipt_id == r.id);
        ensure(linked, "seeded lines point at their receipt id")?;
    }

    let (id, day) = (ReceiptId::new(), date(2027, 2, 2)?);
    let lines = vec![line(id, "Typed A", 200), line(id, "Typed B", 400)];
    let r = receipt(id, "conf-typed", day, 600);
    ensure_eq(
        &db.insert_receipt(r, lines.clone()).await?,
        &id,
        "insert id",
    )?;
    ensure_eq(&db.receipt(id).await?.id, &id, "receipt(id).id")?;
    let window: Vec<ReceiptId> = db
        .receipts_between(day, day)
        .await?
        .iter()
        .map(|r| r.id)
        .collect();
    ensure_eq(&window, &vec![id], "receipts_between id")?;
    let listed = db.all_receipts().await?.iter().any(|r| r.id == id);
    ensure(listed, "all_receipts lists the new id")?;

    let pairs = |ls: &[phosk_model::LineItem]| -> HashSet<(LineItemId, ReceiptId)> {
        ls.iter().map(|l| (l.id, l.receipt_id)).collect()
    };
    let got = db.line_items(id).await?;
    ensure_eq(
        &pairs(&got),
        &pairs(&lines),
        "line ids and their receipt id",
    )
}
