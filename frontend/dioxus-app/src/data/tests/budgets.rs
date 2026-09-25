//! `data::budgets`: envelopes, totals, allocation and the category inspector.

use phosk_planning::budgets as svc;

use super::support::{assert_maps, fresh_db, json, money, server_error, today};
use crate::data::budgets::{
    get_allocation, get_budget_totals, get_categories, get_category_detail,
    get_category_transactions,
};

#[tokio::test]
async fn get_categories_lists_the_eight_envelopes() {
    let cats = get_categories().await.expect("categories");
    let names: Vec<&str> = cats.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Groceries",
            "Going out",
            "Coffee & snacks",
            "Transport",
            "Rent",
            "Health insurance",
            "Shopping",
            "Subscriptions",
        ]
    );
    let rent = &cats[4];
    assert_eq!(rent.budget, money(168_000));
    assert!(rent.fixed);
    assert_eq!(
        json(rent)["budget"],
        168_000,
        "money crosses as i64 centimes"
    );

    let backend = svc::categories(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&cats, &backend);
}

#[tokio::test]
async fn get_budget_totals_rolls_up_the_envelopes() {
    let t = get_budget_totals().await.expect("totals");
    assert_eq!(t.envelope_count, 8);
    // 800 + 400 + 120 + 180 + 1680 + 318 + 500 + 260
    assert_eq!(t.allocated, money(425_800));

    let backend = svc::budget_totals(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&t, &backend);
}

#[tokio::test]
async fn get_allocation_has_one_segment_per_envelope() {
    let a = get_allocation().await.expect("allocation");
    assert_eq!(a.segments.len(), 8);
    let groceries = a
        .segments
        .iter()
        .find(|s| s.name == "Groceries")
        .expect("seg");
    assert_eq!(groceries.cap, money(80_000));
    assert!(!a.ai_advice.text.is_empty());

    let backend = svc::allocation(&fresh_db(), today())
        .await
        .expect("backend");
    assert_maps(&a, &backend);
}

#[tokio::test]
async fn get_category_detail_inspects_one_envelope() {
    let d = get_category_detail("Groceries".into())
        .await
        .expect("detail");
    assert!(!d.guidance.is_empty());

    let backend = svc::category_detail(&fresh_db(), today(), "Groceries")
        .await
        .expect("backend");
    assert_maps(&d, &backend);
}

#[tokio::test]
async fn get_category_transactions_lists_the_envelope_receipts() {
    let rows = get_category_transactions("Groceries".into())
        .await
        .expect("rows");
    let mut ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["t1", "t3", "t6"]);

    let backend = svc::category_transactions(&fresh_db(), today(), "Groceries")
        .await
        .expect("backend");
    assert_maps(&rows, &backend);
}

#[tokio::test]
async fn unknown_envelopes_are_not_found() {
    let msg = server_error(get_category_detail("No such envelope".into()).await, 404);
    assert!(msg.starts_with("not found"), "{msg}");
    let msg = server_error(
        get_category_transactions("No such envelope".into()).await,
        404,
    );
    assert!(msg.starts_with("not found"), "{msg}");
}
