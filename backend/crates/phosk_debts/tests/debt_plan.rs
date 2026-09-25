#![allow(
    // Test-only: see `tests/debt_write.rs` for why the exemption is crate-wide.
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::float_cmp
)]
//! Integration tests for the `phosk_debts` PLAN path (T17): plan adjustment
//! (monthly / day / term) and refinance.
//!
//! Like `tests/debt_write.rs`, every test drives the services through the PORT
//! against `MemoryDb::seeded()` and then re-reads through the READ services, so
//! "amortisation outputs stay correct" is asserted in `list_debts` /
//! `debt_stats` / `debt_detail`, with hand-checked numbers.
//!
//! The coupling rules under test:
//!
//! - `monthly` and `remaining_term` are two ends of the same amortisation, so a
//!   request may set at most one of them; the other is DERIVED.
//! - Setting `monthly` derives the remaining months with the read side's own
//!   engine; `Debt::term` (the whole contract length) becomes months elapsed
//!   since `since` + that horizon. A revolving debt (`term == 0`) stays
//!   revolving.
//! - Setting `remaining_term` derives the annuity instalment, rounded UP to the
//!   centime so the debt clears within the requested months.
//! - `day` is independent of the amortisation.
//! - Refinance changes the rate (and optionally lender / plan) on the
//!   outstanding balance: balance, original amount, opening date and payment
//!   history are left exactly as they were.

use chrono::NaiveDate;

use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_db_memory::MemoryDb;
use phosk_debts::debt_plan::{PlanAdjust, Refinance};
use phosk_debts::debt_write::{NewDebt, NewDebtPayment};
use phosk_debts::{debt_plan, debt_write, debts};
use phosk_model::{Provenance, Source};

/// The pinned read date — also the effective date of every plan change.
const fn as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 18).expect("valid as_of date")
}

fn db() -> MemoryDb {
    MemoryDb::seeded().expect("seed the in-memory adapter")
}

const fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
}

fn find<'a>(list: &'a [debts::DebtDto], id: &str) -> &'a debts::DebtDto {
    list.iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("debt {id} present"))
}

async fn read(db: &MemoryDb, id: &str) -> debts::DebtDto {
    let list = debts::list_debts(db, as_of()).await.expect("list");
    find(&list, id).clone()
}

/// 500 000 c at 6 % (0.5 %/month), 20 000 c/mo — clears in 27 months.
/// Opened 2026-01-15, so on 2026-06-18 five whole months have elapsed.
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

async fn db_with_bike() -> MemoryDb {
    let db = db();
    debt_write::create_debt(&db, bike_loan())
        .await
        .expect("create the bike loan");
    db
}

fn assert_invalid(err: PhoskError, what: &str) {
    match err {
        PhoskError::Invalid(_) => {}
        other => panic!("{what}: expected Invalid, got {other:?}"),
    }
}

// ── adjust plan ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_new_monthly_derives_the_payoff_and_rebases_the_term() {
    let db = db_with_bike().await;
    let stats_before = debts::debt_stats(&db, as_of()).await.expect("stats");

    debt_plan::adjust_plan(
        &db,
        "bike-loan",
        as_of(),
        PlanAdjust {
            monthly: Some(Money::from_centimes(25_000)),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect("the plan adjusts");

    let d = read(&db, "bike-loan").await;
    assert_eq!(d.monthly.centimes(), 25_000);
    // 500 000 c at 0.5 %/month paying 25 000 c: -ln(1 - 0.005·20)/ln(1.005)
    // = 21.1 → 22 months.
    assert_eq!(d.months_to_payoff, 22);
    // round(500 000 · 0.06) = 30 000 a year; · 22 / 12 = 55 000.
    assert_eq!(d.annual_interest.centimes(), 30_000);
    assert_eq!(d.interest_remaining.centimes(), 55_000);
    // 5 months elapsed (15 JAN → 15 JUN) + 22 to go.
    assert_eq!(d.term, 27);
    // Nothing else moves.
    assert_eq!(d.balance.centimes(), 500_000);
    assert_eq!(d.day, 15);
    assert_eq!(d.apr, 0.06);

    // The inspector's forward curve is the same 22-step amortisation.
    let detail = debts::debt_detail(&db, "bike-loan").await.expect("detail");
    let fwd = &detail.decay_series.forward;
    assert_eq!(fwd.len(), 23, "today + 22 monthly steps");
    assert_eq!(fwd[0].centimes(), 500_000);
    // One step: 500 000 + 2 500 interest − 25 000.
    assert_eq!(fwd[1].centimes(), 477_500);
    assert_eq!(fwd.last().map(|m| m.centimes()), Some(0));

    let stats = debts::debt_stats(&db, as_of()).await.expect("stats");
    assert_eq!(
        stats.total_monthly.centimes() - stats_before.total_monthly.centimes(),
        5_000
    );
    assert_eq!(stats.total_owed, stats_before.total_owed);
}

#[tokio::test]
async fn a_remaining_term_derives_the_annuity_rounded_up() {
    let db = db_with_bike().await;
    debt_plan::adjust_plan(
        &db,
        "bike-loan",
        as_of(),
        PlanAdjust {
            remaining_term: Some(12),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect("the plan adjusts");

    let d = read(&db, "bike-loan").await;
    // 5 000 · 0.005 / (1 − 1.005^-12) = 25 / 0.0580947 = CHF 430.3321…
    // → 43 034 c, rounded up so twelve instalments are enough.
    assert_eq!(d.monthly.centimes(), 43_034);
    assert_eq!(d.months_to_payoff, 12);
    assert_eq!(d.term, 5 + 12);
    assert_eq!(d.interest_remaining.centimes(), 30_000);

    let detail = debts::debt_detail(&db, "bike-loan").await.expect("detail");
    assert_eq!(detail.decay_series.forward.len(), 13);
}

#[tokio::test]
async fn a_remaining_term_at_zero_percent_is_a_ceiling_division() {
    let db = db();
    debt_write::create_debt(
        &db,
        NewDebt {
            name: "Dentist".to_owned(),
            kind: "MEDICAL".to_owned(),
            balance: Money::from_centimes(100_000),
            orig: Money::from_centimes(100_000),
            monthly: Money::from_centimes(10_000),
            apr: 0.0,
            ..bike_loan()
        },
    )
    .await
    .expect("create");

    debt_plan::adjust_plan(
        &db,
        "dentist",
        as_of(),
        PlanAdjust {
            remaining_term: Some(3),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect("adjust");

    let d = read(&db, "dentist").await;
    // ceil(100 000 / 3): 33 333 would leave a centime for a fourth month.
    assert_eq!(d.monthly.centimes(), 33_334);
    assert_eq!(d.months_to_payoff, 3);
    assert_eq!(d.interest_remaining.centimes(), 0);
}

#[tokio::test]
async fn a_revolving_debt_stays_revolving_unless_given_a_term() {
    let db = db();
    // The seed card: 340 000 c at 12.9 %, 15 000 c/mo, term 0, since 2023-02-28.
    debt_plan::adjust_plan(
        &db,
        "card",
        as_of(),
        PlanAdjust {
            monthly: Some(Money::from_centimes(16_000)),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect("adjust monthly");
    let d = read(&db, "card").await;
    assert_eq!(d.monthly.centimes(), 16_000);
    assert_eq!(d.term, 0, "a new instalment does not fix a card's term");

    debt_plan::adjust_plan(
        &db,
        "card",
        as_of(),
        PlanAdjust {
            remaining_term: Some(12),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect("adjust term");
    let d = read(&db, "card").await;
    // 3 400 · 0.01075 / (1 − 1.01075^-12) → 30 352 c.
    assert_eq!(d.monthly.centimes(), 30_352);
    assert_eq!(d.months_to_payoff, 12);
    // 28 FEB 2023 → 28 MAY 2026 is 39 whole months (18 JUN < 28 JUN), + 12.
    assert_eq!(d.term, 51);
}

#[tokio::test]
async fn a_new_day_moves_only_the_next_due_date() {
    let db = db_with_bike().await;
    let before = read(&db, "bike-loan").await;

    debt_plan::adjust_plan(
        &db,
        "bike-loan",
        as_of(),
        PlanAdjust {
            day: Some(28),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect("adjust day");

    let d = read(&db, "bike-loan").await;
    assert_eq!(d.day, 28);
    assert_eq!(d.next_label, "28 JUN");
    assert_eq!(d.monthly, before.monthly);
    assert_eq!(
        d.term, before.term,
        "the day does not touch the amortisation"
    );
    assert_eq!(d.months_to_payoff, before.months_to_payoff);

    let log = db.corrections().expect("audit log");
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].field, "day");
    assert_eq!(log[0].old_value, "15");
    assert_eq!(log[0].new_value, "28");
}

#[tokio::test]
async fn adjust_plan_audits_each_change_and_stamps_user_modified() {
    let db = db();
    // The seed `tax` debt is LLM-detected; 210 000 c at 0 %, 35 000 c/mo,
    // term 12, since 2026-03-30 (2 whole months before 2026-06-18).
    debt_plan::adjust_plan(
        &db,
        "tax",
        as_of(),
        PlanAdjust {
            monthly: Some(Money::from_centimes(70_000)),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect("adjust");

    let d = read(&db, "tax").await;
    assert_eq!(d.months_to_payoff, 3);
    assert_eq!(d.term, 5);
    assert_eq!(d.src, "llm", "where the record came from does not change");

    let stored = db.debt_by_slug("tax").await.expect("stored");
    assert_eq!(stored.source, Source::LlmInferred);
    assert_eq!(stored.provenance, Provenance::user_modified());

    let log = db.corrections().expect("audit log");
    let fields: Vec<(&str, &str, &str)> = log
        .iter()
        .map(|e| (e.field.as_str(), e.old_value.as_str(), e.new_value.as_str()))
        .collect();
    assert_eq!(
        fields,
        vec![("monthly", "35000", "70000"), ("term", "12", "5")]
    );
    assert!(log.iter().all(|e| e.entity_id == stored.id.to_string()));
    assert!(log.iter().all(|e| e.at == as_of()));
}

#[tokio::test]
async fn a_no_op_adjust_writes_nothing() {
    let db = db();
    let before = db.debt_by_slug("tax").await.expect("stored");

    debt_plan::adjust_plan(
        &db,
        "tax",
        as_of(),
        PlanAdjust {
            day: Some(30),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect("a no-op is fine");

    let after = db.debt_by_slug("tax").await.expect("stored");
    assert_eq!(after, before, "provenance is not re-stamped by a no-op");
    assert!(db.corrections().expect("audit log").is_empty());
}

#[tokio::test]
async fn adjust_plan_rejects_unusable_requests_and_leaves_the_store_alone() {
    let db = db_with_bike().await;
    let before = db.debt_by_slug("bike-loan").await.expect("stored");

    let bad: Vec<(&str, PlanAdjust)> = vec![
        ("empty", PlanAdjust::default()),
        (
            "over-determined",
            PlanAdjust {
                monthly: Some(Money::from_centimes(25_000)),
                remaining_term: Some(12),
                ..PlanAdjust::default()
            },
        ),
        (
            "day 0",
            PlanAdjust {
                day: Some(0),
                ..PlanAdjust::default()
            },
        ),
        (
            "day 32",
            PlanAdjust {
                day: Some(32),
                ..PlanAdjust::default()
            },
        ),
        (
            "zero monthly",
            PlanAdjust {
                monthly: Some(Money::ZERO),
                ..PlanAdjust::default()
            },
        ),
        (
            // 2 500 c of interest accrue each month; 2 500 c never amortises.
            "monthly that never pays off",
            PlanAdjust {
                monthly: Some(Money::from_centimes(2_500)),
                ..PlanAdjust::default()
            },
        ),
        (
            "zero term",
            PlanAdjust {
                remaining_term: Some(0),
                ..PlanAdjust::default()
            },
        ),
        (
            "term at the revolving sentinel",
            PlanAdjust {
                remaining_term: Some(600),
                ..PlanAdjust::default()
            },
        ),
    ];
    for (what, adjust) in bad {
        let err = debt_plan::adjust_plan(&db, "bike-loan", as_of(), adjust)
            .await
            .expect_err(what);
        assert_invalid(err, what);
    }

    let err = debt_plan::adjust_plan(
        &db,
        "no-such-debt",
        as_of(),
        PlanAdjust {
            day: Some(3),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect_err("unknown slug");
    assert!(matches!(err, PhoskError::NotFound(_)), "got {err:?}");

    assert_eq!(db.debt_by_slug("bike-loan").await.expect("stored"), before);
    assert!(db.corrections().expect("audit log").is_empty());
}

#[tokio::test]
async fn a_paid_off_debt_has_no_plan_to_adjust() {
    let db = db();
    // Clear the seed `tax` debt (210 000 c) in one lump sum.
    debt_write::extra_payment(
        &db,
        "tax",
        NewDebtPayment {
            date: date(2026, 6, 1),
            amount: Money::from_centimes(210_000),
        },
    )
    .await
    .expect("pay it off");

    let err = debt_plan::adjust_plan(
        &db,
        "tax",
        as_of(),
        PlanAdjust {
            remaining_term: Some(3),
            ..PlanAdjust::default()
        },
    )
    .await
    .expect_err("paid off");
    assert_invalid(err, "paid off");
}

// ── refinance ────────────────────────────────────────────────────────────────

fn refi(apr: f64) -> Refinance {
    Refinance {
        apr,
        ..Refinance::default()
    }
}

#[tokio::test]
async fn refinance_reprices_the_balance_and_keeps_the_instalment() {
    let db = db_with_bike().await;
    debt_write::record_payment(
        &db,
        "bike-loan",
        NewDebtPayment {
            date: date(2026, 6, 15),
            amount: Money::from_centimes(20_000),
        },
    )
    .await
    .expect("pay one instalment");
    // 500 000 + 2 500 interest − 20 000.
    let before = read(&db, "bike-loan").await;
    assert_eq!(before.balance.centimes(), 482_500);
    let payments_before = debts::debt_payments(&db, "bike-loan")
        .await
        .expect("payments");
    let stats_before = debts::debt_stats(&db, as_of()).await.expect("stats");

    debt_plan::refinance(&db, "bike-loan", as_of(), refi(0.03))
        .await
        .expect("refinance");

    let d = read(&db, "bike-loan").await;
    assert_eq!(d.apr, 0.03);
    assert_eq!(d.monthly.centimes(), 20_000, "the instalment is kept");
    // round(482 500 · 0.03) = 14 475 a year.
    assert_eq!(d.annual_interest.centimes(), 14_475);
    // 482 500 c at 0.25 %/month paying 20 000 c:
    // -ln(1 − 0.0025·24.125)/ln(1.0025) = 24.9 → 25 months.
    assert_eq!(d.months_to_payoff, 25);
    // 14 475 · 25 / 12 = 30 156 (integer division).
    assert_eq!(d.interest_remaining.centimes(), 30_156);
    assert_eq!(d.term, 5 + 25);

    // History is untouched: same balance, original, opening date, payments.
    assert_eq!(d.balance, before.balance);
    assert_eq!(d.orig, before.orig);
    assert_eq!(d.paid_off_pct, before.paid_off_pct);
    assert_eq!(d.since, before.since);
    assert_eq!(d.lender, before.lender);
    assert_eq!(
        debts::debt_payments(&db, "bike-loan")
            .await
            .expect("payments"),
        payments_before
    );

    let stats = debts::debt_stats(&db, as_of()).await.expect("stats");
    assert_eq!(stats.total_owed, stats_before.total_owed);
    // The bike's run-rate falls from round(482 500 · 0.06) = 28 950 to 14 475.
    assert_eq!(
        stats_before.total_interest_yr.centimes() - stats.total_interest_yr.centimes(),
        28_950 - 14_475
    );

    let log = db.corrections().expect("audit log");
    let fields: Vec<(&str, &str, &str)> = log
        .iter()
        .map(|e| (e.field.as_str(), e.old_value.as_str(), e.new_value.as_str()))
        .collect();
    // 5 elapsed + 25 to go happens to equal the old 30-month term: no entry.
    assert_eq!(fields, vec![("apr", "0.06", "0.03")]);
}

#[tokio::test]
async fn refinance_with_a_new_lender_and_term_derives_the_instalment() {
    let db = db_with_bike().await;
    debt_plan::refinance(
        &db,
        "bike-loan",
        as_of(),
        Refinance {
            apr: 0.03,
            lender: Some("  Other Fake Bank ".to_owned()),
            remaining_term: Some(24),
            ..Refinance::default()
        },
    )
    .await
    .expect("refinance");

    let d = read(&db, "bike-loan").await;
    assert_eq!(d.lender, "Other Fake Bank");
    // 5 000 · 0.0025 / (1 − 1.0025^-24) → CHF 214.9063… → 21 491 c.
    assert_eq!(d.monthly.centimes(), 21_491);
    assert_eq!(d.months_to_payoff, 24);
    assert_eq!(d.term, 29);
    assert_eq!(d.balance.centimes(), 500_000);

    let fields: Vec<String> = db
        .corrections()
        .expect("audit log")
        .into_iter()
        .map(|e| e.field)
        .collect();
    assert_eq!(fields, vec!["lender", "monthly", "apr", "term"]);
}

#[tokio::test]
async fn refinancing_a_card_keeps_it_revolving() {
    let db = db();
    debt_plan::refinance(&db, "card", as_of(), refi(0.079))
        .await
        .expect("refinance");
    let d = read(&db, "card").await;
    assert_eq!(d.apr, 0.079);
    assert_eq!(d.term, 0);
    assert_eq!(d.monthly.centimes(), 15_000);
    // round(340 000 · 0.079) = 26 860; 340 000 c at 0.658 %/month paying
    // 15 000 c clears in 25 months (27 at the old 12.9 %).
    assert_eq!(d.annual_interest.centimes(), 26_860);
    assert_eq!(d.months_to_payoff, 25);
}

#[tokio::test]
async fn refinance_rejects_unusable_terms_and_leaves_the_store_alone() {
    let db = db_with_bike().await;
    let before = db.debt_by_slug("bike-loan").await.expect("stored");

    let bad: Vec<(&str, Refinance)> = vec![
        ("negative apr", refi(-0.01)),
        ("apr above 1", refi(1.5)),
        ("apr NaN", refi(f64::NAN)),
        // 500 000 · 0.6 / 12 = 25 000 c interest a month > the 20 000 c kept.
        ("rate the instalment cannot beat", refi(0.6)),
        (
            "blank lender",
            Refinance {
                apr: 0.03,
                lender: Some("  ".to_owned()),
                ..Refinance::default()
            },
        ),
        (
            "over-determined",
            Refinance {
                apr: 0.03,
                monthly: Some(Money::from_centimes(25_000)),
                remaining_term: Some(12),
                ..Refinance::default()
            },
        ),
    ];
    for (what, r) in bad {
        let err = debt_plan::refinance(&db, "bike-loan", as_of(), r)
            .await
            .expect_err(what);
        assert_invalid(err, what);
    }
    let err = debt_plan::refinance(&db, "no-such-debt", as_of(), refi(0.03))
        .await
        .expect_err("unknown slug");
    assert!(matches!(err, PhoskError::NotFound(_)), "got {err:?}");

    assert_eq!(db.debt_by_slug("bike-loan").await.expect("stored"), before);
    assert!(db.corrections().expect("audit log").is_empty());
}
