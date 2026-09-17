//! The one error taxonomy (ADR-010). Every layer maps into `PhoskError`; the
//! HTTP edge maps it to a status code + a PII-redacted body/log line.
//!
//! Closed set, exhaustive `match` (ADR-009): a new failure mode is a deliberate
//! code change here, never a stringly-typed catch-all. Variants grow as features
//! need them, written test-first.

use thiserror::Error;

/// A domain/infrastructure error in its single canonical form.
///
/// Carried strings are deliberately non-PII (a malformed date, a missing
/// resource kind) — never raw user financial data — so `Display` doubles as a
/// safe log line (ADR-010).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PhoskError {
    /// A date that cannot exist (e.g. an out-of-range year/month/day).
    #[error("invalid date: {0}")]
    InvalidDate(String),

    /// Caller input that is well-formed but not acceptable.
    #[error("invalid input: {0}")]
    Invalid(String),

    /// A referenced resource (by human term or id) does not exist in scope.
    #[error("not found: {0}")]
    NotFound(String),

    /// Exact-arithmetic overflow (e.g. centime addition past i64 range). An
    /// internal fault, not caller input — money math is checked, never wrapping
    /// (ADR §0 money rule).
    #[error("arithmetic overflow: {0}")]
    Overflow(String),
}

impl PhoskError {
    /// HTTP status this error maps to at the edge (ADR-010).
    pub const fn http_status(&self) -> u16 {
        match self {
            Self::InvalidDate(_) | Self::Invalid(_) => 400,
            Self::NotFound(_) => 404,
            Self::Overflow(_) => 500,
        }
    }

    /// Stable, machine-readable error code for the JSON body's `error` field.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidDate(_) => "invalid_date",
            Self::Invalid(_) => "invalid_input",
            Self::NotFound(_) => "not_found",
            Self::Overflow(_) => "overflow",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_is_exhaustive_and_correct() {
        assert_eq!(PhoskError::InvalidDate("x".into()).http_status(), 400);
        assert_eq!(PhoskError::Invalid("x".into()).http_status(), 400);
        assert_eq!(PhoskError::NotFound("x".into()).http_status(), 404);
        assert_eq!(PhoskError::Overflow("x".into()).http_status(), 500);
    }

    #[test]
    fn codes_are_stable() {
        assert_eq!(PhoskError::InvalidDate("x".into()).code(), "invalid_date");
        assert_eq!(PhoskError::Invalid("x".into()).code(), "invalid_input");
        assert_eq!(PhoskError::NotFound("x".into()).code(), "not_found");
        assert_eq!(PhoskError::Overflow("x".into()).code(), "overflow");
    }

    #[test]
    fn display_is_a_safe_log_line() {
        assert_eq!(
            PhoskError::InvalidDate("2026-13-01".into()).to_string(),
            "invalid date: 2026-13-01"
        );
    }
}
