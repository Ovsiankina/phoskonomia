//! `data::debt_actions`: debt and personal-IOU writes, driven through the
//! `*_with` inner fns on a fresh seeded store (never the global one).

use phosk_debts::{debts as svc, personal_ious};

use super::support::{fresh_db, money, server_error as msg, today};
use crate::data::debt_actions::{
    create_debt_with, create_iou_with, delete_debt_with, delete_iou_with, edit_debt_with,
    edit_iou_with, pay_debt_extra_with, pay_debt_with, pay_iou_with, settle_iou_with, DebtForm,
    IouForm, DEBT_KINDS,
};
use crate::data::debts::{map_debt, map_iou};
use phosk_db_memory::MemoryDb;

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

#[test]
fn offered_kinds_are_exactly_the_service_kinds() {
    // Both directions: nothing offered is refused, nothing accepted is hidden.
    assert_eq!(DEBT_KINDS, phosk_debts::debt_write::KINDS);
}

#[tokio::test]
async fn names_and_free_text_are_bounded_so_every_record_stays_manageable() {
    let db = fresh_db();
    // The longest accepted name still yields an id every action accepts.
    let long = "N".repeat(60);
    let mut f = form();
    f.name = long.clone();
    let id = create_debt_with(&db, f)
        .await
        .expect("60 characters is fine");
    pay_debt_with(&db, id.clone(), "10".into())
        .await
        .expect("payable");
    let mut e = form();
    e.name = format!("{long}é").chars().skip(1).collect();
    edit_debt_with(&db, id.clone(), e).await.expect("editable");
    delete_debt_with(&db, id).await.expect("deletable");

    type Mutate = fn(&mut DebtForm);
    let cases: [(Mutate, &str); 3] = [
        (|f| f.name = "n".repeat(61), "Name"),
        (|f| f.lender = "l".repeat(61), "Lender"),
        (|f| f.note = "x".repeat(281), "Note"),
    ];
    for (mutate, field) in cases {
        let mut f = form();
        mutate(&mut f);
        let m = msg(create_debt_with(&db, f.clone()).await);
        assert!(m.starts_with(field), "{field}: {m}");
        let before = debt(&db, "vw").await;
        let m = msg(edit_debt_with(&db, "vw".into(), f).await);
        assert!(m.starts_with(field), "edit {field}: {m}");
        assert_eq!(
            debt(&db, "vw").await,
            before,
            "a refused edit writes nothing"
        );
    }

    // The same person twice at the longest name: the suffixed id still works.
    let mut i = iou_form();
    i.person = "P".repeat(60);
    create_iou_with(&db, i.clone()).await.expect("first");
    let second = create_iou_with(&db, i).await.expect("second, suffixed");
    pay_iou_with(&db, second.clone(), "1".into())
        .await
        .expect("payable");
    delete_iou_with(&db, second).await.expect("deletable");

    type MutateIou = fn(&mut IouForm);
    let cases: [(MutateIou, &str); 2] = [
        (|f| f.person = "p".repeat(61), "Person"),
        (|f| f.reason = "r".repeat(281), "Reason"),
    ];
    for (mutate, field) in cases {
        let mut f = iou_form();
        mutate(&mut f);
        let m = msg(create_iou_with(&db, f.clone()).await);
        assert!(m.starts_with(field), "{field}: {m}");
        let before = iou(&db, "i2").await;
        let m = msg(edit_iou_with(&db, "i2".into(), f).await);
        assert!(m.starts_with(field), "edit {field}: {m}");
        assert_eq!(
            iou(&db, "i2").await,
            before,
            "a refused edit writes nothing"
        );
    }
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
    edit_debt_with(&db, "vw".into(), f).await.expect("edited");
    let d = debt(&db, "vw").await.expect("same id");
    assert_eq!(d.name, "VW lease (renegotiated)");
    assert_eq!(d.orig, money(3_200_000));
    // The balance only moves through payments: an edit opened before a payment
    // must not write the old balance back over it.
    assert_eq!(
        d.balance,
        money(1_820_000),
        "the form's balance is not applied"
    );

    let before = debt(&db, "vw").await;
    let mut bad = form();
    bad.orig = "0".into();
    assert!(msg(edit_debt_with(&db, "vw".into(), bad).await).starts_with("Could not save the debt"));
    assert_eq!(
        debt(&db, "vw").await,
        before,
        "a refused edit writes nothing"
    );
    assert_eq!(
        msg(edit_debt_with(&db, "gone".into(), form()).await),
        "This debt no longer exists."
    );
    assert_eq!(
        msg(edit_debt_with(&db, "../vw".into(), form()).await),
        "This debt no longer exists."
    );
    assert_eq!(
        debt(&db, "vw").await,
        before,
        "a refused edit writes nothing"
    );
}

#[tokio::test]
async fn edit_debt_never_touches_the_plan_or_the_rate() {
    // The plan (instalment / day / term) and the rate change only through
    // T17's `debt_plan` rules, never through this form: a never-amortising
    // instalment, an inconsistent instalment + term pair or a rounded APR
    // sent along with a rename must leave all four exactly as stored.
    let db = fresh_db();
    let before = debt(&db, "vw").await.expect("vw");
    let mut f = form();
    f.name = "VW lease".into();
    f.orig = "32000".into();
    f.monthly = "0.01".into();
    f.term = "1".into();
    f.day = "2".into();
    f.apr = "3.9".into();
    edit_debt_with(&db, "vw".into(), f).await.expect("edited");
    let after = debt(&db, "vw").await.expect("vw");
    assert_eq!(
        (after.monthly, after.day, after.term),
        (before.monthly, before.day, before.term)
    );
    assert_eq!(after.apr.to_bits(), before.apr.to_bits(), "APR untouched");
    // Plan fields are not even parsed on edit, so junk there is no error.
    let mut f = form();
    f.orig = "32000".into();
    f.apr = "not a rate".into();
    edit_debt_with(&db, "vw".into(), f)
        .await
        .expect("plan fields ignored");
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

    let before = iou(&db, "i2").await;
    let mut below = f.clone();
    below.amount = "40".into();
    assert!(msg(edit_iou_with(&db, "i2".into(), below).await).starts_with("Could not save the IOU"));
    assert_eq!(
        iou(&db, "i2").await,
        before,
        "a refused edit writes nothing"
    );
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
        msg(delete_iou_with(&db, "i2".into()).await),
        "This IOU no longer exists."
    );
    assert_eq!(
        msg(delete_iou_with(&db, "../i1".into()).await),
        "This IOU no longer exists."
    );
    assert_eq!(
        msg(settle_iou_with(&db, "i2".into()).await),
        "This IOU no longer exists."
    );
}
