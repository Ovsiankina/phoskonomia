//! `data::new_transaction`: the "NEW transaction" form (T36), driven through
//! `create_transaction_with` on a fresh store (never the shared global one).

use dioxus::prelude::ServerFnError;
use phosk_adapter_db::DatabaseAdapter;
use phosk_db_memory::MemoryDb;
use phosk_ledger::transactions::{list_transactions, TxnFilter};
use phosk_model::Source;

use super::support::{fresh_db, money, today};
use crate::data::new_transaction::{
    create_error_text, create_transaction_with, NewTxnForm, NewTxnLineForm,
};

fn total_only(total: &str) -> NewTxnForm {
    NewTxnForm {
        shop: "  Test Kiosk ".into(),
        date: "2026-06-17".into(),
        category: "Coffee & snacks".into(),
        fixed: false,
        total: total.into(),
        lines: Vec::new(),
    }
}

fn line(name: &str, qty: &str, unit_price: &str, category: &str) -> NewTxnLineForm {
    NewTxnLineForm {
        name: name.into(),
        qty: qty.into(),
        unit_price: unit_price.into(),
        category: category.into(),
    }
}

/// The June list as the page reads it (default filter).
async fn june_ids(db: &MemoryDb) -> Vec<String> {
    list_transactions(db, today(), TxnFilter::default())
        .await
        .expect("list")
        .transactions
        .into_iter()
        .map(|t| t.id)
        .collect()
}

/// A refused request: its HTTP code and the message the page shows.
#[track_caller]
fn refusal<T: std::fmt::Debug>(r: Result<T, ServerFnError>) -> (u16, String) {
    match r {
        Err(err @ ServerFnError::ServerError { code, .. }) => (code, create_error_text(&err)),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_total_only_entry_is_saved_user_entered_and_listed() {
    let db = fresh_db();
    let created = create_transaction_with(&db, total_only("CHF 1'234.50"))
        .await
        .expect("saved");
    assert_eq!(created.amount, money(123_450), "parsed to exact centimes");
    assert_eq!(created.item_count, 0);

    let receipt = db.receipt_by_slug(&created.id).await.expect("stored");
    assert_eq!(receipt.provenance.source, Source::UserEntered);
    assert_eq!(receipt.shop, "Test Kiosk", "trimmed by the ledger");
    assert!(!receipt.fixed);

    let list = list_transactions(&db, today(), TxnFilter::default())
        .await
        .expect("list");
    let row = list
        .transactions
        .iter()
        .find(|t| t.id == created.id)
        .expect("the new entry is in the list");
    assert_eq!(row.amount, money(123_450));
    assert_eq!(row.date, "17 JUN");
    assert_eq!(row.category, "Coffee & snacks");
}

#[tokio::test]
async fn an_itemised_entry_derives_its_total_from_the_lines() {
    let db = fresh_db();
    let mut form = total_only("");
    form.fixed = true;
    form.lines = vec![
        line("Espresso", "2", "3.20", ""),
        line("Croissant", "0,5", "2.20", "Groceries"),
    ];
    let created = create_transaction_with(&db, form).await.expect("saved");
    // 2 × 3.20 + 0.5 × 2.20
    assert_eq!(created.amount, money(750));
    assert_eq!(created.item_count, 2);

    let receipt = db.receipt_by_slug(&created.id).await.expect("stored");
    assert!(receipt.fixed);
    let lines = db.line_items(receipt.id).await.expect("lines");
    assert_eq!(lines.len(), 2);
    assert!(lines
        .iter()
        .all(|l| l.provenance.source == Source::UserEntered));
    assert_eq!(lines[0].category, "Coffee & snacks", "blank inherits");
    assert_eq!(lines[1].category, "Groceries");
    assert!(june_ids(&db).await.contains(&created.id));
}

#[tokio::test]
async fn a_blank_shop_is_refused_and_nothing_is_written() {
    let db = fresh_db();
    let before = june_ids(&db).await;
    let mut form = total_only("12.50");
    form.shop = "   ".into();
    let (code, msg) = refusal(create_transaction_with(&db, form).await);
    assert_eq!(code, 400);
    assert!(msg.starts_with("Shop"), "{msg}");
    assert_eq!(june_ids(&db).await, before);
}

#[tokio::test]
async fn an_entry_with_neither_total_nor_lines_is_refused() {
    let db = fresh_db();
    let (code, msg) = refusal(create_transaction_with(&db, total_only("  ")).await);
    assert_eq!(code, 400);
    assert!(msg.contains("total") && msg.contains("line item"), "{msg}");
}

#[tokio::test]
async fn malformed_text_is_refused_naming_the_field() {
    let db = fresh_db();
    let before = june_ids(&db).await;

    let (code, msg) = refusal(create_transaction_with(&db, total_only("12.345")).await);
    assert_eq!(code, 400);
    assert!(msg.starts_with("Total:"), "{msg}");

    let (_, msg) = refusal(create_transaction_with(&db, total_only("-5")).await);
    assert!(msg.starts_with("Total:"), "{msg}");

    let mut form = total_only("1");
    form.date = "17.06.2026".into();
    let (code, msg) = refusal(create_transaction_with(&db, form).await);
    assert_eq!(code, 400);
    assert!(msg.starts_with("Date:"), "{msg}");

    let mut form = total_only("");
    form.lines = vec![line("Tea", "1", "2", ""), line("Cake", "two", "4", "")];
    let (_, msg) = refusal(create_transaction_with(&db, form).await);
    assert!(msg.starts_with("Line 2:"), "{msg}");

    let mut form = total_only("");
    form.lines = vec![line("Tea", "1", "2,50", "")];
    let (_, msg) = refusal(create_transaction_with(&db, form).await);
    assert!(msg.starts_with("Line 1:"), "{msg}");

    assert_eq!(june_ids(&db).await, before, "nothing was written");
}

#[tokio::test]
async fn a_stated_total_must_match_the_lines() {
    let db = fresh_db();
    let mut form = total_only("9.00");
    form.lines = vec![line("Tea", "1", "2", "")];
    let (code, _) = refusal(create_transaction_with(&db, form).await);
    assert_eq!(code, 400);
}

#[test]
fn an_unreachable_server_reads_as_not_saved() {
    let msg = create_error_text(&ServerFnError::Deserialization("offline".into()));
    assert!(msg.contains("not saved"), "{msg}");
}
