//! `data::edit_transaction`: EDIT and DELETE from the receipt detail (T37),
//! driven through the `*_with` inner fns on a fresh store (never the shared
//! global one).

use dioxus::prelude::ServerFnError;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::cycle::Period;
use phosk_db_memory::MemoryDb;
use phosk_ledger::transactions::{list_transactions, TransactionDto, TxnFilter};
use phosk_model::Source;

use super::support::{fresh_db, money, today};
use crate::data::edit_transaction::{
    delete_transaction_with, edit_transaction_with, txn_action_error_text, EditTxnForm,
};
use crate::data::new_transaction::{create_transaction_with, NewTxnForm};

/// The seeded Migros receipt: itemised (4 lines), 58.75, Groceries, 16 JUN.
const T1: &str = "t1";

/// The edit form as the page pre-fills it for `t1`: nothing changed.
fn unchanged_t1() -> EditTxnForm {
    EditTxnForm {
        id: T1.into(),
        shop: "Migros".into(),
        date: String::new(),
        category: "Groceries".into(),
        fixed: false,
        total: String::new(),
    }
}

/// A June row of the list the page reads, if it is still there.
async fn row(db: &MemoryDb, id: &str) -> Option<TransactionDto> {
    list_transactions(db, today(), TxnFilter::default())
        .await
        .expect("list")
        .transactions
        .into_iter()
        .find(|t| t.id == id)
}

/// The June cycle window the seed's `today()` resolves to.
fn june() -> phosk_core::cycle::CycleWindow {
    Period::Month.resolve(today()).expect("June resolves")
}

/// A total-only entry of CHF 45.00 on 10 JUN; its slug.
async fn total_only(db: &MemoryDb) -> String {
    let form = NewTxnForm {
        shop: "Test Kiosk".into(),
        date: "2026-06-10".into(),
        category: "Groceries".into(),
        fixed: false,
        total: "45.00".into(),
        lines: Vec::new(),
    };
    create_transaction_with(db, form).await.expect("created").id
}

/// A refused request: its HTTP code and the message the page shows.
#[track_caller]
fn refusal<T: std::fmt::Debug>(r: Result<T, ServerFnError>) -> (u16, String) {
    match r {
        Err(err @ ServerFnError::ServerError { code, .. }) => {
            (code, txn_action_error_text(&err, "fallback"))
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn an_edit_is_saved_user_modified_and_the_list_follows() {
    let db = fresh_db();
    let form = EditTxnForm {
        shop: "  Migros Plainpalais ".into(),
        date: "2026-06-12".into(),
        category: "Dining".into(),
        fixed: true,
        ..unchanged_t1()
    };
    let edited = edit_transaction_with(&db, form).await.expect("saved");
    assert_eq!(edited.id, T1, "the record keeps its identity");
    assert_eq!(
        edited.amount,
        money(5_875),
        "an itemised total stays derived"
    );
    assert_eq!(
        edited.date, "12 JUN",
        "the caller gets the new date's label, not the pre-edit one"
    );
    assert_eq!(edited.changed.len(), 4, "shop, category, date, fixed");

    let receipt = db.receipt_by_slug(T1).await.expect("stored");
    assert_eq!(receipt.provenance.source, Source::UserModified);
    assert_eq!(receipt.shop, "Migros Plainpalais", "trimmed");

    let listed = row(&db, T1).await.expect("still in June");
    assert_eq!(
        (
            listed.shop.as_str(),
            listed.category.as_str(),
            listed.date.as_str()
        ),
        ("Migros Plainpalais", "Dining", "12 JUN")
    );
    assert!(listed.fixed);
    assert_eq!(listed.item_count, 4, "the lines are kept");
}

#[tokio::test]
async fn the_total_of_a_total_only_entry_is_editable() {
    let db = fresh_db();
    let slug = total_only(&db).await;
    let form = EditTxnForm {
        id: slug.clone(),
        shop: "Test Kiosk".into(),
        category: "Groceries".into(),
        total: "CHF 1'234.50".into(),
        ..EditTxnForm::default()
    };
    let edited = edit_transaction_with(&db, form).await.expect("saved");
    assert_eq!(edited.amount, money(123_450), "parsed to exact centimes");
    assert_eq!(edited.changed, vec!["amount".to_owned()]);
    assert_eq!(
        row(&db, &slug).await.expect("listed").amount,
        money(123_450)
    );

    let window = june();
    let projected = db
        .transactions_between(window.start, window.end)
        .await
        .expect("projection")
        .into_iter()
        .find(|t| t.shop == "Test Kiosk")
        .expect("the dashboard projection carries this entry");
    assert_eq!(
        projected.amount,
        money(123_450),
        "the dashboard projection follows the edit, not just the receipt list"
    );
}

#[tokio::test]
async fn deleting_a_total_only_entry_removes_its_projection_row() {
    let db = fresh_db();
    let slug = total_only(&db).await;
    let window = june();
    let before = db
        .transactions_between(window.start, window.end)
        .await
        .expect("projection");
    assert!(
        before.iter().any(|t| t.shop == "Test Kiosk"),
        "projected before the delete"
    );

    delete_transaction_with(&db, slug).await.expect("deleted");

    let after = db
        .transactions_between(window.start, window.end)
        .await
        .expect("projection");
    assert!(
        !after.iter().any(|t| t.shop == "Test Kiosk"),
        "the dashboard projection row is gone with the receipt"
    );
}

#[tokio::test]
async fn an_edit_that_changes_nothing_writes_nothing() {
    let db = fresh_db();
    let before = db.receipt_by_slug(T1).await.expect("stored");
    let edited = edit_transaction_with(&db, unchanged_t1())
        .await
        .expect("a no-op is not an error");
    assert!(edited.changed.is_empty());
    let after = db.receipt_by_slug(T1).await.expect("stored");
    assert_eq!(after, before, "provenance and fields untouched");
}

#[tokio::test]
async fn invalid_values_are_refused_before_anything_is_written() {
    let db = fresh_db();
    let before = db.receipt_by_slug(T1).await.expect("stored");
    let cases = [
        ("shop", "   ", "Shop cannot be empty."),
        ("category", "", "Category cannot be empty."),
        ("date", "18.06.2026", "Date: pick a date."),
        ("total", "-5.00", "Total: cannot be negative."),
        (
            "total",
            "2000000.00",
            "Total: too large (at most CHF 1'000'000).",
        ),
    ];
    for (field, value, expected) in cases {
        let mut form = unchanged_t1();
        match field {
            "shop" => form.shop = value.into(),
            "category" => form.category = value.into(),
            "date" => form.date = value.into(),
            _ => form.total = value.into(),
        }
        let (code, message) = refusal(edit_transaction_with(&db, form).await);
        assert_eq!((code, message.as_str()), (400, expected), "{field}={value}");
    }
    assert_eq!(db.receipt_by_slug(T1).await.expect("stored"), before);
}

#[tokio::test]
async fn the_total_of_an_itemised_receipt_is_not_editable() {
    let db = fresh_db();
    let form = EditTxnForm {
        total: "1.00".into(),
        ..unchanged_t1()
    };
    let (code, message) = refusal(edit_transaction_with(&db, form).await);
    assert_eq!(code, 400);
    assert_eq!(
        message,
        "This transaction is itemised: its total comes from its lines. Correct a line instead."
    );
    let receipt = db.receipt_by_slug(T1).await.expect("stored");
    assert_eq!(receipt.amount, money(5_875));
    assert_ne!(receipt.provenance.source, Source::UserModified);
}

#[tokio::test]
async fn editing_an_unknown_transaction_is_a_404() {
    let db = fresh_db();
    let form = EditTxnForm {
        id: "no-such-receipt".into(),
        ..unchanged_t1()
    };
    assert_eq!(
        refusal(edit_transaction_with(&db, form).await),
        (404, "This transaction no longer exists.".to_owned())
    );
}

#[tokio::test]
async fn a_deleted_transaction_leaves_the_list_with_its_lines() {
    let db = fresh_db();
    let id = db.receipt_by_slug(T1).await.expect("stored").id;
    assert!(!db.line_items(id).await.expect("lines").is_empty());

    delete_transaction_with(&db, T1.into())
        .await
        .expect("deleted");

    assert!(row(&db, T1).await.is_none(), "gone from the list");
    assert!(
        db.line_items(id).await.expect("lines").is_empty(),
        "its lines went with it"
    );
}

#[tokio::test]
async fn deleting_an_unknown_transaction_is_a_404() {
    let db = fresh_db();
    assert_eq!(
        refusal(delete_transaction_with(&db, "no-such-receipt".into()).await),
        (404, "This transaction no longer exists.".to_owned())
    );
    delete_transaction_with(&db, T1.into())
        .await
        .expect("first");
    assert_eq!(
        refusal(delete_transaction_with(&db, T1.into()).await),
        (404, "This transaction no longer exists.".to_owned()),
        "a second delete is not a silent success"
    );
}

#[test]
fn an_unreachable_server_shows_the_fallback() {
    let err = ServerFnError::Deserialization("connection reset".into());
    assert_eq!(
        txn_action_error_text(&err, "Could not reach the server."),
        "Could not reach the server."
    );
}

#[test]
fn a_server_error_this_module_never_wrote_shows_the_fallback_not_its_own_text() {
    // What `ServerFnError::from_axum_response` or the "server-only" sentinel
    // (`server-deps` off) can synthesize: a real `ServerError`, but not one of
    // this module's own fixed lines.
    for message in [
        "HTTP 500: internal server error",
        "server-only",
        "unhandled error: panicked at line 12",
    ] {
        let err = ServerFnError::ServerError {
            message: message.to_owned(),
            code: 500,
            details: None,
        };
        assert_eq!(
            txn_action_error_text(&err, "fallback"),
            "fallback",
            "{message} is not one of this module's fixed lines"
        );
    }
}

#[test]
fn a_field_hint_from_the_shared_validators_is_shown_verbatim() {
    for message in [
        "Shop cannot be empty.",
        "Category is too long (at most 60 characters).",
        "Date: pick a date.",
        "Total: too large (at most CHF 1'000'000).",
    ] {
        let err = ServerFnError::ServerError {
            message: message.to_owned(),
            code: 400,
            details: None,
        };
        assert_eq!(txn_action_error_text(&err, "fallback"), message);
    }
}
