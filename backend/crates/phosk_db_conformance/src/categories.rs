//! Categories: create, rename (re-pointing every reference) and
//! delete-if-empty.
//!
//! The category record is [`CategoryCap`] — the user-facing envelope whose
//! `name` is its identity and which receipts, line items, subscriptions and
//! signals reference *by that name*. The checks below assert the port's three
//! write behaviours without leaning on which seeded rows happen to carry a
//! given name: they count references before and after, so the same assertions
//! hold for any store that satisfies the seed contract.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_id::CategoryId;
use phosk_model::{CategoryCap, Provenance, Source};

use crate::support::{Outcome, ensure, ensure_eq, ensure_invalid, ensure_not_found};

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
