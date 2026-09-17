//! The shared `DatabaseAdapter` conformance suite (`phosk_db_conformance`), one
//! test per check, each on a freshly seeded in-memory store.
//! `phosk_db_surreal` runs the same checks against its embedded engine.

use phosk_db_memory::MemoryDb;

phosk_db_conformance::database_adapter_conformance!(async { MemoryDb::seeded() });
