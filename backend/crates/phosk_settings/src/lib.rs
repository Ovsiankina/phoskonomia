//! `phosk_settings` — the **preferences / user-settings service** (ADR-002 L5).
//!
//! This crate is the service that backs the `/config` page. It composes the
//! [`Preference`] records behind the [`DatabaseAdapter`] PORT into the serde
//! DTOs the Dioxus `#[server]` fns return verbatim (camelCase keys), and exposes
//! the write paths (`set` / `reset` a preference), the known-key rule table
//! ([`PREFERENCE_RULES`]) with its [`validate_preference`] check for user input,
//! plus the canonical [`momentum_baseline_cycles`] accessor.
//!
//! [`momentum_baseline_cycles`] is the single source of truth for the trailing-N
//! momentum baseline (build-contract §6): every analytics / signal service that
//! computes a `deltaPct` / `priorAvg` / `histAvg` reads `N` from here, parsing
//! the `momentum_baseline_cycles` preference and defaulting to `3` on
//! [`PhoskError::NotFound`] or a parse failure.
//!
//! **Layering (ADR-010).** Every service fn takes `&dyn DatabaseAdapter` (the
//! PORT); it depends on the PORT trait crate + the foundation crates only, never
//! on a concrete adapter (`phosk_db_memory` is a *dev*-dependency, tests only).
//!
//! **No panics.** There is no `unwrap`/`expect`/`panic!` in this code: every
//! fallible step maps explicitly to a [`PhoskError`].
//!
//! [`Preference`]: phosk_model::Preference
//! [`DatabaseAdapter`]: phosk_adapter_db::DatabaseAdapter
//! [`PhoskError`]: phosk_core::error::PhoskError

pub mod preferences;

pub use preferences::{
    CATEGORY_COLOUR_PREFIX, PREFERENCE_RULES, PreferenceDto, PreferenceRule, SettingsSummaryDto,
    known_preference, momentum_baseline_cycles, preference_rule, preferences, reset_preference,
    set_preference, settings_summary, validate_preference,
};
