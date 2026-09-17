//! Deterministic Swiss seed builders for the richer write-side entities.
//!
//! These layer ON TOP of the existing dashboard transaction line-items (see
//! `lib.rs::seed_transactions`). Every entity carries a stable `slug` matching
//! the wire DTOs (`"t1"`, `"coffee"`, `"vw"`, `"netflix"`, `"i1"`) so the
//! `*_by_slug` port methods resolve deterministically; the typed id is minted
//! per build (random), so relations are wired by id WITHIN one build only.
//!
//! The figures mirror the dioxus wire seed (`frontend/dioxus-app/src/data/*.rs`)
//! so feature tests can pin exact centime values at `as_of = 2026-06-18`. Every
//! fallible step (`NaiveDate::from_ymd_opt`) maps to a [`PhoskError`] — no panic
//! (ADR §0).

// Seed builders are flat data tables. Several return `Result` purely so they
// compose uniformly with the date-parsing fallible ones (keeping `seeded()` a
// single `?`-chain), even when an individual builder cannot currently fail; and
// the `(Vec, Vec)` tuple returns are clearer inline than behind a type alias.
#![allow(
    clippy::unnecessary_wraps,
    clippy::type_complexity,
    clippy::missing_const_for_fn
)]

use chrono::NaiveDate;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{
    AlertId, CategoryId, ChargeId, ChatId, DebtId, FeedItemId, LineItemId, MessageId, PaymentId,
    PersonalIouId, PreferenceId, ReceiptId, SignalId, SubscriptionId, SuggestionId,
};
use phosk_model::{
    AiSuggestion, Alert, AlertAction, BudgetHistory, CategoryCap, Charge, Chat, Debt, DebtPayment,
    FeedItem, LineItem, Message, PersonalIou, Preference, Provenance, Receipt, Signal,
    SignalOccurrence, Source, Subscription,
};

/// Map a `(y, m, d)` triple to a [`NaiveDate`], surfacing an invalid date as a
/// [`PhoskError`] rather than panicking.
fn date(y: i32, m: u32, d: u32) -> Result<NaiveDate, PhoskError> {
    NaiveDate::from_ymd_opt(y, m, d)
        .ok_or_else(|| PhoskError::InvalidDate(format!("seed date {y}-{m:02}-{d:02}")))
}

/// A high-confidence OCR provenance.
fn ocr() -> Provenance {
    Provenance {
        source: Source::Ocr,
        confidence: 0.94,
    }
}

// ── Ledger: receipts + line items ─────────────────────────────────────────────

/// The `t1..t9` receipts from `transactions.rs`, with itemised lines for the
/// grocery/coffee receipts (including a low-confidence line and signal-linked
/// lines on `t1`/`t5`). Slugs are `"t1"..="t9"`.
#[allow(clippy::too_many_lines)]
pub fn seed_receipts_and_lines() -> Result<(Vec<Receipt>, Vec<LineItem>), PhoskError> {
    // (slug, y, m, d, shop, category, amount_cents, fixed)
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
    let mut by_slug: std::collections::HashMap<&str, ReceiptId> = std::collections::HashMap::new();
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

    // Signals are minted below in `seed_signals`; line→signal linkage is by slug
    // resolved at occurrence build time, so lines here carry `None` and the
    // occurrence table (seed_signal_occurrences) records the signal rollups.
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

    // t1 — Migros groceries (one low-confidence line, one signal-linked coffee).
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
    // t2 — Restaurant (beer signal).
    line("t2", "Craft IPA", 2.0, 720, 1_440, "Going out", 0.90)?;
    line("t2", "Main course", 1.0, 5_010, 5_010, "Going out", 0.93)?;
    // t3 — Coop (gruyere signal).
    line("t3", "Gruyère AOP", 0.4, 6_600, 2_640, "Groceries", 0.92)?;
    line("t3", "Vegetables", 1.0, 1_590, 1_590, "Groceries", 0.89)?;
    // t5 — Migros coffee & snacks (coffee + pain signals).
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

// ── Signals ──────────────────────────────────────────────────────────────────

/// The four tracked item-signals + one LLM candidate (`energy-drink`).
/// `series`/`since` mirror `signals.rs::seed_signals`.
pub fn seed_signals() -> Result<Vec<Signal>, PhoskError> {
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

/// Current-cycle occurrences reproducing each signal's `cycleSpend`/`cycleQty`.
/// `coffee`: 16 cups / CHF 89.60; `pain`: 9 / 31.50; `beer`: 4 / 28.80;
/// `gruyere`: 1 / 26.40 (matches `signals.rs`). Modelled as one rollup row per
/// signal for the current cycle (the green phase may split into per-line rows).
pub fn seed_signal_occurrences(signals: &[Signal]) -> Result<Vec<SignalOccurrence>, PhoskError> {
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

// ── Planning: category caps + budget history + alerts ─────────────────────────

/// The 8 budget channels from `budgets.rs::seed_categories` (mixed-case names,
/// distinct from the dashboard `Category` seed). `(spent, proj, hist)` live in
/// `BudgetHistory`; here we carry the cap + fixed flag.
pub fn seed_category_caps() -> Result<Vec<CategoryCap>, PhoskError> {
    // (slug, name, cap_cents, fixed, glyph, note)
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

/// Per-category prior-cycle spend (the trailing-N momentum input). Mirrors the
/// `hist` arrays in `budgets.rs` (CHF → centimes), as cycle-start-dated rows for
/// the six cycles ending May 2026.
pub fn seed_budget_history(caps: &[CategoryCap]) -> Result<Vec<BudgetHistory>, PhoskError> {
    // (cap name, six prior-cycle spends in CHF, cap CHF)
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
    // Six cycle starts: Dec 2025 → May 2026.
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

/// The three dashboard alerts (`a1/a2/a3`). Rule-generated overspend + LLM nudge.
pub fn seed_alerts() -> Result<Vec<Alert>, PhoskError> {
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
        },
    ])
}

// ── Recurring: subscriptions + charges ────────────────────────────────────────

/// Row layout: `(slug, name, amount_cents, cadence, status, source, category, glyph, day, month, since)`.
type SubscriptionRow = (
    &'static str,
    &'static str,
    i64,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    u32,
    &'static str,
    (i32, u32, u32),
);

fn subscription_from_row(row: SubscriptionRow) -> Result<Subscription, PhoskError> {
    let (slug, name, amount, cadence, status, source, cat, glyph, day, month, (sy, sm, sd)) = row;
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
    Ok(Subscription {
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
    })
}

/// The 6 subs from `subscriptions.rs::seed_subscriptions`. Slugs = ids.
pub fn seed_subscriptions() -> Result<Vec<Subscription>, PhoskError> {
    let rows: [SubscriptionRow; 6] = [
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
    rows.into_iter().map(subscription_from_row).collect()
}

/// Three prior charges per subscription (for `priceRose` / `recent`), most
/// recent first becomes oldest→newest here.
pub fn seed_charges(subs: &[Subscription]) -> Result<Vec<Charge>, PhoskError> {
    let mut out = Vec::new();
    for s in subs {
        // Three months of confirmed charges at the current amount; the second
        // is slightly lower to model a recent price rise for `priceRose`.
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

// ── Debts: debts + payments + IOUs ────────────────────────────────────────────

/// Row layout: `(slug, name, lender, kind, balance, orig, monthly, apr, day, term, status, glyph,
/// source, since, note)`.
type DebtRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    i64,
    i64,
    i64,
    f64,
    u32,
    u32,
    &'static str,
    &'static str,
    &'static str,
    (i32, u32, u32),
    &'static str,
);

fn debt_from_row(row: DebtRow) -> Result<Debt, PhoskError> {
    let (
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
    ) = row;
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
    Ok(Debt {
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
    })
}

/// The 4 debts (`vw/card/loan/tax`) from `debts.rs::seed_debts`.
pub fn seed_debts() -> Result<Vec<Debt>, PhoskError> {
    let rows: [DebtRow; 4] = [
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
    rows.into_iter().map(debt_from_row).collect()
}

/// One recent payment per debt (for the detail decay series).
pub fn seed_debt_payments(debts: &[Debt]) -> Result<Vec<DebtPayment>, PhoskError> {
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

/// The 4 personal IOUs (`i1..i4`) from `debts.rs`.
pub fn seed_personal_ious() -> Result<Vec<PersonalIou>, PhoskError> {
    // (slug, dir, person, initials, amount, of, reason, since)
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

// ── Settings: preferences ─────────────────────────────────────────────────────

/// A handful of preferences including `momentum_baseline_cycles = "3"`. Two are
/// `UserModified` so `changed_count` is pinned at 2.
pub fn seed_preferences() -> Vec<Preference> {
    let rule = || Provenance {
        source: Source::RuleGenerated,
        confidence: 1.0,
    };
    let modified = || Provenance {
        source: Source::UserModified,
        confidence: 1.0,
    };
    // (key, value, surface, on_device, modified)
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

// ── AI: feed + chat + suggestions ─────────────────────────────────────────────

/// The AI activity feed entries from `ai.rs`.
pub fn seed_feed_items() -> Result<Vec<FeedItem>, PhoskError> {
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

/// One seeded chat with a short transcript.
pub fn seed_chats_and_messages() -> Result<(Vec<Chat>, Vec<Message>), PhoskError> {
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

/// Candidate AI suggestions.
pub fn seed_ai_suggestions() -> Result<Vec<AiSuggestion>, PhoskError> {
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
