//! The file-backed store is single-process: surrealkv keeps an in-memory index
//! built at open and takes no inter-process lock, so two openers of one store
//! would diverge (and both append to the same commit log). `SurrealDb::file`
//! therefore holds an exclusive lockfile for the handle's lifetime.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code"
)]

use phosk_core::error::PhoskError;
use phosk_db_surreal::SurrealDb;

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "phosk-surreal-lock-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn second_open_of_the_same_store_fails_while_the_first_is_alive() {
    let dir = scratch_dir("second-open");
    let path = dir.join("phosk.db");
    let path = path.to_string_lossy();

    let first = SurrealDb::file(&path).await.expect("first open");
    let err = SurrealDb::file(&path)
        .await
        .expect_err("a second opener must be refused while the first is alive");
    assert!(
        matches!(&err, PhoskError::Invalid(m) if m.contains("already open")),
        "{err:?}"
    );

    // A clone shares the handle (and the lock); dropping every handle frees it.
    let clone = first.clone();
    drop(first);
    assert!(
        SurrealDb::file(&path).await.is_err(),
        "a live clone still holds the lock"
    );
    drop(clone);

    let reopened = SurrealDb::file(&path).await.expect("reopen after drop");
    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
}
