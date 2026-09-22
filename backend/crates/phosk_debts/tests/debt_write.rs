#![allow(
    // Test-only: the workspace denies these in production, but `clippy.toml`'s
    // allow-in-tests only covers `#[test]` bodies, not integration-test helpers
    // or module docs, so the exemption is made explicit crate-wide (mirrors
    // `tests/debts.rs`).
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::float_cmp,
    clippy::suboptimal_flops
)]
//! RED integration tests for the `phosk_debts` WRITE path (T16): debt create /
//! edit / delete, record payment, extra payment.
//!
//! Every test drives the services through `&dyn DatabaseAdapter` against the
//! deterministic Swiss seed (`MemoryDb::seeded()`) and then re-reads through the
//! READ services, so the acceptance criterion "amortisation outputs stay
//! correct" is checked where it matters: in `list_debts` / `debt_stats` /
//! `debt_detail`, not in the writer's own return value.
//!
//! The two payment kinds are deliberately different operations:
//!
//! - `record_payment` is the **scheduled instalment**: one month of interest
//!   accrues (`round(balance · apr/12)`), then the amount applies. Paying
//!   exactly `monthly` therefore lands on the same balance the amortisation
//!   projection predicted for next month, and shortens `monthsToPayoff` by one.
//! - `extra_payment` is an **ad-hoc principal reduction**: no interest accrues,
//!   the balance drops by the full amount, and the payoff horizon shortens by
//!   more than one month.
//!
//! Seed debts: `vw` (LEASE, 1 820 000 c @ 3.9 %, 45 000 c/mo),
//! `card` (CARD, 340 000 c @ 12.9 %), `loan` (LOAN), `tax` (TAX, 0 % APR,
//! 210 000 c, 35 000 c/mo) — one seeded payment each, dated 2026-05-01.

use chrono::NaiveDate;

use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_debts::debt_write::{DebtEdit, NewDebt, NewDebtPayment};
use phosk_debts::{debt_write, debts};

/// The pinned read date for every test (within the May/June-2026 seed cycle).
const fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid as_of date")
}

/// A seeded in-memory adapter, behind the PORT trait object the services take.
fn db() -> MemoryDb {
    MemoryDb::seeded().expect("seed the in-memory adapter")
}

const fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
}

/// Look up one debt DTO by id in a list.
fn find<'a>(list: &'a [debts::DebtDto], id: &str) -> &'a debts::DebtDto {
    list.iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("debt {id} present"))
}

/// A well-formed new debt: 0 % nowhere, so the derived fields are interesting,
/// but the payoff horizon is still hand-checkable (see the assertions).
fn bike_loan() -> NewDebt {
    NewDebt {
        name: "Bike loan".to_owned(),
        lender: "Fake Bank".to_owned(),
        kind: "LOAN".to_owned(),
        balance: Money::from_centimes(500_000),
        orig: Money::from_centimes(600_000),
        monthly: Money::from_centimes(20_000),
        apr: 0.06,
        day: 15,
        term: 30,
        glyph: "▤".to_owned(),
        since: date(2026, 1, 15),
        note: String::new(),
    }
}

fn assert_invalid(err: PhoskError, what: &str) {
    match err {
        PhoskError::Invalid(_) => {}
        other => panic!("{what}: expected Invalid, got {other:?}"),
    }
}

// ── create ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_debt_lands_in_the_read_model_with_correct_amortisation() {
    let db = db();
    let slug = debt_write::create_debt(&db, bike_loan())
        .await
        .expect("the debt is created");
    assert_eq!(slug, "bike-loan", "slug derived from the name");

    let list = debts::list_debts(&db, as_of()).await.expect("list");
    assert_eq!(list.len(), 5, "the seed's four debts plus the new one");
    let d = find(&list, "bike-loan");

    assert_eq!(d.name, "Bike loan");
    assert_eq!(d.lender, "Fake Bank");
    assert_eq!(d.kind, "LOAN");
    assert_eq!(d.balance.centimes(), 500_000);
    assert_eq!(d.orig.centimes(), 600_000);
    assert_eq!(d.monthly.centimes(), 20_000);
    assert_eq!(d.apr, 0.06);
    assert_eq!(d.day, 15);
    assert_eq!(d.term, 30);

    // Derived: round(500_000 * 0.06) = 30_000 centimes a year.
    assert_eq!(d.annual_interest.centimes(), 30_000);
    // Amortising 500_000 c at 0.5 %/month paying 20_000 c clears in 27 months
    // (closed form: -ln(1 - r·B/m)/ln(1+r) = -ln(0.875)/ln(1.005) = 26.8 → 27).
    assert_eq!(d.months_to_payoff, 27);
    // interestRemaining = annualInterest * months / 12 = 30_000 * 27 / 12.
    assert_eq!(d.interest_remaining.centimes(), 67_500);
    assert_eq!(d.paid_off_pct, 100_000.0 / 600_000.0);

    // A fresh, user-typed debt: on track, user-sourced, not auto-detected.
    assert_eq!(d.status, "ok");
    assert_eq!(d.status_label, "ON TRACK");
    assert_eq!(d.src, "user");
    assert_eq!(d.group_label, "LEASES & LOANS");
    assert_eq!(d.since, "JAN 2026");
    // Day 15 has passed on 2026-06-18, so the next instalment is in July.
    assert_eq!(d.next_label, "15 JUL");
    // Amortising spark: balance + (5-i)*monthly, each /100.
    assert_eq!(d.hist, vec![6000.0, 5800.0, 5600.0, 5400.0, 5200.0, 5000.0]);

    // The KPI band counts it and it is not counted as auto-detected.
    let stats = debts::debt_stats(&db, as_of()).await.expect("stats");
    assert_eq!(stats.count, 5);
    assert_eq!(
        stats.auto_count, 1,
        "only the seeded TAX debt is llm-sourced"
    );
    assert_eq!(stats.total_owed.centimes(), 3_350_000 + 500_000);
    assert_eq!(stats.total_monthly.centimes(), 127_000 + 20_000);

    // With no note of its own the inspector falls back to the generic guidance.
    let detail = debts::debt_detail(&db, "bike-loan").await.expect("detail");
    assert_eq!(
        detail.guidance,
        "Keep paying CHF 200 per month to stay on track."
    );
}

#[tokio::test]
async fn create_debt_rejects_a_name_that_is_already_taken() {
    let db = db();
    let dup = NewDebt {
        name: "Bike loan".to_owned(),
        ..bike_loan()
    };
    debt_write::create_debt(&db, dup.clone())
        .await
        .expect("first create");
    let err = debt_write::create_debt(&db, dup)
        .await
        .expect_err("the second create is refused");
    assert_invalid(err, "duplicate name");
    assert_eq!(
        debts::list_debts(&db, as_of()).await.expect("list").len(),
        5,
        "the refused create wrote nothing"
    );
}

/// Every way a new debt can be unusable, with the label the failure reports.
fn unusable_new_debts() -> Vec<(&'static str, NewDebt)> {
    vec![
        (
            "blank name",
            NewDebt {
                name: "   ".to_owned(),
                ..bike_loan()
            },
        ),
        (
            "unsluggable name",
            NewDebt {
                name: "—/—".to_owned(),
                ..bike_loan()
            },
        ),
        (
            "blank lender",
            NewDebt {
                lender: " ".to_owned(),
                ..bike_loan()
            },
        ),
        (
            "unknown kind",
            NewDebt {
                kind: "MORTGAGE".to_owned(),
                ..bike_loan()
            },
        ),
        (
            "negative balance",
            NewDebt {
                balance: Money::from_centimes(-1),
                ..bike_loan()
            },
        ),
        (
            "non-positive original amount",
            NewDebt {
                orig: Money::ZERO,
                ..bike_loan()
            },
        ),
        (
            "balance above the original amount",
            NewDebt {
                balance: Money::from_centimes(600_001),
                ..bike_loan()
            },
        ),
        (
            "negative monthly payment",
            NewDebt {
                monthly: Money::from_centimes(-1),
                ..bike_loan()
            },
        ),
        (
            "apr above 1.0",
            NewDebt {
                apr: 1.5,
                ..bike_loan()
            },
        ),
        (
            "negative apr",
            NewDebt {
                apr: -0.01,
                ..bike_loan()
            },
        ),
        (
            "apr not a number",
            NewDebt {
                apr: f64::NAN,
                ..bike_loan()
            },
        ),
        (
            "day 0",
            NewDebt {
                day: 0,
                ..bike_loan()
            },
        ),
        (
            "day 32",
            NewDebt {
                day: 32,
                ..bike_loan()
            },
        ),
    ]
}

#[tokio::test]
async fn create_debt_rejects_unusable_input() {
    let db = db();
    for (what, input) in unusable_new_debts() {
        let err = debt_write::create_debt(&db, input)
            .await
            .err()
            .unwrap_or_else(|| panic!("{what} must be refused"));
        assert_invalid(err, what);
    }
    assert_eq!(
        debts::list_debts(&db, as_of()).await.expect("list").len(),
        4,
        "no refused create reached the store"
    );
}

// ── edit ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn edit_debt_changes_only_the_given_fields_and_keeps_the_slug() {
    let db = db();
    let before = debts::list_debts(&db, as_of()).await.expect("list");
    let before_vw = find(&before, "vw").clone();

    debt_write::edit_debt(
        &db,
        "vw",
        DebtEdit {
            name: Some("VW lease (renegotiated)".to_owned()),
            monthly: Some(Money::from_centimes(50_000)),
            note: Some("Rate renegotiated.".to_owned()),
            ..DebtEdit::default()
        },
    )
    .await
    .expect("the edit applies");

    let after = debts::list_debts(&db, as_of()).await.expect("list");
    assert_eq!(after.len(), before.len(), "an edit creates nothing");
    let d = find(&after, "vw");
    assert_eq!(d.id, "vw", "the slug is frozen at creation");
    assert_eq!(d.name, "VW lease (renegotiated)");
    assert_eq!(d.monthly.centimes(), 50_000);
    assert_eq!(d.note, "Rate renegotiated.");
    // Untouched fields survive.
    assert_eq!(d.balance, before_vw.balance);
    assert_eq!(d.apr, before_vw.apr);
    assert_eq!(d.lender, before_vw.lender);
    assert_eq!(d.day, before_vw.day);
    // A bigger instalment must shorten the payoff horizon.
    assert!(
        d.months_to_payoff < before_vw.months_to_payoff,
        "paying more clears it sooner: {} !< {}",
        d.months_to_payoff,
        before_vw.months_to_payoff
    );
    assert!(d.interest_remaining < before_vw.interest_remaining);
    // The annual run-rate depends on the balance, which did not move.
    assert_eq!(d.annual_interest, before_vw.annual_interest);
}

#[tokio::test]
async fn edit_debt_reprices_the_amortisation_when_the_apr_changes() {
    let db = db();
    let before = find(&debts::list_debts(&db, as_of()).await.expect("list"), "vw").clone();
    debt_write::edit_debt(
        &db,
        "vw",
        DebtEdit {
            apr: Some(0.0),
            ..DebtEdit::default()
        },
    )
    .await
    .expect("the edit applies");

    let d = find(&debts::list_debts(&db, as_of()).await.expect("list"), "vw").clone();
    assert_eq!(d.apr, 0.0);
    assert_eq!(d.annual_interest.centimes(), 0);
    assert_eq!(d.interest_remaining.centimes(), 0);
    // 1 820 000 c at 45 000 c/month, no interest ⇒ ceil(1_820_000/45_000) = 41.
    assert_eq!(d.months_to_payoff, 41);
    assert!(d.months_to_payoff < before.months_to_payoff);
}

#[tokio::test]
async fn edit_debt_rejects_an_unknown_slug_and_an_inconsistent_merge() {
    let db = db();
    let err = debt_write::edit_debt(
        &db,
        "no-such-debt",
        DebtEdit {
            note: Some("x".to_owned()),
            ..DebtEdit::default()
        },
    )
    .await
    .expect_err("unknown slug is refused");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );

    // The seeded vw debt owes 1 820 000 c on an original 3 200 000 c: cutting
    // the original below the balance would make paidOffPct negative.
    let err = debt_write::edit_debt(
        &db,
        "vw",
        DebtEdit {
            orig: Some(Money::from_centimes(1_000_000)),
            ..DebtEdit::default()
        },
    )
    .await
    .expect_err("an inconsistent merge is refused");
    assert_invalid(err, "orig below balance");

    let err = debt_write::edit_debt(
        &db,
        "vw",
        DebtEdit {
            status: Some("excellent".to_owned()),
            ..DebtEdit::default()
        },
    )
    .await
    .expect_err("an unknown status is refused");
    assert_invalid(err, "unknown status");

    let d = find(&debts::list_debts(&db, as_of()).await.expect("list"), "vw").clone();
    assert_eq!(d.orig.centimes(), 3_200_000, "refused edits wrote nothing");
    assert_eq!(d.status, "ok");
}

#[tokio::test]
async fn edit_debt_rejects_a_rename_onto_another_debt() {
    let db = db();
    // "Cumulus Visa" is another seeded debt; renaming the loan onto it would
    // make two debts answer to the same identity.
    debt_write::create_debt(
        &db,
        NewDebt {
            name: "Cumulus Visa".to_owned(),
            ..bike_loan()
        },
    )
    .await
    .expect("create the collision target");

    let err = debt_write::edit_debt(
        &db,
        "vw",
        DebtEdit {
            name: Some("Cumulus Visa".to_owned()),
            ..DebtEdit::default()
        },
    )
    .await
    .expect_err("the colliding rename is refused");
    assert_invalid(err, "rename collision");
    assert_eq!(
        find(&debts::list_debts(&db, as_of()).await.expect("list"), "vw").name,
        "VW lease"
    );
}

// ── delete ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_debt_removes_it_from_every_read_path_with_its_payments() {
    let db = db();
    let before = debts::debt_stats(&db, as_of()).await.expect("stats");
    let vw = find(&debts::list_debts(&db, as_of()).await.expect("list"), "vw").clone();
    assert!(
        !debts::debt_payments(&db, "vw")
            .await
            .expect("payments")
            .is_empty(),
        "the seed gave vw a payment history"
    );

    debt_write::delete_debt(&db, "vw").await.expect("delete");

    let list = debts::list_debts(&db, as_of()).await.expect("list");
    assert_eq!(list.len(), 3);
    assert!(list.iter().all(|d| d.id != "vw"), "gone from the grid");
    let err = debts::debt_detail(&db, "vw")
        .await
        .expect_err("gone from the inspector");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );

    let after = debts::debt_stats(&db, as_of()).await.expect("stats");
    assert_eq!(after.count, before.count - 1);
    assert_eq!(
        after.total_owed.centimes(),
        before.total_owed.centimes() - vw.balance.centimes()
    );
    assert_eq!(
        after.total_monthly.centimes(),
        before.total_monthly.centimes() - vw.monthly.centimes()
    );

    let err = debt_write::delete_debt(&db, "vw")
        .await
        .expect_err("a second delete reports it");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}

// ── record payment (the scheduled instalment) ─────────────────────────────────

#[tokio::test]
async fn record_payment_accrues_one_month_of_interest_then_applies_the_amount() {
    let db = db();
    // vw: 1 820 000 c at 3.9 % ⇒ one month of interest is
    // round(1_820_000 * 0.039/12) = 5 915 c. Paying the 45 000 c instalment
    // leaves 1 820 000 + 5 915 − 45 000 = 1 780 915 c.
    let balance = debt_write::record_payment(
        &db,
        "vw",
        NewDebtPayment {
            date: date(2026, 6, 1),
            amount: Money::from_centimes(45_000),
        },
    )
    .await
    .expect("the payment is recorded");
    assert_eq!(balance.centimes(), 1_780_915);

    let d = find(&debts::list_debts(&db, as_of()).await.expect("list"), "vw").clone();
    assert_eq!(d.balance.centimes(), 1_780_915);
    // The annual run-rate follows the new balance: round(1_780_915 * 0.039).
    assert_eq!(d.annual_interest.centimes(), 69_456);
}

#[tokio::test]
async fn record_payment_walks_the_debt_one_step_down_its_own_projection() {
    let db = db();
    let before_list = debts::list_debts(&db, as_of()).await.expect("list");
    let before = find(&before_list, "vw").clone();
    let projected = debts::debt_detail(&db, "vw")
        .await
        .expect("detail")
        .decay_series
        .forward;
    assert!(projected.len() > 2, "vw has a forward projection");

    debt_write::record_payment(
        &db,
        "vw",
        NewDebtPayment {
            date: date(2026, 6, 1),
            amount: before.monthly,
        },
    )
    .await
    .expect("the payment is recorded");

    // Paying exactly the instalment lands on the balance the projection had
    // predicted for next month, and shortens the horizon by exactly one month.
    let after_detail = debts::debt_detail(&db, "vw").await.expect("detail");
    assert_eq!(
        after_detail.decay_series.forward.first().copied(),
        projected.get(1).copied(),
        "today's balance is yesterday's month-1 projection"
    );
    assert_eq!(
        after_detail.decay_series.forward.len(),
        projected.len() - 1,
        "the remaining curve is one month shorter"
    );

    let after = find(&debts::list_debts(&db, as_of()).await.expect("list"), "vw").clone();
    assert_eq!(
        after.months_to_payoff,
        before.months_to_payoff - 1,
        "one instalment = one month closer to payoff"
    );
    assert!(after.interest_remaining < before.interest_remaining);
    assert!(after.paid_off_pct > before.paid_off_pct);
}

#[tokio::test]
async fn record_payment_appends_to_the_payment_history_newest_first() {
    let db = db();
    let before = debts::debt_payments(&db, "vw").await.expect("payments");

    debt_write::record_payment(
        &db,
        "vw",
        NewDebtPayment {
            date: date(2026, 6, 1),
            amount: Money::from_centimes(45_000),
        },
    )
    .await
    .expect("the payment is recorded");

    let after = debts::debt_payments(&db, "vw").await.expect("payments");
    assert_eq!(after.len(), before.len() + 1);
    let newest = &after[0];
    assert_eq!(newest.date, "01 JUN 2026");
    assert_eq!(newest.amount.centimes(), 45_000);
    assert_eq!(
        newest.balance.centimes(),
        1_780_915,
        "the row carries the balance the payment left behind"
    );
    assert_eq!(newest.note, "− CHF 450 paid");
    // Another debt's history is untouched.
    assert_eq!(
        debts::debt_payments(&db, "tax")
            .await
            .expect("payments")
            .len(),
        1
    );
}

#[tokio::test]
async fn record_payment_can_clear_a_debt_exactly() {
    let db = db();
    // The TAX debt carries no interest: 210 000 c outstanding on 420 000 c.
    let balance = debt_write::record_payment(
        &db,
        "tax",
        NewDebtPayment {
            date: date(2026, 6, 30),
            amount: Money::from_centimes(210_000),
        },
    )
    .await
    .expect("the final payment is recorded");
    assert_eq!(balance.centimes(), 0);

    let d = find(&debts::list_debts(&db, as_of()).await.expect("list"), "tax").clone();
    assert_eq!(d.balance.centimes(), 0);
    assert_eq!(d.months_to_payoff, 0);
    assert_eq!(d.paid_off_pct, 1.0);
    assert_eq!(d.annual_interest.centimes(), 0);
    assert_eq!(d.interest_remaining.centimes(), 0);
}

#[tokio::test]
async fn record_payment_refuses_a_non_positive_amount_or_an_overpayment() {
    let db = db();
    for (what, cents) in [("zero", 0_i64), ("negative", -1)] {
        let err = debt_write::record_payment(
            &db,
            "tax",
            NewDebtPayment {
                date: date(2026, 6, 30),
                amount: Money::from_centimes(cents),
            },
        )
        .await
        .err()
        .unwrap_or_else(|| panic!("{what} amount must be refused"));
        assert_invalid(err, what);
    }

    // TAX owes 210 000 c and accrues nothing: one centime more is an overpayment.
    let err = debt_write::record_payment(
        &db,
        "tax",
        NewDebtPayment {
            date: date(2026, 6, 30),
            amount: Money::from_centimes(210_001),
        },
    )
    .await
    .expect_err("an overpayment is refused");
    assert_invalid(err, "overpayment");

    let err = debt_write::record_payment(
        &db,
        "no-such-debt",
        NewDebtPayment {
            date: date(2026, 6, 30),
            amount: Money::from_centimes(100),
        },
    )
    .await
    .expect_err("an unknown debt is refused");
    assert!(
        matches!(err, PhoskError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );

    let d = find(&debts::list_debts(&db, as_of()).await.expect("list"), "tax").clone();
    assert_eq!(d.balance.centimes(), 210_000, "nothing was written");
    assert_eq!(
        debts::debt_payments(&db, "tax")
            .await
            .expect("payments")
            .len(),
        1
    );
}

// ── extra payment (ad-hoc principal reduction) ────────────────────────────────

#[tokio::test]
async fn extra_payment_reduces_principal_only_and_shortens_the_payoff() {
    let db = db();
    let before = find(&debts::list_debts(&db, as_of()).await.expect("list"), "vw").clone();

    let balance = debt_write::extra_payment(
        &db,
        "vw",
        NewDebtPayment {
            date: date(2026, 6, 20),
            amount: Money::from_centimes(200_000),
        },
    )
    .await
    .expect("the extra payment is recorded");
    // No interest accrues on a lump sum: the balance drops by the full amount.
    assert_eq!(balance.centimes(), 1_820_000 - 200_000);

    let after = find(&debts::list_debts(&db, as_of()).await.expect("list"), "vw").clone();
    assert_eq!(after.balance.centimes(), 1_620_000);
    assert_eq!(after.annual_interest.centimes(), 63_180); // round(1_620_000 * 0.039)
    assert!(
        after.months_to_payoff < before.months_to_payoff - 1,
        "a lump sum buys more than one month: {} vs {}",
        after.months_to_payoff,
        before.months_to_payoff
    );
    assert!(after.interest_remaining < before.interest_remaining);

    let newest = &debts::debt_payments(&db, "vw").await.expect("payments")[0];
    assert_eq!(newest.date, "20 JUN 2026");
    assert_eq!(newest.amount.centimes(), 200_000);
    assert_eq!(newest.balance.centimes(), 1_620_000);
}

#[tokio::test]
async fn extra_payment_beats_a_scheduled_payment_of_the_same_size() {
    // Same debt, same amount, same day: the lump sum skips a month of interest,
    // so it must leave a strictly smaller balance than the instalment does.
    let scheduled = db();
    let lump = db();
    let amount = Money::from_centimes(45_000);

    let a = debt_write::record_payment(
        &scheduled,
        "vw",
        NewDebtPayment {
            date: date(2026, 6, 1),
            amount,
        },
    )
    .await
    .expect("scheduled");
    let b = debt_write::extra_payment(
        &lump,
        "vw",
        NewDebtPayment {
            date: date(2026, 6, 1),
            amount,
        },
    )
    .await
    .expect("lump sum");

    assert_eq!(b.centimes(), 1_775_000);
    assert!(
        b.centimes() < a.centimes(),
        "the lump sum skipped the 5 915 c of interest: {} !< {}",
        b.centimes(),
        a.centimes()
    );
    assert_eq!(a.centimes() - b.centimes(), 5_915);
}

#[tokio::test]
async fn extra_payment_refuses_more_than_the_outstanding_balance() {
    let db = db();
    let err = debt_write::extra_payment(
        &db,
        "tax",
        NewDebtPayment {
            date: date(2026, 6, 20),
            amount: Money::from_centimes(210_001),
        },
    )
    .await
    .expect_err("an overpayment is refused");
    assert_invalid(err, "overpayment");

    let err = debt_write::extra_payment(
        &db,
        "tax",
        NewDebtPayment {
            date: date(2026, 6, 20),
            amount: Money::ZERO,
        },
    )
    .await
    .expect_err("a zero payment is refused");
    assert_invalid(err, "zero amount");

    let d = find(&debts::list_debts(&db, as_of()).await.expect("list"), "tax").clone();
    assert_eq!(d.balance.centimes(), 210_000, "nothing was written");
}

// ── the KPI band follows the writes ───────────────────────────────────────────

#[tokio::test]
async fn debt_stats_follow_creates_payments_and_deletes() {
    let db = db();
    let before = debts::debt_stats(&db, as_of()).await.expect("stats");
    assert_eq!(before.count, 4);

    debt_write::create_debt(&db, bike_loan())
        .await
        .expect("create");
    debt_write::extra_payment(
        &db,
        "vw",
        NewDebtPayment {
            date: date(2026, 6, 20),
            amount: Money::from_centimes(200_000),
        },
    )
    .await
    .expect("extra payment");
    debt_write::delete_debt(&db, "card").await.expect("delete");

    let after = debts::debt_stats(&db, as_of()).await.expect("stats");
    assert_eq!(after.count, 4, "+1 created, −1 deleted");
    assert_eq!(
        after.total_owed.centimes(),
        3_350_000 + 500_000 - 200_000 - 340_000
    );
    assert_eq!(
        after.total_monthly.centimes(),
        127_000 + 20_000 - 15_000,
        "the deleted card's instalment is gone"
    );
    // The card carried the highest APR; with it gone the avalanche target moves.
    assert_ne!(after.avalanche_target, "card");
    assert!(
        after.weighted_apr < before.weighted_apr,
        "dropping the 12.9 % card lowers the balance-weighted APR"
    );
}
