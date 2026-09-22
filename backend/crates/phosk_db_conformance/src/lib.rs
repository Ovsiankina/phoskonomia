//! `phosk_db_conformance` — one conformance suite for every
//! [`DatabaseAdapter`](phosk_adapter_db::DatabaseAdapter).
//!
//! Every port method is exercised by at least one check. A check is an
//! `async fn(&dyn DatabaseAdapter) -> Outcome` that returns the first failing
//! assertion instead of panicking. [`database_adapter_conformance!`] stamps out
//! one `#[tokio::test]` per check, each on a **fresh** store:
//!
//! ```text
//! // backend/crates/<adapter>/tests/conformance.rs
//! phosk_db_conformance::database_adapter_conformance!(async { MyDb::seeded() });
//! ```
//!
//! The calling crate needs `tokio` (`macros`, `rt`) as a dev-dependency.
//!
//! ## The store contract
//!
//! The factory must yield a store holding the shared deterministic Swiss seed
//! (the one `MemoryDb::seeded` and `SurrealDb::seeded` load). The port has no
//! write path for the budget config, caps, history, alerts or feed items, so
//! those checks read seeded values. Dashboard transactions are never written
//! directly either — only as the projection `insert_receipt` maintains. Write
//! checks create their own records with fresh ids and `conf-*` slugs (the
//! category checks are the exception: renaming and delete-if-empty are about
//! the *seeded* references, so they act on a seeded category).
//!
//! ## Deliberately not asserted
//!
//! - Order of rows that share a date, or have none (line items): the port says
//!   "stored order" / "oldest→newest", but the `SurrealDB` adapter keeps no
//!   insertion sequence for most tables, so same-day rows (budget history,
//!   charges, debt payments) and a receipt's lines can come back in any order
//!   there. Ordering checks use distinct dates. Chat messages are the
//!   exception: both adapters return them in append order, and the
//!   `phosk_adapter_db::contract` suite checks that for same-day lines.
//! - Recording the same charge, payment, message or suggestion id twice:
//!   `phosk_db_memory` appends a duplicate, `phosk_db_surreal` overwrites. The
//!   port does not say which is right.

pub mod ai;
pub mod categories;
pub mod dashboard;
pub mod debts;
pub mod ids;
pub mod ledger;
pub mod planning;
pub mod recurring;
pub mod settings;
pub mod signals;
mod support;

pub use support::{Failure, Outcome};

/// Generate one `#[tokio::test]` per conformance check.
///
/// `$factory` is an expression evaluating to a future of
/// `Result<Adapter, E: Display>` (e.g. `async { MemoryDb::seeded() }`); it is
/// expanded inside every test, so each check runs on its own fresh store.
#[macro_export]
macro_rules! database_adapter_conformance {
    ($factory:expr) => {
        $crate::database_adapter_conformance!(@each $factory;
            dashboard::transactions_between_is_inclusive_on_both_bounds,
            dashboard::transactions_between_empty_and_inverted_windows,
            dashboard::categories_lists_the_seeded_caps,
            dashboard::budget_config_returns_the_seeded_config,
            ledger::insert_receipt_appends_and_reads_back,
            ledger::insert_receipt_binds_lines_to_the_receipt,
            ledger::insert_receipt_same_slug_replaces_in_place,
            ledger::insert_receipt_projects_a_dashboard_transaction,
            ledger::insert_receipt_same_slug_replaces_the_projection,
            ledger::receipts_between_filters_inclusively,
            ledger::receipt_lookups_report_not_found,
            ledger::update_line_item_replaces_the_stored_line,
            ledger::record_correction_accepts_events,
            ids::category_ids_round_trip_as_typed_ids,
            ids::receipt_ids_round_trip_as_typed_ids,
            signals::signals_lookup_by_id_and_slug_agree,
            signals::signal_occurrences_belong_to_their_signal,
            signals::set_signal_tracked_flips_the_flag,
            signals::delete_signal_removes_it_everywhere,
            planning::category_caps_lookup_by_name_agrees,
            planning::set_category_cap_sets_clears_and_stamps_provenance,
            categories::insert_category_appends_and_rejects_duplicates,
            categories::rename_category_repoints_every_reference,
            categories::delete_category_only_when_unreferenced,
            categories::merge_categories_folds_one_into_the_other,
            categories::split_category_carves_out_only_the_named_lines,
            planning::budget_history_is_oldest_to_newest,
            planning::spend_history_returns_the_recorded_cycles,
            planning::alerts_lookup_and_status_update,
            recurring::subscriptions_lookup_by_id_and_slug_agree,
            recurring::upsert_subscription_inserts_then_replaces,
            recurring::subscription_charges_are_scoped_and_oldest_first,
            recurring::delete_subscription_removes_it_and_its_charges,
            debts::debts_lookup_by_id_and_slug_agree,
            debts::upsert_debt_inserts_then_replaces,
            debts::debt_payments_are_scoped_and_oldest_first,
            debts::upsert_personal_iou_inserts_then_replaces,
            debts::personal_iou_lookup_by_slug_agrees,
            debts::delete_personal_iou_removes_it,
            settings::preferences_lookup_by_key,
            settings::set_preference_updates_or_creates,
            settings::reset_preference_restores_the_default,
            ai::dismiss_feed_item_removes_it,
            ai::latest_chat_holds_the_seeded_transcript,
            ai::append_message_reads_back_oldest_first,
            ai::clear_chat_empties_only_that_chat,
            ai::enqueue_suggestion_appends_it,
            ai::update_suggestion_status_changes_only_the_target,
        );
    };
    (@each $factory:expr; $($module:ident :: $check:ident),+ $(,)?) => {
        $(
            #[::tokio::test]
            async fn $check() -> $crate::Outcome {
                let db = $factory.await.map_err(|e| {
                    $crate::Failure(::std::format!("building a fresh store: {e}"))
                })?;
                $crate::$module::$check(&db).await
            }
        )+
    };
}
