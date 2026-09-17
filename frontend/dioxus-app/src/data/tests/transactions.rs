//! `data::transactions`: the filtered list, receipt lines and receipt detail.

use phosk_ledger::{line_items, transactions as svc};

use super::support::{assert_maps, fresh_db, json, money, server_error, today};
use crate::data::transactions::{
    get_transaction, get_transaction_lines, list_transactions, TransactionListDto, TxnFilter,
};

/// The wire list for `filter`, checked against the backend on a fresh store.
async fn listed(filter: TxnFilter) -> TransactionListDto {
    let wire = list_transactions(filter.clone()).await.expect("list");
    let svc_filter = svc::TxnFilter {
        period: filter.period,
        shop: filter.shop,
        category: filter.category,
        sort: filter.sort,
        q: filter.q,
    };
    let backend = svc::list_transactions(&fresh_db(), today(), svc_filter)
        .await
        .expect("backend");
    assert_maps(&wire, &backend);
    wire
}

fn ids(list: &TransactionListDto) -> Vec<&str> {
    list.transactions.iter().map(|t| t.id.as_str()).collect()
}

#[tokio::test]
async fn the_unfiltered_list_is_the_june_ledger() {
    let list = listed(TxnFilter::default()).await;
    assert_eq!(list.summary.entry_count, 9);
    assert_eq!(list.summary.period_label, "JUN 2026");
    // t1..t9: 58.75 + 64.50 + 42.30 + 129.90 + 12.80 + 29.90 + 34.00 + 1680 + 318.
    assert_eq!(list.summary.total_amount, money(237_015));
    assert_eq!(json(&list)["summary"]["totalAmount"], 237_015);
    assert_eq!(list.available_shops.len(), 8);
    assert!(list.available_categories.iter().any(|c| c == "Rent"));

    let t1 = list.transactions.iter().find(|t| t.id == "t1").expect("t1");
    assert_eq!(
        (t1.date.as_str(), t1.shop.as_str(), t1.category.as_str()),
        ("16 JUN", "Migros", "Groceries")
    );
    assert_eq!(t1.amount, money(5_875));
    assert_eq!((t1.item_count, t1.low_conf_count), (4, 1));
    assert!(!t1.fixed);
    let t8 = list.transactions.iter().find(|t| t.id == "t8").expect("t8");
    assert!(t8.fixed, "rent is a standing charge");
}

#[tokio::test]
async fn filters_narrow_and_sort_the_list() {
    let migros = listed(TxnFilter {
        shop: "Migros".into(),
        ..TxnFilter::default()
    })
    .await;
    let mut got = ids(&migros);
    got.sort_unstable();
    assert_eq!(got, ["t1", "t5"]);
    assert_eq!(migros.summary.total_amount, money(7_155));
    assert_eq!(migros.available_shops.len(), 8, "options span the window");

    let groceries = listed(TxnFilter {
        category: "Groceries".into(),
        ..TxnFilter::default()
    })
    .await;
    assert_eq!(groceries.summary.entry_count, 3);

    let by_amount = listed(TxnFilter {
        sort: "amount".into(),
        ..TxnFilter::default()
    })
    .await;
    assert_eq!(ids(&by_amount)[..2], ["t8", "t9"]);

    let search = listed(TxnFilter {
        q: "COOP".into(),
        ..TxnFilter::default()
    })
    .await;
    assert_eq!(ids(&search), ["t3"], "search ignores case");

    let today_only = listed(TxnFilter {
        period: "day".into(),
        ..TxnFilter::default()
    })
    .await;
    assert_eq!(today_only.summary.entry_count, 0, "nothing on 18 JUN");
}

#[tokio::test]
async fn bad_filters_fall_back_or_match_nothing() {
    let month = listed(TxnFilter::default()).await;
    let bad_period = listed(TxnFilter {
        period: "fortnight".into(),
        ..TxnFilter::default()
    })
    .await;
    assert_eq!(bad_period, month, "an unknown period means the month cycle");

    let unknown_shop = listed(TxnFilter {
        shop: "No Such Shop".into(),
        ..TxnFilter::default()
    })
    .await;
    assert!(unknown_shop.transactions.is_empty());
    assert_eq!(unknown_shop.summary.total_amount, money(0));
}

#[tokio::test]
async fn get_transaction_lines_returns_the_parsed_receipt() {
    let l = get_transaction_lines("t1".into()).await.expect("lines");
    assert_eq!(l.lines.len(), 4);
    assert_eq!(l.low_conf, 1);
    let bananas = l.lines.iter().find(|x| x.name == "Bananas").expect("line");
    assert!((bananas.qty - 1.2).abs() < f64::EPSILON);
    assert_eq!(
        (bananas.unit_price, bananas.line_total),
        (money(320), money(384))
    );
    let bread = l
        .lines
        .iter()
        .find(|x| x.name == "Bread (unclear)")
        .expect("line");
    assert!(
        bread.confidence < 0.7,
        "the flagged line keeps its confidence"
    );

    let backend = line_items::transaction_lines(&fresh_db(), "t1")
        .await
        .expect("backend");
    assert_maps(&l, &backend);
}

#[tokio::test]
async fn get_transaction_describes_the_receipt_source() {
    let d = get_transaction("t1".into()).await.expect("detail");
    assert_eq!(d.id, "t1");
    assert_eq!((d.source.kind.as_str(), d.ocr_regions), ("PHOTO", 12));
    assert_eq!(d.source.ocr_engine, "PADDLEOCR");
    // mean(0.88, 0.91, 0.58, 0.95)
    assert!((d.avg_confidence - 0.83).abs() < 1e-9);
    assert_eq!(
        json(&d)["source"]["type"],
        "PHOTO",
        "`kind` crosses as `type`"
    );

    let backend = svc::transaction_detail(&fresh_db(), "t1")
        .await
        .expect("backend");
    assert_maps(&d, &backend);

    let manual = get_transaction("t8".into()).await.expect("detail");
    assert_eq!(
        (manual.source.kind.as_str(), manual.ocr_regions),
        ("MANUAL", 0)
    );
}

#[tokio::test]
async fn unknown_receipts_are_not_found() {
    let msg = server_error(get_transaction("t404".into()).await);
    assert!(msg.starts_with("not found"), "{msg}");
    let msg = server_error(get_transaction_lines("t404".into()).await);
    assert!(msg.starts_with("not found"), "{msg}");
}
