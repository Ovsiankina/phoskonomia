//! `phosk_adapter_db` — the `DatabaseAdapter` **port** (ADR-000: one fat trait).
//!
//! This is the L2 contract seam between feature code and whatever persists the
//! data. Per ADR-000 there is **one** fat `DatabaseAdapter` trait covering all
//! domains — it is deliberately *not* split into per-domain stores. Feature
//! crates depend on this trait and take `&dyn DatabaseAdapter` /
//! `Arc<dyn DatabaseAdapter>`; concrete adapters (e.g. `phosk_db_memory`) are
//! wired in only by the `bin/*` composition root and are never imported here or
//! by feature crates (ADR-010 layering).
//!
//! The trait is **async** (persistence is I/O) and must stay **object-safe** so
//! that `Arc<dyn DatabaseAdapter + Send + Sync>` works as a swappable handle.
//! That is why it uses [`mod@async_trait`]: it desugars `async fn` in the trait
//! to a boxed future, which keeps the trait `dyn`-compatible.
//!
//! The trait covers the reads every page needs plus the write paths that exist
//! so far (receipt insert, line edits, caps, statuses, upserts, …). Each method
//! returns `Result<_, PhoskError>` (the one taxonomy, ADR-010); an adapter maps
//! its own failures into a `PhoskError` and never panics. Every implementation
//! must pass the shared per-method suite in `phosk_db_conformance`. Amounts stay as exact
//! [`Money`](phosk_core::money::Money) inside [`Transaction`]/[`Category`]/
//! [`BudgetConfig`]; CHF-number conversion is the HTTP edge's job, not the
//! port's.

#[cfg(feature = "contract")]
pub mod contract;

use async_trait::async_trait;
use chrono::NaiveDate;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{
    AlertId, ChatId, DebtId, PersonalIouId, ReceiptId, SignalId, SubscriptionId, SuggestionId,
};
use phosk_model::{
    AiSuggestion, Alert, BudgetConfig, BudgetHistory, Category, CategoryCap, Charge, Chat,
    CorrectionEvent, Debt, DebtPayment, FeedItem, LineItem, Message, PersonalIou, Preference,
    Receipt, Signal, SignalOccurrence, Subscription, Transaction,
};

/// The single fat database port (ADR-000): one async, object-safe trait covering
/// every domain the backend persists. A technology swap is a new `impl` of this
/// trait, never a change to feature code.
///
/// Object-safe by construction (async methods via [`async_trait`], no generic
/// methods, no `Self`-returning methods), so `Arc<dyn DatabaseAdapter + Send +
/// Sync>` is the canonical handle feature crates hold.
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    /// Return every [`Transaction`] whose `date` falls in the inclusive range
    /// `[from, to]`, in unspecified order.
    ///
    /// The bounds are inclusive on both ends so a caller can ask for an exact
    /// cycle window (`CycleWindow { start, end }`) without off-by-one fudging.
    /// An empty range (no transactions) is a successful empty `Vec`, not an
    /// error.
    ///
    /// # Errors
    /// Returns a [`PhoskError`] if the underlying store fails to answer the
    /// query (e.g. an adapter I/O fault mapped into the taxonomy). A `from`
    /// later than `to` is the adapter's contract to reject as
    /// [`PhoskError::Invalid`] if it cannot satisfy it; implementations should
    /// document their choice.
    async fn transactions_between(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Transaction>, PhoskError>;

    /// Return every configured spending [`Category`] (name + optional cap), in
    /// unspecified order. A store with no categories yields an empty `Vec`.
    ///
    /// # Errors
    /// Returns a [`PhoskError`] if the underlying store fails to answer.
    async fn categories(&self) -> Result<Vec<Category>, PhoskError>;

    /// Return the global [`BudgetConfig`] (cycle ceiling + savings target).
    ///
    /// This is a single required record; an adapter that has none configured
    /// reports it as [`PhoskError::NotFound`] rather than inventing defaults.
    ///
    /// # Errors
    /// Returns a [`PhoskError`] if no config exists or the store fails to
    /// answer.
    async fn budget_config(&self) -> Result<BudgetConfig, PhoskError>;

    // ── Ledger ───────────────────────────────────────────────────────────────

    /// Every [`Receipt`] whose `date` falls in the inclusive range `[from, to]`.
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails or `from > to`.
    async fn receipts_between(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Receipt>, PhoskError>;

    /// The [`Receipt`] with the given typed id.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if no such receipt exists.
    async fn receipt(&self, id: ReceiptId) -> Result<Receipt, PhoskError>;

    /// The [`Receipt`] with the given stable seed/UI `slug` (`"t1"`).
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if no receipt carries that slug.
    async fn receipt_by_slug(&self, slug: &str) -> Result<Receipt, PhoskError>;

    /// The [`LineItem`]s of a [`Receipt`], in stored order.
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn line_items(&self, receipt: ReceiptId) -> Result<Vec<LineItem>, PhoskError>;

    /// Every [`Receipt`] regardless of date (unfiltered list / shop directory).
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn all_receipts(&self) -> Result<Vec<Receipt>, PhoskError>;

    /// Persist a [`Receipt`] together with its [`LineItem`]s; returns its id.
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn insert_receipt(
        &self,
        r: Receipt,
        lines: Vec<LineItem>,
    ) -> Result<ReceiptId, PhoskError>;

    /// Replace a stored [`LineItem`] with a corrected version.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if the line does not exist.
    async fn update_line_item(&self, line: LineItem) -> Result<(), PhoskError>;

    /// Append a [`CorrectionEvent`] to the audit log.
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn record_correction(&self, ev: CorrectionEvent) -> Result<(), PhoskError>;

    // ── Signals ──────────────────────────────────────────────────────────────

    /// Every [`Signal`] (tracked + candidates).
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn signals(&self) -> Result<Vec<Signal>, PhoskError>;

    /// The [`Signal`] with the given typed id.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if absent.
    async fn signal(&self, id: SignalId) -> Result<Signal, PhoskError>;

    /// The [`Signal`] with the given slug (`"coffee"`).
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if absent.
    async fn signal_by_slug(&self, slug: &str) -> Result<Signal, PhoskError>;

    /// Every [`SignalOccurrence`] rolled into a signal.
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn signal_occurrences(&self, id: SignalId) -> Result<Vec<SignalOccurrence>, PhoskError>;

    /// Flip a signal's `tracked` flag (track a candidate / dismiss a signal).
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if no such signal.
    async fn set_signal_tracked(&self, id: SignalId, tracked: bool) -> Result<(), PhoskError>;

    /// Remove a signal entirely (used to dismiss a candidate so it leaves both
    /// the tracked and the candidate lists without being promoted).
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if no such signal.
    async fn delete_signal(&self, id: SignalId) -> Result<(), PhoskError>;

    // ── Planning ─────────────────────────────────────────────────────────────

    /// Every per-category envelope ([`CategoryCap`]).
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn category_caps(&self) -> Result<Vec<CategoryCap>, PhoskError>;

    /// The [`CategoryCap`] with the given category `name`.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if absent.
    async fn category_cap_by_name(&self, name: &str) -> Result<CategoryCap, PhoskError>;

    /// Set (or clear, with `None`) a category's cap by name.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if no such category.
    async fn set_category_cap(&self, name: &str, cap: Option<Money>) -> Result<(), PhoskError>;

    /// The prior-cycle [`BudgetHistory`] rows for a category name (oldest→newest).
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn budget_history(&self, category: &str) -> Result<Vec<BudgetHistory>, PhoskError>;

    /// Every [`Alert`].
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn alerts(&self) -> Result<Vec<Alert>, PhoskError>;

    /// The [`Alert`] with the given typed id.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if absent.
    async fn alert(&self, id: AlertId) -> Result<Alert, PhoskError>;

    /// Update an alert's `status` (`"active" | "dismissed" | "snoozed"`).
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if no such alert.
    async fn update_alert_status(&self, id: AlertId, status: &str) -> Result<(), PhoskError>;

    // ── Recurring ────────────────────────────────────────────────────────────

    /// Every [`Subscription`].
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn subscriptions(&self) -> Result<Vec<Subscription>, PhoskError>;

    /// The [`Subscription`] with the given typed id.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if absent.
    async fn subscription(&self, id: SubscriptionId) -> Result<Subscription, PhoskError>;

    /// The [`Subscription`] with the given slug (`"netflix"`).
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if absent.
    async fn subscription_by_slug(&self, slug: &str) -> Result<Subscription, PhoskError>;

    /// The recorded [`Charge`]s of a subscription (oldest→newest).
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn subscription_charges(&self, id: SubscriptionId) -> Result<Vec<Charge>, PhoskError>;

    /// Insert or update a [`Subscription`]; returns its id.
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn upsert_subscription(&self, s: Subscription) -> Result<SubscriptionId, PhoskError>;

    /// Delete a [`Subscription`] **and every [`Charge`] recorded against it** —
    /// a charge has no meaning without its subscription, so the cascade is part
    /// of the port contract rather than the caller's job.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if no subscription has that id (so a second
    /// delete of the same id reports it), or any store failure.
    async fn delete_subscription(&self, id: SubscriptionId) -> Result<(), PhoskError>;

    /// Record a billing [`Charge`].
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn record_charge(&self, c: Charge) -> Result<(), PhoskError>;

    // ── Debts ────────────────────────────────────────────────────────────────

    /// Every institutional [`Debt`].
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn debts(&self) -> Result<Vec<Debt>, PhoskError>;

    /// The [`Debt`] with the given typed id.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if absent.
    async fn debt(&self, id: DebtId) -> Result<Debt, PhoskError>;

    /// The [`Debt`] with the given slug (`"vw"`).
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if absent.
    async fn debt_by_slug(&self, slug: &str) -> Result<Debt, PhoskError>;

    /// The recorded [`DebtPayment`]s for a debt (oldest→newest).
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn debt_payments(&self, id: DebtId) -> Result<Vec<DebtPayment>, PhoskError>;

    /// Insert or update a [`Debt`]; returns its id.
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn upsert_debt(&self, d: Debt) -> Result<DebtId, PhoskError>;

    /// Record a [`DebtPayment`].
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn record_debt_payment(&self, p: DebtPayment) -> Result<(), PhoskError>;

    /// Every personal [`PersonalIou`].
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn personal_ious(&self) -> Result<Vec<PersonalIou>, PhoskError>;

    /// Insert or update a [`PersonalIou`]; returns its id.
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn upsert_personal_iou(&self, i: PersonalIou) -> Result<PersonalIouId, PhoskError>;

    // ── Analytics support ────────────────────────────────────────────────────

    /// A fast-path multi-cycle spend history (the service can also resolve
    /// windows itself and call [`Self::receipts_between`] per cycle).
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn spend_history(
        &self,
        cycles: u32,
        as_of: NaiveDate,
    ) -> Result<Vec<BudgetHistory>, PhoskError>;

    // ── Settings ─────────────────────────────────────────────────────────────

    /// Every stored [`Preference`].
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn preferences(&self) -> Result<Vec<Preference>, PhoskError>;

    /// The [`Preference`] for a key.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if the key is unset.
    async fn preference(&self, key: &str) -> Result<Preference, PhoskError>;

    /// Set a preference value (creating the key if absent).
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn set_preference(&self, key: &str, value: &str) -> Result<(), PhoskError>;

    /// Reset a preference to a default value, clearing the user-modified override
    /// (provenance returns to `RuleGenerated`). Creates the key if absent.
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn reset_preference(&self, key: &str, default_value: &str) -> Result<(), PhoskError>;

    // ── AI ───────────────────────────────────────────────────────────────────

    /// Every AI activity [`FeedItem`].
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn feed_items(&self) -> Result<Vec<FeedItem>, PhoskError>;

    /// The [`Message`]s of a chat (oldest→newest).
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn chat_messages(&self, chat: ChatId) -> Result<Vec<Message>, PhoskError>;

    /// The most recent [`Chat`], if any.
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn latest_chat(&self) -> Result<Option<Chat>, PhoskError>;

    /// Dismiss an AI activity [`FeedItem`] by its stringified id
    /// ([`phosk_id::FeedItemId`] rendered via `Display`). Removes it from the feed.
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if no feed item has that id.
    async fn dismiss_feed_item(&self, id: &str) -> Result<(), PhoskError>;

    /// Append a [`Message`] to its chat.
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn append_message(&self, m: Message) -> Result<(), PhoskError>;

    /// Clear all messages of a chat.
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn clear_chat(&self, chat: ChatId) -> Result<(), PhoskError>;

    /// Every candidate [`AiSuggestion`].
    ///
    /// # Errors
    /// [`PhoskError`] if the store fails to answer.
    async fn ai_suggestions(&self) -> Result<Vec<AiSuggestion>, PhoskError>;

    /// Append a machine-proposed [`AiSuggestion`] to the approval queue.
    ///
    /// This is the ONLY write path AI-derived proposals take: the receipt-intake
    /// pipeline and the AI write-tools build a candidate suggestion (always
    /// `status == "open"`) and enqueue it here for human approval — they never
    /// mutate domain state directly. Idempotency is the caller's concern (the
    /// pipeline keys off a content hash); this method appends what it is given.
    ///
    /// # Errors
    /// [`PhoskError`] if the store rejects the write.
    async fn enqueue_suggestion(&self, s: AiSuggestion) -> Result<SuggestionId, PhoskError>;

    /// Update a suggestion's `status` (`"open" | "accepted" | "dismissed"`).
    ///
    /// # Errors
    /// [`PhoskError::NotFound`] if no such suggestion.
    async fn update_suggestion_status(
        &self,
        id: SuggestionId,
        status: &str,
    ) -> Result<(), PhoskError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use phosk_core::money::Money;

    /// A minimal stand-in adapter proving the trait is object-safe and that an
    /// `Arc<dyn DatabaseAdapter>` can be built from it and driven. This is *not*
    /// the real in-memory adapter (that is `phosk_db_memory`, a later stage) —
    /// it returns fixed, trivial answers just to exercise the contract shape.
    struct DummyAdapter;

    #[async_trait]
    impl DatabaseAdapter for DummyAdapter {
        async fn transactions_between(
            &self,
            _from: NaiveDate,
            _to: NaiveDate,
        ) -> Result<Vec<Transaction>, PhoskError> {
            Ok(Vec::new())
        }

        async fn categories(&self) -> Result<Vec<Category>, PhoskError> {
            Ok(vec![Category {
                name: "GROCERIES".to_owned(),
                cap: Some(Money::from_chf(800, 0).expect("valid cap")),
            }])
        }

        async fn budget_config(&self) -> Result<BudgetConfig, PhoskError> {
            Ok(BudgetConfig {
                monthly_budget: Money::from_chf(4200, 0).expect("valid budget"),
                savings_target: Money::from_chf(900, 0).expect("valid target"),
            })
        }

        async fn receipts_between(
            &self,
            _from: NaiveDate,
            _to: NaiveDate,
        ) -> Result<Vec<Receipt>, PhoskError> {
            Ok(Vec::new())
        }
        async fn receipt(&self, _id: ReceiptId) -> Result<Receipt, PhoskError> {
            Err(PhoskError::NotFound("receipt".to_owned()))
        }
        async fn receipt_by_slug(&self, _slug: &str) -> Result<Receipt, PhoskError> {
            Err(PhoskError::NotFound("receipt".to_owned()))
        }
        async fn line_items(&self, _receipt: ReceiptId) -> Result<Vec<LineItem>, PhoskError> {
            Ok(Vec::new())
        }
        async fn all_receipts(&self) -> Result<Vec<Receipt>, PhoskError> {
            Ok(Vec::new())
        }
        async fn insert_receipt(
            &self,
            _r: Receipt,
            _lines: Vec<LineItem>,
        ) -> Result<ReceiptId, PhoskError> {
            Ok(ReceiptId::new())
        }
        async fn update_line_item(&self, _line: LineItem) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn record_correction(&self, _ev: CorrectionEvent) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn signals(&self) -> Result<Vec<Signal>, PhoskError> {
            Ok(Vec::new())
        }
        async fn signal(&self, _id: SignalId) -> Result<Signal, PhoskError> {
            Err(PhoskError::NotFound("signal".to_owned()))
        }
        async fn signal_by_slug(&self, _slug: &str) -> Result<Signal, PhoskError> {
            Err(PhoskError::NotFound("signal".to_owned()))
        }
        async fn signal_occurrences(
            &self,
            _id: SignalId,
        ) -> Result<Vec<SignalOccurrence>, PhoskError> {
            Ok(Vec::new())
        }
        async fn set_signal_tracked(
            &self,
            _id: SignalId,
            _tracked: bool,
        ) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn delete_signal(&self, _id: SignalId) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn category_caps(&self) -> Result<Vec<CategoryCap>, PhoskError> {
            Ok(Vec::new())
        }
        async fn category_cap_by_name(&self, _name: &str) -> Result<CategoryCap, PhoskError> {
            Err(PhoskError::NotFound("category".to_owned()))
        }
        async fn set_category_cap(
            &self,
            _name: &str,
            _cap: Option<Money>,
        ) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn budget_history(&self, _category: &str) -> Result<Vec<BudgetHistory>, PhoskError> {
            Ok(Vec::new())
        }
        async fn alerts(&self) -> Result<Vec<Alert>, PhoskError> {
            Ok(Vec::new())
        }
        async fn alert(&self, _id: AlertId) -> Result<Alert, PhoskError> {
            Err(PhoskError::NotFound("alert".to_owned()))
        }
        async fn update_alert_status(&self, _id: AlertId, _status: &str) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn subscriptions(&self) -> Result<Vec<Subscription>, PhoskError> {
            Ok(Vec::new())
        }
        async fn subscription(&self, _id: SubscriptionId) -> Result<Subscription, PhoskError> {
            Err(PhoskError::NotFound("subscription".to_owned()))
        }
        async fn subscription_by_slug(&self, _slug: &str) -> Result<Subscription, PhoskError> {
            Err(PhoskError::NotFound("subscription".to_owned()))
        }
        async fn subscription_charges(
            &self,
            _id: SubscriptionId,
        ) -> Result<Vec<Charge>, PhoskError> {
            Ok(Vec::new())
        }
        async fn upsert_subscription(
            &self,
            _s: Subscription,
        ) -> Result<SubscriptionId, PhoskError> {
            Ok(SubscriptionId::new())
        }
        async fn delete_subscription(&self, _id: SubscriptionId) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn record_charge(&self, _c: Charge) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn debts(&self) -> Result<Vec<Debt>, PhoskError> {
            Ok(Vec::new())
        }
        async fn debt(&self, _id: DebtId) -> Result<Debt, PhoskError> {
            Err(PhoskError::NotFound("debt".to_owned()))
        }
        async fn debt_by_slug(&self, _slug: &str) -> Result<Debt, PhoskError> {
            Err(PhoskError::NotFound("debt".to_owned()))
        }
        async fn debt_payments(&self, _id: DebtId) -> Result<Vec<DebtPayment>, PhoskError> {
            Ok(Vec::new())
        }
        async fn upsert_debt(&self, _d: Debt) -> Result<DebtId, PhoskError> {
            Ok(DebtId::new())
        }
        async fn record_debt_payment(&self, _p: DebtPayment) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn personal_ious(&self) -> Result<Vec<PersonalIou>, PhoskError> {
            Ok(Vec::new())
        }
        async fn upsert_personal_iou(&self, _i: PersonalIou) -> Result<PersonalIouId, PhoskError> {
            Ok(PersonalIouId::new())
        }
        async fn spend_history(
            &self,
            _cycles: u32,
            _as_of: NaiveDate,
        ) -> Result<Vec<BudgetHistory>, PhoskError> {
            Ok(Vec::new())
        }
        async fn preferences(&self) -> Result<Vec<Preference>, PhoskError> {
            Ok(Vec::new())
        }
        async fn preference(&self, _key: &str) -> Result<Preference, PhoskError> {
            Err(PhoskError::NotFound("preference".to_owned()))
        }
        async fn set_preference(&self, _key: &str, _value: &str) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn reset_preference(
            &self,
            _key: &str,
            _default_value: &str,
        ) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn feed_items(&self) -> Result<Vec<FeedItem>, PhoskError> {
            Ok(Vec::new())
        }
        async fn chat_messages(&self, _chat: ChatId) -> Result<Vec<Message>, PhoskError> {
            Ok(Vec::new())
        }
        async fn latest_chat(&self) -> Result<Option<Chat>, PhoskError> {
            Ok(None)
        }
        async fn dismiss_feed_item(&self, _id: &str) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn append_message(&self, _m: Message) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn clear_chat(&self, _chat: ChatId) -> Result<(), PhoskError> {
            Ok(())
        }
        async fn ai_suggestions(&self) -> Result<Vec<AiSuggestion>, PhoskError> {
            Ok(Vec::new())
        }
        async fn enqueue_suggestion(&self, s: AiSuggestion) -> Result<SuggestionId, PhoskError> {
            Ok(s.id)
        }
        async fn update_suggestion_status(
            &self,
            _id: SuggestionId,
            _status: &str,
        ) -> Result<(), PhoskError> {
            Ok(())
        }
    }

    fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    /// The load-bearing ADR-000 assertion: the port is object-safe, so it can be
    /// held behind a shared `Arc<dyn DatabaseAdapter>` handle (`Send + Sync`).
    #[test]
    fn arc_dyn_database_adapter_is_constructible() {
        let db: Arc<dyn DatabaseAdapter> = Arc::new(DummyAdapter);
        // Use it as a trait object behind the Arc so the test can't be reduced
        // to a concrete-type call by the optimiser/reviewer.
        let _shared: Arc<dyn DatabaseAdapter + Send + Sync> = db;
    }

    #[tokio::test]
    async fn dyn_adapter_methods_are_callable_through_the_trait_object() {
        let db: Arc<dyn DatabaseAdapter> = Arc::new(DummyAdapter);

        let txns = db
            .transactions_between(naive(2026, 6, 1), naive(2026, 6, 30))
            .await
            .expect("query ok");
        assert!(txns.is_empty());

        let cats = db.categories().await.expect("categories ok");
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].name, "GROCERIES");

        let cfg = db.budget_config().await.expect("config ok");
        assert_eq!(cfg.monthly_budget.centimes(), 420_000);
        assert_eq!(cfg.savings_target.centimes(), 90_000);
    }
}
