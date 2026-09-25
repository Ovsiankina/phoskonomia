//! `phosk_db_surreal` — the embedded-SurrealDB [`DatabaseAdapter`] (ADR-005/010).
//!
//! A SECOND concrete L3 implementation of the `phosk_adapter_db` PORT, kept in
//! **lockstep** with `phosk_db_memory` by the shared
//! [`contract`](phosk_adapter_db::contract) suite and the per-method
//! `phosk_db_conformance` suite (`tests/conformance.rs`). It embeds SurrealDB —
//! there is **no external server**:
//!
//! * [`SurrealDb::memory`] opens the in-memory `kv-mem` engine (tests).
//! * [`SurrealDb::file`] opens a file-backed `kv-surrealkv` store (the prod path).
//!
//! ## Layering & vendor confinement (ADR-010)
//!
//! This crate depends on the PORT trait, the domain, and the surreal driver only.
//! It is wired in **only** by a composition root (`bin/*` or the Dioxus server
//! context) and is **never** imported by a feature crate — they see `&dyn
//! DatabaseAdapter`. The surreal [`Thing`](surrealdb::sql::Thing) and the query
//! layer are confined **entirely** here (the [`store`] module); only typed
//! [`phosk_id`] ids and domain types cross the boundary, never a `Thing`.
//!
//! ## Storage shape
//!
//! Every domain entity already derives `serde`, so each is stored as its full
//! serialized object under a single `doc` field of a per-entity table, keyed by a
//! natural string key (the entity's UUID id, or a synthetic relation key). The
//! surreal record `id` (a `Thing`) is therefore **never read back** — we only ever
//! deserialize `doc` — which both confines `Thing` and sidesteps SurrealDB's
//! reserved `id` field clashing with the model's own `id`.
//!
//! A table scan comes back in record-key order, not insertion order, so reads
//! the port documents as oldest→newest sort on the entity's date after
//! filtering. Rows that share a date keep scan order. Chat messages are the
//! exception: they are stamped with an insertion sequence (see [`store`]) and
//! read back in append order, same-day lines included.
//!
//! ## No panics (ADR §0)
//!
//! Every surreal/serde failure is mapped into a [`PhoskError`]; there is no
//! `unwrap`, `expect`, or `panic!` in non-test code.

// Style lints deliberately allowed, matching `phosk_db_memory`:
// - `assigning_clones`: the `x = s.to_owned()` writes read clearer than
//   `clone_into`, and obscuring them buys nothing for these tiny fields.
// - `redundant_pub_crate`: the internal `store`/`seed`/`migrate` modules expose
//   `pub(crate)` helpers on purpose — the visibility documents the crate-internal
//   seam even though the modules are private.
// - `option_if_let_else`: the explicit `match`/`if let` reads clearer than a
//   `map_or_else` with a multi-line closure.
// - `doc_markdown`: prose mentions `SurrealDB`/`SurrealKv`/`kv-*` proper nouns
//   that are not code items.
#![allow(
    clippy::assigning_clones,
    clippy::redundant_pub_crate,
    clippy::option_if_let_else,
    clippy::doc_markdown
)]

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
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, Mem, SurrealKv};

mod migrate;
mod seed;
mod store;

use store::{Bucket, Store};

/// The embedded-SurrealDB adapter handle.
///
/// Holds an owned [`Surreal<Db>`] connection to an embedded engine. Cheap to
/// share behind `Arc<dyn DatabaseAdapter>`; the handle is internally
/// reference-counted, so [`Clone`] yields another view of the same store.
#[derive(Debug, Clone)]
pub struct SurrealDb {
    store: Store,
}

impl SurrealDb {
    /// The fixed namespace/database the adapter pins (single-tenant desktop app).
    const NS: &'static str = "phoskonomia";
    const DB: &'static str = "main";

    /// Open the **in-memory** engine (`kv-mem`) — ephemeral, for tests. Runs the
    /// versioned migration on connect.
    ///
    /// # Errors
    /// [`PhoskError`] if the engine fails to start or the migration fails.
    pub async fn memory() -> Result<Self, PhoskError> {
        let db = Surreal::new::<Mem>(())
            .await
            .map_err(|e| PhoskError::Invalid(format!("surreal mem engine: {e}")))?;
        Self::init(db).await
    }

    /// Open a **file-backed** store (`kv-surrealkv`) at `path` — the prod path.
    /// The directory is created by the engine if absent. Runs the migration.
    ///
    /// # Errors
    /// [`PhoskError`] if the engine fails to open `path` or the migration fails.
    pub async fn file(path: &str) -> Result<Self, PhoskError> {
        let db = Surreal::new::<SurrealKv>(path)
            .await
            .map_err(|e| PhoskError::Invalid(format!("surreal file engine at {path}: {e}")))?;
        Self::init(db).await
    }

    /// Select the namespace/database and run the versioned migration.
    async fn init(db: Surreal<Db>) -> Result<Self, PhoskError> {
        db.use_ns(Self::NS)
            .use_db(Self::DB)
            .await
            .map_err(|e| PhoskError::Invalid(format!("surreal use ns/db: {e}")))?;
        let store = Store::new(db);
        migrate::run(&store).await?;
        // Chat and receipt lines written from now on must sort after the
        // stored ones.
        store.resume_sequence(Bucket::Message).await?;
        store.resume_sequence(Bucket::LineItem).await?;
        Ok(Self { store })
    }

    /// An in-memory adapter loaded with the deterministic Swiss seed — the
    /// fixture both adapters share so behaviour matches (ADR: lockstep).
    ///
    /// # Errors
    /// [`PhoskError`] if the engine, migration, or seed insert fails.
    pub async fn seeded() -> Result<Self, PhoskError> {
        let me = Self::memory().await?;
        seed::load(&me.store).await?;
        Ok(me)
    }

    /// A **file-backed** adapter, seeded only if the store is empty (first run).
    /// Idempotent: re-opening an existing store does not re-seed.
    ///
    /// # Errors
    /// [`PhoskError`] if the engine, migration, or seed insert fails.
    pub async fn file_seeded(path: &str) -> Result<Self, PhoskError> {
        let me = Self::file(path).await?;
        if me.store.count(Bucket::BudgetConfig).await? == 0 {
            seed::load(&me.store).await?;
        }
        Ok(me)
    }

    /// Whether any stored row still names `category` — the guard behind
    /// `delete_category` (receipts, their lines, subscriptions and signals).
    async fn category_is_referenced(&self, category: &str) -> Result<bool, PhoskError> {
        let receipts: Vec<Receipt> = self.store.list(Bucket::Receipt).await?;
        if receipts.iter().any(|r| r.category == category) {
            return Ok(true);
        }
        let lines: Vec<LineItem> = self.store.list(Bucket::LineItem).await?;
        if lines.iter().any(|l| l.category == category) {
            return Ok(true);
        }
        let subs: Vec<Subscription> = self.store.list(Bucket::Subscription).await?;
        if subs.iter().any(|s| s.category == category) {
            return Ok(true);
        }
        let signals: Vec<Signal> = self.store.list(Bucket::Signal).await?;
        Ok(signals.iter().any(|s| s.parent == category))
    }
}

#[async_trait]
impl DatabaseAdapter for SurrealDb {
    // ── Dashboard slice ────────────────────────────────────────────────────────

    #[tracing::instrument(level = "debug", skip(self))]
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
        let all: Vec<Transaction> = self.store.list(Bucket::Transaction).await?;
        Ok(all
            .into_iter()
            .filter(|t| t.date >= from && t.date <= to)
            .collect())
    }

    async fn categories(&self) -> Result<Vec<Category>, PhoskError> {
        self.store.list(Bucket::Category).await
    }

    async fn budget_config(&self) -> Result<BudgetConfig, PhoskError> {
        self.store
            .get::<BudgetConfig>(Bucket::BudgetConfig, "singleton")
            .await?
            .ok_or_else(|| PhoskError::NotFound("budget config".to_owned()))
    }

    // ── Ledger ─────────────────────────────────────────────────────────────────

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
        let all: Vec<Receipt> = self.store.list(Bucket::Receipt).await?;
        Ok(all
            .into_iter()
            .filter(|r| r.date >= from && r.date <= to)
            .collect())
    }

    async fn receipt(&self, id: ReceiptId) -> Result<Receipt, PhoskError> {
        self.store
            .get::<Receipt>(Bucket::Receipt, &id.to_string())
            .await?
            .ok_or_else(|| PhoskError::NotFound(format!("receipt {id}")))
    }

    async fn receipt_by_slug(&self, slug: &str) -> Result<Receipt, PhoskError> {
        let all: Vec<Receipt> = self.store.list(Bucket::Receipt).await?;
        all.into_iter()
            .find(|r| r.slug == slug)
            .ok_or_else(|| PhoskError::NotFound(format!("receipt slug {slug}")))
    }

    async fn line_items(&self, receipt: ReceiptId) -> Result<Vec<LineItem>, PhoskError> {
        // Lines written before sequencing (no `seq`) have no recorded order;
        // they come first, by id, so the result is at least deterministic.
        let all: Vec<LineItem> = self
            .store
            .list_in_insertion_order(Bucket::LineItem, |l: &LineItem| l.id.to_string())
            .await?;
        Ok(all
            .into_iter()
            .filter(|l| l.receipt_id == receipt)
            .collect())
    }

    async fn all_receipts(&self) -> Result<Vec<Receipt>, PhoskError> {
        self.store.list(Bucket::Receipt).await
    }

    async fn insert_receipt(
        &self,
        r: Receipt,
        lines: Vec<LineItem>,
    ) -> Result<ReceiptId, PhoskError> {
        // Idempotency: the receipt's stable `slug` is the key. A re-import of the
        // same slug REPLACES the prior receipt (keeping its stored id) and swaps
        // its lines; a fresh slug is appended. Mirrors `phosk_db_memory`.
        let existing: Vec<Receipt> = self.store.list(Bucket::Receipt).await?;
        let stable_id = existing
            .iter()
            .find(|x| x.slug == r.slug)
            .map_or(r.id, |x| x.id);

        let mut receipt = r;
        receipt.id = stable_id;
        self.store
            .put(Bucket::Receipt, &stable_id.to_string(), &receipt)
            .await?;

        // Drop old lines for this receipt, then attach the new set rebound to it.
        let old_lines: Vec<LineItem> = self.store.list(Bucket::LineItem).await?;
        for old in old_lines.into_iter().filter(|l| l.receipt_id == stable_id) {
            self.store
                .delete(Bucket::LineItem, &old.id.to_string())
                .await?;
        }
        for mut line in lines {
            line.receipt_id = stable_id;
            self.store
                .put_in_sequence(Bucket::LineItem, &line.id.to_string(), &line)
                .await?;
        }

        // Maintain the dashboard `Transaction` projection of the receipt (port
        // contract) so the written spend reaches `transactions_between`. Keyed
        // on the stored receipt id — a re-import replaces the projected row
        // instead of double-counting the spend. (The seeded rows, which have no
        // receipt behind them, are keyed on a content digest and never collide
        // with a receipt id.) Mirrors `phosk_db_memory`.
        let projected = Transaction {
            date: receipt.date,
            shop: receipt.shop,
            category: receipt.category,
            amount: receipt.amount,
        };
        self.store
            .put(Bucket::Transaction, &stable_id.to_string(), &projected)
            .await?;
        Ok(stable_id)
    }

    async fn delete_receipt(&self, id: ReceiptId) -> Result<(), PhoskError> {
        let key = id.to_string();
        if self
            .store
            .get::<Receipt>(Bucket::Receipt, &key)
            .await?
            .is_none()
        {
            return Err(PhoskError::NotFound(format!("receipt {id}")));
        }
        let lines: Vec<LineItem> = self.store.list(Bucket::LineItem).await?;
        for line in lines.into_iter().filter(|l| l.receipt_id == id) {
            self.store
                .delete(Bucket::LineItem, &line.id.to_string())
                .await?;
        }
        // The dashboard projection `insert_receipt` maintains is keyed on the
        // receipt id, so deleting that key removes the spend from
        // `transactions_between` without touching the seeded rows (keyed on a
        // content digest). Mirrors `phosk_db_memory`.
        self.store.delete(Bucket::Transaction, &key).await?;
        self.store.delete(Bucket::Receipt, &key).await
    }

    async fn update_line_item(&self, line: LineItem) -> Result<(), PhoskError> {
        let key = line.id.to_string();
        if self
            .store
            .get::<LineItem>(Bucket::LineItem, &key)
            .await?
            .is_none()
        {
            return Err(PhoskError::NotFound(format!("line item {}", line.id)));
        }
        // In place: the line keeps its position on the receipt.
        self.store
            .put_keeping_sequence(Bucket::LineItem, &key, &line)
            .await
    }

    async fn record_correction(&self, ev: CorrectionEvent) -> Result<(), PhoskError> {
        self.store
            .put(Bucket::Correction, &ev.id.to_string(), &ev)
            .await
    }

    // ── Signals ────────────────────────────────────────────────────────────────

    async fn signals(&self) -> Result<Vec<Signal>, PhoskError> {
        self.store.list(Bucket::Signal).await
    }

    async fn signal(&self, id: SignalId) -> Result<Signal, PhoskError> {
        self.store
            .get::<Signal>(Bucket::Signal, &id.to_string())
            .await?
            .ok_or_else(|| PhoskError::NotFound(format!("signal {id}")))
    }

    async fn signal_by_slug(&self, slug: &str) -> Result<Signal, PhoskError> {
        let all: Vec<Signal> = self.store.list(Bucket::Signal).await?;
        all.into_iter()
            .find(|s| s.slug == slug)
            .ok_or_else(|| PhoskError::NotFound(format!("signal slug {slug}")))
    }

    async fn signal_occurrences(&self, id: SignalId) -> Result<Vec<SignalOccurrence>, PhoskError> {
        let all: Vec<SignalOccurrence> = self.store.list(Bucket::SignalOccurrence).await?;
        Ok(all.into_iter().filter(|o| o.signal_id == id).collect())
    }

    async fn set_signal_tracked(&self, id: SignalId, tracked: bool) -> Result<(), PhoskError> {
        let key = id.to_string();
        let mut s = self
            .store
            .get::<Signal>(Bucket::Signal, &key)
            .await?
            .ok_or_else(|| PhoskError::NotFound(format!("signal {id}")))?;
        s.tracked = tracked;
        self.store.put(Bucket::Signal, &key, &s).await
    }

    async fn delete_signal(&self, id: SignalId) -> Result<(), PhoskError> {
        let key = id.to_string();
        if self
            .store
            .get::<Signal>(Bucket::Signal, &key)
            .await?
            .is_none()
        {
            return Err(PhoskError::NotFound(format!("signal {id}")));
        }
        self.store.delete(Bucket::Signal, &key).await
    }

    // ── Planning ───────────────────────────────────────────────────────────────

    async fn category_caps(&self) -> Result<Vec<CategoryCap>, PhoskError> {
        self.store.list(Bucket::CategoryCap).await
    }

    async fn category_cap_by_name(&self, name: &str) -> Result<CategoryCap, PhoskError> {
        let all: Vec<CategoryCap> = self.store.list(Bucket::CategoryCap).await?;
        all.into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| PhoskError::NotFound(format!("category {name}")))
    }

    async fn set_category_cap(&self, name: &str, cap: Option<Money>) -> Result<(), PhoskError> {
        let all: Vec<CategoryCap> = self.store.list(Bucket::CategoryCap).await?;
        let mut c = all
            .into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| PhoskError::NotFound(format!("category {name}")))?;
        c.cap = cap;
        c.provenance = Provenance {
            source: Source::UserModified,
            confidence: 1.0,
        };
        self.store
            .put(Bucket::CategoryCap, &c.id.to_string(), &c)
            .await
    }

    async fn insert_category(&self, c: CategoryCap) -> Result<CategoryId, PhoskError> {
        let existing: Vec<CategoryCap> = self.store.list(Bucket::CategoryCap).await?;
        if existing.iter().any(|x| x.name == c.name) {
            return Err(PhoskError::Invalid(format!(
                "category {} already exists",
                c.name
            )));
        }
        let id = c.id;
        self.store
            .put(Bucket::CategoryCap, &id.to_string(), &c)
            .await?;
        Ok(id)
    }

    async fn rename_category(&self, from: &str, to: &str) -> Result<(), PhoskError> {
        // Every guard runs against the current state before the first write, so
        // a rejected rename writes nothing. The re-point itself is a sequence of
        // per-record `put`s (the store exposes no multi-record transaction): a
        // store fault part-way leaves the records already moved, which is why
        // the category record is written LAST — until it moves, the old name is
        // still the live one and re-running the rename finishes the job.
        let caps: Vec<CategoryCap> = self.store.list(Bucket::CategoryCap).await?;
        let mut target = caps
            .iter()
            .find(|c| c.name == from)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("category {from}")))?;
        if from == to {
            return Ok(());
        }
        if caps.iter().any(|c| c.name == to) {
            return Err(PhoskError::Invalid(format!("category {to} already exists")));
        }

        let receipts: Vec<Receipt> = self.store.list(Bucket::Receipt).await?;
        for mut r in receipts.into_iter().filter(|r| r.category == from) {
            r.category = to.to_owned();
            self.store
                .put(Bucket::Receipt, &r.id.to_string(), &r)
                .await?;
        }
        let lines: Vec<LineItem> = self.store.list(Bucket::LineItem).await?;
        for mut l in lines.into_iter().filter(|l| l.category == from) {
            l.category = to.to_owned();
            self.store
                .put_keeping_sequence(Bucket::LineItem, &l.id.to_string(), &l)
                .await?;
        }
        let subs: Vec<Subscription> = self.store.list(Bucket::Subscription).await?;
        for mut s in subs.into_iter().filter(|s| s.category == from) {
            s.category = to.to_owned();
            self.store
                .put(Bucket::Subscription, &s.id.to_string(), &s)
                .await?;
        }
        let signals: Vec<Signal> = self.store.list(Bucket::Signal).await?;
        for mut s in signals.into_iter().filter(|s| s.parent == from) {
            s.parent = to.to_owned();
            self.store
                .put(Bucket::Signal, &s.id.to_string(), &s)
                .await?;
        }

        target.name = to.to_owned();
        target.provenance = Provenance {
            source: Source::UserModified,
            confidence: 1.0,
        };
        self.store
            .put(Bucket::CategoryCap, &target.id.to_string(), &target)
            .await
    }

    async fn delete_category(&self, name: &str) -> Result<(), PhoskError> {
        let caps: Vec<CategoryCap> = self.store.list(Bucket::CategoryCap).await?;
        let target = caps
            .into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| PhoskError::NotFound(format!("category {name}")))?;
        if self.category_is_referenced(name).await? {
            return Err(PhoskError::Invalid(format!(
                "category {name} is still referenced"
            )));
        }
        self.store
            .delete(Bucket::CategoryCap, &target.id.to_string())
            .await
    }

    async fn merge_categories(&self, from: &str, into: &str) -> Result<u32, PhoskError> {
        // Same discipline as `rename_category`: every guard runs against the
        // current state before the first write, and the source record is
        // removed LAST — the store exposes no multi-record transaction, so
        // until that delete lands the merge is simply re-runnable.
        let caps: Vec<CategoryCap> = self.store.list(Bucket::CategoryCap).await?;
        let source = caps
            .iter()
            .find(|c| c.name == from)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("category {from}")))?;
        let mut target = caps
            .iter()
            .find(|c| c.name == into)
            .cloned()
            .ok_or_else(|| PhoskError::NotFound(format!("category {into}")))?;
        if from == into {
            return Err(PhoskError::Invalid(format!(
                "category {from} cannot be merged into itself"
            )));
        }

        let mut moved: u32 = 0;
        let receipts: Vec<Receipt> = self.store.list(Bucket::Receipt).await?;
        for mut r in receipts.into_iter().filter(|r| r.category == from) {
            r.category = into.to_owned();
            self.store
                .put(Bucket::Receipt, &r.id.to_string(), &r)
                .await?;
            moved = moved.saturating_add(1);
        }
        let lines: Vec<LineItem> = self.store.list(Bucket::LineItem).await?;
        for mut l in lines.into_iter().filter(|l| l.category == from) {
            l.category = into.to_owned();
            self.store
                .put_keeping_sequence(Bucket::LineItem, &l.id.to_string(), &l)
                .await?;
            moved = moved.saturating_add(1);
        }
        let subs: Vec<Subscription> = self.store.list(Bucket::Subscription).await?;
        for mut s in subs.into_iter().filter(|s| s.category == from) {
            s.category = into.to_owned();
            self.store
                .put(Bucket::Subscription, &s.id.to_string(), &s)
                .await?;
            moved = moved.saturating_add(1);
        }
        let signals: Vec<Signal> = self.store.list(Bucket::Signal).await?;
        for mut s in signals.into_iter().filter(|s| s.parent == from) {
            s.parent = into.to_owned();
            self.store
                .put(Bucket::Signal, &s.id.to_string(), &s)
                .await?;
            moved = moved.saturating_add(1);
        }

        target.provenance = Provenance {
            source: Source::UserModified,
            confidence: 1.0,
        };
        self.store
            .put(Bucket::CategoryCap, &target.id.to_string(), &target)
            .await?;
        self.store
            .delete(Bucket::CategoryCap, &source.id.to_string())
            .await?;
        Ok(moved)
    }

    async fn split_category(
        &self,
        from: &str,
        new: CategoryCap,
        lines: &[LineItemId],
    ) -> Result<u32, PhoskError> {
        // Guards first, then the line moves, and the new category record LAST:
        // a store fault part-way leaves lines pointing at a category that does
        // not exist yet, which the re-run repairs — whereas writing the record
        // first would leave a stray empty category behind.
        let caps: Vec<CategoryCap> = self.store.list(Bucket::CategoryCap).await?;
        if !caps.iter().any(|c| c.name == from) {
            return Err(PhoskError::NotFound(format!("category {from}")));
        }
        if caps.iter().any(|c| c.name == new.name) {
            return Err(PhoskError::Invalid(format!(
                "category {} already exists",
                new.name
            )));
        }

        let stored: Vec<LineItem> = self.store.list(Bucket::LineItem).await?;
        for id in lines {
            let line = stored
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
        for mut l in stored.into_iter().filter(|l| lines.contains(&l.id)) {
            l.category = new.name.clone();
            l.provenance = Provenance::user_modified();
            self.store
                .put_keeping_sequence(Bucket::LineItem, &l.id.to_string(), &l)
                .await?;
            moved = moved.saturating_add(1);
        }
        self.store
            .put(Bucket::CategoryCap, &new.id.to_string(), &new)
            .await?;
        Ok(moved)
    }

    async fn budget_history(&self, category: &str) -> Result<Vec<BudgetHistory>, PhoskError> {
        let caps: Vec<CategoryCap> = self.store.list(Bucket::CategoryCap).await?;
        let Some(cap) = caps.iter().find(|c| c.name == category) else {
            return Ok(Vec::new());
        };
        let id = cap.id;
        let all: Vec<BudgetHistory> = self.store.list(Bucket::BudgetHistory).await?;
        // The store scans in record-key order; the port promises oldest→newest.
        let mut rows: Vec<BudgetHistory> =
            all.into_iter().filter(|h| h.category_id == id).collect();
        rows.sort_by_key(|h| h.cycle_start);
        Ok(rows)
    }

    async fn alerts(&self) -> Result<Vec<Alert>, PhoskError> {
        self.store.list(Bucket::Alert).await
    }

    async fn alert(&self, id: AlertId) -> Result<Alert, PhoskError> {
        self.store
            .get::<Alert>(Bucket::Alert, &id.to_string())
            .await?
            .ok_or_else(|| PhoskError::NotFound(format!("alert {id}")))
    }

    async fn update_alert_status(&self, id: AlertId, status: &str) -> Result<(), PhoskError> {
        let key = id.to_string();
        let mut a = self
            .store
            .get::<Alert>(Bucket::Alert, &key)
            .await?
            .ok_or_else(|| PhoskError::NotFound(format!("alert {id}")))?;
        a.status = status.to_owned();
        self.store.put(Bucket::Alert, &key, &a).await
    }

    // ── Recurring ──────────────────────────────────────────────────────────────

    async fn subscriptions(&self) -> Result<Vec<Subscription>, PhoskError> {
        self.store.list(Bucket::Subscription).await
    }

    async fn subscription(&self, id: SubscriptionId) -> Result<Subscription, PhoskError> {
        self.store
            .get::<Subscription>(Bucket::Subscription, &id.to_string())
            .await?
            .ok_or_else(|| PhoskError::NotFound(format!("subscription {id}")))
    }

    async fn subscription_by_slug(&self, slug: &str) -> Result<Subscription, PhoskError> {
        let all: Vec<Subscription> = self.store.list(Bucket::Subscription).await?;
        all.into_iter()
            .find(|s| s.slug == slug)
            .ok_or_else(|| PhoskError::NotFound(format!("subscription slug {slug}")))
    }

    async fn subscription_charges(&self, id: SubscriptionId) -> Result<Vec<Charge>, PhoskError> {
        let all: Vec<Charge> = self.store.list(Bucket::Charge).await?;
        // Record-key scan order is arbitrary; the port promises oldest→newest.
        let mut charges: Vec<Charge> = all
            .into_iter()
            .filter(|c| c.subscription_id == id)
            .collect();
        charges.sort_by_key(|c| c.date);
        Ok(charges)
    }

    async fn upsert_subscription(&self, s: Subscription) -> Result<SubscriptionId, PhoskError> {
        let id = s.id;
        self.store
            .put(Bucket::Subscription, &id.to_string(), &s)
            .await?;
        Ok(id)
    }

    async fn delete_subscription(&self, id: SubscriptionId) -> Result<(), PhoskError> {
        let key = id.to_string();
        if self
            .store
            .get::<Subscription>(Bucket::Subscription, &key)
            .await?
            .is_none()
        {
            return Err(PhoskError::NotFound(format!("subscription {id}")));
        }
        // Cascade: a charge without its subscription is an orphan.
        let charges: Vec<Charge> = self.store.list(Bucket::Charge).await?;
        for c in charges.iter().filter(|c| c.subscription_id == id) {
            self.store.delete(Bucket::Charge, &c.id.to_string()).await?;
        }
        self.store.delete(Bucket::Subscription, &key).await
    }

    async fn record_charge(&self, c: Charge) -> Result<(), PhoskError> {
        self.store.put(Bucket::Charge, &c.id.to_string(), &c).await
    }

    // ── Debts ──────────────────────────────────────────────────────────────────

    async fn debts(&self) -> Result<Vec<Debt>, PhoskError> {
        self.store.list(Bucket::Debt).await
    }

    async fn debt(&self, id: DebtId) -> Result<Debt, PhoskError> {
        self.store
            .get::<Debt>(Bucket::Debt, &id.to_string())
            .await?
            .ok_or_else(|| PhoskError::NotFound(format!("debt {id}")))
    }

    async fn debt_by_slug(&self, slug: &str) -> Result<Debt, PhoskError> {
        let all: Vec<Debt> = self.store.list(Bucket::Debt).await?;
        all.into_iter()
            .find(|d| d.slug == slug)
            .ok_or_else(|| PhoskError::NotFound(format!("debt slug {slug}")))
    }

    async fn debt_payments(&self, id: DebtId) -> Result<Vec<DebtPayment>, PhoskError> {
        let all: Vec<DebtPayment> = self.store.list(Bucket::DebtPayment).await?;
        // Record-key scan order is arbitrary; the port promises oldest→newest.
        let mut payments: Vec<DebtPayment> = all.into_iter().filter(|p| p.debt_id == id).collect();
        payments.sort_by_key(|p| p.date);
        Ok(payments)
    }

    async fn upsert_debt(&self, d: Debt) -> Result<DebtId, PhoskError> {
        let id = d.id;
        self.store.put(Bucket::Debt, &id.to_string(), &d).await?;
        Ok(id)
    }

    async fn record_debt_payment(&self, p: DebtPayment) -> Result<(), PhoskError> {
        self.store
            .put(Bucket::DebtPayment, &p.id.to_string(), &p)
            .await
    }

    async fn personal_ious(&self) -> Result<Vec<PersonalIou>, PhoskError> {
        self.store.list(Bucket::PersonalIou).await
    }

    async fn upsert_personal_iou(&self, i: PersonalIou) -> Result<PersonalIouId, PhoskError> {
        let id = i.id;
        self.store
            .put(Bucket::PersonalIou, &id.to_string(), &i)
            .await?;
        Ok(id)
    }

    // ── Analytics support ──────────────────────────────────────────────────────

    async fn spend_history(
        &self,
        _cycles: u32,
        _as_of: NaiveDate,
    ) -> Result<Vec<BudgetHistory>, PhoskError> {
        self.store.list(Bucket::BudgetHistory).await
    }

    // ── Settings ───────────────────────────────────────────────────────────────

    async fn preferences(&self) -> Result<Vec<Preference>, PhoskError> {
        self.store.list(Bucket::Preference).await
    }

    async fn preference(&self, key: &str) -> Result<Preference, PhoskError> {
        // Keyed by the natural `key` string (the user-facing preference key).
        self.store
            .get::<Preference>(Bucket::Preference, key)
            .await?
            .ok_or_else(|| PhoskError::NotFound(format!("preference {key}")))
    }

    async fn set_preference(&self, key: &str, value: &str) -> Result<(), PhoskError> {
        let pref = match self
            .store
            .get::<Preference>(Bucket::Preference, key)
            .await?
        {
            Some(mut p) => {
                p.value = value.to_owned();
                p.provenance = Provenance {
                    source: Source::UserModified,
                    confidence: 1.0,
                };
                p
            }
            None => Preference {
                id: phosk_id::PreferenceId::new(),
                key: key.to_owned(),
                value: value.to_owned(),
                surface: String::new(),
                stored_on_device: true,
                provenance: Provenance {
                    source: Source::UserModified,
                    confidence: 1.0,
                },
            },
        };
        self.store.put(Bucket::Preference, key, &pref).await
    }

    async fn reset_preference(&self, key: &str, default_value: &str) -> Result<(), PhoskError> {
        let pref = match self
            .store
            .get::<Preference>(Bucket::Preference, key)
            .await?
        {
            Some(mut p) => {
                p.value = default_value.to_owned();
                p.provenance = Provenance {
                    source: Source::RuleGenerated,
                    confidence: 1.0,
                };
                p
            }
            None => Preference {
                id: phosk_id::PreferenceId::new(),
                key: key.to_owned(),
                value: default_value.to_owned(),
                surface: String::new(),
                stored_on_device: true,
                provenance: Provenance {
                    source: Source::RuleGenerated,
                    confidence: 1.0,
                },
            },
        };
        self.store.put(Bucket::Preference, key, &pref).await
    }

    // ── AI ─────────────────────────────────────────────────────────────────────

    async fn feed_items(&self) -> Result<Vec<FeedItem>, PhoskError> {
        self.store.list(Bucket::FeedItem).await
    }

    async fn dismiss_feed_item(&self, id: &str) -> Result<(), PhoskError> {
        let all: Vec<FeedItem> = self.store.list(Bucket::FeedItem).await?;
        let Some(item) = all.into_iter().find(|f| f.id.to_string() == id) else {
            return Err(PhoskError::NotFound(format!("feed item {id}")));
        };
        self.store
            .delete(Bucket::FeedItem, &item.id.to_string())
            .await
    }

    async fn chat_messages(&self, chat: ChatId) -> Result<Vec<Message>, PhoskError> {
        // Oldest→newest per the port: a plain table scan has no order. Lines
        // stored before sequencing (no `seq`) fall back to date order.
        let all: Vec<Message> = self
            .store
            .list_in_insertion_order(Bucket::Message, |m: &Message| m.at)
            .await?;
        Ok(all.into_iter().filter(|m| m.chat_id == chat).collect())
    }

    async fn latest_chat(&self) -> Result<Option<Chat>, PhoskError> {
        let all: Vec<Chat> = self.store.list(Bucket::Chat).await?;
        Ok(all.into_iter().max_by_key(|c| c.started))
    }

    async fn append_message(&self, m: Message) -> Result<(), PhoskError> {
        self.store
            .put_in_sequence(Bucket::Message, &m.id.to_string(), &m)
            .await
    }

    async fn clear_chat(&self, chat: ChatId) -> Result<(), PhoskError> {
        let all: Vec<Message> = self.store.list(Bucket::Message).await?;
        for m in all.into_iter().filter(|m| m.chat_id == chat) {
            self.store
                .delete(Bucket::Message, &m.id.to_string())
                .await?;
        }
        Ok(())
    }

    async fn ai_suggestions(&self) -> Result<Vec<AiSuggestion>, PhoskError> {
        self.store.list(Bucket::AiSuggestion).await
    }

    async fn enqueue_suggestion(&self, s: AiSuggestion) -> Result<SuggestionId, PhoskError> {
        let id = s.id;
        self.store
            .put(Bucket::AiSuggestion, &id.to_string(), &s)
            .await?;
        Ok(id)
    }

    async fn update_suggestion_status(
        &self,
        id: SuggestionId,
        status: &str,
    ) -> Result<(), PhoskError> {
        let key = id.to_string();
        let mut s = self
            .store
            .get::<AiSuggestion>(Bucket::AiSuggestion, &key)
            .await?
            .ok_or_else(|| PhoskError::NotFound(format!("suggestion {id}")))?;
        s.status = status.to_owned();
        self.store.put(Bucket::AiSuggestion, &key, &s).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    #[tokio::test]
    async fn empty_memory_db_has_no_config() {
        let db = SurrealDb::memory().await.expect("mem engine");
        assert!(
            db.budget_config().await.is_err(),
            "no config before seeding"
        );
        assert!(db.categories().await.expect("ok").is_empty());
    }

    #[tokio::test]
    async fn seeded_dashboard_matches_the_swiss_seed() {
        let db = SurrealDb::seeded().await.expect("seeded");
        let june = db
            .transactions_between(naive(2026, 6, 1), naive(2026, 6, 30))
            .await
            .expect("query ok");
        assert_eq!(june.len(), 28);
        let cfg = db.budget_config().await.expect("config");
        assert_eq!(cfg.monthly_budget.centimes(), 420_000);
    }

    #[tokio::test]
    async fn inverted_window_is_rejected() {
        let db = SurrealDb::seeded().await.expect("seeded");
        let err = db
            .transactions_between(naive(2026, 6, 30), naive(2026, 6, 1))
            .await
            .expect_err("inverted rejected");
        assert_eq!(err.code(), "invalid_input");
    }

    #[tokio::test]
    async fn usable_as_arc_dyn_database_adapter() {
        use std::sync::Arc;
        let db: Arc<dyn DatabaseAdapter> = Arc::new(SurrealDb::seeded().await.expect("seeded"));
        assert_eq!(
            db.budget_config()
                .await
                .expect("config")
                .savings_target
                .centimes(),
            90_000
        );
    }

    /// The **file-backed** prod engine (`kv-surrealkv`): data survives a close +
    /// re-open, and `file_seeded` is idempotent (the first-run seed guard does not
    /// re-seed an existing store, and a write made between opens persists).
    #[tokio::test]
    async fn file_engine_persists_across_reopen() {
        let dir = std::env::temp_dir().join(format!("phosk_surreal_test_{}", uuid()));
        let path = dir.to_string_lossy().into_owned();

        // First open: empty store gets seeded; mutate one record.
        {
            let db = SurrealDb::file_seeded(&path).await.expect("first open");
            assert_eq!(db.categories().await.expect("cats").len(), 8);
            db.set_preference("currency", "EUR").await.expect("write");
        }
        // Re-open the SAME path: the seed is not duplicated and the write survived.
        {
            let db = SurrealDb::file_seeded(&path).await.expect("reopen");
            assert_eq!(
                db.categories().await.expect("cats").len(),
                8,
                "first-run guard did not re-seed"
            );
            assert_eq!(
                db.preference("currency").await.expect("pref").value,
                "EUR",
                "the write persisted to disk"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Chat lines appended after a reopen sort after the stored ones, even when
    /// an earlier run left sequence numbers far ahead of this process's
    /// counter; a line stored without a sequence number sorts first.
    ///
    /// Multi-threaded: the file engine finishes closing on background tasks,
    /// and a write after reopening on the current-thread runtime breaks reads.
    #[tokio::test(flavor = "multi_thread")]
    async fn chat_order_survives_reopening_a_store_from_an_earlier_run() {
        const EARLIER_RUN_SEQ: i64 = 1 << 60;
        let dir = std::env::temp_dir().join(format!("phosk_surreal_chat_{}", uuid()));
        let path = dir.to_string_lossy().into_owned();

        let chat_id = {
            let db = SurrealDb::file_seeded(&path).await.expect("first open");
            let chat = db.latest_chat().await.expect("read").expect("seed chat");
            db.clear_chat(chat.id).await.expect("clear");
            let line = |text: &str| Message {
                id: phosk_id::MessageId::new(),
                chat_id: chat.id,
                who: "usr".to_owned(),
                text: text.to_owned(),
                at: chat.started,
            };
            let earlier = line("earlier run");
            let doc = serde_json::to_string(&earlier).expect("serialize");
            db.store
                .raw()
                .query("UPSERT type::thing($tb, $id) CONTENT { doc: $doc, seq: $seq } RETURN NONE")
                .bind(("tb", Bucket::Message.table()))
                .bind(("id", earlier.id.to_string()))
                .bind(("doc", doc))
                .bind(("seq", EARLIER_RUN_SEQ))
                .await
                .expect("raw write")
                .check()
                .expect("raw write ok");
            let legacy = line("no sequence");
            db.store
                .put(Bucket::Message, &legacy.id.to_string(), &legacy)
                .await
                .expect("legacy write");
            chat.id
        };

        let db = SurrealDb::file(&path).await.expect("reopen");
        db.append_message(Message {
            id: phosk_id::MessageId::new(),
            chat_id,
            who: "sys".to_owned(),
            text: "after reopen".to_owned(),
            at: naive(2026, 6, 19),
        })
        .await
        .expect("append");
        let texts: Vec<String> = db
            .chat_messages(chat_id)
            .await
            .expect("messages")
            .into_iter()
            .map(|m| m.text)
            .collect();
        assert_eq!(texts, ["no sequence", "earlier run", "after reopen"]);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Chat lines stored before sequencing (no `seq`) read back oldest-first by
    /// date, whatever their record-key order, and ahead of sequenced lines.
    #[tokio::test]
    async fn unsequenced_chat_lines_read_back_in_date_order() {
        let db = SurrealDb::memory().await.expect("mem engine");
        let chat_id = phosk_id::ChatId::new();
        let line = |text: &str, at: NaiveDate| Message {
            id: phosk_id::MessageId::new(),
            chat_id,
            who: "usr".to_owned(),
            text: text.to_owned(),
            at,
        };
        // The record keys scan in the opposite order of the dates.
        for (key, legacy) in [
            ("a", line("newer legacy", naive(2026, 6, 3))),
            ("b", line("older legacy", naive(2026, 6, 1))),
        ] {
            db.store
                .put(Bucket::Message, key, &legacy)
                .await
                .expect("legacy write");
        }
        db.append_message(line("sequenced", naive(2026, 5, 1)))
            .await
            .expect("append");
        let texts: Vec<String> = db
            .chat_messages(chat_id)
            .await
            .expect("messages")
            .into_iter()
            .map(|m| m.text)
            .collect();
        assert_eq!(texts, ["older legacy", "newer legacy", "sequenced"]);
    }

    /// Line items stored before sequencing (no `seq`) still load, ahead of
    /// sequenced lines of the same receipt and ordered among themselves by id.
    #[tokio::test]
    async fn unsequenced_line_items_load_first() {
        let db = SurrealDb::memory().await.expect("mem engine");
        let receipt_id = ReceiptId::new();
        let line = |name: &str| LineItem {
            id: LineItemId::new(),
            receipt_id,
            name: name.to_owned(),
            qty: 1.0,
            unit_price: Money::from_centimes(100),
            line_total: Money::from_centimes(100),
            category: "Groceries".to_owned(),
            signal_id: None,
            provenance: Provenance::user_modified(),
        };
        let mut legacy = vec![line("legacy one"), line("legacy two")];
        legacy.sort_by_key(|l| l.id.to_string());
        for l in legacy.iter().rev() {
            db.store
                .put(Bucket::LineItem, &l.id.to_string(), l)
                .await
                .expect("legacy write");
        }
        let fresh = line("sequenced");
        db.store
            .put_in_sequence(Bucket::LineItem, &fresh.id.to_string(), &fresh)
            .await
            .expect("sequenced write");
        let mut want = legacy;
        want.push(fresh);
        assert_eq!(db.line_items(receipt_id).await.expect("lines"), want);
    }

    /// A tiny random suffix so concurrent test runs use distinct store dirs
    /// (the crate forbids the `uuid` dep elsewhere; a nanos-based token suffices).
    fn uuid() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    }
}
