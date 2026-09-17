//! Recurring: subscriptions and their charges.

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::money::Money;
use phosk_id::{ChargeId, SubscriptionId};
use phosk_model::{Charge, Provenance, Subscription};

use crate::support::{Outcome, date, ensure, ensure_eq, ensure_not_found, first};

/// The list, the id lookup and the slug lookup return the same records;
/// unknown ids and slugs are `NotFound`.
pub async fn subscriptions_lookup_by_id_and_slug_agree(db: &dyn DatabaseAdapter) -> Outcome {
    let subs = db.subscriptions().await?;
    ensure_eq(&subs.len(), &6, "subscription count")?;
    for s in &subs {
        ensure_eq(&db.subscription(s.id).await?, s, "subscription(id)")?;
        let by_slug = db.subscription_by_slug(&s.slug).await?;
        ensure_eq(&by_slug, s, "subscription_by_slug")?;
    }
    let unknown = db.subscription(SubscriptionId::new()).await;
    ensure_not_found(unknown, "subscription(unknown)")?;
    let unknown = db.subscription_by_slug("conf-none").await;
    ensure_not_found(unknown, "subscription_by_slug(unknown)")
}

/// An unknown id is inserted; a known id is replaced in place.
pub async fn upsert_subscription_inserts_then_replaces(db: &dyn DatabaseAdapter) -> Outcome {
    let before = db.subscriptions().await?;
    let sub = Subscription {
        id: SubscriptionId::new(),
        slug: "conf-sub".to_owned(),
        name: "Conformance Plus".to_owned(),
        provenance: Provenance::user_entered(),
        ..first(before.clone())?
    };
    let id = db.upsert_subscription(sub.clone()).await?;
    ensure_eq(&id, &sub.id, "upsert returns the id")?;
    let count = db.subscriptions().await?.len();
    ensure_eq(&count, &(before.len() + 1), "count after insert")?;
    ensure_eq(&db.subscription(id).await?, &sub, "inserted")?;

    let sub = Subscription {
        amount: Money::from_centimes(2_490),
        status: "paused".to_owned(),
        provenance: Provenance::user_modified(),
        ..sub
    };
    db.upsert_subscription(sub.clone()).await?;
    let count = db.subscriptions().await?.len();
    ensure_eq(&count, &(before.len() + 1), "count after replace")?;
    ensure_eq(
        &db.subscription_by_slug("conf-sub").await?,
        &sub,
        "replaced",
    )?;
    for s in &before {
        ensure_eq(&db.subscription(s.id).await?, s, "others untouched")?;
    }
    Ok(())
}

/// Charges are scoped to their subscription and come back oldest→newest.
pub async fn subscription_charges_are_scoped_and_oldest_first(db: &dyn DatabaseAdapter) -> Outcome {
    let other = first(db.subscriptions().await?)?;
    let other_count = db.subscription_charges(other.id).await?.len();
    let sub = Subscription {
        id: SubscriptionId::new(),
        slug: "conf-charges".to_owned(),
        ..other.clone()
    };
    db.upsert_subscription(sub.clone()).await?;

    let mut recorded = Vec::new();
    for (month, cents) in [(1, 1_990), (2, 1_990), (3, 2_490), (4, 2_490)] {
        let c = Charge {
            id: ChargeId::new(),
            subscription_id: sub.id,
            date: date(2027, month, 5)?,
            amount: Money::from_centimes(cents),
            note: "confirmed".to_owned(),
            provenance: Provenance::user_entered(),
        };
        db.record_charge(c.clone()).await?;
        recorded.push(c);
    }

    let got = db.subscription_charges(sub.id).await?;
    ensure_eq(&got, &recorded, "charges, oldest→newest")?;
    let untouched = db.subscription_charges(other.id).await?.len();
    ensure_eq(
        &untouched,
        &other_count,
        "other subscription's charges untouched",
    )?;
    let none = db.subscription_charges(SubscriptionId::new()).await?;
    ensure(none.is_empty(), "unknown subscription has no charges")
}
