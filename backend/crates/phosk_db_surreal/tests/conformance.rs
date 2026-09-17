//! The shared `DatabaseAdapter` conformance suite (`phosk_db_conformance`), one
//! test per check, each on a freshly seeded in-memory (`kv-mem`) engine.
//! `phosk_db_memory` runs the same checks against its in-process store.

use phosk_db_surreal::SurrealDb;

phosk_db_conformance::database_adapter_conformance!(SurrealDb::seeded());
