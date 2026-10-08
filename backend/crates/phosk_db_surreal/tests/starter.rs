//! `SurrealDb::file_starter` — the real-use store: categories, a zero budget and
//! default preferences on first run, none of the demo ledger, and nothing
//! rewritten on re-open.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code"
)]

use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_db_surreal::SurrealDb;

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "phosk-surreal-starter-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn first_open_writes_categories_but_no_demo_ledger() {
    let dir = scratch_dir("fresh");
    let path = dir.join("phosk.db");
    let db = SurrealDb::file_starter(&path.to_string_lossy())
        .await
        .expect("open");

    let categories = db.categories().await.expect("categories");
    let caps = db.category_caps().await.expect("caps");
    assert!(!categories.is_empty());
    assert_eq!(categories.len(), caps.len());
    for c in &categories {
        assert!(
            caps.iter().any(|cap| cap.name == c.name),
            "ledger category {} has no budget row of the same name",
            c.name
        );
    }
    assert_eq!(
        db.budget_config()
            .await
            .expect("budget")
            .monthly_budget
            .centimes(),
        0
    );

    let from = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let to = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    assert!(
        db.transactions_between(from, to)
            .await
            .expect("tx")
            .is_empty()
    );
    assert!(db.debts().await.expect("debts").is_empty());
    assert!(db.subscriptions().await.expect("subs").is_empty());

    // The assistant panel needs a session to write to, with no messages yet.
    let chat = db
        .latest_chat()
        .await
        .expect("chat")
        .expect("a starter chat");
    assert!(
        db.chat_messages(chat.id)
            .await
            .expect("messages")
            .is_empty()
    );

    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn reopen_keeps_user_changes() {
    let dir = scratch_dir("reopen");
    let path = dir.join("phosk.db");
    let path = path.to_string_lossy().into_owned();

    let db = SurrealDb::file_starter(&path).await.expect("open");
    let name = db.categories().await.expect("categories")[0].name.clone();
    db.delete_category(&name).await.expect("delete");
    let after = db.categories().await.expect("categories").len();
    drop(db);

    let db = SurrealDb::file_starter(&path).await.expect("reopen");
    assert_eq!(db.categories().await.expect("categories").len(), after);

    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The starter store's two category views (the dashboard `Category` rows and
/// the Budgets `CategoryCap` records) stay one list through every write.
#[tokio::test(flavor = "multi_thread")]
async fn category_writes_keep_both_views_aligned() {
    use phosk_core::money::Money;

    let db = SurrealDb::memory_starter().await.expect("starter");
    let aligned = |cats: Vec<phosk_model::Category>, caps: Vec<phosk_model::CategoryCap>| {
        let mut a: Vec<(String, Option<i64>)> = cats
            .into_iter()
            .map(|c| (c.name, c.cap.map(Money::centimes)))
            .collect();
        let mut b: Vec<(String, Option<i64>)> = caps
            .into_iter()
            .map(|c| (c.name, c.cap.map(Money::centimes)))
            .collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "dashboard rows and budget records disagree");
    };
    let check = || async {
        aligned(
            db.categories().await.expect("categories"),
            db.category_caps().await.expect("caps"),
        );
    };

    check().await;
    db.set_category_cap("Groceries", Some(Money::from_centimes(60_000)))
        .await
        .expect("cap");
    check().await;
    db.set_category_cap("Shopping", Some(Money::from_centimes(10_000)))
        .await
        .expect("cap");
    db.rename_category("Groceries", "Food")
        .await
        .expect("rename");
    check().await;
    db.merge_categories("Shopping", "Food")
        .await
        .expect("merge");
    check().await;
    assert_eq!(
        db.category_cap_by_name("Food").await.expect("food").cap,
        Some(Money::from_centimes(70_000)),
        "the caps folded"
    );
    db.delete_category("Other").await.expect("delete");
    check().await;
}
