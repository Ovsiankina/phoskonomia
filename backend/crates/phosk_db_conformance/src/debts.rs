//! Debts: institutional debts, their payments, and personal IOUs.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_id::{DebtId, PaymentId, PersonalIouId};
use phosk_model::{Debt, DebtPayment, PersonalIou, Provenance};

use crate::support::{Outcome, date, ensure, ensure_eq, ensure_not_found, first};

/// The list, the id lookup and the slug lookup return the same records;
/// unknown ids and slugs are `NotFound`.
pub async fn debts_lookup_by_id_and_slug_agree(db: &dyn DatabaseAdapter) -> Outcome {
    let debts = db.debts().await?;
    ensure_eq(&debts.len(), &4, "debt count")?;
    for d in &debts {
        ensure_eq(&db.debt(d.id).await?, d, "debt(id)")?;
        ensure_eq(&db.debt_by_slug(&d.slug).await?, d, "debt_by_slug")?;
    }
    ensure_not_found(db.debt(DebtId::new()).await, "debt(unknown)")?;
    ensure_not_found(db.debt_by_slug("conf-none").await, "debt_by_slug(unknown)")
}

/// An unknown id is inserted; a known id is replaced in place.
pub async fn upsert_debt_inserts_then_replaces(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.debts().await?;
    let debt = Debt {
        id: DebtId::new(),
        slug: "conf-debt".to_owned(),
        apr: 0.25,
        provenance: Provenance::user_entered(),
        ..first(before.clone())?
    };
    let id = db.upsert_debt(debt.clone()).await?;
    ensure_eq(&id, &debt.id, "upsert returns the id")?;
    ensure_eq(&db.debt(id).await?, &debt, "inserted")?;

    let debt = Debt {
        balance: Money::from_centimes(1_700_000),
        term: 36,
        provenance: Provenance::user_modified(),
        ..debt
    };
    db.upsert_debt(debt.clone()).await?;
    let count = db.debts().await?.len();
    ensure_eq(&count, &(before.len() + 1), "one new debt, no duplicate")?;
    ensure_eq(&db.debt_by_slug("conf-debt").await?, &debt, "replaced")?;
    for d in &before {
        ensure_eq(&db.debt(d.id).await?, d, "others untouched")?;
    }
    Ok(())
}

/// A deleted debt is gone from every read path, takes its payments with it,
/// leaves its neighbours alone, and cannot be deleted twice.
pub async fn delete_debt_removes_it_and_its_payments(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.debts().await?;
    let victim = first(before.clone())?;
    let keeper = before
        .get(1)
        .ok_or("the seed has more than one debt")?
        .clone();
    let keeper_payments = db.debt_payments(keeper.id).await?;
    db.record_debt_payment(DebtPayment {
        id: PaymentId::new(),
        debt_id: victim.id,
        date: date(2027, 5, 9)?,
        amount: Money::from_centimes(12_500),
        balance_after: Money::from_centimes(87_500),
        provenance: Provenance::user_entered(),
    })
    .await?;
    ensure(
        !db.debt_payments(victim.id).await?.is_empty(),
        "the victim has a payment history",
    )?;

    db.delete_debt(victim.id).await?;

    let after = db.debts().await?;
    ensure_eq(&after.len(), &(before.len() - 1), "count after delete")?;
    ensure(
        after.iter().all(|d| d.id != victim.id),
        "debts() no longer lists it",
    )?;
    ensure_not_found(db.debt(victim.id).await, "debt(deleted)")?;
    ensure_not_found(db.debt_by_slug(&victim.slug).await, "by slug")?;
    ensure(
        db.debt_payments(victim.id).await?.is_empty(),
        "its payments went with it",
    )?;
    ensure_eq(
        &db.debt_payments(keeper.id).await?,
        &keeper_payments,
        "another debt's payments untouched",
    )?;
    ensure_eq(&db.debt(keeper.id).await?, &keeper, "keeper intact")?;
    ensure_not_found(db.delete_debt(victim.id).await, "delete again")
}

/// Payments are scoped to their debt and come back oldest→newest.
pub async fn debt_payments_are_scoped_and_oldest_first(db: &dyn DatabaseAdapter) -> Outcome {
    let other = first(db.debts().await?)?;
    let other_count = db.debt_payments(other.id).await?.len();
    let debt = Debt {
        id: DebtId::new(),
        slug: "conf-paid".to_owned(),
        ..other.clone()
    };
    db.upsert_debt(debt.clone()).await?;

    let mut recorded = Vec::new();
    for month in 1..=4 {
        let p = DebtPayment {
            id: PaymentId::new(),
            debt_id: debt.id,
            date: date(2027, month, 25)?,
            amount: Money::from_centimes(10_000),
            balance_after: Money::from_centimes(100_000 - i64::from(month) * 10_000),
            provenance: Provenance::user_entered(),
        };
        db.record_debt_payment(p.clone()).await?;
        recorded.push(p);
    }

    let got = db.debt_payments(debt.id).await?;
    ensure_eq(&got, &recorded, "payments, oldest→newest")?;
    let untouched = db.debt_payments(other.id).await?.len();
    ensure_eq(&untouched, &other_count, "other debt's payments untouched")?;
    let none = db.debt_payments(DebtId::new()).await?;
    ensure(none.is_empty(), "unknown debt has no payments")
}

/// Personal IOUs: an unknown id is inserted, a known id replaced in place.
pub async fn upsert_personal_iou_inserts_then_replaces(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.personal_ious().await?;
    ensure_eq(&before.len(), &4, "IOU count")?;
    let iou = PersonalIou {
        id: PersonalIouId::new(),
        slug: "conf-iou".to_owned(),
        person: "Test Person".to_owned(),
        provenance: Provenance::user_entered(),
        ..first(before.clone())?
    };
    let id = db.upsert_personal_iou(iou.clone()).await?;
    ensure_eq(&id, &iou.id, "upsert returns the id")?;

    let iou = PersonalIou {
        amount: Money::from_centimes(4_500),
        provenance: Provenance::user_modified(),
        ..iou
    };
    db.upsert_personal_iou(iou.clone()).await?;
    let after = db.personal_ious().await?;
    ensure_eq(
        &after.len(),
        &(before.len() + 1),
        "one new IOU, no duplicate",
    )?;
    ensure(after.contains(&iou), "the replaced IOU is listed verbatim")?;
    let kept = before.iter().all(|i| after.contains(i));
    ensure(kept, "other IOUs untouched")
}

/// The list and the slug lookup return the same records; an unknown slug is
/// `NotFound`.
pub async fn personal_iou_lookup_by_slug_agrees(db: &dyn DatabaseAdapter) -> Outcome {
    let ious = db.personal_ious().await?;
    for i in &ious {
        ensure_eq(&db.personal_iou_by_slug(&i.slug).await?, i, "by slug")?;
    }
    ensure_not_found(
        db.personal_iou_by_slug("conf-none").await,
        "personal_iou_by_slug(unknown)",
    )
}

/// A deleted IOU is gone from every read path, leaves its neighbours alone, and
/// cannot be deleted twice.
pub async fn delete_personal_iou_removes_it(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.personal_ious().await?;
    let victim = first(before.clone())?;

    db.delete_personal_iou(victim.id).await?;

    let after = db.personal_ious().await?;
    ensure_eq(&after.len(), &(before.len() - 1), "count after delete")?;
    ensure(
        after.iter().all(|i| i.id != victim.id),
        "personal_ious() no longer lists it",
    )?;
    ensure_not_found(
        db.personal_iou_by_slug(&victim.slug).await,
        "by slug after delete",
    )?;
    let kept = before
        .iter()
        .filter(|i| i.id != victim.id)
        .all(|i| after.contains(i));
    ensure(kept, "other IOUs untouched")?;
    ensure_not_found(db.delete_personal_iou(victim.id).await, "delete again")
}
