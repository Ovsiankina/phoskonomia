//! Categories: create, rename (re-pointing every reference), delete-if-empty,
//! merge (folding one category into another) and split (carving a new one out
//! of a hand-picked set of line items).
//!
//! The category record is [`CategoryCap`] — the user-facing envelope whose
//! `name` is its identity and which receipts, line items, subscriptions and
//! signals reference *by that name*. The checks below assert the port's three
//! write behaviours without leaning on which seeded rows happen to carry a
//! given name: they count references before and after, so the same assertions
//! hold for any store that satisfies the seed contract.

use std::collections::BTreeSet;

use phosk_adapter_db::{DatabaseAdapter, merged_cap};
use phosk_core::money::Money;
use phosk_id::{CategoryId, LineItemId, ReceiptId};
use phosk_model::{Category, CategoryCap, LineItem, Provenance, Receipt, Source, Transaction};

use crate::support::{
    Outcome, date, ensure, ensure_eq, ensure_invalid, ensure_not_found, line, receipt,
};

/// A synthetic, unreferenced category (not part of the seed).
fn cap(name: &str, slug: &str) -> CategoryCap {
    CategoryCap {
        id: CategoryId::new(),
        slug: slug.to_owned(),
        name: name.to_owned(),
        cap: Some(Money::from_centimes(12_500)),
        fixed: false,
        glyph: "◆".to_owned(),
        note: "created by the conformance suite".to_owned(),
        provenance: Provenance {
            source: Source::UserEntered,
            confidence: 1.0,
        },
    }
}

/// Every stored line item, across every receipt.
async fn all_lines(db: &dyn DatabaseAdapter) -> Result<Vec<LineItem>, crate::Failure> {
    let mut out = Vec::new();
    for r in db.all_receipts().await? {
        out.extend(db.line_items(r.id).await?);
    }
    Ok(out)
}

/// The stored line items naming `category`.
async fn lines_in(
    db: &dyn DatabaseAdapter,
    category: &str,
) -> Result<Vec<LineItem>, crate::Failure> {
    Ok(all_lines(db)
        .await?
        .into_iter()
        .filter(|l| l.category == category)
        .collect())
}

/// How many rows anywhere in the store name `category`.
async fn references(db: &dyn DatabaseAdapter, category: &str) -> Result<usize, crate::Failure> {
    let receipts = db.all_receipts().await?;
    let mut count = receipts.iter().filter(|r| r.category == category).count();
    for r in &receipts {
        count += db
            .line_items(r.id)
            .await?
            .iter()
            .filter(|l| l.category == category)
            .count();
    }
    count += db
        .subscriptions()
        .await?
        .iter()
        .filter(|s| s.category == category)
        .count();
    count += db
        .signals()
        .await?
        .iter()
        .filter(|s| s.parent == category)
        .count();
    Ok(count)
}

/// `insert_category` appends a readable record and refuses a duplicate name
/// (the name is the identity, ADR-008).
pub async fn insert_category_appends_and_rejects_duplicates(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.category_caps().await?.len();
    let fresh = cap("Conformance Cat", "conf-cat");

    let id = db.insert_category(fresh.clone()).await?;
    ensure_eq(&id, &fresh.id, "insert_category returns the record's id")?;
    ensure_eq(
        &db.category_caps().await?.len(),
        &(before + 1),
        "one more category",
    )?;
    ensure_eq(
        &db.category_cap_by_name("Conformance Cat").await?,
        &fresh,
        "the stored record reads back unchanged",
    )?;

    let again = cap("Conformance Cat", "conf-cat-2");
    ensure_invalid(db.insert_category(again).await, "duplicate name")?;
    let seeded = cap("Groceries", "conf-groceries");
    ensure_invalid(db.insert_category(seeded).await, "seeded duplicate name")?;
    ensure_eq(
        &db.category_caps().await?.len(),
        &(before + 1),
        "a rejected insert changed nothing",
    )
}

/// `rename_category` moves the record and every reference to it, keeps the id
/// and slug, stamps `UserModified`, and rejects a collision.
pub async fn rename_category_repoints_every_reference(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.category_cap_by_name("Groceries").await?;
    let referencing = references(db, "Groceries").await?;
    ensure(referencing > 0, "the seed references Groceries")?;

    db.rename_category("Groceries", "Food").await?;

    let after = db.category_cap_by_name("Food").await?;
    ensure_eq(&after.id, &before.id, "the id is stable across a rename")?;
    ensure_eq(&after.slug, &before.slug, "the slug is stable")?;
    let want = CategoryCap {
        name: "Food".to_owned(),
        provenance: after.provenance,
        ..before.clone()
    };
    ensure_eq(&after, &want, "only the name and provenance changed")?;
    ensure_eq(
        &after.provenance.source,
        &Source::UserModified,
        "a rename is a user edit",
    )?;
    ensure_not_found(
        db.category_cap_by_name("Groceries").await,
        "the old name is gone",
    )?;

    ensure_eq(
        &references(db, "Groceries").await?,
        &0,
        "no row still names the old category",
    )?;
    ensure_eq(
        &references(db, "Food").await?,
        &referencing,
        "every reference moved to the new name",
    )?;

    ensure_not_found(
        db.rename_category("Groceries", "Whatever").await,
        "rename(unknown)",
    )?;
    ensure_invalid(
        db.rename_category("Food", "Transport").await,
        "rename onto an existing category",
    )?;
    ensure_eq(
        &db.category_cap_by_name("Food").await?,
        &after,
        "a rejected rename changed nothing",
    )
}

/// `delete_category` removes an unreferenced category and refuses one that
/// still carries history.
pub async fn delete_category_only_when_unreferenced(db: &dyn DatabaseAdapter) -> Outcome {
    ensure(
        references(db, "Groceries").await? > 0,
        "the seed references Groceries",
    )?;
    ensure_invalid(
        db.delete_category("Groceries").await,
        "delete a referenced category",
    )?;
    ensure(
        db.category_cap_by_name("Groceries").await.is_ok(),
        "the refused delete kept the category",
    )?;

    ensure_not_found(db.delete_category("conf-none").await, "delete(unknown)")?;

    let empty = cap("Conformance Empty", "conf-empty");
    db.insert_category(empty.clone()).await?;
    let before = db.category_caps().await?.len();
    db.delete_category("Conformance Empty").await?;
    ensure_not_found(
        db.category_cap_by_name("Conformance Empty").await,
        "the deleted category is gone",
    )?;
    ensure_eq(
        &db.category_caps().await?.len(),
        &(before - 1),
        "one fewer category",
    )
}

/// `merge_categories` moves every reference onto the target, drops the source
/// record, folds the source's cap into the target's (`merged_cap`), leaves the
/// rest of the target record untouched apart from its provenance, and refuses
/// an unknown or self-directed merge.
pub async fn merge_categories_folds_one_into_the_other(db: &dyn DatabaseAdapter) -> Outcome {
    let source_refs = references(db, "Coffee & snacks").await?;
    let target_refs = references(db, "Going out").await?;
    ensure(source_refs > 0, "the seed references Coffee & snacks")?;
    let source_before = db.category_cap_by_name("Coffee & snacks").await?;
    let target_before = db.category_cap_by_name("Going out").await?;
    let caps_before = db.category_caps().await?.len();

    let moved = db.merge_categories("Coffee & snacks", "Going out").await?;

    ensure_eq(
        &usize::try_from(moved).unwrap_or(usize::MAX),
        &source_refs,
        "the reported count is the number of rows re-pointed",
    )?;
    ensure_eq(
        &references(db, "Coffee & snacks").await?,
        &0,
        "no row still names the source",
    )?;
    ensure_eq(
        &references(db, "Going out").await?,
        &(source_refs + target_refs),
        "every reference landed on the target",
    )?;
    ensure_not_found(
        db.category_cap_by_name("Coffee & snacks").await,
        "the source record is gone",
    )?;
    ensure_eq(
        &db.category_caps().await?.len(),
        &(caps_before - 1),
        "exactly one category disappeared",
    )?;

    let target_after = db.category_cap_by_name("Going out").await?;
    let want = CategoryCap {
        cap: merged_cap(target_before.cap, source_before.cap)?,
        provenance: target_after.provenance,
        ..target_before
    };
    ensure_eq(
        &target_after,
        &want,
        "the target keeps its id, slug, glyph and note and takes the folded cap",
    )?;
    ensure_eq(
        &target_after.provenance.source,
        &Source::UserModified,
        "a merge is a user edit of the target",
    )?;

    ensure_not_found(
        db.merge_categories("conf-none", "Going out").await,
        "merge(unknown source)",
    )?;
    ensure_not_found(
        db.merge_categories("Going out", "conf-none").await,
        "merge(unknown target)",
    )?;
    ensure_invalid(
        db.merge_categories("Going out", "Going out").await,
        "merge a category into itself",
    )?;
    ensure_eq(
        &db.category_cap_by_name("Going out").await?,
        &target_after,
        "a rejected merge changed nothing",
    )
}

/// `split_category` creates the new category and re-points exactly the named
/// lines — leaving the other lines, their receipts and the amounts alone — and
/// writes nothing at all when it refuses.
pub async fn split_category_carves_out_only_the_named_lines(db: &dyn DatabaseAdapter) -> Outcome {
    let groceries = lines_in(db, "Groceries").await?;
    ensure(
        groceries.len() >= 2,
        "the seed has at least two Groceries lines",
    )?;
    let picked = groceries.first().ok_or("a Groceries line")?.clone();
    let foreign = all_lines(db)
        .await?
        .into_iter()
        .find(|l| l.category != "Groceries")
        .ok_or("a line outside Groceries")?;
    let parent_before = db.receipt(picked.receipt_id).await?;
    let caps_before = db.category_caps().await?.len();
    let new = cap("Conformance Split", "conf-split");

    // Every rejection path first: none of them may write.
    ensure_not_found(
        db.split_category("conf-none", new.clone(), &[picked.id])
            .await,
        "split(unknown source category)",
    )?;
    ensure_not_found(
        db.split_category("Groceries", new.clone(), &[LineItemId::new()])
            .await,
        "split(unknown line)",
    )?;
    ensure_invalid(
        db.split_category("Groceries", new.clone(), &[picked.id, foreign.id])
            .await,
        "split(line outside the source category)",
    )?;
    ensure_invalid(
        db.split_category("Groceries", cap("Transport", "conf-taken"), &[picked.id])
            .await,
        "split(name already taken)",
    )?;
    ensure_eq(
        &db.category_caps().await?.len(),
        &caps_before,
        "not one rejected split created a category",
    )?;
    ensure_eq(
        &lines_in(db, "Groceries").await?.len(),
        &groceries.len(),
        "not one rejected split moved a line",
    )?;

    let moved = db
        .split_category("Groceries", new.clone(), &[picked.id])
        .await?;

    ensure_eq(&moved, &1, "one line moved")?;
    ensure_eq(
        &db.category_cap_by_name("Conformance Split").await?,
        &new,
        "the new category reads back as handed over",
    )?;
    let carved = lines_in(db, "Conformance Split").await?;
    ensure_eq(&carved.len(), &1, "the new category holds the picked line")?;
    let line = carved.first().ok_or("the carved line")?;
    ensure_eq(&line.id, &picked.id, "it is the line that was named")?;
    ensure_eq(
        &line.receipt_id,
        &picked.receipt_id,
        "a split does not move a line between receipts",
    )?;
    ensure_eq(
        &line.line_total,
        &picked.line_total,
        "the amount is untouched",
    )?;
    ensure_eq(
        &line.provenance.source,
        &Source::UserModified,
        "a moved line records the human re-judgement",
    )?;
    ensure_eq(
        &lines_in(db, "Groceries").await?.len(),
        &(groceries.len() - 1),
        "the other Groceries lines stayed",
    )?;
    ensure_eq(
        &db.receipt(picked.receipt_id).await?,
        &parent_before,
        "the receipt the line hangs off is untouched",
    )
}

/// The dashboard [`Category`] row named `name`, if any.
async fn row(db: &dyn DatabaseAdapter, name: &str) -> Result<Option<Category>, crate::Failure> {
    Ok(db.categories().await?.into_iter().find(|c| c.name == name))
}

/// The names of every dashboard [`Category`] row this suite did not create.
async fn other_rows(db: &dyn DatabaseAdapter) -> Result<BTreeSet<String>, crate::Failure> {
    Ok(db
        .categories()
        .await?
        .into_iter()
        .map(|c| c.name)
        .filter(|n| !n.starts_with("Conf "))
        .collect())
}

/// Every category write keeps the dashboard [`Category`] rows in step with the
/// records: an insert or a split adds a row with the record's cap, a cap edit
/// re-caps it, a rename moves it, a merge leaves only the target (with the
/// folded cap) and a delete removes it. No other row changes.
pub async fn category_rows_follow_every_category_write(db: &dyn DatabaseAdapter) -> Outcome {
    let seeded = other_rows(db).await?;

    db.insert_category(cap("Conf A", "conf-a")).await?;
    ensure_eq(
        &row(db, "Conf A").await?,
        &Some(Category {
            name: "Conf A".to_owned(),
            cap: Some(Money::from_centimes(12_500)),
        }),
        "an insert adds the row with the record's cap",
    )?;

    db.set_category_cap("Conf A", Some(Money::from_centimes(30_000)))
        .await?;
    ensure_eq(
        &row(db, "Conf A").await?.and_then(|c| c.cap),
        &Some(Money::from_centimes(30_000)),
        "a cap edit re-caps the row",
    )?;

    db.rename_category("Conf A", "Conf B").await?;
    ensure_eq(
        &row(db, "Conf A").await?,
        &None,
        "the old name's row is gone",
    )?;
    ensure_eq(
        &row(db, "Conf B").await?.and_then(|c| c.cap),
        &Some(Money::from_centimes(30_000)),
        "the renamed row keeps its cap",
    )?;

    db.insert_category(cap("Conf C", "conf-c")).await?;
    db.merge_categories("Conf C", "Conf B").await?;
    ensure_eq(
        &row(db, "Conf C").await?,
        &None,
        "the merged-away row is gone",
    )?;
    ensure_eq(
        &row(db, "Conf B").await?.and_then(|c| c.cap),
        &Some(Money::from_centimes(42_500)),
        "the target row takes the folded cap",
    )?;
    ensure_eq(
        &db.category_cap_by_name("Conf B").await?.cap,
        &Some(Money::from_centimes(42_500)),
        "the target record takes the folded cap",
    )?;

    // A split carves a category out of lines, so it needs a line to move.
    let rid = ReceiptId::new();
    let picked = LineItem {
        category: "Conf B".to_owned(),
        ..line(rid, "Conf item", 500)
    };
    let r = Receipt {
        category: "Conf B".to_owned(),
        ..receipt(rid, "conf-rows", date(2027, 3, 3)?, 500)
    };
    db.insert_receipt(r, vec![picked.clone()]).await?;
    db.split_category("Conf B", cap("Conf D", "conf-d"), &[picked.id])
        .await?;
    ensure_eq(
        &row(db, "Conf D").await?.and_then(|c| c.cap),
        &Some(Money::from_centimes(12_500)),
        "a split adds the new category's row",
    )?;

    db.insert_category(cap("Conf E", "conf-e")).await?;
    db.delete_category("Conf E").await?;
    ensure_eq(&row(db, "Conf E").await?, &None, "a delete removes the row")?;

    ensure_eq(
        &other_rows(db).await?,
        &seeded,
        "no other category row changed",
    )
}

/// A rename or a merge carries the dashboard transactions with it, so the
/// dashboard and the ledger never disagree on a category's name; a category a
/// dashboard transaction still names cannot be deleted.
pub async fn dashboard_transactions_follow_rename_and_merge(db: &dyn DatabaseAdapter) -> Outcome {
    let day = date(2027, 4, 4)?;
    db.insert_category(cap("Conf Tx", "conf-tx")).await?;
    db.insert_category(cap("Conf Sink", "conf-sink")).await?;
    let rid = ReceiptId::new();
    let r = Receipt {
        category: "Conf Tx".to_owned(),
        ..receipt(rid, "conf-tx-receipt", day, 900)
    };
    db.insert_receipt(r, Vec::new()).await?;
    let names =
        |txs: Vec<Transaction>| -> Vec<String> { txs.into_iter().map(|t| t.category).collect() };

    db.rename_category("Conf Tx", "Conf Tx2").await?;
    ensure_eq(
        &names(db.transactions_between(day, day).await?),
        &vec!["Conf Tx2".to_owned()],
        "a rename re-points the projected transaction",
    )?;

    let moved = db.merge_categories("Conf Tx2", "Conf Sink").await?;
    ensure_eq(&moved, &1, "the projection is not counted as a moved row")?;
    ensure_eq(
        &names(db.transactions_between(day, day).await?),
        &vec!["Conf Sink".to_owned()],
        "a merge re-points the projected transaction",
    )?;
    ensure_eq(
        &db.receipt(rid).await?.category,
        &"Conf Sink".to_owned(),
        "the receipt moved too",
    )?;
    ensure_invalid(
        db.delete_category("Conf Sink").await,
        "delete a category that still carries spend",
    )
}
