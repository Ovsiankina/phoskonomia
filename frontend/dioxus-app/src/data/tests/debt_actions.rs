//! `data::debt_actions`: debt and personal-IOU writes, driven through the
//! `*_with` inner fns on a fresh seeded store (never the global one).

use phosk_debts::{debts as svc, personal_ious};

use super::support::{fresh_db, money, today};
use crate::data::debt_actions::{
    create_debt_with, create_iou_with, delete_debt_with, delete_iou_with, edit_debt_with,
    edit_iou_with, pay_debt_extra_with, pay_debt_with, pay_iou_with, settle_iou_with, DebtForm,
    IouForm, DEBT_KINDS,
};
use crate::data::debts::{map_debt, map_iou};
use dioxus::prelude::ServerFnError;
use phosk_db_memory::MemoryDb;

/// The page-facing message of a failed action.
#[track_caller]
fn msg<T: std::fmt::Debug>(r: Result<T, ServerFnError>) -> String {
    match r {
        Err(ServerFnError::ServerError { message, .. }) => message,
        other => panic!("expected a server error, got {other:?}"),
    }
}

fn form() -> DebtForm {
    DebtForm {
        name: "Bike loan".into(),
        lender: "Velo Bank".into(),
        kind: "LOAN".into(),
        balance: "1'200.50".into(),
        orig: "2000".into(),
        monthly: "100".into(),
        apr: "4.9".into(),
        day: "15".into(),
        term: "20".into(),
        note: "  bought a bike ".into(),
    }
}

async fn debt(db: &MemoryDb, id: &str) -> Option<svc::DebtDto> {
    svc::list_debts(db, today())
        .await
        .expect("debts")
        .into_iter()
        .find(|d| d.id == id)
}

async fn iou(db: &MemoryDb, id: &str) -> Option<personal_ious::PersonalIouDto> {
    personal_ious::list_personal_ious(db)
        .await
        .expect("ious")
        .into_iter()
        .find(|i| i.id == id)
}

// ── debts ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_debt_parses_chf_and_percent_server_side() {
    let db = fresh_db();
    let id = create_debt_with(&db, form()).await.expect("created");
    assert_eq!(id, "bike-loan");
    let d = debt(&db, &id).await.expect("listed");
    assert_eq!(
        (d.balance, d.orig, d.monthly),
        (money(120_050), money(200_000), money(10_000))
    );
    assert!((d.apr - 0.049).abs() < 1e-12, "4.9 % is a 0.049 rate");
    assert_eq!((d.day, d.term, d.kind.as_str()), (15, 20, "LOAN"));
    assert_eq!(d.note, "bought a bike");
    assert!(!d.glyph.is_empty(), "the server picks a glyph for the kind");
}

#[tokio::test]
async fn create_debt_rejects_bad_input_with_fixed_texts() {
    let db = fresh_db();
    type Mutate = fn(&mut DebtForm);
    let cases: [(Mutate, &str); 7] = [
        (|f| f.balance = "12,50".into(), "Balance"),
        (|f| f.orig = "lots".into(), "Original amount"),
        (|f| f.monthly = "-5".into(), "Monthly"),
        (|f| f.apr = "150".into(), "APR"),
        (|f| f.day = "32".into(), "Payment day"),
        (|f| f.term = "x".into(), "Term"),
        (|f| f.kind = "YACHT".into(), "Type"),
    ];
    for (mutate, field) in cases {
        let mut f = form();
        mutate(&mut f);
        let m = msg(create_debt_with(&db, f).await);
        assert!(m.starts_with(field), "{field}: {m}");
    }
    // A service-side refusal (balance above the original) is a fixed text too,
    // never the `PhoskError` detail with its centime figures.
    let mut f = form();
    f.balance = "3000".into();
    let m = msg(create_debt_with(&db, f).await);
    assert!(m.starts_with("Could not save the debt"), "{m}");
    assert!(!m.chars().any(|c| c.is_ascii_digit()), "{m}");
    assert!(
        debt(&db, "bike-loan").await.is_none(),
        "nothing was written"
    );
    // A taken name is refused the same way.
    create_debt_with(&db, form()).await.expect("first one");
    assert!(msg(create_debt_with(&db, form()).await).starts_with("Could not save the debt"));
}

#[tokio::test]
async fn every_offered_kind_is_accepted() {
    let db = fresh_db();
    for (n, kind) in DEBT_KINDS.iter().enumerate() {
        let mut f = form();
        f.name = format!("Kind test {n}");
        f.kind = (*kind).to_owned();
        create_debt_with(&db, f).await.expect(kind);
    }
}

#[tokio::test]
async fn edit_debt_rewrites_the_fields_and_keeps_the_id() {
    let db = fresh_db();
    let mut f = form();
    f.name = "VW lease (renegotiated)".into();
    f.orig = "32000".into();
    f.balance = "1".into();
    f.monthly = "500".into();
    f.apr = "3".into();
    edit_debt_with(&db, "vw".into(), f).await.expect("edited");
    let d = debt(&db, "vw").await.expect("same id");
    assert_eq!(d.name, "VW lease (renegotiated)");
    assert_eq!(d.monthly, money(50_000));
    assert!((d.apr - 0.03).abs() < 1e-12);
    // The balance only moves through payments: an edit opened before a payment
    // must not write the old balance back over it.
    assert_eq!(
        d.balance,
        money(1_820_000),
        "the form's balance is not applied"
    );

    let mut bad = form();
    bad.orig = "0".into();
    assert!(msg(edit_debt_with(&db, "vw".into(), bad).await).starts_with("Could not save the debt"));
    assert_eq!(
        msg(edit_debt_with(&db, "gone".into(), form()).await),
        "This debt no longer exists."
    );
    assert_eq!(
        msg(edit_debt_with(&db, "../vw".into(), form()).await),
        "This debt no longer exists."
    );
}

#[tokio::test]
async fn delete_debt_removes_it_from_the_list() {
    let db = fresh_db();
    delete_debt_with(&db, "card".into()).await.expect("deleted");
    assert!(debt(&db, "card").await.is_none());
    assert_eq!(
        msg(delete_debt_with(&db, "card".into()).await),
        "This debt no longer exists."
    );
}

#[tokio::test]
async fn scheduled_payment_accrues_a_month_of_interest() {
    let db = fresh_db();
    // 18'200 at 3.9 %: + round(1'820'000 · 0.039 / 12) = 5'915 interest − 450.
    pay_debt_with(&db, "vw".into(), "450".into())
        .await
        .expect("paid");
    assert_eq!(debt(&db, "vw").await.expect("vw").balance, money(1_780_915));
    let history = svc::debt_payments(&db, "vw").await.expect("payments");
    assert_eq!(history.len(), 2, "the seeded payment plus this one");

    for bad in ["0", "abc", "999999"] {
        let m = msg(pay_debt_with(&db, "vw".into(), bad.into()).await);
        assert!(
            m.starts_with("Payment") || m.starts_with("The payment"),
            "{bad}: {m}"
        );
    }
    assert_eq!(
        msg(pay_debt_with(&db, "gone".into(), "1".into()).await),
        "This debt no longer exists."
    );
}

#[tokio::test]
async fn extra_payment_reduces_principal_and_can_clear_the_debt() {
    let db = fresh_db();
    pay_debt_extra_with(&db, "vw".into(), "1000".into())
        .await
        .expect("paid");
    assert_eq!(debt(&db, "vw").await.expect("vw").balance, money(1_720_000));
    assert!(map_debt(debt(&db, "vw").await.expect("vw")).actions.pay);

    // More than is owed is refused; exactly the balance clears it, and the
    // server then withdraws the payment actions.
    assert!(
        msg(pay_debt_extra_with(&db, "tax".into(), "2100.01".into()).await)
            .starts_with("The payment")
    );
    pay_debt_extra_with(&db, "tax".into(), "2100".into())
        .await
        .expect("cleared");
    let tax = map_debt(debt(&db, "tax").await.expect("still listed"));
    assert_eq!(tax.balance, money(0));
    assert!(!tax.actions.pay, "no payment on a paid-off debt");
}

// ── personal IOUs ─────────────────────────────────────────────────────────────

fn iou_form() -> IouForm {
    IouForm {
        dir: "out".into(),
        person: "Anna Muster".into(),
        amount: "80.50".into(),
        reason: "concert tickets".into(),
    }
}

#[tokio::test]
async fn create_iou_starts_outstanding_in_full() {
    let db = fresh_db();
    let id = create_iou_with(&db, iou_form()).await.expect("created");
    let i = iou(&db, &id).await.expect("listed");
    assert_eq!((i.amount, i.of), (money(8_050), money(8_050)));
    assert_eq!((i.dir.as_str(), i.initials.as_str()), ("out", "AM"));
    let wire = map_iou(i);
    assert!(wire.actions.pay && wire.actions.settle);

    let mut bad = iou_form();
    bad.dir = "sideways".into();
    assert!(msg(create_iou_with(&db, bad).await).starts_with("Direction"));
    let mut bad = iou_form();
    bad.amount = "0".into();
    assert!(msg(create_iou_with(&db, bad).await).starts_with("Could not save the IOU"));
    let mut bad = iou_form();
    bad.amount = "1.234".into();
    assert!(msg(create_iou_with(&db, bad).await).starts_with("Amount"));
}

#[tokio::test]
async fn edit_iou_moves_the_original_but_not_what_was_repaid() {
    let db = fresh_db();
    // i2: Marco owes 45 of 90 → 45 repaid.
    let f = IouForm {
        dir: "in".into(),
        person: "Marco".into(),
        amount: "100".into(),
        reason: "dinner".into(),
    };
    edit_iou_with(&db, "i2".into(), f.clone())
        .await
        .expect("edited");
    let i = iou(&db, "i2").await.expect("i2");
    assert_eq!((i.of, i.amount), (money(10_000), money(5_500)));
    assert_eq!(i.reason, "dinner");

    let mut below = f.clone();
    below.amount = "40".into();
    assert!(msg(edit_iou_with(&db, "i2".into(), below).await).starts_with("Could not save the IOU"));
    assert_eq!(
        msg(edit_iou_with(&db, "nope".into(), f).await),
        "This IOU no longer exists."
    );
}

#[tokio::test]
async fn iou_payment_settle_and_delete() {
    let db = fresh_db();
    pay_iou_with(&db, "i2".into(), "20".into())
        .await
        .expect("paid");
    assert_eq!(iou(&db, "i2").await.expect("i2").amount, money(2_500));
    assert!(msg(pay_iou_with(&db, "i2".into(), "26".into()).await).starts_with("The payment"));
    assert!(msg(pay_iou_with(&db, "i2".into(), "".into()).await).starts_with("Payment"));

    settle_iou_with(&db, "i2".into()).await.expect("settled");
    let settled = map_iou(iou(&db, "i2").await.expect("kept, fully repaid"));
    assert_eq!(settled.amount, money(0));
    assert!(!settled.actions.pay && !settled.actions.settle);

    delete_iou_with(&db, "i2".into()).await.expect("deleted");
    assert!(iou(&db, "i2").await.is_none());
    assert_eq!(
        msg(settle_iou_with(&db, "i2".into()).await),
        "This IOU no longer exists."
    );
}
