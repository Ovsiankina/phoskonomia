//! `migrate` — the versioned schema migration run on every connect.
//!
//! SurrealDB is schemaless by default; this step makes the table set explicit and
//! records a monotonically-increasing `SCHEMA_VERSION` in a singleton
//! `phosk_meta` record so future migrations can branch on the stored version. It
//! is **idempotent**: `DEFINE TABLE ... IF NOT EXISTS` and an `UPSERT` of the
//! version make re-running on an existing store a no-op.
//!
//! Each entity table is defined `SCHEMALESS` on purpose: the row payload is the
//! opaque `doc` object owned by the domain's own `serde` contract (see
//! [`store`](crate::store)), so the DB does not duplicate — and cannot drift
//! from — the Rust field schema.

use std::fmt::Write as _;

use phosk_core::error::PhoskError;

use crate::store::{Bucket, Store};

/// The current schema version. Bump when the migration body changes.
const SCHEMA_VERSION: i64 = 2;

/// Define every table and stamp the schema version. Idempotent.
///
/// # Errors
/// [`PhoskError`] if any `DEFINE`/`UPSERT` statement fails.
pub(crate) async fn run(store: &Store) -> Result<(), PhoskError> {
    let db = store.raw();

    // Define each per-entity table (schemaless: the `doc` payload is owned by the
    // domain serde contract). `IF NOT EXISTS` keeps re-runs a no-op.
    let mut ddl = String::new();
    for bucket in Bucket::ALL {
        // `writeln!` into a String is infallible; surface any error anyway.
        writeln!(
            ddl,
            "DEFINE TABLE IF NOT EXISTS {} SCHEMALESS;",
            bucket.table()
        )
        .map_err(|e| PhoskError::Invalid(format!("surreal migrate ddl build: {e}")))?;
    }
    // The migration-metadata table + a singleton version stamp.
    ddl.push_str("DEFINE TABLE IF NOT EXISTS phosk_meta SCHEMALESS;\n");

    db.query(ddl)
        .await
        .map_err(|e| PhoskError::Invalid(format!("surreal migrate ddl: {e}")))?
        .check()
        .map_err(|e| PhoskError::Invalid(format!("surreal migrate ddl-check: {e}")))?;

    db.query("UPSERT phosk_meta:schema CONTENT { version: $v } RETURN NONE")
        .bind(("v", SCHEMA_VERSION))
        .await
        .map_err(|e| PhoskError::Invalid(format!("surreal migrate version: {e}")))?
        .check()
        .map_err(|e| PhoskError::Invalid(format!("surreal migrate version-check: {e}")))?;

    tracing::debug!(version = SCHEMA_VERSION, "surreal schema migrated");
    Ok(())
}
