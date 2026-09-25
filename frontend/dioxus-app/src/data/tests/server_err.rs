//! `data::server_err`: the sanitising boundary from [`PhoskError`] to
//! [`ServerFnError`] (F3). `Invalid`/`NotFound` are the two variants feature
//! services write with a screen in mind, so their message passes through;
//! every other variant becomes one fixed, generic message, whatever detail
//! (a filesystem path, in this case) an adapter put in the carried string.

use dioxus::prelude::ServerFnError;
use phosk_core::error::PhoskError;

use crate::data::server_err;

/// Pulls the sanitised `(message, code)` out of the mapped [`ServerFnError`].
fn mapped(err: PhoskError) -> (String, u16) {
    match server_err(err) {
        ServerFnError::ServerError { message, code, .. } => (message, code),
        other => panic!("expected a ServerError, got {other:?}"),
    }
}

#[test]
fn invalid_passes_its_message_through_at_400() {
    let (message, code) = mapped(PhoskError::Invalid("unknown preference key".into()));
    assert_eq!(message, "invalid input: unknown preference key");
    assert_eq!(code, 400);
}

#[test]
fn not_found_passes_its_message_through_at_404() {
    let (message, code) = mapped(PhoskError::NotFound("receipt t404".into()));
    assert_eq!(message, "not found: receipt t404");
    assert_eq!(code, 404);
}

#[test]
fn an_adapter_error_carrying_a_path_becomes_a_generic_message() {
    // Stand-in for the real case: `build_db`'s `PhoskError::Invalid(format!("create
    // surreal dir: {e}"))` embeds the OS error, which embeds the path — but that
    // failure never reaches `server_err` (build_session's own mapping is always
    // generic, tested separately). Every path an adapter/internal failure CAN
    // reach `server_err` through is a non-`Invalid`/`NotFound` variant, so this
    // covers `server_err` refusing to echo one.
    let path = "/home/maintainer/.config/phoskonomia/phosk-data/surreal/phosk.db";
    let (message, code) = mapped(PhoskError::Overflow(format!(
        "centime addition overflowed while writing {path}"
    )));
    assert!(!message.contains(path), "leaked path: {message}");
    assert!(!message.contains("surreal"), "leaked detail: {message}");
    assert_eq!(code, 500);
}

#[test]
fn invalid_date_is_also_generic_not_passed_through() {
    // `InvalidDate` is internal calendar-math failure (a bad seed/cycle date),
    // never a value the user typed, so it gets the same treatment as the other
    // non-`Invalid`/`NotFound` variants rather than its raw message.
    let (message, code) = mapped(PhoskError::InvalidDate("2026-13-40".into()));
    assert!(!message.contains("2026-13-40"), "leaked detail: {message}");
    assert_eq!(code, 400, "InvalidDate's own http_status");
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
