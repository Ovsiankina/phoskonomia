//! The shared PORT-conformance suite, run against the embedded-SurrealDB adapter.
//!
//! This is the SAME `phosk_adapter_db::contract::run_all` that `phosk_db_memory`
//! runs. Both passing proves the two L3 adapters are behaviourally identical on
//! the deterministic Swiss seed (ADR: memory + surreal kept in lockstep).

#![allow(clippy::expect_used, clippy::unwrap_used, reason = "test code")]

use phosk_db_surreal::SurrealDb;

#[tokio::test]
async fn surreal_adapter_satisfies_the_port_contract() {
    let db = SurrealDb::seeded().await.expect("seeded embedded surreal");
    phosk_adapter_db::contract::run_all(&db)
        .await
        .expect("embedded-surreal adapter must satisfy the shared PORT contract");
}
