//! `seed` — the deterministic Swiss seed loader (mirror of
//! `phosk_db_memory`'s seed).
//!
//! It rebuilds the SAME fixture `phosk_db_memory::seeded()` produces — the
//! May/June 2026 transaction line-items, eight category caps, six subscriptions,
//! four debts, four IOUs, the signal set, alerts, preferences, AI feed/chat/
//! suggestions — and writes it through the [`Store`] into the embedded engine.
//! Because both adapters load identical data, the shared
//! [`contract`](phosk_adapter_db::contract) suite asserts identical behaviour
//! (ADR: memory + surreal kept in lockstep).
//!
//! The figures are duplicated here (rather than imported) on purpose: ADR-010
//! keeps concrete L3 adapters from depending on one another. The lockstep is
//! enforced by the contract suite, not by code sharing.
//!
//! Every fallible step (`NaiveDate::from_ymd_opt`, `Money::from_chf`) maps to a
//! [`PhoskError`]; there is no panic (ADR §0).

// Several builders return `Result` purely so they compose uniformly with the
// date-parsing fallible ones (keeping `load()` a single `?`-chain), even when an
// individual builder cannot currently fail — exactly as `phosk_db_memory::seed`.
#![allow(
    clippy::too_many_lines,
    clippy::unnecessary_wraps,
    clippy::type_complexity
)]

use std::collections::HashMap;

use chrono::NaiveDate;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{
    AlertId, CategoryId, ChargeId, ChatId, DebtId, FeedItemId, LineItemId, MessageId, PaymentId,
    PersonalIouId, PreferenceId, ReceiptId, SignalId, SubscriptionId, SuggestionId,
};
use phosk_model::{
    AiSuggestion, Alert, AlertAction, BudgetConfig, BudgetHistory, Category, CategoryCap, Charge,
    Chat, Debt, DebtPayment, FeedItem, LineItem, Message, PersonalIou, Preference, Provenance,
    Receipt, Signal, SignalOccurrence, Source, Subscription, Transaction,
};

use crate::store::{Bucket, Store};

/// Map a `(y, m, d)` triple to a [`NaiveDate`], surfacing an invalid date.
fn date(y: i32, m: u32, d: u32) -> Result<NaiveDate, PhoskError> {
    NaiveDate::from_ymd_opt(y, m, d)
        .ok_or_else(|| PhoskError::InvalidDate(format!("seed date {y}-{m:02}-{d:02}")))
}

/// A high-confidence OCR provenance (mirrors the memory seed).
const fn ocr() -> Provenance {
    Provenance {
        source: Source::Ocr,
        confidence: 0.94,
    }
}

/// Load the entire deterministic Swiss seed into `store`.
///
/// # Errors
/// [`PhoskError`] if any seed value is invalid or a write fails.
pub(crate) async fn load(store: &Store) -> Result<(), PhoskError> {
    // ── Dashboard trio ──────────────────────────────────────────────────────
    for t in transactions()? {
        // Transactions have no typed id; key on a content digest (date+shop+amt)
        // so a re-seed is idempotent and order-independent.
        let key = format!("{}|{}|{}", t.date, t.shop, t.amount.centimes());
        store.put(Bucket::Transaction, &key, &t).await?;
    }
    for c in categories()? {
        store.put(Bucket::Category, &c.name, &c).await?;
    }
    store
        .put(Bucket::BudgetConfig, "singleton", &budget()?)
        .await?;

    // ── Ledger ──────────────────────────────────────────────────────────────
    let (receipts, lines) = receipts_and_lines()?;
    for r in &receipts {
        store.put(Bucket::Receipt, &r.id.to_string(), r).await?;
    }
    for l in &lines {
        store
            .put_in_sequence(Bucket::LineItem, &l.id.to_string(), l)
            .await?;
    }

    // ── Signals ─────────────────────────────────────────────────────────────
    let signals = signals()?;
    for s in &signals {
        store.put(Bucket::Signal, &s.id.to_string(), s).await?;
    }
    for (i, o) in signal_occurrences(&signals)?.into_iter().enumerate() {
        // Occurrences have no own id; key on (signal, index).
        let key = format!("{}|{i}", o.signal_id);
        store.put(Bucket::SignalOccurrence, &key, &o).await?;
    }

    // ── Planning ────────────────────────────────────────────────────────────
    let caps = category_caps()?;
    for c in &caps {
        store.put(Bucket::CategoryCap, &c.id.to_string(), c).await?;
    }
    for (i, h) in budget_history(&caps)?.into_iter().enumerate() {
        let key = format!("{}|{i}", h.category_id);
        store.put(Bucket::BudgetHistory, &key, &h).await?;
    }
    for a in alerts()? {
        store.put(Bucket::Alert, &a.id.to_string(), &a).await?;
    }

    // ── Recurring ───────────────────────────────────────────────────────────
    let subs = subscriptions()?;
    for s in &subs {
        store
            .put(Bucket::Subscription, &s.id.to_string(), s)
            .await?;
    }
    for c in charges(&subs)? {
        store.put(Bucket::Charge, &c.id.to_string(), &c).await?;
    }

    // ── Debts ───────────────────────────────────────────────────────────────
    let debts = debts()?;
    for d in &debts {
        store.put(Bucket::Debt, &d.id.to_string(), d).await?;
    }
    for p in debt_payments(&debts)? {
        store
            .put(Bucket::DebtPayment, &p.id.to_string(), &p)
            .await?;
    }
    for i in personal_ious()? {
        store
            .put(Bucket::PersonalIou, &i.id.to_string(), &i)
            .await?;
    }

    // ── Settings ────────────────────────────────────────────────────────────
    for p in preferences() {
        // Preferences are keyed on their natural `key` (the port `preference`
        // method looks them up by it).
        store.put(Bucket::Preference, &p.key.clone(), &p).await?;
    }

    // ── AI ──────────────────────────────────────────────────────────────────
    for f in feed_items()? {
        store.put(Bucket::FeedItem, &f.id.to_string(), &f).await?;
    }
    let (chats, messages) = chats_and_messages()?;
    for c in &chats {
        store.put(Bucket::Chat, &c.id.to_string(), c).await?;
    }
    // In transcript order: chat messages are read back by insertion sequence.
    for m in messages {
        store
            .put_in_sequence(Bucket::Message, &m.id.to_string(), &m)
            .await?;
    }
    for s in ai_suggestions()? {
        store
            .put(Bucket::AiSuggestion, &s.id.to_string(), &s)
            .await?;
    }

    Ok(())
}

// ── Dashboard builders (mirror phosk_db_memory::lib) ───────────────────────────

fn tx(
    year: i32,
    month: u32,
    day: u32,
    shop: &str,
    category: &str,
    chf: i64,
    cents: u8,
) -> Result<Transaction, PhoskError> {
    Ok(Transaction {
        date: date(year, month, day)?,
        shop: shop.to_owned(),
        category: category.to_owned(),
        amount: Money::from_chf(chf, cents)?,
    })
}

fn category(name: &str, chf: i64, cents: u8) -> Result<Category, PhoskError> {
    Ok(Category {
        name: name.to_owned(),
        cap: Some(Money::from_chf(chf, cents)?),
    })
}

fn categories() -> Result<Vec<Category>, PhoskError> {
    Ok(vec![
        category("GROCERIES", 800, 0)?,
        category("DINING & CAFÉS", 350, 0)?,
        category("HOUSING", 1680, 0)?,
        category("HEALTH", 520, 0)?,
        category("TRANSPORT", 280, 0)?,
        category("SUBSCRIPTIONS", 190, 0)?,
        category("UTILITIES", 240, 0)?,
        category("HOUSEHOLD", 200, 0)?,
    ])
}

fn budget() -> Result<BudgetConfig, PhoskError> {
    Ok(BudgetConfig {
        monthly_budget: Money::from_chf(4200, 0)?,
        savings_target: Money::from_chf(900, 0)?,
    })
}

fn transactions() -> Result<Vec<Transaction>, PhoskError> {
    Ok(vec![
        // June 2026 (28 line-items).
        tx(2026, 6, 1, "Migros", "GROCERIES", 58, 75)?,
        tx(2026, 6, 1, "Landlord", "HOUSING", 1680, 0)?,
        tx(2026, 6, 1, "Krankenkasse", "HEALTH", 318, 0)?,
        tx(2026, 6, 2, "Denner", "GROCERIES", 44, 30)?,
        tx(2026, 6, 2, "Coop", "GROCERIES", 35, 20)?,
        tx(2026, 6, 3, "SBB", "TRANSPORT", 18, 50)?,
        tx(2026, 6, 4, "Starbucks", "DINING & CAFÉS", 12, 80)?,
        tx(2026, 6, 5, "Netflix", "SUBSCRIPTIONS", 14, 95)?,
        tx(2026, 6, 5, "Activ Fitness", "SUBSCRIPTIONS", 89, 0)?,
        tx(2026, 6, 6, "Volg", "GROCERIES", 28, 60)?,
        tx(2026, 6, 7, "Apotheke", "HEALTH", 67, 50)?,
        tx(2026, 6, 8, "Coop", "GROCERIES", 92, 40)?,
        tx(2026, 6, 8, "Avec", "GROCERIES", 16, 80)?,
        tx(2026, 6, 9, "Restaurant Kreuz", "DINING & CAFÉS", 74, 30)?,
        tx(2026, 6, 10, "Manor", "HOUSEHOLD", 48, 0)?,
        tx(2026, 6, 11, "Migros", "GROCERIES", 61, 75)?,
        tx(2026, 6, 12, "Spotify", "SUBSCRIPTIONS", 12, 95)?,
        tx(2026, 6, 12, "Sunrise", "SUBSCRIPTIONS", 45, 0)?,
        tx(2026, 6, 13, "Denner", "GROCERIES", 36, 40)?,
        tx(2026, 6, 14, "Coop", "GROCERIES", 85, 20)?,
        tx(2026, 6, 15, "Apotheke", "HEALTH", 28, 40)?,
        tx(2026, 6, 16, "Galaxus", "HOUSEHOLD", 129, 90)?,
        tx(2026, 6, 16, "Restaurant Linde", "DINING & CAFÉS", 64, 50)?,
        tx(2026, 6, 17, "Swisscom", "SUBSCRIPTIONS", 79, 0)?,
        tx(2026, 6, 18, "Coop Pronto", "GROCERIES", 12, 40)?,
        tx(2026, 6, 18, "Starbucks", "DINING & CAFÉS", 7, 20)?,
        tx(2026, 6, 19, "Migros", "GROCERIES", 53, 85)?,
        tx(2026, 6, 19, "SBB", "TRANSPORT", 6, 80)?,
        // May 2026 (39 line-items).
        tx(2026, 5, 1, "Migros", "GROCERIES", 56, 30)?,
        tx(2026, 5, 1, "Landlord", "HOUSING", 1680, 0)?,
        tx(2026, 5, 1, "Krankenkasse", "HEALTH", 318, 0)?,
        tx(2026, 5, 2, "Denner", "GROCERIES", 42, 50)?,
        tx(2026, 5, 3, "SBB", "TRANSPORT", 21, 0)?,
        tx(2026, 5, 4, "Starbucks", "DINING & CAFÉS", 14, 20)?,
        tx(2026, 5, 5, "Netflix", "SUBSCRIPTIONS", 14, 95)?,
        tx(2026, 5, 5, "Activ Fitness", "SUBSCRIPTIONS", 89, 0)?,
        tx(2026, 5, 5, "Spotify", "SUBSCRIPTIONS", 12, 95)?,
        tx(2026, 5, 6, "Coop", "GROCERIES", 87, 60)?,
        tx(2026, 5, 7, "Apotheke", "HEALTH", 52, 80)?,
        tx(2026, 5, 8, "Avec", "GROCERIES", 14, 90)?,
        tx(2026, 5, 9, "Restaurant Kreuz", "DINING & CAFÉS", 74, 30)?,
        tx(2026, 5, 10, "Migros", "GROCERIES", 59, 40)?,
        tx(2026, 5, 11, "Manor", "HOUSEHOLD", 45, 50)?,
        tx(2026, 5, 12, "Denner", "GROCERIES", 38, 20)?,
        tx(2026, 5, 13, "Coop", "GROCERIES", 89, 30)?,
        tx(2026, 5, 14, "Apotheke", "HEALTH", 31, 60)?,
        tx(2026, 5, 15, "Swisscom", "SUBSCRIPTIONS", 79, 0)?,
        tx(2026, 5, 16, "Galaxus", "HOUSEHOLD", 110, 20)?,
        tx(2026, 5, 17, "Restaurant Linde", "DINING & CAFÉS", 62, 0)?,
        tx(2026, 5, 18, "Migros", "GROCERIES", 55, 80)?,
        tx(2026, 5, 19, "SBB", "TRANSPORT", 18, 50)?,
        tx(2026, 5, 20, "Starbucks", "DINING & CAFÉS", 13, 90)?,
        tx(2026, 5, 21, "Coop", "GROCERIES", 93, 10)?,
        tx(2026, 5, 22, "Sunrise", "SUBSCRIPTIONS", 45, 0)?,
        tx(2026, 5, 23, "Denner", "GROCERIES", 40, 80)?,
        tx(2026, 5, 24, "Apotheke", "HEALTH", 36, 20)?,
        tx(2026, 5, 25, "Manor", "HOUSEHOLD", 50, 80)?,
        tx(2026, 5, 26, "Avec", "GROCERIES", 16, 50)?,
        tx(2026, 5, 26, "Starbucks", "DINING & CAFÉS", 15, 40)?,
        tx(2026, 5, 27, "Migros", "GROCERIES", 64, 20)?,
        tx(2026, 5, 28, "Restaurant Kreuz", "DINING & CAFÉS", 64, 50)?,
        tx(2026, 5, 29, "Coop", "GROCERIES", 81, 50)?,
        tx(2026, 5, 29, "SBB", "TRANSPORT", 21, 0)?,
        tx(2026, 5, 30, "Apotheke", "HEALTH", 24, 0)?,
        tx(2026, 5, 3, "Electricity/Gas provider", "UTILITIES", 89, 20)?,
        tx(2026, 5, 15, "Internet provider", "UTILITIES", 32, 30)?,
        tx(2026, 5, 27, "Water/Sewage", "UTILITIES", 31, 30)?,
    ])
}

// ── Ledger builders (mirror phosk_db_memory::seed) ─────────────────────────────

fn receipts_and_lines() -> Result<(Vec<Receipt>, Vec<LineItem>), PhoskError> {
    let rows: [(&str, i32, u32, u32, &str, &str, i64, bool); 9] = [
        ("t1", 2026, 6, 16, "Migros", "Groceries", 5_875, false),
        (
            "t2",
            2026,
            6,
            16,
            "Restaurant Linde",
            "Going out",
            6_450,
            false,
        ),
        ("t3", 2026, 6, 13, "Coop", "Groceries", 4_230, false),
        ("t4", 2026, 6, 13, "Galaxus", "Shopping", 12_990, false),
        ("t5", 2026, 6, 11, "Migros", "Coffee & snacks", 1_280, false),
        ("t6", 2026, 6, 9, "Denner", "Groceries", 2_990, false),
        ("t7", 2026, 6, 7, "SBB", "Transport", 3_400, false),
        ("t8", 2026, 6, 1, "Landlord", "Rent", 168_000, true),
        (
            "t9",
            2026,
            6,
            1,
            "Helsana",
            "Health insurance",
            31_800,
            true,
        ),
    ];

    let mut receipts = Vec::with_capacity(rows.len());
    let mut by_slug: HashMap<&str, ReceiptId> = HashMap::new();
    for (slug, y, m, d, shop, cat, cents, fixed) in rows {
        let id = ReceiptId::new();
        by_slug.insert(slug, id);
        let photo = !fixed;
        receipts.push(Receipt {
            id,
            slug: slug.to_owned(),
            shop: shop.to_owned(),
            date: date(y, m, d)?,
            category: cat.to_owned(),
            amount: Money::from_centimes(cents),
            fixed,
            provenance: if fixed {
                Provenance::user_entered()
            } else {
                ocr()
            },
            source_kind: if photo {
                "PHOTO".to_owned()
            } else {
                "MANUAL".to_owned()
            },
            ocr_engine: if photo {
                "PADDLEOCR".to_owned()
            } else {
                String::new()
            },
            ocr_regions: if photo { 12 } else { 0 },
        });
    }

    let mut lines = Vec::new();
    let mut line = |receipt_slug: &str,
                    name: &str,
                    qty: f64,
                    unit_cents: i64,
                    total_cents: i64,
                    cat: &str,
                    conf: f64|
     -> Result<(), PhoskError> {
        let receipt_id = *by_slug
            .get(receipt_slug)
            .ok_or_else(|| PhoskError::NotFound(format!("seed receipt {receipt_slug}")))?;
        lines.push(LineItem {
            id: LineItemId::new(),
            receipt_id,
            name: name.to_owned(),
            qty,
            unit_price: Money::from_centimes(unit_cents),
            line_total: Money::from_centimes(total_cents),
            category: cat.to_owned(),
            signal_id: None,
            provenance: Provenance {
                source: Source::Ocr,
                confidence: conf,
            },
        });
        Ok(())
    };

    line(
        "t1",
        "Oat-milk flat white",
        1.0,
        560,
        560,
        "Coffee & snacks",
        0.88,
    )?;
    line("t1", "Bananas", 1.2, 320, 384, "Groceries", 0.91)?;
    line("t1", "Bread (unclear)", 1.0, 431, 431, "Groceries", 0.58)?;
    line("t1", "Mixed basket", 1.0, 4_500, 4_500, "Groceries", 0.95)?;
    line("t2", "Craft IPA", 2.0, 720, 1_440, "Going out", 0.90)?;
    line("t2", "Main course", 1.0, 5_010, 5_010, "Going out", 0.93)?;
    line("t3", "Gruyère AOP", 0.4, 6_600, 2_640, "Groceries", 0.92)?;
    line("t3", "Vegetables", 1.0, 1_590, 1_590, "Groceries", 0.89)?;
    line(
        "t5",
        "Oat-milk flat white",
        1.0,
        560,
        560,
        "Coffee & snacks",
        0.87,
    )?;
    line(
        "t5",
        "Pain au chocolat",
        2.0,
        360,
        720,
        "Coffee & snacks",
        0.84,
    )?;

    Ok((receipts, lines))
}

// ── Signals ────────────────────────────────────────────────────────────────────

fn signals() -> Result<Vec<Signal>, PhoskError> {
    Ok(vec![
        Signal {
            id: SignalId::new(),
            slug: "coffee".to_owned(),
            label: "Oat-milk flat white".to_owned(),
            parent: "Coffee & snacks".to_owned(),
            unit: "cups".to_owned(),
            since: date(2026, 3, 1)?,
            tracked: true,
            desc: String::new(),
            series: vec![
                6.0, 7.0, 9.0, 8.0, 11.0, 10.0, 12.0, 11.0, 13.0, 12.0, 14.0, 16.0,
            ],
            delta_pct: 28,
            txns: 12,
            provenance: Provenance::user_entered(),
        },
        Signal {
            id: SignalId::new(),
            slug: "pain".to_owned(),
            label: "Pain au chocolat".to_owned(),
            parent: "Coffee & snacks".to_owned(),
            unit: "pcs".to_owned(),
            since: date(2026, 1, 1)?,
            tracked: true,
            desc: String::new(),
            series: vec![4.0, 5.0, 5.0, 6.0, 5.0, 7.0, 6.0, 8.0, 7.0, 9.0, 8.0, 9.0],
            delta_pct: 12,
            txns: 7,
            provenance: Provenance::user_entered(),
        },
        Signal {
            id: SignalId::new(),
            slug: "beer".to_owned(),
            label: "Craft IPA".to_owned(),
            parent: "Going out".to_owned(),
            unit: "bottles".to_owned(),
            since: date(2026, 2, 1)?,
            tracked: true,
            desc: String::new(),
            series: vec![9.0, 8.0, 10.0, 7.0, 9.0, 6.0, 8.0, 5.0, 7.0, 6.0, 5.0, 4.0],
            delta_pct: -22,
            txns: 3,
            provenance: Provenance::user_entered(),
        },
        Signal {
            id: SignalId::new(),
            slug: "gruyere".to_owned(),
            label: "Gruyère AOP".to_owned(),
            parent: "Groceries".to_owned(),
            unit: "kg".to_owned(),
            since: date(2025, 12, 1)?,
            tracked: true,
            desc: String::new(),
            series: vec![2.0, 3.0, 2.0, 4.0, 3.0, 3.0, 4.0, 3.0, 5.0, 4.0, 4.0, 5.0],
            delta_pct: 7,
            txns: 4,
            provenance: Provenance::user_entered(),
        },
        Signal {
            id: SignalId::new(),
            slug: "energy-drink".to_owned(),
            label: "Energy drinks".to_owned(),
            parent: "Groceries".to_owned(),
            unit: "cans".to_owned(),
            since: date(2026, 6, 1)?,
            tracked: false,
            desc: "Showing up 3× this cycle — track it?".to_owned(),
            series: vec![1.0, 2.0, 4.0, 6.0],
            delta_pct: 0,
            txns: 3,
            provenance: Provenance {
                source: Source::LlmInferred,
                confidence: 0.66,
            },
        },
    ])
}

fn signal_occurrences(signals: &[Signal]) -> Result<Vec<SignalOccurrence>, PhoskError> {
    let id_of = |slug: &str| -> Result<SignalId, PhoskError> {
        signals
            .iter()
            .find(|s| s.slug == slug)
            .map(|s| s.id)
            .ok_or_else(|| PhoskError::NotFound(format!("seed signal {slug}")))
    };
    let d = date(2026, 6, 16)?;
    Ok(vec![
        SignalOccurrence {
            signal_id: id_of("coffee")?,
            line_item_id: LineItemId::new(),
            date: d,
            shop: "Migros".to_owned(),
            qty: 16.0,
            amount: Money::from_centimes(8_960),
        },
        SignalOccurrence {
            signal_id: id_of("pain")?,
            line_item_id: LineItemId::new(),
            date: d,
            shop: "Migros".to_owned(),
            qty: 9.0,
            amount: Money::from_centimes(3_150),
        },
        SignalOccurrence {
            signal_id: id_of("beer")?,
            line_item_id: LineItemId::new(),
            date: d,
            shop: "Restaurant Linde".to_owned(),
            qty: 4.0,
            amount: Money::from_centimes(2_880),
        },
        SignalOccurrence {
            signal_id: id_of("gruyere")?,
            line_item_id: LineItemId::new(),
            date: date(2026, 6, 13)?,
            shop: "Coop".to_owned(),
            qty: 1.0,
            amount: Money::from_centimes(2_640),
        },
        SignalOccurrence {
            signal_id: id_of("energy-drink")?,
            line_item_id: LineItemId::new(),
            date: date(2026, 6, 14)?,
            shop: "Denner".to_owned(),
            qty: 6.0,
            amount: Money::from_centimes(2_340),
        },
    ])
}

// ── Planning ────────────────────────────────────────────────────────────────────

fn category_caps() -> Result<Vec<CategoryCap>, PhoskError> {
    let rows: [(&str, &str, i64, bool, &str, &str); 8] = [
        (
            "groceries",
            "Groceries",
            80_000,
            false,
            "▤",
            "Three large baskets pushed the run-rate above cap.",
        ),
        (
            "going-out",
            "Going out",
            40_000,
            false,
            "◇",
            "On pace to exceed — dining out is up this cycle.",
        ),
        (
            "coffee-snacks",
            "Coffee & snacks",
            12_000,
            false,
            "☕",
            "Oat-milk flat whites are the leading signal.",
        ),
        (
            "transport",
            "Transport",
            18_000,
            false,
            "→",
            "Comfortably under cap.",
        ),
        (
            "rent",
            "Rent",
            168_000,
            true,
            "⌂",
            "Fixed charge — not tunable from here.",
        ),
        (
            "health-insurance",
            "Health insurance",
            31_800,
            true,
            "+",
            "Fixed charge.",
        ),
        (
            "shopping",
            "Shopping",
            50_000,
            false,
            "◫",
            "Well within cap.",
        ),
        (
            "subscriptions",
            "Subscriptions",
            26_000,
            false,
            "↻",
            "Steady — review flagged charges on the Subs page.",
        ),
    ];
    Ok(rows
        .into_iter()
        .map(|(slug, name, cap, fixed, glyph, note)| CategoryCap {
            id: CategoryId::new(),
            slug: slug.to_owned(),
            name: name.to_owned(),
            cap: Some(Money::from_centimes(cap)),
            fixed,
            glyph: glyph.to_owned(),
            note: note.to_owned(),
            provenance: Provenance::user_entered(),
        })
        .collect())
}

fn budget_history(caps: &[CategoryCap]) -> Result<Vec<BudgetHistory>, PhoskError> {
    let hist: [(&str, [i64; 6], i64); 8] = [
        ("Groceries", [720, 690, 810, 740, 760, 880], 800),
        ("Going out", [280, 410, 360, 300, 380, 445], 400),
        ("Coffee & snacks", [80, 95, 110, 102, 118, 136], 120),
        ("Transport", [150, 160, 140, 170, 158, 169], 180),
        ("Rent", [1680, 1680, 1680, 1680, 1680, 1680], 1680),
        ("Health insurance", [318, 318, 318, 318, 318, 318], 318),
        ("Shopping", [420, 280, 510, 330, 460, 380], 500),
        ("Subscriptions", [210, 220, 230, 240, 235, 250], 260),
    ];
    let starts = [
        date(2025, 12, 1)?,
        date(2026, 1, 1)?,
        date(2026, 2, 1)?,
        date(2026, 3, 1)?,
        date(2026, 4, 1)?,
        date(2026, 5, 1)?,
    ];
    let mut out = Vec::new();
    for (name, spends, cap_chf) in hist {
        let Some(cap) = caps.iter().find(|c| c.name == name) else {
            continue;
        };
        for (i, chf) in spends.into_iter().enumerate() {
            out.push(BudgetHistory {
                category_id: cap.id,
                cycle_start: starts[i],
                spent: Money::from_centimes(chf * 100),
                cap: Some(Money::from_centimes(cap_chf * 100)),
            });
        }
    }
    Ok(out)
}

fn alerts() -> Result<Vec<Alert>, PhoskError> {
    let created = date(2026, 6, 18)?;
    let view = AlertAction {
        label: "VIEW".to_owned(),
        kind: "navigate".to_owned(),
    };
    let raise = AlertAction {
        label: "RAISE CAP".to_owned(),
        kind: "apply".to_owned(),
    };
    let dismiss = AlertAction {
        label: "DISMISS".to_owned(),
        kind: "dismiss".to_owned(),
    };
    let snooze = AlertAction {
        label: "SNOOZE".to_owned(),
        kind: "snooze".to_owned(),
    };
    Ok(vec![
        Alert {
            id: AlertId::new(),
            slug: "a1".to_owned(),
            tone: "alert".to_owned(),
            tag: "GOING OUT".to_owned(),
            head: "Going out is at 78% of cap".to_owned(),
            body: "Projected to exceed the CHF 400 cap by month-end.".to_owned(),
            kind: "at_risk".to_owned(),
            source: Source::RuleGenerated,
            status: "active".to_owned(),
            target: Some("going-out".to_owned()),
            actions: vec![view.clone(), raise, dismiss.clone()],
            created,
            snooze: None,
        },
        Alert {
            id: AlertId::new(),
            slug: "a2".to_owned(),
            tone: "warn".to_owned(),
            tag: "BUDGET".to_owned(),
            head: "Groceries run-rate above cap".to_owned(),
            body: "Three large baskets pushed projected spend over CHF 800.".to_owned(),
            kind: "over_budget".to_owned(),
            source: Source::RuleGenerated,
            status: "active".to_owned(),
            target: Some("groceries".to_owned()),
            actions: vec![view.clone(), dismiss.clone()],
            created,
            snooze: None,
        },
        Alert {
            id: AlertId::new(),
            slug: "a3".to_owned(),
            tone: "llm".to_owned(),
            tag: "SAVINGS".to_owned(),
            head: "On track to hit your savings target".to_owned(),
            body: "Keep this pace and you'll save CHF 900 this cycle.".to_owned(),
            kind: "savings".to_owned(),
            source: Source::LlmInferred,
            status: "active".to_owned(),
            target: None,
            actions: vec![view, snooze, dismiss],
            created,
            snooze: None,
        },
    ])
}

// ── Recurring ───────────────────────────────────────────────────────────────────

fn subscriptions() -> Result<Vec<Subscription>, PhoskError> {
    let rows: [(
        &str,
        &str,
        i64,
        &str,
        &str,
        &str,
        &str,
        &str,
        u32,
        &str,
        (i32, u32, u32),
    ); 6] = [
        (
            "netflix",
            "Netflix",
            1_990,
            "monthly",
            "soon",
            "user",
            "Entertainment",
            "▶",
            22,
            "",
            (2024, 1, 22),
        ),
        (
            "spotify",
            "Spotify Family",
            1_595,
            "monthly",
            "ok",
            "user",
            "Entertainment",
            "♫",
            28,
            "",
            (2022, 6, 28),
        ),
        (
            "icloud",
            "iCloud+ 2TB",
            999,
            "monthly",
            "ok",
            "llm",
            "Cloud",
            "☁",
            15,
            "",
            (2026, 3, 15),
        ),
        (
            "gym",
            "Gym membership",
            8_900,
            "monthly",
            "watch",
            "user",
            "Health",
            "⊕",
            1,
            "",
            (2023, 9, 1),
        ),
        (
            "nyt",
            "NYT Digital",
            1_700,
            "yearly",
            "ok",
            "llm",
            "News",
            "▤",
            0,
            "FEB",
            (2025, 2, 1),
        ),
        (
            "domain",
            "Domain + email",
            4_200,
            "yearly",
            "ok",
            "user",
            "Tools",
            "@",
            0,
            "NOV",
            (2021, 11, 1),
        ),
    ];
    let mut out = Vec::new();
    for (slug, name, amount, cadence, status, source, cat, glyph, day, month, (sy, sm, sd)) in rows
    {
        let (source, prov) = match source {
            "llm" => (
                Source::LlmInferred,
                Provenance {
                    source: Source::LlmInferred,
                    confidence: 0.72,
                },
            ),
            _ => (Source::UserEntered, Provenance::user_entered()),
        };
        out.push(Subscription {
            id: SubscriptionId::new(),
            slug: slug.to_owned(),
            name: name.to_owned(),
            amount: Money::from_centimes(amount),
            cadence: cadence.to_owned(),
            day,
            month: month.to_owned(),
            status: status.to_owned(),
            category: cat.to_owned(),
            glyph: glyph.to_owned(),
            since: date(sy, sm, sd)?,
            note: String::new(),
            source,
            provenance: prov,
        });
    }
    Ok(out)
}

fn charges(subs: &[Subscription]) -> Result<Vec<Charge>, PhoskError> {
    let mut out = Vec::new();
    for s in subs {
        let dates = [date(2026, 3, 1)?, date(2026, 4, 1)?, date(2026, 5, 1)?];
        let amounts = [
            Money::from_centimes((s.amount.centimes() - 200).max(0)),
            s.amount,
            s.amount,
        ];
        for (i, d) in dates.into_iter().enumerate() {
            out.push(Charge {
                id: ChargeId::new(),
                subscription_id: s.id,
                date: d,
                amount: amounts[i],
                note: "confirmed".to_owned(),
                provenance: if s.source == Source::LlmInferred {
                    Provenance {
                        source: Source::LlmInferred,
                        confidence: 0.72,
                    }
                } else {
                    Provenance::user_entered()
                },
            });
        }
    }
    Ok(out)
}

// ── Debts ───────────────────────────────────────────────────────────────────────

fn debts() -> Result<Vec<Debt>, PhoskError> {
    let rows: [(
        &str,
        &str,
        &str,
        &str,
        i64,
        i64,
        i64,
        f64,
        u32,
        u32,
        &str,
        &str,
        &str,
        (i32, u32, u32),
        &str,
    ); 4] = [
        (
            "vw",
            "VW lease",
            "AMAG Leasing",
            "LEASE",
            1_820_000,
            3_200_000,
            45_000,
            0.039,
            1,
            48,
            "ok",
            "⊟",
            "user",
            (2024, 1, 1),
            "On schedule. Low rate — no rush to overpay.",
        ),
        (
            "card",
            "Cumulus Visa",
            "Migros Bank",
            "CARD",
            340_000,
            340_000,
            15_000,
            0.129,
            28,
            0,
            "high",
            "▭",
            "user",
            (2023, 2, 28),
            "Highest rate you carry — target this first.",
        ),
        (
            "loan",
            "Renovation loan",
            "PostFinance",
            "LOAN",
            980_000,
            1_500_000,
            32_000,
            0.052,
            5,
            36,
            "ok",
            "▤",
            "user",
            (2023, 9, 5),
            "Steady amortization.",
        ),
        (
            "tax",
            "Tax instalment",
            "Kanton ZH",
            "TAX",
            210_000,
            420_000,
            35_000,
            0.0,
            30,
            12,
            "due",
            "§",
            "llm",
            (2026, 3, 30),
            "No interest. Clear it before the deadline.",
        ),
    ];
    let mut out = Vec::new();
    for (
        slug,
        name,
        lender,
        kind,
        balance,
        orig,
        monthly,
        apr,
        day,
        term,
        status,
        glyph,
        source,
        (sy, sm, sd),
        note,
    ) in rows
    {
        let (source, prov) = match source {
            "llm" => (
                Source::LlmInferred,
                Provenance {
                    source: Source::LlmInferred,
                    confidence: 0.7,
                },
            ),
            _ => (Source::UserEntered, Provenance::user_entered()),
        };
        out.push(Debt {
            id: DebtId::new(),
            slug: slug.to_owned(),
            name: name.to_owned(),
            lender: lender.to_owned(),
            kind: kind.to_owned(),
            balance: Money::from_centimes(balance),
            orig: Money::from_centimes(orig),
            monthly: Money::from_centimes(monthly),
            apr,
            day,
            term,
            status: status.to_owned(),
            glyph: glyph.to_owned(),
            since: date(sy, sm, sd)?,
            note: note.to_owned(),
            source,
            provenance: prov,
        });
    }
    Ok(out)
}

fn debt_payments(debts: &[Debt]) -> Result<Vec<DebtPayment>, PhoskError> {
    let mut out = Vec::new();
    let d = date(2026, 5, 1)?;
    for debt in debts {
        let after =
            Money::from_centimes((debt.balance.centimes() + debt.monthly.centimes()).max(0));
        out.push(DebtPayment {
            id: PaymentId::new(),
            debt_id: debt.id,
            date: d,
            amount: debt.monthly,
            balance_after: after,
            provenance: Provenance::user_entered(),
        });
    }
    Ok(out)
}

fn personal_ious() -> Result<Vec<PersonalIou>, PhoskError> {
    let rows: [(&str, &str, &str, &str, i64, i64, &str, (i32, u32, u32)); 4] = [
        (
            "i1",
            "in",
            "Léa",
            "L",
            12_000,
            12_000,
            "Concert tickets",
            (2026, 5, 1),
        ),
        (
            "i2",
            "in",
            "Marco",
            "M",
            4_500,
            9_000,
            "Split ski cabin",
            (2026, 2, 1),
        ),
        (
            "i3",
            "out",
            "Sophie",
            "S",
            6_000,
            6_000,
            "Borrowed for the deposit",
            (2026, 4, 1),
        ),
        (
            "i4",
            "out",
            "Dad",
            "D",
            20_000,
            50_000,
            "Car repair loan",
            (2026, 1, 1),
        ),
    ];
    let mut out = Vec::new();
    for (slug, dir, person, initials, amount, of, reason, (sy, sm, sd)) in rows {
        out.push(PersonalIou {
            id: PersonalIouId::new(),
            slug: slug.to_owned(),
            dir: dir.to_owned(),
            person: person.to_owned(),
            initials: initials.to_owned(),
            amount: Money::from_centimes(amount),
            of: Money::from_centimes(of),
            reason: reason.to_owned(),
            since: date(sy, sm, sd)?,
            provenance: Provenance::user_entered(),
        });
    }
    Ok(out)
}

// ── Settings ────────────────────────────────────────────────────────────────────

fn preferences() -> Vec<Preference> {
    let rule = || Provenance {
        source: Source::RuleGenerated,
        confidence: 1.0,
    };
    let modified = || Provenance {
        source: Source::UserModified,
        confidence: 1.0,
    };
    let rows: [(&str, &str, &str, bool, bool); 5] = [
        ("momentum_baseline_cycles", "3", "analytics", true, false),
        ("currency", "CHF", "general", true, false),
        ("cycle_period", "month", "general", true, false),
        ("low_confidence_threshold", "0.7", "ai", true, true),
        ("telemetry", "off", "privacy", true, true),
    ];
    rows.into_iter()
        .map(
            |(key, value, surface, on_device, modified_flag)| Preference {
                id: PreferenceId::new(),
                key: key.to_owned(),
                value: value.to_owned(),
                surface: surface.to_owned(),
                stored_on_device: on_device,
                provenance: if modified_flag { modified() } else { rule() },
            },
        )
        .collect()
}

// ── AI ──────────────────────────────────────────────────────────────────────────

fn feed_items() -> Result<Vec<FeedItem>, PhoskError> {
    let at = date(2026, 6, 18)?;
    Ok(vec![
        FeedItem {
            id: FeedItemId::new(),
            kind: "categorize".to_owned(),
            text: "Categorised 4 lines on the Migros receipt".to_owned(),
            conf: Some(0.93),
            state: None,
            at,
            actions: vec!["VIEW".to_owned()],
            cand: false,
        },
        FeedItem {
            id: FeedItemId::new(),
            kind: "detect".to_owned(),
            text: "Detected a possible recurring charge: iCloud+".to_owned(),
            conf: Some(0.72),
            state: None,
            at,
            actions: vec!["CONFIRM".to_owned(), "DISMISS".to_owned()],
            cand: true,
        },
        FeedItem {
            id: FeedItemId::new(),
            kind: "reprocess".to_owned(),
            text: "Re-processing the latest import…".to_owned(),
            conf: None,
            state: Some("running".to_owned()),
            at,
            actions: Vec::new(),
            cand: false,
        },
    ])
}

fn chats_and_messages() -> Result<(Vec<Chat>, Vec<Message>), PhoskError> {
    let started = date(2026, 6, 18)?;
    let chat = Chat {
        id: ChatId::new(),
        started,
    };
    let messages = vec![
        Message {
            id: MessageId::new(),
            chat_id: chat.id,
            who: "usr".to_owned(),
            text: "How am I doing this cycle?".to_owned(),
            at: started,
        },
        Message {
            id: MessageId::new(),
            chat_id: chat.id,
            who: "sys".to_owned(),
            text: "You're at 78% of your going-out cap with 12 days left.".to_owned(),
            at: started,
        },
    ];
    Ok((vec![chat], messages))
}

fn ai_suggestions() -> Result<Vec<AiSuggestion>, PhoskError> {
    Ok(vec![
        AiSuggestion {
            id: SuggestionId::new(),
            kind: "budget_cut".to_owned(),
            text: "Lower the Going out cap to CHF 350 to protect savings.".to_owned(),
            confidence: 0.81,
            target: Some("going-out".to_owned()),
            estimated_savings: Some(Money::from_centimes(5_000)),
            status: "open".to_owned(),
        },
        AiSuggestion {
            id: SuggestionId::new(),
            kind: "recurring".to_owned(),
            text: "iCloud+ looks like a subscription — track it?".to_owned(),
            confidence: 0.72,
            target: Some("icloud".to_owned()),
            estimated_savings: None,
            status: "open".to_owned(),
        },
    ])
}
