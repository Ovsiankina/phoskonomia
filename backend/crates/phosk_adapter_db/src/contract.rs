//! `contract` — the reusable PORT-conformance suite (ADR: memory + surreal kept
//! in lockstep).
//!
//! Every concrete [`DatabaseAdapter`](crate::DatabaseAdapter) must obey the same
//! observable behaviour. Rather than copy assertions into each adapter's tests,
//! this module exposes one suite that runs against **any** `&dyn DatabaseAdapter`
//! already loaded with the **deterministic Swiss seed** (the May/June 2026
//! cycles, eight category caps, six subscriptions, four debts, four IOUs, …).
//! `phosk_db_memory` and `phosk_db_surreal` each build their seeded adapter and
//! call [`run_all`]; if both pass, they are behaviourally identical on the seed.
//!
//! The suite is **read-and-write**: it asserts seed invariants (counts, exact
//! centime totals, slug lookups, relation wiring) AND exercises every write path
//! (insert/upsert/update/delete/record), then re-reads to prove the mutation
//! landed. Each check returns `Result<(), String>` so a failure names itself; the
//! caller (an adapter's `#[tokio::test]`) maps any `Err` to a test failure.
//!
//! Enabled by the `contract` feature so the bare port crate ships no test surface.

#![allow(clippy::missing_panics_doc, clippy::too_many_lines)]

use chrono::NaiveDate;
use phosk_core::money::Money;

use crate::DatabaseAdapter;

/// Build a [`NaiveDate`] or return a descriptive error (no panic in the suite).
fn d(y: i32, m: u32, day: u32) -> Result<NaiveDate, String> {
    NaiveDate::from_ymd_opt(y, m, day).ok_or_else(|| format!("bad test date {y}-{m:02}-{day:02}"))
}

/// Assert `cond`, attaching `msg` on failure.
fn check(cond: bool, msg: impl Into<String>) -> Result<(), String> {
    if cond { Ok(()) } else { Err(msg.into()) }
}

/// Sum a slice of [`Money`] into centimes, surfacing overflow as a `String`.
fn total_cents(amounts: impl IntoIterator<Item = Money>) -> Result<i64, String> {
    Money::sum(amounts)
        .map(Money::centimes)
        .map_err(|e| e.to_string())
}

/// Run the **entire** conformance suite against a seeded adapter.
///
/// `db` must already hold the deterministic Swiss seed (e.g. via the adapter's
/// `seeded()` constructor). The write-path checks mutate `db`, so pass a fresh
/// seeded instance.
///
/// # Errors
/// Returns the first failing assertion's message; `Ok(())` means full parity.
pub async fn run_all(db: &dyn DatabaseAdapter) -> Result<(), String> {
    dashboard_slice(db).await?;
    ledger(db).await?;
    signals(db).await?;
    planning(db).await?;
    recurring(db).await?;
    debts(db).await?;
    settings(db).await?;
    ai(db).await?;
    chat_seed_order(db).await?;
    write_paths(db).await?;
    chat_append_order(db).await?;
    Ok(())
}

/// Dashboard trio: transactions windows, categories, budget config.
async fn dashboard_slice(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let june = db
        .transactions_between(d(2026, 6, 1)?, d(2026, 6, 30)?)
        .await
        .map_err(|e| e.to_string())?;
    check(
        june.len() == 28,
        format!("June window: 28 expected, got {}", june.len()),
    )?;
    check(
        total_cents(june.iter().map(|t| t.amount))? == 322_245,
        "June total must be CHF 3222.45",
    )?;

    let may = db
        .transactions_between(d(2026, 5, 1)?, d(2026, 5, 31)?)
        .await
        .map_err(|e| e.to_string())?;
    check(
        may.len() == 39,
        format!("May window: 39 expected, got {}", may.len()),
    )?;
    check(
        total_cents(may.iter().map(|t| t.amount))? == 378_770,
        "May total must be CHF 3787.70",
    )?;

    // Inclusive single-day window.
    let day1 = db
        .transactions_between(d(2026, 6, 1)?, d(2026, 6, 1)?)
        .await
        .map_err(|e| e.to_string())?;
    check(day1.len() == 3, "June 1 has 3 line-items")?;
    check(
        total_cents(day1.iter().map(|t| t.amount))? == 205_675,
        "June 1 total CHF 2056.75",
    )?;

    // Empty + inverted windows.
    let april = db
        .transactions_between(d(2026, 4, 1)?, d(2026, 4, 30)?)
        .await
        .map_err(|e| e.to_string())?;
    check(april.is_empty(), "April window must be empty")?;
    let inverted = db
        .transactions_between(d(2026, 6, 30)?, d(2026, 6, 1)?)
        .await;
    check(inverted.is_err(), "inverted window must be rejected")?;

    let cats = db.categories().await.map_err(|e| e.to_string())?;
    check(
        cats.len() == 8,
        format!("8 categories expected, got {}", cats.len()),
    )?;
    let groceries = cats
        .iter()
        .find(|c| c.name == "GROCERIES")
        .ok_or("GROCERIES category missing")?;
    check(
        groceries.cap.map(Money::centimes) == Some(80_000),
        "GROCERIES cap must be CHF 800.00",
    )?;

    let cfg = db.budget_config().await.map_err(|e| e.to_string())?;
    check(
        cfg.monthly_budget.centimes() == 420_000,
        "monthly budget CHF 4200",
    )?;
    check(
        cfg.savings_target.centimes() == 90_000,
        "savings target CHF 900",
    )?;
    Ok(())
}

/// Ledger: receipts (count, by-slug, by-id), line items + relation wiring.
async fn ledger(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let all = db.all_receipts().await.map_err(|e| e.to_string())?;
    check(
        all.len() == 9,
        format!("9 seed receipts expected, got {}", all.len()),
    )?;

    let t8 = db.receipt_by_slug("t8").await.map_err(|e| e.to_string())?;
    check(t8.amount.centimes() == 168_000, "t8 (rent) is CHF 1680")?;
    check(t8.fixed, "t8 is a fixed charge")?;

    // by-id round-trips to the same record as by-slug.
    let t8_by_id = db.receipt(t8.id).await.map_err(|e| e.to_string())?;
    check(
        t8_by_id.slug == "t8",
        "receipt(id) resolves the same record as by-slug",
    )?;

    // A missing slug is NotFound.
    check(
        db.receipt_by_slug("nope").await.is_err(),
        "missing slug is NotFound",
    )?;

    // t1 has 4 line items all bound to its id.
    let t1 = db.receipt_by_slug("t1").await.map_err(|e| e.to_string())?;
    let lines = db.line_items(t1.id).await.map_err(|e| e.to_string())?;
    check(
        lines.len() == 4,
        format!("t1 has 4 lines, got {}", lines.len()),
    )?;
    check(
        lines.iter().all(|l| l.receipt_id == t1.id),
        "all t1 lines bound to t1",
    )?;

    // receipts_between filters by date.
    let in_june = db
        .receipts_between(d(2026, 6, 1)?, d(2026, 6, 30)?)
        .await
        .map_err(|e| e.to_string())?;
    check(in_june.len() == 9, "all 9 seed receipts fall in June")?;
    let earlier = db
        .receipts_between(d(2026, 1, 1)?, d(2026, 1, 31)?)
        .await
        .map_err(|e| e.to_string())?;
    check(earlier.is_empty(), "no seed receipts in January")?;
    Ok(())
}

/// Signals: five total, four tracked, slug lookup, occurrences, track/delete.
async fn signals(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let sigs = db.signals().await.map_err(|e| e.to_string())?;
    check(
        sigs.len() == 5,
        format!("5 signals expected, got {}", sigs.len()),
    )?;
    let tracked = sigs.iter().filter(|s| s.tracked).count();
    check(tracked == 4, format!("4 tracked signals, got {tracked}"))?;

    let coffee = db
        .signal_by_slug("coffee")
        .await
        .map_err(|e| e.to_string())?;
    check(coffee.label == "Oat-milk flat white", "coffee label")?;
    check(coffee.delta_pct == 28, "coffee delta_pct is +28")?;

    let occ = db
        .signal_occurrences(coffee.id)
        .await
        .map_err(|e| e.to_string())?;
    check(occ.len() == 1, "one rolled-up coffee occurrence")?;
    check(
        occ[0].amount.centimes() == 8_960,
        "coffee cycle spend CHF 89.60",
    )?;
    Ok(())
}

/// Planning: category caps, history rows, alerts.
async fn planning(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let caps = db.category_caps().await.map_err(|e| e.to_string())?;
    check(
        caps.len() == 8,
        format!("8 category caps, got {}", caps.len()),
    )?;
    let rent = db
        .category_cap_by_name("Rent")
        .await
        .map_err(|e| e.to_string())?;
    check(rent.fixed, "Rent cap is fixed")?;
    check(
        rent.cap.map(Money::centimes) == Some(168_000),
        "Rent cap CHF 1680",
    )?;

    let hist = db
        .budget_history("Groceries")
        .await
        .map_err(|e| e.to_string())?;
    check(
        hist.len() == 6,
        format!("6 history rows for Groceries, got {}", hist.len()),
    )?;
    check(
        hist.iter().any(|h| h.spent.centimes() == 88_000),
        "Groceries history includes the CHF 880 cycle",
    )?;

    let alerts = db.alerts().await.map_err(|e| e.to_string())?;
    check(alerts.len() == 3, format!("3 alerts, got {}", alerts.len()))?;
    check(
        alerts.iter().all(|a| a.status == "active"),
        "all seed alerts active",
    )?;
    Ok(())
}

/// Recurring: subscriptions, by-slug, charges.
async fn recurring(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let subs = db.subscriptions().await.map_err(|e| e.to_string())?;
    check(
        subs.len() == 6,
        format!("6 subscriptions, got {}", subs.len()),
    )?;
    let netflix = db
        .subscription_by_slug("netflix")
        .await
        .map_err(|e| e.to_string())?;
    check(netflix.amount.centimes() == 1_990, "Netflix CHF 19.90")?;
    let charges = db
        .subscription_charges(netflix.id)
        .await
        .map_err(|e| e.to_string())?;
    check(
        charges.len() == 3,
        format!("3 Netflix charges, got {}", charges.len()),
    )?;
    Ok(())
}

/// Debts: four debts, by-slug, payments, four IOUs.
async fn debts(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let debts = db.debts().await.map_err(|e| e.to_string())?;
    check(debts.len() == 4, format!("4 debts, got {}", debts.len()))?;
    let vw = db.debt_by_slug("vw").await.map_err(|e| e.to_string())?;
    check(vw.balance.centimes() == 1_820_000, "VW balance CHF 18200")?;
    let pays = db.debt_payments(vw.id).await.map_err(|e| e.to_string())?;
    check(pays.len() == 1, "one seed payment for VW")?;

    let ious = db.personal_ious().await.map_err(|e| e.to_string())?;
    check(ious.len() == 4, format!("4 IOUs, got {}", ious.len()))?;
    Ok(())
}

/// Settings: preferences, keyed lookup, modified-provenance count.
async fn settings(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let prefs = db.preferences().await.map_err(|e| e.to_string())?;
    check(
        prefs.len() == 5,
        format!("5 preferences, got {}", prefs.len()),
    )?;
    let baseline = db
        .preference("momentum_baseline_cycles")
        .await
        .map_err(|e| e.to_string())?;
    check(baseline.value == "3", "momentum baseline is 3")?;
    check(
        db.preference("does_not_exist").await.is_err(),
        "missing pref is NotFound",
    )?;
    Ok(())
}

/// AI: feed items, chat transcript, suggestions.
async fn ai(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let feed = db.feed_items().await.map_err(|e| e.to_string())?;
    check(feed.len() == 3, format!("3 feed items, got {}", feed.len()))?;

    let chat = db
        .latest_chat()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("a seed chat must exist")?;
    let msgs = db.chat_messages(chat.id).await.map_err(|e| e.to_string())?;
    check(
        msgs.len() == 2,
        format!("2 chat messages, got {}", msgs.len()),
    )?;

    let sugg = db.ai_suggestions().await.map_err(|e| e.to_string())?;
    check(
        sugg.len() == 2,
        format!("2 suggestions, got {}", sugg.len()),
    )?;
    Ok(())
}

/// Exercise every write path, then re-read to prove the mutation persisted.
async fn write_paths(db: &dyn DatabaseAdapter) -> Result<(), String> {
    // set_category_cap → cap changes + provenance flips to UserModified.
    db.set_category_cap("Transport", Some(Money::from_centimes(20_000)))
        .await
        .map_err(|e| e.to_string())?;
    let transport = db
        .category_cap_by_name("Transport")
        .await
        .map_err(|e| e.to_string())?;
    check(
        transport.cap.map(Money::centimes) == Some(20_000),
        "Transport cap updated",
    )?;

    // set_signal_tracked: promote the candidate, then delete it.
    let candidate = db
        .signal_by_slug("energy-drink")
        .await
        .map_err(|e| e.to_string())?;
    check(!candidate.tracked, "energy-drink starts untracked")?;
    db.set_signal_tracked(candidate.id, true)
        .await
        .map_err(|e| e.to_string())?;
    let promoted = db
        .signal_by_slug("energy-drink")
        .await
        .map_err(|e| e.to_string())?;
    check(promoted.tracked, "energy-drink now tracked")?;
    db.delete_signal(candidate.id)
        .await
        .map_err(|e| e.to_string())?;
    check(
        db.signal_by_slug("energy-drink").await.is_err(),
        "deleted signal is gone",
    )?;
    check(
        db.delete_signal(candidate.id).await.is_err(),
        "re-delete is NotFound",
    )?;

    // update_alert_status.
    let alert = db.alerts().await.map_err(|e| e.to_string())?.remove(0);
    db.update_alert_status(alert.id, "dismissed")
        .await
        .map_err(|e| e.to_string())?;
    let back = db.alert(alert.id).await.map_err(|e| e.to_string())?;
    check(back.status == "dismissed", "alert status updated")?;

    // update_suggestion_status.
    let sugg = db
        .ai_suggestions()
        .await
        .map_err(|e| e.to_string())?
        .remove(0);
    db.update_suggestion_status(sugg.id, "accepted")
        .await
        .map_err(|e| e.to_string())?;
    let suggs = db.ai_suggestions().await.map_err(|e| e.to_string())?;
    check(
        suggs
            .iter()
            .any(|s| s.id == sugg.id && s.status == "accepted"),
        "suggestion status updated",
    )?;

    // set_preference creates/updates; reset_preference reverts provenance.
    db.set_preference("currency", "EUR")
        .await
        .map_err(|e| e.to_string())?;
    let cur = db.preference("currency").await.map_err(|e| e.to_string())?;
    check(cur.value == "EUR", "preference updated")?;
    db.set_preference("brand_new_key", "x")
        .await
        .map_err(|e| e.to_string())?;
    check(
        db.preference("brand_new_key")
            .await
            .map_err(|e| e.to_string())?
            .value
            == "x",
        "new preference created",
    )?;

    // append_message → clear_chat.
    let chat = db
        .latest_chat()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("seed chat exists")?;
    db.append_message(phosk_model::Message {
        id: phosk_id::MessageId::new(),
        chat_id: chat.id,
        who: "usr".to_owned(),
        text: "test".to_owned(),
        at: d(2026, 6, 19)?,
    })
    .await
    .map_err(|e| e.to_string())?;
    check(
        db.chat_messages(chat.id)
            .await
            .map_err(|e| e.to_string())?
            .len()
            == 3,
        "message appended",
    )?;
    db.clear_chat(chat.id).await.map_err(|e| e.to_string())?;
    check(
        db.chat_messages(chat.id)
            .await
            .map_err(|e| e.to_string())?
            .is_empty(),
        "chat cleared",
    )?;

    // dismiss_feed_item by stringified id.
    let feed = db.feed_items().await.map_err(|e| e.to_string())?;
    let first_id = feed[0].id.to_string();
    db.dismiss_feed_item(&first_id)
        .await
        .map_err(|e| e.to_string())?;
    check(
        db.feed_items().await.map_err(|e| e.to_string())?.len() == 2,
        "feed item dismissed",
    )?;
    check(
        db.dismiss_feed_item(&first_id).await.is_err(),
        "re-dismiss is NotFound",
    )?;

    // insert_receipt fresh slug appends; same slug replaces in place.
    let before = db.all_receipts().await.map_err(|e| e.to_string())?.len();
    let rid = phosk_id::ReceiptId::new();
    let receipt = sample_receipt(rid, "ctrct1", 1_234);
    let returned = db
        .insert_receipt(receipt, vec![sample_line(rid, "Apples", 1_234)])
        .await
        .map_err(|e| e.to_string())?;
    check(returned == rid, "insert returns the receipt id")?;
    check(
        db.all_receipts().await.map_err(|e| e.to_string())?.len() == before + 1,
        "receipt appended",
    )?;
    let got = db
        .receipt_by_slug("ctrct1")
        .await
        .map_err(|e| e.to_string())?;
    check(got.amount.centimes() == 1_234, "inserted amount readable")?;
    check(
        db.line_items(rid).await.map_err(|e| e.to_string())?.len() == 1,
        "inserted line readable",
    )?;

    // Same-slug re-import: stable id, replaced payload + lines, no duplicate.
    let rid2 = phosk_id::ReceiptId::new();
    let replacement = sample_receipt(rid2, "ctrct1", 9_999);
    let stable = db
        .insert_receipt(
            replacement,
            vec![
                sample_line(rid2, "New A", 4_000),
                sample_line(rid2, "New B", 5_999),
            ],
        )
        .await
        .map_err(|e| e.to_string())?;
    check(stable == rid, "re-import keeps the original stored id")?;
    check(
        db.all_receipts().await.map_err(|e| e.to_string())?.len() == before + 1,
        "re-import does not duplicate",
    )?;
    let got2 = db
        .receipt_by_slug("ctrct1")
        .await
        .map_err(|e| e.to_string())?;
    check(got2.amount.centimes() == 9_999, "amount replaced")?;
    let lines2 = db.line_items(rid).await.map_err(|e| e.to_string())?;
    check(lines2.len() == 2, "lines replaced with the new set")?;
    check(
        lines2.iter().all(|l| l.receipt_id == rid),
        "lines rebound to stable id",
    )?;

    // update_line_item.
    let mut edited = lines2[0].clone();
    "Edited".clone_into(&mut edited.name);
    db.update_line_item(edited.clone())
        .await
        .map_err(|e| e.to_string())?;
    let reread = db.line_items(rid).await.map_err(|e| e.to_string())?;
    check(
        reread
            .iter()
            .any(|l| l.id == edited.id && l.name == "Edited"),
        "line item updated",
    )?;

    // upsert_subscription / record_charge.
    let mut sub = db
        .subscription_by_slug("netflix")
        .await
        .map_err(|e| e.to_string())?;
    sub.amount = Money::from_centimes(2_490);
    db.upsert_subscription(sub.clone())
        .await
        .map_err(|e| e.to_string())?;
    check(
        db.subscription(sub.id)
            .await
            .map_err(|e| e.to_string())?
            .amount
            .centimes()
            == 2_490,
        "subscription upserted",
    )?;
    db.record_charge(phosk_model::Charge {
        id: phosk_id::ChargeId::new(),
        subscription_id: sub.id,
        date: d(2026, 6, 1)?,
        amount: sub.amount,
        note: "test".to_owned(),
        provenance: phosk_model::Provenance::user_entered(),
    })
    .await
    .map_err(|e| e.to_string())?;
    check(
        db.subscription_charges(sub.id)
            .await
            .map_err(|e| e.to_string())?
            .len()
            == 4,
        "charge recorded",
    )?;

    // upsert_debt / record_debt_payment.
    let mut debt = db.debt_by_slug("vw").await.map_err(|e| e.to_string())?;
    debt.balance = Money::from_centimes(1_700_000);
    db.upsert_debt(debt.clone())
        .await
        .map_err(|e| e.to_string())?;
    check(
        db.debt(debt.id)
            .await
            .map_err(|e| e.to_string())?
            .balance
            .centimes()
            == 1_700_000,
        "debt upserted",
    )?;
    db.record_debt_payment(phosk_model::DebtPayment {
        id: phosk_id::PaymentId::new(),
        debt_id: debt.id,
        date: d(2026, 6, 1)?,
        amount: Money::from_centimes(45_000),
        balance_after: Money::from_centimes(1_655_000),
        provenance: phosk_model::Provenance::user_entered(),
    })
    .await
    .map_err(|e| e.to_string())?;
    check(
        db.debt_payments(debt.id)
            .await
            .map_err(|e| e.to_string())?
            .len()
            == 2,
        "debt payment recorded",
    )?;

    // upsert_personal_iou.
    let mut iou = db
        .personal_ious()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("a seed IOU exists")?;
    iou.amount = Money::from_centimes(13_000);
    db.upsert_personal_iou(iou.clone())
        .await
        .map_err(|e| e.to_string())?;
    check(
        db.personal_ious()
            .await
            .map_err(|e| e.to_string())?
            .iter()
            .any(|i| i.id == iou.id && i.amount.centimes() == 13_000),
        "IOU upserted",
    )?;

    // record_correction (write-only audit log; just must not error).
    db.record_correction(phosk_model::CorrectionEvent {
        id: phosk_id::CorrectionId::new(),
        entity_id: edited.id.to_string(),
        field: "name".to_owned(),
        old_value: "New A".to_owned(),
        new_value: "Edited".to_owned(),
        at: d(2026, 6, 19)?,
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn sample_receipt(id: phosk_id::ReceiptId, slug: &str, cents: i64) -> phosk_model::Receipt {
    phosk_model::Receipt {
        id,
        slug: slug.to_owned(),
        shop: "Migros".to_owned(),
        date: NaiveDate::from_ymd_opt(2026, 6, 20).unwrap_or_default(),
        category: "Groceries".to_owned(),
        amount: Money::from_centimes(cents),
        fixed: false,
        provenance: phosk_model::Provenance {
            source: phosk_model::Source::Ocr,
            confidence: 0.9,
        },
        source_kind: "PHOTO".to_owned(),
        ocr_engine: "PADDLEOCR".to_owned(),
        ocr_regions: 4,
    }
}

fn sample_line(receipt: phosk_id::ReceiptId, name: &str, cents: i64) -> phosk_model::LineItem {
    phosk_model::LineItem {
        id: phosk_id::LineItemId::new(),
        receipt_id: receipt,
        name: name.to_owned(),
        qty: 1.0,
        unit_price: Money::from_centimes(cents),
        line_total: Money::from_centimes(cents),
        category: "Groceries".to_owned(),
        signal_id: None,
        provenance: phosk_model::Provenance {
            source: phosk_model::Source::Ocr,
            confidence: 0.9,
        },
    }
}

/// `chat_messages` is ordered oldest→newest: the seeded transcript reads back
/// question first, answer second.
async fn chat_seed_order(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let chat = db
        .latest_chat()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("a seed chat must exist")?;
    let who: Vec<String> = db
        .chat_messages(chat.id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|m| m.who)
        .collect();
    check(
        who == ["usr", "sys"],
        format!("seed chat reads back usr then sys, got {who:?}"),
    )
}

/// `chat_messages` returns lines in append order, even though every line of a
/// turn carries the same `at` date and ids are random. Runs after
/// [`write_paths`] has cleared the seed chat.
async fn chat_append_order(db: &dyn DatabaseAdapter) -> Result<(), String> {
    let chat = db
        .latest_chat()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("a seed chat must exist")?;
    let at = d(2026, 6, 19)?;
    let sent: Vec<String> = (0..8).map(|i| format!("line {i}")).collect();
    for (i, text) in sent.iter().enumerate() {
        db.append_message(phosk_model::Message {
            id: phosk_id::MessageId::new(),
            chat_id: chat.id,
            who: if i % 2 == 0 { "usr" } else { "sys" }.to_owned(),
            text: text.clone(),
            at,
        })
        .await
        .map_err(|e| e.to_string())?;
    }
    let got: Vec<String> = db
        .chat_messages(chat.id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|m| m.text)
        .collect();
    check(
        got == sent,
        format!("chat lines read back in append order: sent {sent:?}, got {got:?}"),
    )?;
    db.clear_chat(chat.id).await.map_err(|e| e.to_string())
}
