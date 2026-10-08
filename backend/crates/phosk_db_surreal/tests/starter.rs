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

// ── Open-time category reconcile ────────────────────────────────────────────

/// Write `doc` straight into a closed store's `phosk_category` table, the way
/// a build from before the category views were kept in step could leave it.
/// Goes around the adapter on purpose: no port write produces this state.
async fn raw_category_write(path: &str, key: &str, doc: Option<&phosk_model::Category>) {
    use surrealdb::Surreal;
    use surrealdb::engine::local::SurrealKv;

    let db = Surreal::new::<SurrealKv>(path).await.expect("raw open");
    db.use_ns("phoskonomia")
        .use_db("main")
        .await
        .expect("raw ns");
    match doc {
        Some(row) => {
            let doc = serde_json::to_string(row).expect("serialize");
            db.query("UPSERT type::thing('phosk_category', $id) CONTENT { doc: $doc } RETURN NONE")
                .bind(("id", key.to_owned()))
                .bind(("doc", doc))
                .await
                .expect("raw put")
                .check()
                .expect("raw put check");
        }
        None => {
            db.query("DELETE type::thing('phosk_category', $id)")
                .bind(("id", key.to_owned()))
                .await
                .expect("raw delete")
                .check()
                .expect("raw delete check");
        }
    }
}

fn row(name: &str, cap: Option<i64>) -> phosk_model::Category {
    phosk_model::Category {
        name: name.to_owned(),
        cap: cap.map(phosk_core::money::Money::from_centimes),
    }
}

type View = Vec<(String, Option<i64>)>;

/// The two category views, as sorted `(name, cap centimes)` lists.
async fn views(db: &SurrealDb) -> (View, View) {
    let mut rows: View = db
        .categories()
        .await
        .expect("categories")
        .into_iter()
        .map(|c| (c.name, c.cap.map(phosk_core::money::Money::centimes)))
        .collect();
    let mut caps: View = db
        .category_caps()
        .await
        .expect("caps")
        .into_iter()
        .map(|c| (c.name, c.cap.map(phosk_core::money::Money::centimes)))
        .collect();
    rows.sort();
    caps.sort();
    (rows, caps)
}

async fn cap_ids(db: &SurrealDb) -> Vec<String> {
    let mut ids: Vec<String> = db
        .category_caps()
        .await
        .expect("caps")
        .into_iter()
        .map(|c| c.id.to_string())
        .collect();
    ids.sort();
    ids
}

/// A store put out of step behind the adapter's back comes back as one list
/// on the next real-mode open, keeps every referenced name, and a further
/// open changes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn starter_open_reconciles_out_of_step_categories() {
    use phosk_core::money::Money;
    use phosk_model::{Provenance, Receipt};

    let dir = scratch_dir("reconcile");
    let path = dir.join("phosk.db").to_string_lossy().into_owned();

    let db = SurrealDb::file_starter(&path).await.expect("open");
    db.set_category_cap("Groceries", Some(Money::from_centimes(50_000)))
        .await
        .expect("cap");
    // A receipt booked into a name only the dashboard rows know.
    let receipt = Receipt {
        id: phosk_id::ReceiptId::new(),
        slug: "legacy-shop".to_owned(),
        shop: "Legacy shop".to_owned(),
        date: NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
        category: "Legacy".to_owned(),
        amount: Money::from_centimes(1_250),
        fixed: false,
        provenance: Provenance::user_entered(),
        source_kind: "MANUAL".to_owned(),
        ocr_engine: String::new(),
        ocr_regions: 0,
    };
    db.insert_receipt(receipt, Vec::new())
        .await
        .expect("receipt");
    drop(db);

    // Out of step: a stale cap, a missing row, an orphan row nothing names,
    // and an orphan row a receipt still names.
    raw_category_write(&path, "Groceries", Some(&row("Groceries", None))).await;
    raw_category_write(&path, "Health", None).await;
    raw_category_write(&path, "Stale", Some(&row("Stale", Some(9_900)))).await;
    raw_category_write(&path, "Legacy", Some(&row("Legacy", Some(4_000)))).await;

    let db = SurrealDb::file_starter(&path).await.expect("reopen");
    let (rows, caps) = views(&db).await;
    assert_eq!(rows, caps, "dashboard rows and budget records disagree");
    let names: Vec<&str> = rows.iter().map(|(n, _)| n.as_str()).collect();
    assert!(!names.contains(&"Stale"), "unreferenced orphan row dropped");
    assert!(names.contains(&"Health"), "missing row recreated");
    assert!(
        rows.contains(&("Groceries".to_owned(), Some(50_000))),
        "row takes the budget record's cap"
    );
    let legacy = db
        .category_cap_by_name("Legacy")
        .await
        .expect("referenced orphan gets a budget record");
    assert_eq!(legacy.cap, None, "created uncapped");
    assert_eq!(legacy.slug, "legacy");
    let first_ids = cap_ids(&db).await;
    drop(db);

    // Idempotent: a second open finds nothing to do.
    let db = SurrealDb::file_starter(&path).await.expect("third open");
    assert_eq!(views(&db).await, (rows.clone(), caps));
    assert_eq!(cap_ids(&db).await, first_ids, "no record re-created");

    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The demo store is never reconciled: an out-of-step row survives a
/// `file_seeded` open untouched.
#[tokio::test(flavor = "multi_thread")]
async fn seeded_open_leaves_categories_alone() {
    let dir = scratch_dir("seeded-untouched");
    let path = dir.join("phosk.db").to_string_lossy().into_owned();

    let db = SurrealDb::file_seeded(&path).await.expect("open");
    drop(db);
    raw_category_write(&path, "Stale", Some(&row("Stale", Some(9_900)))).await;

    let db = SurrealDb::file_seeded(&path).await.expect("reopen");
    assert!(
        db.categories()
            .await
            .expect("categories")
            .iter()
            .any(|c| c.name == "Stale"),
        "file_seeded must not reconcile"
    );
    assert!(db.category_cap_by_name("Stale").await.is_err());

    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}
