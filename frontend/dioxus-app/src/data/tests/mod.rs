//! Data-layer tests: the [`chf`](super::chf) formatter, the backend → view-struct
//! mapping, and every `#[server]` fn.
//!
//! With the `server` feature the `#[server]` macro runs the fn body in-process
//! (no HTTP), so the server-fn tests call the real fns under
//! `cargo test --no-default-features --features server`. They read the
//! process-global stack, which in test builds is the hermetic seeded one (see
//! `composition::stack`). All tests in this binary share that store, so no test
//! may write through it. Write paths are tested through their `*_with` inner
//! fns on a fresh `MemoryDb::seeded()`.
//!
//! The mapping checks compare each wire DTO's JSON with the JSON of the backend
//! service DTO it was built from (same camelCase names, money as i64 centimes).
//! A dropped, renamed or swapped field changes that JSON.

mod chf;

#[cfg(feature = "server")]
mod ai;
#[cfg(feature = "server")]
mod analytics;
#[cfg(feature = "server")]
mod budgets;
#[cfg(feature = "server")]
mod categories;
#[cfg(feature = "server")]
mod cycle;
#[cfg(feature = "server")]
mod dashboard;
#[cfg(feature = "server")]
mod debts;
#[cfg(feature = "server")]
mod settings;
#[cfg(feature = "server")]
mod signals;
#[cfg(feature = "server")]
mod subscriptions;
#[cfg(feature = "server")]
mod transactions;

/// The global stack the server-fn tests read is the in-process fakes, not a
/// live OCR engine or model (read-only calls; the fakes keep no state).
#[cfg(feature = "server")]
#[tokio::test]
async fn test_builds_run_on_the_hermetic_stack() {
    let session = crate::data::build_session().await.expect("session");
    let ocr = session.ocr().extract(b"not an image").await.expect("ocr");
    assert!(
        ocr.full_text.starts_with("MIGROS GENEVE"),
        "the canned FakeOcr receipt"
    );
    let reply = session.llm().complete("ping").await.expect("llm");
    assert_eq!(reply, "echo: ping", "the FakeLlm echo");
}

/// Shared helpers for the server-side tests.
#[cfg(feature = "server")]
mod support {
    use dioxus::prelude::ServerFnError;
    use phosk_db_memory::MemoryDb;
    use serde::Serialize;
    use serde_json::Value;

    pub(super) use crate::data::today;
    pub(super) use phosk_core::money::Money;

    /// A fresh seeded store, independent of the process-global one.
    pub(super) fn fresh_db() -> MemoryDb {
        MemoryDb::seeded().expect("the seed builds")
    }

    /// Exact money from centimes.
    pub(super) const fn money(centimes: i64) -> Money {
        Money::from_centimes(centimes)
    }

    /// The JSON wire form of a DTO.
    pub(super) fn json<T: Serialize>(dto: &T) -> Value {
        serde_json::to_value(dto).expect("the DTO serializes")
    }

    /// The wire DTO carries exactly the backend DTO's fields and values.
    #[track_caller]
    pub(super) fn assert_maps<W: Serialize, B: Serialize>(wire: &W, backend: &B) {
        assert_eq!(
            json(wire),
            json(backend),
            "wire DTO must mirror the backend DTO"
        );
    }

    /// [`assert_maps`], ignoring every `key` field (ids minted per store).
    #[track_caller]
    pub(super) fn assert_maps_except<W: Serialize, B: Serialize>(wire: &W, backend: &B, key: &str) {
        fn strip(v: &mut Value, key: &str) {
            match v {
                Value::Object(m) => {
                    m.remove(key);
                    m.values_mut().for_each(|x| strip(x, key));
                }
                Value::Array(a) => a.iter_mut().for_each(|x| strip(x, key)),
                _ => {}
            }
        }
        let (mut w, mut b) = (json(wire), json(backend));
        strip(&mut w, key);
        strip(&mut b, key);
        assert_eq!(
            w, b,
            "wire DTO must mirror the backend DTO (ignoring `{key}`)"
        );
    }

    /// The message of a server-side failure; any other outcome fails the test.
    #[track_caller]
    pub(super) fn server_error<T: std::fmt::Debug>(r: Result<T, ServerFnError>) -> String {
        match r {
            Err(ServerFnError::ServerError { message, code, .. }) => {
                assert_eq!(code, 500, "server failures surface as 500");
                message
            }
            other => panic!("expected a server error, got {other:?}"),
        }
    }
}
