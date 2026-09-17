//! The shared PORT-conformance suite, run against the in-memory adapter.
//!
//! `phosk_db_surreal` runs the SAME `phosk_adapter_db::contract::run_all` against
//! its embedded-SurrealDB adapter; if both pass, the two L3 adapters are
//! behaviourally identical on the deterministic Swiss seed (ADR: lockstep).

#![allow(clippy::expect_used, clippy::unwrap_used, reason = "test code")]

use phosk_db_memory::MemoryDb;

#[tokio::test]
async fn memory_adapter_satisfies_the_port_contract() {
    let db = MemoryDb::seeded().expect("seed is valid");
    phosk_adapter_db::contract::run_all(&db)
        .await
        .expect("in-memory adapter must satisfy the shared PORT contract");
}
