//! `phosk_core` — foundation primitives shared by every crate (ADR-002 L0).
//!
//! Cross-cutting infra, not feature logic: the one error taxonomy (ADR-010),
//! the cycle/period engine (used by almost every read endpoint), and — as
//! features need them — typed IDs, CHF-typed money, and provenance (ADR-011).
//! Splittable later into `phosk_money` / `phosk_id` / `phosk_time` if any earns
//! a crate (ADR-005 grow-into-crates).

pub mod cycle;
pub mod error;
pub mod money;
