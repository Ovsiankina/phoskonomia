//! `phosk_db_memory` — the in-memory [`DatabaseAdapter`] (ADR-005, L3 adapter).
//!
//! [`MemoryDb`] is a swappable, fully in-process implementation of the
//! `phosk_adapter_db` PORT. It holds plain `Vec`s of [`Transaction`] /
//! [`Category`] plus a single [`BudgetConfig`], answers the read-only dashboard
//! queries directly from them, and ships a deterministic Swiss seed
//! ([`MemoryDb::seeded`]) covering the May 2026 and June 2026 cycles so the
//! dashboard slice and its tests have realistic, fixed data to run against.
//!
//! **Layering (ADR-010).** This is an L3 concrete adapter: it depends on the
//! PORT trait crate (`phosk_adapter_db`) and the domain (`phosk_model`,
//! `phosk_core`), never the other way round. It is wired in **only** by a
//! `bin/*` composition root and is **never** imported by feature crates — they
//! see only `&dyn DatabaseAdapter` / `Arc<dyn DatabaseAdapter>`.
//!
//! **No panics (ADR §0).** Construction maps every fallible step
//! ([`Money::from_chf`] for the seed amounts, [`NaiveDate::from_ymd_opt`] for
//! the seed dates) explicitly into a [`PhoskError`]; there is no `unwrap`,
//! `expect`, or `panic!` in non-test code.
//!
//! ## Seed provenance
//!
//! The seed's transaction **line-items** are the ground truth: the per-day
//! series and the asserted cycle totals are computed *from* them. The SEED
//! SPEC's prose summaries (rolled-up category totals, a "spent to date"
//! headline, a top-shops ranking) are internally inconsistent with that
//! line-item list, so they are **not** used as authoritative numbers here — see
//! the crate's test module for the totals the line-items actually produce.

// The write-path methods deliberately hold a `Mutex` guard across a `find` +
// mutate; the guard's lifetime IS the critical section, so the nursery
// `significant_drop_tightening` lint's suggestion does not apply. `clone_into`
// micro-opts are not worth obscuring the simple `= x.to_owned()` writes.
#![allow(clippy::significant_drop_tightening, clippy::assigning_clones)]

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::NaiveDate;
use phosk_adapter_db::DatabaseAdapter;
use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_id::{
    AlertId, CategoryId, ChatId, DebtId, LineItemId, PersonalIouId, ReceiptId, SignalId,
    SubscriptionId, SuggestionId,
};
use phosk_model::{
    AiSuggestion, Alert, BudgetConfig, BudgetHistory, Category, CategoryCap, Charge, Chat,
    CorrectionEvent, Debt, DebtPayment, FeedItem, LineItem, Message, PersonalIou, Preference,
    Provenance, Receipt, Signal, SignalOccurrence, Source, Subscription, Transaction,
};

mod seed;

/// An in-memory [`DatabaseAdapter`].
///
/// The dashboard's read-only data held in plain `Vec`s, with a single
/// [`BudgetConfig`]. Construct an empty one with [`MemoryDb::new`], or the
/// deterministic Swiss fixture with [`MemoryDb::seeded`].
#[derive(Debug)]
pub struct MemoryDb {
    transactions: Vec<Transaction>,
    // The dashboard `Transaction` projection of every receipt written through
    // `insert_receipt`, keyed by the stored receipt id so a re-import replaces
    // its row. Read back alongside `transactions` by `transactions_between`.
    receipt_transactions: Mutex<Vec<(ReceiptId, Transaction)>>,
    categories: Vec<Category>,
    budget: BudgetConfig,
    // Richer write-side entities (ADR-008 / locked decision #3). Mutable
    // collections sit behind a `Mutex` so the `&self` async port methods can
    // record writes without an `&mut self` (the trait is `&self`-only).
    // Receipts sit behind a `Mutex` so the `&self` write path (`insert_receipt`)
    // can append/replace without `&mut self` (the port trait is `&self`-only).
    receipts: Mutex<Vec<Receipt>>,
    line_items: Mutex<Vec<LineItem>>,
    corrections: Mutex<Vec<CorrectionEvent>>,
    signals: Mutex<Vec<Signal>>,
    signal_occurrences: Vec<SignalOccurrence>,
    category_caps: Mutex<Vec<CategoryCap>>,
    budget_history: Vec<BudgetHistory>,
    alerts: Mutex<Vec<Alert>>,
    subscriptions: Mutex<Vec<Subscription>>,
    charges: Mutex<Vec<Charge>>,
    debts: Mutex<Vec<Debt>>,
    debt_payments: Mutex<Vec<DebtPayment>>,
    personal_ious: Mutex<Vec<PersonalIou>>,
    preferences: Mutex<Vec<Preference>>,
    feed_items: Mutex<Vec<FeedItem>>,
    chats: Vec<Chat>,
    messages: Mutex<Vec<Message>>,
    ai_suggestions: Mutex<Vec<AiSuggestion>>,
}

impl MemoryDb {
    /// Build a `MemoryDb` from the dashboard trio; all richer collections start
    /// empty. The transactions/categories may be in any order; range queries
    /// filter on the fly.
    #[must_use]
    pub const fn new(
        transactions: Vec<Transaction>,
        categories: Vec<Category>,
        budget: BudgetConfig,
    ) -> Self {
        Self {
            transactions,
            receipt_transactions: Mutex::new(Vec::new()),
            categories,
            budget,
            receipts: Mutex::new(Vec::new()),
            line_items: Mutex::new(Vec::new()),
            corrections: Mutex::new(Vec::new()),
            signals: Mutex::new(Vec::new()),
            signal_occurrences: Vec::new(),
            category_caps: Mutex::new(Vec::new()),
            budget_history: Vec::new(),
            alerts: Mutex::new(Vec::new()),
            subscriptions: Mutex::new(Vec::new()),
            charges: Mutex::new(Vec::new()),
            debts: Mutex::new(Vec::new()),
            debt_payments: Mutex::new(Vec::new()),
            personal_ious: Mutex::new(Vec::new()),
            preferences: Mutex::new(Vec::new()),
            feed_items: Mutex::new(Vec::new()),
            chats: Vec::new(),
            messages: Mutex::new(Vec::new()),
            ai_suggestions: Mutex::new(Vec::new()),
        }
    }

    /// The deterministic Swiss seed: the May 2026 and June 2026 cycles, the
    /// eight category caps, and the global CHF 4200 / CHF 900 budget config.
    ///
    /// The transaction line-items below are the canonical source of all derived
    /// figures (daily series, cycle totals); see the module docs on seed
    /// provenance.
    ///
    /// # Errors
    /// Returns a [`PhoskError`] only if a hard-coded seed amount or date is
    /// somehow invalid (it never is for the values below) — this is surfaced
    /// rather than panicked per the no-panic rule (ADR §0).
    pub fn seeded() -> Result<Self, PhoskError> {
        let transactions = seed_transactions()?;
        let categories = seed_categories()?;
        let budget = seed_budget()?;

        let (receipts, line_items) = seed::seed_receipts_and_lines()?;
        let signals = seed::seed_signals()?;
        let signal_occurrences = seed::seed_signal_occurrences(&signals)?;
        let category_caps = seed::seed_category_caps()?;
        let budget_history = seed::seed_budget_history(&category_caps)?;
        let alerts = seed::seed_alerts()?;
        let subscriptions = seed::seed_subscriptions()?;
        let charges = seed::seed_charges(&subscriptions)?;
        let debts = seed::seed_debts()?;
        let debt_payments = seed::seed_debt_payments(&debts)?;
        let personal_ious = seed::seed_personal_ious()?;
        let preferences = seed::seed_preferences();
        let feed_items = seed::seed_feed_items()?;
        let (chats, messages) = seed::seed_chats_and_messages()?;
        let ai_suggestions = seed::seed_ai_suggestions()?;

        Ok(Self {
            transactions,
            receipt_transactions: Mutex::new(Vec::new()),
            categories,
            budget,
            receipts: Mutex::new(receipts),
            line_items: Mutex::new(line_items),
            corrections: Mutex::new(Vec::new()),
            signals: Mutex::new(signals),
            signal_occurrences,
            category_caps: Mutex::new(category_caps),
            budget_history,
            alerts: Mutex::new(alerts),
            subscriptions: Mutex::new(subscriptions),
            charges: Mutex::new(charges),
            debts: Mutex::new(debts),
            debt_payments: Mutex::new(debt_payments),
            personal_ious: Mutex::new(personal_ious),
            preferences: Mutex::new(preferences),
            feed_items: Mutex::new(feed_items),
            chats,
            messages: Mutex::new(messages),
            ai_suggestions: Mutex::new(ai_suggestions),
        })
    }
}

impl MemoryDb {
    /// Whether any stored row still names `category` — the guard behind
    /// `delete_category` (receipts, their lines, subscriptions and signals).
    fn category_is_referenced(&self, category: &str) -> Result<bool, PhoskError> {
        if lock(&self.receipts)?.iter().any(|r| r.category == category) {
            return Ok(true);
        }
        if lock(&self.line_items)?
            .iter()
            .any(|l| l.category == category)
        {
            return Ok(true);
        }
        if lock(&self.subscriptions)?
            .iter()
            .any(|s| s.category == category)
        {
            return Ok(true);
        }
        Ok(lock(&self.signals)?.iter().any(|s| s.parent == category))
    }
}

/// Lock a `Mutex` collection, mapping poisoning to a [`PhoskError`] (no panic).
fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, PhoskError> {
    m.lock()
        .map_err(|_| PhoskError::Invalid("in-memory store lock poisoned".to_owned()))
}

#[async_trait]
impl DatabaseAdapter for MemoryDb {
    #[tracing::instrument(level = "debug", skip_all, fields(from = %from, to = %to))]
    async fn transactions_between(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Transaction>, PhoskError> {
        if from > to {
            return Err(PhoskError::Invalid(format!(
                "transactions_between: from ({from}) is after to ({to})"
            )));
        }
        let mut matched: Vec<Transaction> = self
            .transactions
            .iter()
            .filter(|tx| tx.date >= from && tx.date <= to)
            .cloned()
            .collect();
        // Plus the projection of every receipt written through `insert_receipt`
        // (port contract) — a manually created or approved spend is a
        // transaction like any other.
        matched.extend(
            lock(&self.receipt_transactions)?
                .iter()
                .filter(|(_, tx)| tx.date >= from && tx.date <= to)
                .map(|(_, tx)| tx.clone()),
        );
        tracing::debug!(count = matched.len(), "filtered transactions in window");
        Ok(matched)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn categories(&self) -> Result<Vec<Category>, PhoskError> {
        tracing::debug!(count = self.categories.len(), "returning categories");
        Ok(self.categories.clone())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn budget_config(&self) -> Result<BudgetConfig, PhoskError> {
        tracing::debug!("returning budget config");
        Ok(self.budget.clone())
    }

    // ── Ledger ───────────────────────────────────────────────────────────────

    async fn receipts_between(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Receipt>, PhoskError> {
        if from > to {
            return Err(PhoskError::Invalid(format!(
                "receipts_between: from ({from}) is after to ({to})"
            )));
        }
        Ok(lock(&self.receipts)?
            .iter()
            .filter(|r| r.date >= from && r.date <= to)
            .cloned()
            .collect())
    }

    async fn receipt(&self, id: ReceiptId) -> Result<Receipt, PhoskError> {
        lock(&self.receipts)?
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("receipt {id}")))
    }

    async fn receipt_by_slug(&self, slug: &str) -> Result<Receipt, PhoskError> {
        lock(&self.receipts)?
            .iter()
            .find(|r| r.slug == slug)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("receipt slug {slug}")))
    }

    async fn line_items(&self, receipt: ReceiptId) -> Result<Vec<LineItem>, PhoskError> {
        let lines = lock(&self.line_items)?;
        Ok(lines
            .iter()
            .filter(|l| l.receipt_id == receipt)
            .cloned()
            .collect())
    }

    async fn all_receipts(&self) -> Result<Vec<Receipt>, PhoskError> {
        Ok(lock(&self.receipts)?.clone())
    }

    async fn insert_receipt(
        &self,
        r: Receipt,
        lines: Vec<LineItem>,
    ) -> Result<ReceiptId, PhoskError> {
        // ADR idempotency: a receipt's stable `slug` is its idempotency key. A
        // re-import of the same slug REPLACES the prior receipt and its lines in
        // place (same id), rather than creating a duplicate; a fresh slug is
        // appended. The returned id is always the stored receipt's id.
        let id = r.id;
        let slug = r.slug.clone();

        // Lock receipts, then lines, then the dashboard projection, in a fixed
        // order to avoid deadlock.
        let mut receipts = lock(&self.receipts)?;
        let mut all_lines = lock(&self.line_items)?;

        let (stored_id, stored) = if let Some(slot) = receipts.iter_mut().find(|x| x.slug == slug) {
            // Replace the existing receipt's payload but keep its stable id so
            // existing relations (line_items.receipt_id) stay valid.
            let existing_id = slot.id;
            let mut replacement = r;
            replacement.id = existing_id;
            *slot = replacement;
            // Drop the old lines for this receipt and re-attach the new set.
            all_lines.retain(|l| l.receipt_id != existing_id);
            for mut line in lines {
                line.receipt_id = existing_id;
                all_lines.push(line);
            }
            (existing_id, slot.clone())
        } else {
            for mut line in lines {
                // Defensive: bind every incoming line to this receipt's id so a
                // caller that minted lines before the receipt id still links.
                line.receipt_id = id;
                all_lines.push(line);
            }
            receipts.push(r.clone());
            (id, r)
        };

        // Maintain the dashboard `Transaction` projection of the receipt (port
        // contract) so the written spend reaches `transactions_between` and the
        // cycle aggregates. Keyed on the stored receipt id, so a re-import
        // replaces its row instead of double-counting the spend.
        let projected = Transaction {
            date: stored.date,
            shop: stored.shop,
            category: stored.category,
            amount: stored.amount,
        };
        let mut mirror = lock(&self.receipt_transactions)?;
        if let Some(slot) = mirror.iter_mut().find(|(rid, _)| *rid == stored_id) {
            slot.1 = projected;
        } else {
            mirror.push((stored_id, projected));
        }
        Ok(stored_id)
    }

    async fn update_line_item(&self, line: LineItem) -> Result<(), PhoskError> {
        let mut lines = lock(&self.line_items)?;
        let slot = lines
            .iter_mut()
            .find(|l| l.id == line.id)
            .ok_or_else(|| PhoskError::NotFound(format!("line item {}", line.id)))?;
        *slot = line;
        Ok(())
    }

    async fn record_correction(&self, ev: CorrectionEvent) -> Result<(), PhoskError> {
        lock(&self.corrections)?.push(ev);
        Ok(())
    }

    // ── Signals ──────────────────────────────────────────────────────────────

    async fn signals(&self) -> Result<Vec<Signal>, PhoskError> {
        Ok(lock(&self.signals)?.clone())
    }

    async fn signal(&self, id: SignalId) -> Result<Signal, PhoskError> {
        lock(&self.signals)?
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("signal {id}")))
    }

    async fn signal_by_slug(&self, slug: &str) -> Result<Signal, PhoskError> {
        lock(&self.signals)?
            .iter()
            .find(|s| s.slug == slug)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("signal slug {slug}")))
    }

    async fn signal_occurrences(&self, id: SignalId) -> Result<Vec<SignalOccurrence>, PhoskError> {
        Ok(self
            .signal_occurrences
            .iter()
            .filter(|o| o.signal_id == id)
            .cloned()
            .collect())
    }

    async fn set_signal_tracked(&self, id: SignalId, tracked: bool) -> Result<(), PhoskError> {
        let mut signals = lock(&self.signals)?;
        let s = signals
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| PhoskError::NotFound(format!("signal {id}")))?;
        s.tracked = tracked;
        Ok(())
    }

    async fn delete_signal(&self, id: SignalId) -> Result<(), PhoskError> {
        let mut signals = lock(&self.signals)?;
        let before = signals.len();
        signals.retain(|s| s.id != id);
        if signals.len() == before {
            return Err(PhoskError::NotFound(format!("signal {id}")));
        }
        Ok(())
    }

    // ── Planning ─────────────────────────────────────────────────────────────

    async fn category_caps(&self) -> Result<Vec<CategoryCap>, PhoskError> {
        Ok(lock(&self.category_caps)?.clone())
    }

    async fn category_cap_by_name(&self, name: &str) -> Result<CategoryCap, PhoskError> {
        lock(&self.category_caps)?
            .iter()
            .find(|c| c.name == name)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("category {name}")))
    }

    async fn set_category_cap(&self, name: &str, cap: Option<Money>) -> Result<(), PhoskError> {
        let mut caps = lock(&self.category_caps)?;
        let c = caps
            .iter_mut()
            .find(|c| c.name == name)
            .ok_or_else(|| PhoskError::NotFound(format!("category {name}")))?;
        c.cap = cap;
        c.provenance = Provenance {
            source: Source::UserModified,
            confidence: 1.0,
        };
        Ok(())
    }

    async fn insert_category(&self, c: CategoryCap) -> Result<CategoryId, PhoskError> {
        let mut caps = lock(&self.category_caps)?;
        if caps.iter().any(|x| x.name == c.name) {
            return Err(PhoskError::Invalid(format!(
                "category {} already exists",
                c.name
            )));
        }
        let id = c.id;
        caps.push(c);
        Ok(id)
    }

    async fn rename_category(&self, from: &str, to: &str) -> Result<(), PhoskError> {
        // The guards run before anything is written, so a rejected rename
        // leaves every collection untouched. The locks are taken in the same
        // order as everywhere else in this file (caps → receipts → lines →
        // subscriptions → signals) and held for the whole re-point, so a
        // concurrent reader sees the rename all at once, never half of it.
        let mut caps = lock(&self.category_caps)?;
        if !caps.iter().any(|c| c.name == from) {
            return Err(PhoskError::NotFound(format!("category {from}")));
        }
        if from == to {
            return Ok(());
        }
        if caps.iter().any(|c| c.name == to) {
            return Err(PhoskError::Invalid(format!("category {to} already exists")));
        }
        {
            let target = caps
                .iter_mut()
                .find(|c| c.name == from)
                .ok_or_else(|| PhoskError::NotFound(format!("category {from}")))?;
            target.name = to.to_owned();
            target.provenance = Provenance {
                source: Source::UserModified,
                confidence: 1.0,
            };
        }

        let mut receipts = lock(&self.receipts)?;
        let mut lines = lock(&self.line_items)?;
        let mut subscriptions = lock(&self.subscriptions)?;
        let mut signals = lock(&self.signals)?;
        for r in receipts.iter_mut().filter(|r| r.category == from) {
            r.category = to.to_owned();
        }
        for l in lines.iter_mut().filter(|l| l.category == from) {
            l.category = to.to_owned();
        }
        for s in subscriptions.iter_mut().filter(|s| s.category == from) {
            s.category = to.to_owned();
        }
        for s in signals.iter_mut().filter(|s| s.parent == from) {
            s.parent = to.to_owned();
        }
        Ok(())
    }

    async fn delete_category(&self, name: &str) -> Result<(), PhoskError> {
        let mut caps = lock(&self.category_caps)?;
        if !caps.iter().any(|c| c.name == name) {
            return Err(PhoskError::NotFound(format!("category {name}")));
        }
        if self.category_is_referenced(name)? {
            return Err(PhoskError::Invalid(format!(
                "category {name} is still referenced"
            )));
        }
        caps.retain(|c| c.name != name);
        Ok(())
    }

    async fn merge_categories(&self, from: &str, into: &str) -> Result<u32, PhoskError> {
        // Same shape as `rename_category`: every guard before the first write,
        // the locks taken in this file's usual order (caps → receipts → lines
        // → subscriptions → signals) and all held to the end, so a concurrent
        // reader sees the merge whole — never a store where the source record
        // is already gone but its history has not moved.
        let mut caps = lock(&self.category_caps)?;
        if !caps.iter().any(|c| c.name == from) {
            return Err(PhoskError::NotFound(format!("category {from}")));
        }
        if !caps.iter().any(|c| c.name == into) {
            return Err(PhoskError::NotFound(format!("category {into}")));
        }
        if from == into {
            return Err(PhoskError::Invalid(format!(
                "category {from} cannot be merged into itself"
            )));
        }

        let mut receipts = lock(&self.receipts)?;
        let mut lines = lock(&self.line_items)?;
        let mut subscriptions = lock(&self.subscriptions)?;
        let mut signals = lock(&self.signals)?;
        let mut moved: u32 = 0;
        for r in receipts.iter_mut().filter(|r| r.category == from) {
            r.category = into.to_owned();
            moved = moved.saturating_add(1);
        }
        for l in lines.iter_mut().filter(|l| l.category == from) {
            l.category = into.to_owned();
            moved = moved.saturating_add(1);
        }
        for s in subscriptions.iter_mut().filter(|s| s.category == from) {
            s.category = into.to_owned();
            moved = moved.saturating_add(1);
        }
        for s in signals.iter_mut().filter(|s| s.parent == from) {
            s.parent = into.to_owned();
            moved = moved.saturating_add(1);
        }

        if let Some(target) = caps.iter_mut().find(|c| c.name == into) {
            target.provenance = Provenance {
                source: Source::UserModified,
                confidence: 1.0,
            };
        }
        caps.retain(|c| c.name != from);
        Ok(moved)
    }

    async fn split_category(
        &self,
        from: &str,
        new: CategoryCap,
        lines: &[LineItemId],
    ) -> Result<u32, PhoskError> {
        // Both locks are taken before the first write and held across it, so
        // the new category and the lines it carries appear together.
        let mut caps = lock(&self.category_caps)?;
        if !caps.iter().any(|c| c.name == from) {
            return Err(PhoskError::NotFound(format!("category {from}")));
        }
        if caps.iter().any(|c| c.name == new.name) {
            return Err(PhoskError::Invalid(format!(
                "category {} already exists",
                new.name
            )));
        }

        let mut items = lock(&self.line_items)?;
        for id in lines {
            let line = items
                .iter()
                .find(|l| l.id == *id)
                .ok_or_else(|| PhoskError::NotFound(format!("line item {id}")))?;
            if line.category != from {
                return Err(PhoskError::Invalid(format!(
                    "line item {id} is not in category {from}"
                )));
            }
        }

        let mut moved: u32 = 0;
        for l in items.iter_mut().filter(|l| lines.contains(&l.id)) {
            l.category = new.name.clone();
            l.provenance = Provenance::user_modified();
            moved = moved.saturating_add(1);
        }
        caps.push(new);
        Ok(moved)
    }

    async fn budget_history(&self, category: &str) -> Result<Vec<BudgetHistory>, PhoskError> {
        // Resolve the category name → id via the caps, then filter history.
        let caps = lock(&self.category_caps)?;
        let Some(cap) = caps.iter().find(|c| c.name == category) else {
            return Ok(Vec::new());
        };
        let id = cap.id;
        drop(caps);
        Ok(self
            .budget_history
            .iter()
            .filter(|h| h.category_id == id)
            .cloned()
            .collect())
    }

    async fn alerts(&self) -> Result<Vec<Alert>, PhoskError> {
        Ok(lock(&self.alerts)?.clone())
    }

    async fn alert(&self, id: AlertId) -> Result<Alert, PhoskError> {
        lock(&self.alerts)?
            .iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("alert {id}")))
    }

    async fn update_alert_status(&self, id: AlertId, status: &str) -> Result<(), PhoskError> {
        let mut alerts = lock(&self.alerts)?;
        let a = alerts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| PhoskError::NotFound(format!("alert {id}")))?;
        a.status = status.to_owned();
        Ok(())
    }

    // ── Recurring ────────────────────────────────────────────────────────────

    async fn subscriptions(&self) -> Result<Vec<Subscription>, PhoskError> {
        Ok(lock(&self.subscriptions)?.clone())
    }

    async fn subscription(&self, id: SubscriptionId) -> Result<Subscription, PhoskError> {
        lock(&self.subscriptions)?
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("subscription {id}")))
    }

    async fn subscription_by_slug(&self, slug: &str) -> Result<Subscription, PhoskError> {
        lock(&self.subscriptions)?
            .iter()
            .find(|s| s.slug == slug)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("subscription slug {slug}")))
    }

    async fn subscription_charges(&self, id: SubscriptionId) -> Result<Vec<Charge>, PhoskError> {
        let charges = lock(&self.charges)?;
        Ok(charges
            .iter()
            .filter(|c| c.subscription_id == id)
            .cloned()
            .collect())
    }

    async fn upsert_subscription(&self, s: Subscription) -> Result<SubscriptionId, PhoskError> {
        let mut subs = lock(&self.subscriptions)?;
        let id = s.id;
        if let Some(slot) = subs.iter_mut().find(|x| x.id == s.id) {
            *slot = s;
        } else {
            subs.push(s);
        }
        Ok(id)
    }

    async fn delete_subscription(&self, id: SubscriptionId) -> Result<(), PhoskError> {
        let mut subs = lock(&self.subscriptions)?;
        let before = subs.len();
        subs.retain(|s| s.id != id);
        if subs.len() == before {
            return Err(PhoskError::NotFound(format!("subscription {id}")));
        }
        // Cascade: a charge without its subscription is an orphan.
        lock(&self.charges)?.retain(|c| c.subscription_id != id);
        Ok(())
    }

    async fn record_charge(&self, c: Charge) -> Result<(), PhoskError> {
        lock(&self.charges)?.push(c);
        Ok(())
    }

    // ── Debts ────────────────────────────────────────────────────────────────

    async fn debts(&self) -> Result<Vec<Debt>, PhoskError> {
        Ok(lock(&self.debts)?.clone())
    }

    async fn debt(&self, id: DebtId) -> Result<Debt, PhoskError> {
        lock(&self.debts)?
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("debt {id}")))
    }

    async fn debt_by_slug(&self, slug: &str) -> Result<Debt, PhoskError> {
        lock(&self.debts)?
            .iter()
            .find(|d| d.slug == slug)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("debt slug {slug}")))
    }

    async fn debt_payments(&self, id: DebtId) -> Result<Vec<DebtPayment>, PhoskError> {
        let payments = lock(&self.debt_payments)?;
        Ok(payments
            .iter()
            .filter(|p| p.debt_id == id)
            .cloned()
            .collect())
    }

    async fn upsert_debt(&self, d: Debt) -> Result<DebtId, PhoskError> {
        let mut debts = lock(&self.debts)?;
        let id = d.id;
        if let Some(slot) = debts.iter_mut().find(|x| x.id == d.id) {
            *slot = d;
        } else {
            debts.push(d);
        }
        Ok(id)
    }

    async fn record_debt_payment(&self, p: DebtPayment) -> Result<(), PhoskError> {
        lock(&self.debt_payments)?.push(p);
        Ok(())
    }

    async fn personal_ious(&self) -> Result<Vec<PersonalIou>, PhoskError> {
        Ok(lock(&self.personal_ious)?.clone())
    }

    async fn upsert_personal_iou(&self, i: PersonalIou) -> Result<PersonalIouId, PhoskError> {
        let mut ious = lock(&self.personal_ious)?;
        let id = i.id;
        if let Some(slot) = ious.iter_mut().find(|x| x.id == i.id) {
            *slot = i;
        } else {
            ious.push(i);
        }
        Ok(id)
    }

    // ── Analytics support ────────────────────────────────────────────────────

    async fn spend_history(
        &self,
        _cycles: u32,
        _as_of: NaiveDate,
    ) -> Result<Vec<BudgetHistory>, PhoskError> {
        // The analytics service resolves per-cycle windows itself via
        // `receipts_between`; this fast path returns the seeded history rows.
        Ok(self.budget_history.clone())
    }

    // ── Settings ─────────────────────────────────────────────────────────────

    async fn preferences(&self) -> Result<Vec<Preference>, PhoskError> {
        Ok(lock(&self.preferences)?.clone())
    }

    async fn preference(&self, key: &str) -> Result<Preference, PhoskError> {
        lock(&self.preferences)?
            .iter()
            .find(|p| p.key == key)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("preference {key}")))
    }

    async fn set_preference(&self, key: &str, value: &str) -> Result<(), PhoskError> {
        let mut prefs = lock(&self.preferences)?;
        if let Some(p) = prefs.iter_mut().find(|p| p.key == key) {
            p.value = value.to_owned();
            p.provenance = Provenance {
                source: Source::UserModified,
                confidence: 1.0,
            };
        } else {
            prefs.push(Preference {
                id: phosk_id::PreferenceId::new(),
                key: key.to_owned(),
                value: value.to_owned(),
                surface: String::new(),
                stored_on_device: true,
                provenance: Provenance {
                    source: Source::UserModified,
                    confidence: 1.0,
                },
            });
        }
        Ok(())
    }

    async fn reset_preference(&self, key: &str, default_value: &str) -> Result<(), PhoskError> {
        let mut prefs = lock(&self.preferences)?;
        if let Some(p) = prefs.iter_mut().find(|p| p.key == key) {
            p.value = default_value.to_owned();
            p.provenance = Provenance {
                source: Source::RuleGenerated,
                confidence: 1.0,
            };
        } else {
            prefs.push(Preference {
                id: phosk_id::PreferenceId::new(),
                key: key.to_owned(),
                value: default_value.to_owned(),
                surface: String::new(),
                stored_on_device: true,
                provenance: Provenance {
                    source: Source::RuleGenerated,
                    confidence: 1.0,
                },
            });
        }
        Ok(())
    }

    // ── AI ───────────────────────────────────────────────────────────────────

    async fn feed_items(&self) -> Result<Vec<FeedItem>, PhoskError> {
        Ok(lock(&self.feed_items)?.clone())
    }

    async fn dismiss_feed_item(&self, id: &str) -> Result<(), PhoskError> {
        let mut feed = lock(&self.feed_items)?;
        let before = feed.len();
        feed.retain(|f| f.id.to_string() != id);
        if feed.len() == before {
            return Err(PhoskError::NotFound(format!("feed item {id}")));
        }
        Ok(())
    }

    async fn chat_messages(&self, chat: ChatId) -> Result<Vec<Message>, PhoskError> {
        let messages = lock(&self.messages)?;
        Ok(messages
            .iter()
            .filter(|m| m.chat_id == chat)
            .cloned()
            .collect())
    }

    async fn latest_chat(&self) -> Result<Option<Chat>, PhoskError> {
        Ok(self.chats.iter().max_by_key(|c| c.started).cloned())
    }

    async fn append_message(&self, m: Message) -> Result<(), PhoskError> {
        lock(&self.messages)?.push(m);
        Ok(())
    }

    async fn clear_chat(&self, chat: ChatId) -> Result<(), PhoskError> {
        lock(&self.messages)?.retain(|m| m.chat_id != chat);
        Ok(())
    }

    async fn ai_suggestions(&self) -> Result<Vec<AiSuggestion>, PhoskError> {
        Ok(lock(&self.ai_suggestions)?.clone())
    }

    async fn enqueue_suggestion(&self, s: AiSuggestion) -> Result<SuggestionId, PhoskError> {
        let id = s.id;
        lock(&self.ai_suggestions)?.push(s);
        Ok(id)
    }

    async fn update_suggestion_status(
        &self,
        id: SuggestionId,
        status: &str,
    ) -> Result<(), PhoskError> {
        let mut suggestions = lock(&self.ai_suggestions)?;
        let s = suggestions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| PhoskError::NotFound(format!("suggestion {id}")))?;
        s.status = status.to_owned();
        Ok(())
    }
}

/// Build a seed transaction, mapping every fallible step to a [`PhoskError`].
fn tx(
    year: i32,
    month: u32,
    day: u32,
    shop: &str,
    category: &str,
    chf: i64,
    cents: u8,
) -> Result<Transaction, PhoskError> {
    let date = NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| {
        PhoskError::InvalidDate(format!("seed date {year}-{month:02}-{day:02} is invalid"))
    })?;
    let amount = Money::from_chf(chf, cents)?;
    Ok(Transaction {
        date,
        shop: shop.to_owned(),
        category: category.to_owned(),
        amount,
    })
}

/// Build a seed category with a budget cap, mapping the amount to a [`PhoskError`].
fn category(name: &str, chf: i64, cents: u8) -> Result<Category, PhoskError> {
    Ok(Category {
        name: name.to_owned(),
        cap: Some(Money::from_chf(chf, cents)?),
    })
}

/// The eight Swiss category caps from the SEED SPEC (CHF, whole-franc caps).
fn seed_categories() -> Result<Vec<Category>, PhoskError> {
    Ok(vec![
        category("GROCERIES", 800, 0)?,
        category("DINING & CAFÉS", 350, 0)?,
        category("HOUSING", 1680, 0)?,
        category("HEALTH", 520, 0)?,
        category("TRANSPORT", 280, 0)?,
        category("SUBSCRIPTIONS", 190, 0)?,
        category("UTILITIES", 240, 0)?,
        category("HOUSEHOLD", 200, 0)?,
    ])
}

/// The global budget config: CHF 4200 monthly ceiling, CHF 900 savings target.
fn seed_budget() -> Result<BudgetConfig, PhoskError> {
    Ok(BudgetConfig {
        monthly_budget: Money::from_chf(4200, 0)?,
        savings_target: Money::from_chf(900, 0)?,
    })
}

/// The full May + June 2026 transaction line-items from the SEED SPEC, verbatim.
#[allow(clippy::too_many_lines)] // a flat, verbatim data table is clearer whole.
fn seed_transactions() -> Result<Vec<Transaction>, PhoskError> {
    Ok(vec![
        // ── June 2026 (19 days, 28 line-items) ──────────────────────────────
        // June 1 — fixed / large.
        tx(2026, 6, 1, "Migros", "GROCERIES", 58, 75)?,
        tx(2026, 6, 1, "Landlord", "HOUSING", 1680, 0)?,
        tx(2026, 6, 1, "Krankenkasse", "HEALTH", 318, 0)?,
        // June 2–6.
        tx(2026, 6, 2, "Denner", "GROCERIES", 44, 30)?,
        tx(2026, 6, 2, "Coop", "GROCERIES", 35, 20)?,
        tx(2026, 6, 3, "SBB", "TRANSPORT", 18, 50)?,
        tx(2026, 6, 4, "Starbucks", "DINING & CAFÉS", 12, 80)?,
        tx(2026, 6, 5, "Netflix", "SUBSCRIPTIONS", 14, 95)?,
        tx(2026, 6, 5, "Activ Fitness", "SUBSCRIPTIONS", 89, 0)?,
        tx(2026, 6, 6, "Volg", "GROCERIES", 28, 60)?,
        // June 7–11.
        tx(2026, 6, 7, "Apotheke", "HEALTH", 67, 50)?,
        tx(2026, 6, 8, "Coop", "GROCERIES", 92, 40)?,
        tx(2026, 6, 8, "Avec", "GROCERIES", 16, 80)?,
        tx(2026, 6, 9, "Restaurant Kreuz", "DINING & CAFÉS", 74, 30)?,
        tx(2026, 6, 10, "Manor", "HOUSEHOLD", 48, 0)?,
        tx(2026, 6, 11, "Migros", "GROCERIES", 61, 75)?,
        // June 12–16.
        tx(2026, 6, 12, "Spotify", "SUBSCRIPTIONS", 12, 95)?,
        tx(2026, 6, 12, "Sunrise", "SUBSCRIPTIONS", 45, 0)?,
        tx(2026, 6, 13, "Denner", "GROCERIES", 36, 40)?,
        tx(2026, 6, 14, "Coop", "GROCERIES", 85, 20)?,
        tx(2026, 6, 15, "Apotheke", "HEALTH", 28, 40)?,
        tx(2026, 6, 16, "Galaxus", "HOUSEHOLD", 129, 90)?,
        tx(2026, 6, 16, "Restaurant Linde", "DINING & CAFÉS", 64, 50)?,
        // June 17–19.
        tx(2026, 6, 17, "Swisscom", "SUBSCRIPTIONS", 79, 0)?,
        tx(2026, 6, 18, "Coop Pronto", "GROCERIES", 12, 40)?,
        tx(2026, 6, 18, "Starbucks", "DINING & CAFÉS", 7, 20)?,
        tx(2026, 6, 19, "Migros", "GROCERIES", 53, 85)?,
        tx(2026, 6, 19, "SBB", "TRANSPORT", 6, 80)?,
        // ── May 2026 (30 days, 39 line-items) ───────────────────────────────
        // May 1–5 — fixed / large.
        tx(2026, 5, 1, "Migros", "GROCERIES", 56, 30)?,
        tx(2026, 5, 1, "Landlord", "HOUSING", 1680, 0)?,
        tx(2026, 5, 1, "Krankenkasse", "HEALTH", 318, 0)?,
        tx(2026, 5, 2, "Denner", "GROCERIES", 42, 50)?,
        tx(2026, 5, 3, "SBB", "TRANSPORT", 21, 0)?,
        tx(2026, 5, 4, "Starbucks", "DINING & CAFÉS", 14, 20)?,
        tx(2026, 5, 5, "Netflix", "SUBSCRIPTIONS", 14, 95)?,
        tx(2026, 5, 5, "Activ Fitness", "SUBSCRIPTIONS", 89, 0)?,
        tx(2026, 5, 5, "Spotify", "SUBSCRIPTIONS", 12, 95)?,
        // May 6–10.
        tx(2026, 5, 6, "Coop", "GROCERIES", 87, 60)?,
        tx(2026, 5, 7, "Apotheke", "HEALTH", 52, 80)?,
        tx(2026, 5, 8, "Avec", "GROCERIES", 14, 90)?,
        tx(2026, 5, 9, "Restaurant Kreuz", "DINING & CAFÉS", 74, 30)?,
        tx(2026, 5, 10, "Migros", "GROCERIES", 59, 40)?,
        // May 11–15.
        tx(2026, 5, 11, "Manor", "HOUSEHOLD", 45, 50)?,
        tx(2026, 5, 12, "Denner", "GROCERIES", 38, 20)?,
        tx(2026, 5, 13, "Coop", "GROCERIES", 89, 30)?,
        tx(2026, 5, 14, "Apotheke", "HEALTH", 31, 60)?,
        tx(2026, 5, 15, "Swisscom", "SUBSCRIPTIONS", 79, 0)?,
        // May 16–20.
        tx(2026, 5, 16, "Galaxus", "HOUSEHOLD", 110, 20)?,
        tx(2026, 5, 17, "Restaurant Linde", "DINING & CAFÉS", 62, 0)?,
        tx(2026, 5, 18, "Migros", "GROCERIES", 55, 80)?,
        tx(2026, 5, 19, "SBB", "TRANSPORT", 18, 50)?,
        tx(2026, 5, 20, "Starbucks", "DINING & CAFÉS", 13, 90)?,
        // May 21–25.
        tx(2026, 5, 21, "Coop", "GROCERIES", 93, 10)?,
        tx(2026, 5, 22, "Sunrise", "SUBSCRIPTIONS", 45, 0)?,
        tx(2026, 5, 23, "Denner", "GROCERIES", 40, 80)?,
        tx(2026, 5, 24, "Apotheke", "HEALTH", 36, 20)?,
        tx(2026, 5, 25, "Manor", "HOUSEHOLD", 50, 80)?,
        // May 26–30.
        tx(2026, 5, 26, "Avec", "GROCERIES", 16, 50)?,
        tx(2026, 5, 26, "Starbucks", "DINING & CAFÉS", 15, 40)?,
        tx(2026, 5, 27, "Migros", "GROCERIES", 64, 20)?,
        tx(2026, 5, 28, "Restaurant Kreuz", "DINING & CAFÉS", 64, 50)?,
        tx(2026, 5, 29, "Coop", "GROCERIES", 81, 50)?,
        tx(2026, 5, 29, "SBB", "TRANSPORT", 21, 0)?,
        tx(2026, 5, 30, "Apotheke", "HEALTH", 24, 0)?,
        // May utilities (spread across the month).
        tx(2026, 5, 3, "Electricity/Gas provider", "UTILITIES", 89, 20)?,
        tx(2026, 5, 15, "Internet provider", "UTILITIES", 32, 30)?,
        tx(2026, 5, 27, "Water/Sewage", "UTILITIES", 31, 30)?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    /// Inclusive June 2026 cycle window.
    const JUNE_FROM: (i32, u32, u32) = (2026, 6, 1);
    const JUNE_TO: (i32, u32, u32) = (2026, 6, 30);
    /// Inclusive May 2026 cycle window.
    const MAY_FROM: (i32, u32, u32) = (2026, 5, 1);
    const MAY_TO: (i32, u32, u32) = (2026, 5, 31);

    fn total(txns: &[Transaction]) -> Money {
        Money::sum(txns.iter().map(|t| t.amount)).expect("seed total never overflows")
    }

    #[test]
    fn seeded_builds_without_error() {
        let db = MemoryDb::seeded().expect("seed is valid");
        assert!(!db.transactions.is_empty());
    }

    #[tokio::test]
    async fn seeded_has_eight_categories_all_with_caps() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let cats = db.categories().await.expect("categories ok");
        assert_eq!(cats.len(), 8, "SEED SPEC defines eight category caps");
        assert!(
            cats.iter().all(|c| c.cap.is_some()),
            "every seeded category has a budget cap"
        );
        // The eight names, by identity (ADR-008).
        let names: Vec<&str> = cats.iter().map(|c| c.name.as_str()).collect();
        for expected in [
            "GROCERIES",
            "DINING & CAFÉS",
            "HOUSING",
            "HEALTH",
            "TRANSPORT",
            "SUBSCRIPTIONS",
            "UTILITIES",
            "HOUSEHOLD",
        ] {
            assert!(names.contains(&expected), "missing category {expected}");
        }
    }

    #[tokio::test]
    async fn groceries_cap_is_eight_hundred_chf() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let cats = db.categories().await.expect("categories ok");
        let groceries = cats
            .iter()
            .find(|c| c.name == "GROCERIES")
            .expect("GROCERIES present");
        assert_eq!(
            groceries.cap.expect("has cap").centimes(),
            80_000,
            "GROCERIES cap is CHF 800.00"
        );
    }

    #[tokio::test]
    async fn seeded_budget_config_matches_global_spec() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let cfg = db.budget_config().await.expect("config ok");
        assert_eq!(
            cfg.monthly_budget.centimes(),
            420_000,
            "monthly budget is CHF 4200.00"
        );
        assert_eq!(
            cfg.savings_target.centimes(),
            90_000,
            "savings target is CHF 900.00"
        );
    }

    #[tokio::test]
    async fn june_window_holds_28_line_items() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let june = db
            .transactions_between(
                naive(JUNE_FROM.0, JUNE_FROM.1, JUNE_FROM.2),
                naive(JUNE_TO.0, JUNE_TO.1, JUNE_TO.2),
            )
            .await
            .expect("query ok");
        assert_eq!(june.len(), 28, "June 2026 has 28 seeded line-items");
        assert!(
            june.iter()
                .all(|t| t.date.format("%Y-%m").to_string() == "2026-06"),
            "every June-window transaction is dated in June 2026"
        );
    }

    #[tokio::test]
    async fn may_window_holds_39_line_items() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let may = db
            .transactions_between(
                naive(MAY_FROM.0, MAY_FROM.1, MAY_FROM.2),
                naive(MAY_TO.0, MAY_TO.1, MAY_TO.2),
            )
            .await
            .expect("query ok");
        assert_eq!(may.len(), 39, "May 2026 has 39 seeded line-items");
        assert!(
            may.iter()
                .all(|t| t.date.format("%Y-%m").to_string() == "2026-05"),
            "every May-window transaction is dated in May 2026"
        );
    }

    #[tokio::test]
    async fn windows_are_disjoint_and_cover_the_whole_seed() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let june = db
            .transactions_between(
                naive(JUNE_FROM.0, JUNE_FROM.1, JUNE_FROM.2),
                naive(JUNE_TO.0, JUNE_TO.1, JUNE_TO.2),
            )
            .await
            .expect("query ok");
        let may = db
            .transactions_between(
                naive(MAY_FROM.0, MAY_FROM.1, MAY_FROM.2),
                naive(MAY_TO.0, MAY_TO.1, MAY_TO.2),
            )
            .await
            .expect("query ok");
        // 28 + 39 = 67 line-items total, and no transaction lands in both windows.
        assert_eq!(june.len() + may.len(), 67, "whole seed is 67 line-items");
    }

    /// The "known June total assertable from the seed": the June line-items sum
    /// to CHF 3222.45 (= the SEED SPEC's own per-day daily series sum). The
    /// spec's prose "spent to date" headline (2614.40) is inconsistent with its
    /// line-items and is deliberately *not* asserted — the line-items are
    /// canonical. See module docs on seed provenance.
    #[tokio::test]
    async fn june_total_from_line_items_is_3222_45_chf() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let june = db
            .transactions_between(
                naive(JUNE_FROM.0, JUNE_FROM.1, JUNE_FROM.2),
                naive(JUNE_TO.0, JUNE_TO.1, JUNE_TO.2),
            )
            .await
            .expect("query ok");
        assert_eq!(
            total(&june).centimes(),
            322_245,
            "June 2026 line-items sum to CHF 3222.45"
        );
    }

    /// The May line-items sum to CHF 3787.70 (again the line-item ground truth,
    /// not the spec's inconsistent prose total of 3980.20).
    #[tokio::test]
    async fn may_total_from_line_items_is_3787_70_chf() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let may = db
            .transactions_between(
                naive(MAY_FROM.0, MAY_FROM.1, MAY_FROM.2),
                naive(MAY_TO.0, MAY_TO.1, MAY_TO.2),
            )
            .await
            .expect("query ok");
        assert_eq!(
            total(&may).centimes(),
            378_770,
            "May 2026 line-items sum to CHF 3787.70"
        );
    }

    /// The seed's fixed HOUSING rent (CHF 1680.00) is present in both cycles —
    /// the regression anchor called out by the SEED SPEC.
    #[tokio::test]
    async fn housing_rent_is_1680_in_both_cycles() {
        let db = MemoryDb::seeded().expect("seed is valid");
        for (from, to) in [
            (
                naive(JUNE_FROM.0, JUNE_FROM.1, JUNE_FROM.2),
                naive(JUNE_TO.0, JUNE_TO.1, JUNE_TO.2),
            ),
            (
                naive(MAY_FROM.0, MAY_FROM.1, MAY_FROM.2),
                naive(MAY_TO.0, MAY_TO.1, MAY_TO.2),
            ),
        ] {
            let txns = db.transactions_between(from, to).await.expect("query ok");
            let rent = txns
                .iter()
                .find(|t| t.category == "HOUSING")
                .expect("HOUSING rent present");
            assert_eq!(rent.amount.centimes(), 168_000, "rent is CHF 1680.00");
            assert_eq!(rent.shop, "Landlord");
        }
    }

    /// Inclusive filtering on both ends: a single-day window on June 1 returns
    /// exactly that day's three fixed line-items, and a window's `from`/`to`
    /// boundaries are themselves included.
    #[tokio::test]
    async fn date_filter_is_inclusive_on_both_bounds() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let day1 = db
            .transactions_between(naive(2026, 6, 1), naive(2026, 6, 1))
            .await
            .expect("query ok");
        assert_eq!(
            day1.len(),
            3,
            "June 1 has 3 line-items (Migros, rent, insurance)"
        );
        assert_eq!(
            total(&day1).centimes(),
            205_675,
            "June 1 total is CHF 2056.75 (matches the seed's daily series)"
        );

        // A window ending exactly on June 19 includes June 19's transactions.
        let through_19 = db
            .transactions_between(naive(2026, 6, 19), naive(2026, 6, 19))
            .await
            .expect("query ok");
        assert_eq!(through_19.len(), 2, "June 19: Migros + SBB");
    }

    #[tokio::test]
    async fn empty_window_is_ok_and_empty() {
        let db = MemoryDb::seeded().expect("seed is valid");
        // April 2026: before any seeded data.
        let april = db
            .transactions_between(naive(2026, 4, 1), naive(2026, 4, 30))
            .await
            .expect("query ok");
        assert!(april.is_empty(), "no seeded data in April");
    }

    #[tokio::test]
    async fn inverted_window_is_rejected_as_invalid() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let err = db
            .transactions_between(naive(2026, 6, 30), naive(2026, 6, 1))
            .await
            .expect_err("from after to must be rejected");
        assert_eq!(err.http_status(), 400, "an inverted window is caller error");
        assert_eq!(err.code(), "invalid_input");
    }

    #[tokio::test]
    async fn empty_db_answers_all_three_queries() {
        let db = MemoryDb::new(
            Vec::new(),
            Vec::new(),
            BudgetConfig {
                monthly_budget: Money::ZERO,
                savings_target: Money::ZERO,
            },
        );
        assert!(
            db.transactions_between(naive(2026, 6, 1), naive(2026, 6, 30))
                .await
                .expect("query ok")
                .is_empty()
        );
        assert!(db.categories().await.expect("categories ok").is_empty());
        assert_eq!(
            db.budget_config().await.expect("config ok").monthly_budget,
            Money::ZERO
        );
    }

    fn sample_receipt(slug: &str, cents: i64) -> Receipt {
        Receipt {
            id: ReceiptId::new(),
            slug: slug.to_owned(),
            shop: "Migros".to_owned(),
            date: naive(2026, 6, 20),
            category: "Groceries".to_owned(),
            amount: Money::from_centimes(cents),
            fixed: false,
            provenance: Provenance {
                source: Source::Ocr,
                confidence: 0.9,
            },
            source_kind: "PHOTO".to_owned(),
            ocr_engine: "PADDLEOCR".to_owned(),
            ocr_regions: 4,
        }
    }

    fn sample_line(receipt: ReceiptId, name: &str, cents: i64) -> LineItem {
        LineItem {
            id: phosk_id::LineItemId::new(),
            receipt_id: receipt,
            name: name.to_owned(),
            qty: 1.0,
            unit_price: Money::from_centimes(cents),
            line_total: Money::from_centimes(cents),
            category: "Groceries".to_owned(),
            signal_id: None,
            provenance: Provenance {
                source: Source::Ocr,
                confidence: 0.9,
            },
        }
    }

    /// A fresh-slug `insert_receipt` appends the receipt and its lines, both
    /// readable back through the port.
    #[tokio::test]
    async fn insert_receipt_appends_new_receipt_and_lines() {
        let db = MemoryDb::seeded().expect("seed is valid");
        let before = db.all_receipts().await.expect("list ok").len();

        let r = sample_receipt("new1", 1_234);
        let id = r.id;
        let lines = vec![sample_line(id, "Apples", 1_234)];
        let returned = db.insert_receipt(r, lines).await.expect("insert ok");

        assert_eq!(returned, id, "insert returns the stored receipt id");
        let after = db.all_receipts().await.expect("list ok");
        assert_eq!(after.len(), before + 1, "one receipt appended");
        let got = db.receipt_by_slug("new1").await.expect("by slug ok");
        assert_eq!(got.amount.centimes(), 1_234);
        let got_lines = db.line_items(id).await.expect("lines ok");
        assert_eq!(got_lines.len(), 1, "its single line is attached");
        assert_eq!(got_lines[0].receipt_id, id, "line bound to the receipt id");
    }

    /// Re-inserting the SAME slug is idempotent: it replaces the receipt in place
    /// (same stored id, no duplicate) and swaps its lines, per the ADR.
    #[tokio::test]
    async fn insert_receipt_same_slug_replaces_in_place() {
        let db = MemoryDb::seeded().expect("seed is valid");

        let first = sample_receipt("dup1", 1_000);
        let first_id = first.id;
        db.insert_receipt(first, vec![sample_line(first_id, "Old", 1_000)])
            .await
            .expect("first insert ok");
        let count_after_first = db.all_receipts().await.expect("list ok").len();

        // A re-import carries a NEW minted id but the SAME slug.
        let second = sample_receipt("dup1", 2_500);
        let second_lines = vec![
            sample_line(second.id, "New A", 1_000),
            sample_line(second.id, "New B", 1_500),
        ];
        let returned = db
            .insert_receipt(second, second_lines)
            .await
            .expect("second insert ok");

        // No duplicate row.
        let after = db.all_receipts().await.expect("list ok");
        assert_eq!(
            after.len(),
            count_after_first,
            "re-import does not duplicate"
        );
        // Stable id preserved (the first insert's id), payload replaced.
        assert_eq!(returned, first_id, "stored id is stable across re-import");
        let got = db.receipt_by_slug("dup1").await.expect("by slug ok");
        assert_eq!(got.id, first_id, "id unchanged");
        assert_eq!(got.amount.centimes(), 2_500, "amount replaced");
        // Old lines dropped, new lines attached and rebound to the stable id.
        let lines = db.line_items(first_id).await.expect("lines ok");
        assert_eq!(lines.len(), 2, "lines replaced with the new set");
        assert!(lines.iter().all(|l| l.receipt_id == first_id));
        assert!(lines.iter().all(|l| l.name != "Old"), "old line removed");
    }

    /// The adapter is usable behind the `Arc<dyn DatabaseAdapter>` handle that
    /// feature crates hold (ADR-000 object-safety, exercised via `MemoryDb`).
    #[tokio::test]
    async fn usable_as_arc_dyn_database_adapter() {
        use std::sync::Arc;
        let db: Arc<dyn DatabaseAdapter> = Arc::new(MemoryDb::seeded().expect("seed is valid"));
        let cfg = db.budget_config().await.expect("config ok");
        assert_eq!(cfg.monthly_budget.centimes(), 420_000);
    }
}
