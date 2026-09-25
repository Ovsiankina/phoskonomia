//! `data::server_err` / `data::session_err`: the sanitising boundary from
//! [`PhoskError`] to the [`ServerFnError`] a `#[server]` fn returns (F3).
//!
//! `PhoskError` has no storage/internal variant: adapters report driver, I/O
//! and transport failures as `Invalid` (and echo ids / tokens in `NotFound`),
//! so NO variant's carried string may reach the client — each maps to one
//! fixed message keyed on the variant, at its own `http_status()`.

use dioxus::prelude::ServerFnError;
use phosk_core::error::PhoskError;

use crate::data::{server_err, server_msg, session_err};

/// Pulls the sanitised `(message, code)` out of a mapped [`ServerFnError`].
fn parts(err: ServerFnError) -> (String, u16) {
    match err {
        ServerFnError::ServerError { message, code, .. } => (message, code),
        other => panic!("expected a ServerError, got {other:?}"),
    }
}

fn mapped(err: PhoskError) -> (String, u16) {
    parts(server_err(err))
}

const PATH: &str = "/home/maintainer/.config/phoskonomia/phosk-data/db";

#[test]
fn a_surreal_driver_error_in_invalid_leaks_neither_path_nor_driver_text() {
    // The real shape: `phosk_db_surreal`'s `Invalid(format!("surreal {context}: {e}"))`.
    let (message, code) = mapped(PhoskError::Invalid(format!(
        "surreal get: IO error: permission denied at {PATH}"
    )));
    assert!(!message.contains(PATH), "leaked path: {message}");
    assert!(!message.contains("surreal"), "leaked driver: {message}");
    assert!(!message.contains("permission"), "leaked io: {message}");
    assert_eq!(message, server_msg::INVALID);
    assert_eq!(code, 400);
}

#[test]
fn a_blob_io_error_in_invalid_is_the_same_fixed_text() {
    // `phosk_storage_fs`: `Invalid(format!("read blob: {io err}"))`.
    let (message, _) = mapped(PhoskError::Invalid(format!(
        "read blob: {PATH}/ab/cd: EACCES"
    )));
    assert_eq!(message, server_msg::INVALID);
}

#[test]
fn not_found_does_not_echo_the_internal_id() {
    let id = "0192f3a4-7b1c-7e2d-9f00-1234567890ab";
    let (message, code) = mapped(PhoskError::NotFound(format!("receipt {id}")));
    assert!(!message.contains(id), "leaked id: {message}");
    assert!(!message.contains("receipt"), "leaked detail: {message}");
    assert_eq!(message, server_msg::NOT_FOUND);
    assert_eq!(code, 404);
}

#[test]
fn not_found_does_not_echo_caller_input() {
    // `phosk_storage_fs`: `NotFound(format!("storage ref {token}"))`.
    let (message, _) = mapped(PhoskError::NotFound("storage ref <script>".into()));
    assert!(!message.contains("<script>"), "echoed input: {message}");
}

#[test]
fn overflow_and_invalid_date_get_their_own_fixed_text() {
    let (message, code) = mapped(PhoskError::Overflow(format!("sum while writing {PATH}")));
    assert_eq!(message, server_msg::OVERFLOW);
    assert_eq!(code, 500);
    let (message, code) = mapped(PhoskError::InvalidDate("2026-13-40".into()));
    assert_eq!(message, server_msg::INVALID_DATE);
    assert_eq!(code, 400);
}

#[test]
fn status_codes_match_phosk_error_http_status_for_every_variant() {
    for err in [
        PhoskError::InvalidDate("x".into()),
        PhoskError::Invalid("x".into()),
        PhoskError::NotFound("x".into()),
        PhoskError::Overflow("x".into()),
    ] {
        let expected = err.http_status();
        let (_, code) = mapped(err);
        assert_eq!(code, expected);
    }
}

#[test]
fn session_err_is_always_generic_and_503() {
    // `build_session` failures come from adapter construction, e.g.
    // `phosk_db_surreal`'s `Invalid(format!("surreal file engine at {path}: {e}"))`.
    for err in [
        PhoskError::Invalid(format!("surreal file engine at {PATH}: locked")),
        PhoskError::NotFound("model llama3".into()),
    ] {
        let (message, code) = parts(session_err(&err));
        assert_eq!(message, server_msg::UNAVAILABLE);
        assert!(!message.contains(PATH), "leaked path: {message}");
        assert_eq!(code, 503);
    }
}
