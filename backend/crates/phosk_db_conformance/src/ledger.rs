//! Ledger: receipts, line items and the correction audit log.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_id::{CorrectionId, ReceiptId};
use phosk_model::{CorrectionEvent, LineItem, Provenance, Receipt};

use crate::support::{
    Outcome, date, ensure, ensure_eq, ensure_invalid, ensure_not_found, line, receipt, sorted,
};

/// A fresh-slug insert returns the caller's id and every read path sees the
/// exact receipt and lines that were written.
pub async fn insert_receipt_appends_and_reads_back(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.all_receipts().await?.len();
    let id = ReceiptId::new();
    let r = receipt(id, "conf-new", date(2027, 1, 10)?, 1_000);
    let lines = vec![line(id, "Apples", 400), line(id, "Bread", 600)];

    let returned = db.insert_receipt(r.clone(), lines.clone()).await?;
    ensure_eq(&returned, &id, "insert_receipt returns the new id")?;

    let all = db.all_receipts().await?;
    ensure_eq(&all.len(), &(before + 1), "receipt count after insert")?;
    ensure(all.contains(&r), "all_receipts lists the new receipt")?;
    ensure_eq(&db.receipt(id).await?, &r, "receipt(id)")?;
    ensure_eq(
        &db.receipt_by_slug("conf-new").await?,
        &r,
        "receipt_by_slug",
    )?;
    let got = db.line_items(id).await?;
    let by_id = |l: &LineItem| l.id.to_string();
    ensure_eq(&sorted(got, by_id), &sorted(lines, by_id), "line_items")
}

/// Lines minted against some other receipt id are rebound to the stored one.
pub async fn insert_receipt_binds_lines_to_the_receipt(db: &dyn DatabaseAdapter) -> Outcome {
    let id = ReceiptId::new();
    let stray = line(ReceiptId::new(), "Stray", 200);
    let r = receipt(id, "conf-bind", date(2027, 1, 10)?, 200);
    db.insert_receipt(r, vec![stray.clone()]).await?;
    let rebound = LineItem {
        receipt_id: id,
        ..stray
    };
    ensure_eq(&db.line_items(id).await?, &vec![rebound], "rebound line")
}

/// Re-importing a known slug replaces the receipt in place: stable id, new
/// payload, new line set, no duplicate.
pub async fn insert_receipt_same_slug_replaces_in_place(db: &dyn DatabaseAdapter) -> Outcome {
    let (first_id, day) = (ReceiptId::new(), date(2027, 1, 10)?);
    let old = vec![line(first_id, "Old", 1_000)];
    db.insert_receipt(receipt(first_id, "conf-dup", day, 1_000), old)
        .await?;
    let count = db.all_receipts().await?.len();

    let second_id = ReceiptId::new();
    let new = vec![
        line(second_id, "New A", 1_000),
        line(second_id, "New B", 1_500),
    ];
    let again = receipt(second_id, "conf-dup", day, 2_500);
    let returned = db.insert_receipt(again, new).await?;
    ensure_eq(&returned, &first_id, "re-import keeps the stored id")?;
    ensure_eq(&db.all_receipts().await?.len(), &count, "no duplicate")?;
    let got = db.receipt_by_slug("conf-dup").await?;
    ensure_eq(&got.id, &first_id, "stored id")?;
    ensure_eq(&got.amount.centimes(), &2_500, "replaced amount")?;
    ensure_not_found(db.receipt(second_id).await, "receipt(re-import id)")?;

    let lines = db.line_items(first_id).await?;
    let mut names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
    names.sort_unstable();
    ensure_eq(&names, &vec!["New A", "New B"], "line set replaced")?;
    let rebound = lines.iter().all(|l| l.receipt_id == first_id);
    ensure(rebound, "new lines rebound to the stored id")?;
    let orphans = db.line_items(second_id).await?;
    ensure(orphans.is_empty(), "no lines under the discarded id")
}

/// Writing the SAME rows (same ids, same slug) twice — what a concurrent
/// double-approve of one proposal degrades to — is one booking: one receipt,
/// one projected transaction, one set of lines.
pub async fn insert_receipt_same_rows_twice_is_one_booking(db: &dyn DatabaseAdapter) -> Outcome {
    let (id, day) = (ReceiptId::new(), date(2027, 1, 12)?);
    let r = receipt(id, "conf-twice", day, 2_500);
    let lines = vec![line(id, "A", 1_000), line(id, "B", 1_500)];
    let count = db.all_receipts().await?.len();
    db.insert_receipt(r.clone(), lines.clone()).await?;
    let again = db.insert_receipt(r, lines.clone()).await?;
    ensure_eq(&again, &id, "same id")?;
    ensure_eq(&db.all_receipts().await?.len(), &(count + 1), "one receipt")?;
    let projected = db.transactions_between(day, day).await?;
    ensure_eq(&projected.len(), &1, "one projected transaction")?;
    let stored: Vec<String> = db
        .line_items(id)
        .await?
        .iter()
        .map(|l| l.id.to_string())
        .collect();
    let written: Vec<String> = lines.iter().map(|l| l.id.to_string()).collect();
    ensure_eq(
        &sorted(stored, Clone::clone),
        &sorted(written, Clone::clone),
        "one set of lines",
    )
}

/// A stored receipt is projected into the dashboard `Transaction` view, so a
/// written spend is visible to the cycle aggregates (`transactions_between`).
pub async fn insert_receipt_projects_a_dashboard_transaction(db: &dyn DatabaseAdapter) -> Outcome {
    let day = date(2027, 1, 10)?;
    let id = ReceiptId::new();
    let r = receipt(id, "conf-projected", day, 1_234);
    db.insert_receipt(r.clone(), vec![line(id, "Apples", 1_234)])
        .await?;

    let projected = db.transactions_between(day, day).await?;
    ensure_eq(&projected.len(), &1, "one projected transaction")?;
    let got = projected.into_iter().next().ok_or("no projected row")?;
    ensure_eq(&got.date, &r.date, "projected date")?;
    ensure_eq(&got.shop, &r.shop, "projected shop")?;
    ensure_eq(&got.category, &r.category, "projected category")?;
    ensure_eq(&got.amount, &r.amount, "projected amount")
}

/// Re-importing a known slug replaces its projected transaction in place rather
/// than double-counting the spend.
pub async fn insert_receipt_same_slug_replaces_the_projection(db: &dyn DatabaseAdapter) -> Outcome {
    let day = date(2027, 1, 10)?;
    let first = ReceiptId::new();
    db.insert_receipt(receipt(first, "conf-proj-dup", day, 1_000), Vec::new())
        .await?;
    let again = Receipt {
        amount: Money::from_centimes(2_500),
        ..receipt(ReceiptId::new(), "conf-proj-dup", day, 2_500)
    };
    db.insert_receipt(again, Vec::new()).await?;

    let projected = db.transactions_between(day, day).await?;
    ensure_eq(&projected.len(), &1, "no duplicate projected transaction")?;
    let got = projected.into_iter().next().ok_or("no projected row")?;
    ensure_eq(
        &got.amount.centimes(),
        &2_500,
        "projection follows the replacement",
    )
}

/// `delete_receipt` takes back everything `insert_receipt` wrote — the receipt,
/// its lines and its dashboard projection — and leaves its neighbours alone.
/// Deleting an unknown (or already deleted) receipt is `NotFound`.
pub async fn delete_receipt_removes_it_its_lines_and_its_projection(
    db: &dyn DatabaseAdapter,
) -> Outcome {
    let day = date(2027, 1, 10)?;
    let (gone, kept) = (ReceiptId::new(), ReceiptId::new());
    db.insert_receipt(
        receipt(gone, "conf-del", day, 1_000),
        vec![line(gone, "Apples", 400), line(gone, "Bread", 600)],
    )
    .await?;
    db.insert_receipt(
        receipt(kept, "conf-del-keep", day, 700),
        vec![line(kept, "Cheese", 700)],
    )
    .await?;
    let before = db.all_receipts().await?.len();

    db.delete_receipt(gone).await?;
    ensure_eq(&db.all_receipts().await?.len(), &(before - 1), "count")?;
    ensure_not_found(db.receipt(gone).await, "receipt(deleted)")?;
    ensure_not_found(db.receipt_by_slug("conf-del").await, "receipt_by_slug")?;
    ensure(
        db.line_items(gone).await?.is_empty(),
        "no orphan line items",
    )?;

    let projected = db.transactions_between(day, day).await?;
    ensure_eq(&projected.len(), &1, "only the surviving projection")?;
    let got = projected.into_iter().next().ok_or("no projected row")?;
    ensure_eq(&got.amount.centimes(), &700, "the deleted spend is gone")?;

    ensure_eq(
        &db.receipt(kept).await?.amount.centimes(),
        &700,
        "neighbour",
    )?;
    ensure_eq(&db.line_items(kept).await?.len(), &1, "neighbour's lines")?;

    ensure_not_found(db.delete_receipt(gone).await, "delete_receipt(twice)")?;
    ensure_not_found(
        db.delete_receipt(ReceiptId::new()).await,
        "delete_receipt(unknown)",
    )
}

/// `receipts_between` keeps exactly the receipts dated inside `[from, to]`,
/// and rejects `from > to` as invalid input.
pub async fn receipts_between_filters_inclusively(db: &dyn DatabaseAdapter) -> Outcome {
    let (d10, d11, d12) = (date(2027, 1, 10)?, date(2027, 1, 11)?, date(2027, 1, 12)?);
    let (a, b) = (ReceiptId::new(), ReceiptId::new());
    db.insert_receipt(receipt(a, "conf-a", d10, 100), Vec::new())
        .await?;
    db.insert_receipt(receipt(b, "conf-b", d12, 200), Vec::new())
        .await?;
    let ids = |rs: Vec<Receipt>| -> Vec<ReceiptId> {
        sorted(rs, |r| r.id.to_string())
            .into_iter()
            .map(|r| r.id)
            .collect()
    };

    let mut both = vec![a, b];
    both.sort_by_key(ToString::to_string);
    let got = ids(db.receipts_between(d10, d12).await?);
    ensure_eq(&got, &both, "[10, 12] holds both bounds")?;
    let got = ids(db.receipts_between(d10, d10).await?);
    ensure_eq(&got, &vec![a], "[10, 10]")?;
    ensure(db.receipts_between(d11, d11).await?.is_empty(), "[11, 11]")?;

    let june = db
        .receipts_between(date(2026, 6, 1)?, date(2026, 6, 30)?)
        .await?;
    ensure_eq(&june.len(), &9, "seeded June receipts")?;
    let inverted = db.receipts_between(d12, d10).await;
    ensure_invalid(inverted, "receipts_between(inverted)")
}

/// Unknown ids and slugs are `NotFound`; an unknown receipt has no lines.
pub async fn receipt_lookups_report_not_found(db: &dyn DatabaseAdapter) -> Outcome {
    ensure_not_found(db.receipt(ReceiptId::new()).await, "receipt")?;
    ensure_not_found(db.receipt_by_slug("conf-none").await, "receipt_by_slug")?;
    let lines = db.line_items(ReceiptId::new()).await?;
    ensure(lines.is_empty(), "unknown receipt has no lines")
}

/// `update_line_item` replaces the stored line and leaves its siblings alone;
/// editing a line that does not exist is `NotFound`.
pub async fn update_line_item_replaces_the_stored_line(db: &dyn DatabaseAdapter) -> Outcome {
    let id = ReceiptId::new();
    let (keep, edit) = (line(id, "Keep", 300), line(id, "Edit", 500));
    let r = receipt(id, "conf-edit", date(2027, 1, 10)?, 800);
    db.insert_receipt(r, vec![keep.clone(), edit.clone()])
        .await?;

    let edited = LineItem {
        name: "Edited".to_owned(),
        category: "Household".to_owned(),
        line_total: Money::from_centimes(450),
        provenance: Provenance::user_modified(),
        ..edit
    };
    db.update_line_item(edited.clone()).await?;
    let got = db.line_items(id).await?;
    ensure_eq(&got.len(), &2, "line count after edit")?;
    ensure(got.contains(&edited), "edited line stored verbatim")?;
    ensure(got.contains(&keep), "sibling line untouched")?;

    let ghost = line(id, "Ghost", 100);
    ensure_not_found(db.update_line_item(ghost).await, "update_line_item")
}

/// The audit log accepts events (it has no read path on the port) and is
/// separate from the entity: recording an event does not apply the edit.
pub async fn record_correction_accepts_events(db: &dyn DatabaseAdapter) -> Outcome {
    let id = ReceiptId::new();
    let target = line(id, "Before", 200);
    let r = receipt(id, "conf-audit", date(2027, 1, 10)?, 200);
    db.insert_receipt(r, vec![target.clone()]).await?;
    for (old, new) in [("Before", "After"), ("After", "Final")] {
        db.record_correction(CorrectionEvent {
            id: CorrectionId::new(),
            entity_id: target.id.to_string(),
            field: "name".to_owned(),
            old_value: old.to_owned(),
            new_value: new.to_owned(),
            at: date(2027, 1, 11)?,
        })
        .await?;
    }
    ensure_eq(
        &db.line_items(id).await?,
        &vec![target],
        "audited line unchanged",
    )
}
