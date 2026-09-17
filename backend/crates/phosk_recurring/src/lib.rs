//! `phosk_recurring` — the recurring-charges bounded context (subscriptions ·
//! recurring detection), exposing a **service** over the `DatabaseAdapter` PORT
//! (ADR-010 gateway).
//!
//! Two feature modules:
//!
//! - [`subscriptions`] — the Subscriptions page reads (list, KPI stats, billing
//!   sweep, inspector) and the dashboard recurring panel. Mirrors the wire DTOs
//!   in `frontend/dioxus-app/src/data/subscriptions.rs` and the dashboard
//!   `RecurringDto`/`RecurringListDto`.
//! - [`recurring_detect`] — the AI recurring-detection pre-pass: scan receipts
//!   for repeated same-shop/same-amount monthly charges and surface candidate
//!   subscriptions (`source == LlmInferred`); confirming one flips it to
//!   `UserEntered`.
//!
//! **Layering (ADR-010).** Every service takes `&dyn DatabaseAdapter` and an
//! `as_of: NaiveDate` cycle anchor, returning `Result<Dto, PhoskError>`. It
//! depends only on the PORT trait crate plus the domain/foundation crates; a
//! technology swap is a new adapter `impl`, never a change here.
//!
//! **Money & errors (ADR §0).** All amounts stay as exact [`Money`] (i64
//! centimes); DTO money fields serialize via `phosk_model::money_centimes`. No
//! `unwrap`/`expect`/`panic!`: every fallible step maps to a [`PhoskError`].
//!
//! The fn signatures and DTO shapes are the locked contract the test agents pin
//! against.
//!
//! [`Money`]: phosk_core::money::Money
//! [`PhoskError`]: phosk_core::error::PhoskError

pub mod recurring_detect;
pub mod subscriptions;
