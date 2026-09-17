//! `store` — the vendor-confined SurrealDB persistence layer.
//!
//! This is the **only** module that touches the surreal driver. Every domain
//! entity is `serde`, so we store each as its full serialized object under a
//! single `doc` field of a per-entity table, keyed by a natural string key. We
//! **never** read the surreal record `id` (a [`Thing`](surrealdb::sql::Thing)) —
//! we only ever select and deserialize `doc` — so no `Thing` ever leaves this
//! module, and the model's own `id` field never clashes with SurrealDB's reserved
//! record `id`.
//!
//! All four primitives ([`Store::put`], [`Store::get`], [`Store::delete`],
//! [`Store::list`]) go through bound-parameter `query()` calls; a surreal or serde
//! failure maps to a [`PhoskError`] (no panic, ADR §0).
//!
//! A table scan has no defined order, so buckets whose port contract is ordered
//! (chat messages) are written with [`Store::put_in_sequence`], which adds an
//! insertion `seq` next to `doc`, and read with
//! [`Store::list_in_insertion_order`].

use std::sync::atomic::{AtomicI64, Ordering};

use phosk_core::error::PhoskError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

/// The next insertion sequence number handed out by
/// [`Store::put_in_sequence`]. Process-wide, so every handle in the process
/// shares one strictly increasing series; [`Store::resume_sequence`] moves it
/// past what a reopened store already holds. The embedded engine is owned by a
/// single process, so an in-process counter is enough.
static NEXT_SEQ: AtomicI64 = AtomicI64::new(1);

/// One row of an ordered bucket: the opaque document plus its sequence number
/// (`0` for records written without one; real numbers start at `1`).
#[derive(Debug, Deserialize)]
struct SequencedDoc {
    doc: String,
    seq: i64,
}

/// The per-entity tables. The string is the SurrealDB table name; record ids are
/// `⟨table⟩:⟨natural-key⟩`. Confined here so the rest of the crate names buckets
/// symbolically, never as stringly-typed table names.
#[derive(Debug, Clone, Copy)]
pub enum Bucket {
    Transaction,
    Category,
    BudgetConfig,
    Receipt,
    LineItem,
    Correction,
    Signal,
    SignalOccurrence,
    CategoryCap,
    BudgetHistory,
    Alert,
    Subscription,
    Charge,
    Debt,
    DebtPayment,
    PersonalIou,
    Preference,
    FeedItem,
    Chat,
    Message,
    AiSuggestion,
}

impl Bucket {
    /// The SurrealDB table name. A `phosk_` prefix keeps the schema namespaced.
    pub(crate) const fn table(self) -> &'static str {
        match self {
            Self::Transaction => "phosk_transaction",
            Self::Category => "phosk_category",
            Self::BudgetConfig => "phosk_budget_config",
            Self::Receipt => "phosk_receipt",
            Self::LineItem => "phosk_line_item",
            Self::Correction => "phosk_correction",
            Self::Signal => "phosk_signal",
            Self::SignalOccurrence => "phosk_signal_occurrence",
            Self::CategoryCap => "phosk_category_cap",
            Self::BudgetHistory => "phosk_budget_history",
            Self::Alert => "phosk_alert",
            Self::Subscription => "phosk_subscription",
            Self::Charge => "phosk_charge",
            Self::Debt => "phosk_debt",
            Self::DebtPayment => "phosk_debt_payment",
            Self::PersonalIou => "phosk_personal_iou",
            Self::Preference => "phosk_preference",
            Self::FeedItem => "phosk_feed_item",
            Self::Chat => "phosk_chat",
            Self::Message => "phosk_message",
            Self::AiSuggestion => "phosk_ai_suggestion",
        }
    }

    /// Every bucket, for schema definition / migration.
    pub(crate) const ALL: [Self; 21] = [
        Self::Transaction,
        Self::Category,
        Self::BudgetConfig,
        Self::Receipt,
        Self::LineItem,
        Self::Correction,
        Self::Signal,
        Self::SignalOccurrence,
        Self::CategoryCap,
        Self::BudgetHistory,
        Self::Alert,
        Self::Subscription,
        Self::Charge,
        Self::Debt,
        Self::DebtPayment,
        Self::PersonalIou,
        Self::Preference,
        Self::FeedItem,
        Self::Chat,
        Self::Message,
        Self::AiSuggestion,
    ];
}

/// A thin, `Clone`-able wrapper over the embedded surreal connection.
#[derive(Debug, Clone)]
pub struct Store {
    db: Surreal<Db>,
}

impl Store {
    /// Wrap an already-connected (ns/db-selected) surreal handle.
    pub(crate) const fn new(db: Surreal<Db>) -> Self {
        Self { db }
    }

    /// The raw handle — used by the migration step only.
    pub(crate) const fn raw(&self) -> &Surreal<Db> {
        &self.db
    }

    /// Map a surreal driver error into the taxonomy.
    fn map_err(context: &str, e: &surrealdb::Error) -> PhoskError {
        PhoskError::Invalid(format!("surreal {context}: {e}"))
    }

    /// Upsert `value` into `bucket` under record key `key`.
    ///
    /// The record is `{ doc: <serialized value> }`; `CONTENT` replaces the whole
    /// record, so this is an idempotent insert-or-replace.
    pub(crate) async fn put<T: Serialize + Sync>(
        &self,
        bucket: Bucket,
        key: &str,
        value: &T,
    ) -> Result<(), PhoskError> {
        // Store the entity as an OPAQUE serialized JSON string under `doc`, not as
        // a surreal-interpreted object: that guarantees a lossless round-trip
        // (surreal does not drop `null`/`None` fields, reinterpret numbers, or
        // touch the model's `id`) and keeps the payload fully owned by the domain
        // serde contract. It is the strongest form of vendor confinement.
        let doc = serde_json::to_string(value)
            .map_err(|e| PhoskError::Invalid(format!("serialize {}: {e}", bucket.table())))?;
        let sql = "UPSERT type::thing($tb, $id) CONTENT { doc: $doc } RETURN NONE";
        self.db
            .query(sql)
            .bind(("tb", bucket.table()))
            .bind(("id", key.to_owned()))
            .bind(("doc", doc))
            .await
            .map_err(|e| Self::map_err("put", &e))?
            .check()
            .map_err(|e| Self::map_err("put-check", &e))?;
        Ok(())
    }

    /// Fetch and deserialize the single record at `bucket:key`, if present.
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        bucket: Bucket,
        key: &str,
    ) -> Result<Option<T>, PhoskError> {
        let sql = "SELECT VALUE doc FROM type::thing($tb, $id)";
        let mut res = self
            .db
            .query(sql)
            .bind(("tb", bucket.table()))
            .bind(("id", key.to_owned()))
            .await
            .map_err(|e| Self::map_err("get", &e))?;
        let docs: Vec<String> = res.take(0).map_err(|e| Self::map_err("get-take", &e))?;
        match docs.into_iter().next() {
            None => Ok(None),
            Some(doc) => serde_json::from_str(&doc)
                .map(Some)
                .map_err(|e| PhoskError::Invalid(format!("deserialize {}: {e}", bucket.table()))),
        }
    }

    /// Fetch and deserialize every record in `bucket`.
    pub(crate) async fn list<T: DeserializeOwned>(
        &self,
        bucket: Bucket,
    ) -> Result<Vec<T>, PhoskError> {
        let sql = "SELECT VALUE doc FROM type::table($tb)";
        let mut res = self
            .db
            .query(sql)
            .bind(("tb", bucket.table()))
            .await
            .map_err(|e| Self::map_err("list", &e))?;
        let docs: Vec<String> = res.take(0).map_err(|e| Self::map_err("list-take", &e))?;
        docs.into_iter()
            .map(|doc| {
                serde_json::from_str(&doc).map_err(|e| {
                    PhoskError::Invalid(format!("deserialize {}: {e}", bucket.table()))
                })
            })
            .collect()
    }

    /// Upsert `value` like [`Store::put`], and also stamp the record with the
    /// next insertion sequence number, so [`Store::list_in_insertion_order`] can
    /// return the bucket in the order its records were written.
    ///
    /// Used for buckets whose port contract is ordered (chat messages): record
    /// keys are random UUIDs and a table scan has no defined order.
    pub(crate) async fn put_in_sequence<T: Serialize + Sync>(
        &self,
        bucket: Bucket,
        key: &str,
        value: &T,
    ) -> Result<(), PhoskError> {
        let doc = serde_json::to_string(value)
            .map_err(|e| PhoskError::Invalid(format!("serialize {}: {e}", bucket.table())))?;
        let seq = NEXT_SEQ.fetch_add(1, Ordering::SeqCst);
        let sql = "UPSERT type::thing($tb, $id) CONTENT { doc: $doc, seq: $seq } RETURN NONE";
        self.db
            .query(sql)
            .bind(("tb", bucket.table()))
            .bind(("id", key.to_owned()))
            .bind(("doc", doc))
            .bind(("seq", seq))
            .await
            .map_err(|e| Self::map_err("put-seq", &e))?
            .check()
            .map_err(|e| Self::map_err("put-seq-check", &e))?;
        Ok(())
    }

    /// Fetch and deserialize every record in `bucket`, oldest write first (by
    /// the sequence number [`Store::put_in_sequence`] stamps). Records written
    /// without one sort first, ordered among themselves by `tiebreak` (sequence
    /// numbers are unique, so it never reorders sequenced records).
    pub(crate) async fn list_in_insertion_order<T, K, F>(
        &self,
        bucket: Bucket,
        tiebreak: F,
    ) -> Result<Vec<T>, PhoskError>
    where
        T: DeserializeOwned,
        K: Ord,
        F: Fn(&T) -> K,
    {
        // A missing `seq` cannot be decoded as an `Option`, so default it here.
        let sql = "SELECT doc, seq ?? 0 AS seq FROM type::table($tb)";
        let mut res = self
            .db
            .query(sql)
            .bind(("tb", bucket.table()))
            .await
            .map_err(|e| Self::map_err("list-seq", &e))?;
        let rows: Vec<SequencedDoc> = res
            .take(0)
            .map_err(|e| Self::map_err("list-seq-take", &e))?;
        let mut records = rows
            .into_iter()
            .map(|row| {
                serde_json::from_str::<T>(&row.doc)
                    .map(|value| (row.seq, value))
                    .map_err(|e| {
                        PhoskError::Invalid(format!("deserialize {}: {e}", bucket.table()))
                    })
            })
            .collect::<Result<Vec<(i64, T)>, PhoskError>>()?;
        records.sort_by_key(|(seq, value)| (*seq, tiebreak(value)));
        Ok(records.into_iter().map(|(_, value)| value).collect())
    }

    /// Move the process-wide sequence past every number already stored in
    /// `bucket`, so writes after a reopen still sort after older records.
    pub(crate) async fn resume_sequence(&self, bucket: Bucket) -> Result<(), PhoskError> {
        let sql = "SELECT VALUE seq FROM type::table($tb) WHERE type::is::int(seq)";
        let mut res = self
            .db
            .query(sql)
            .bind(("tb", bucket.table()))
            .await
            .map_err(|e| Self::map_err("resume-seq", &e))?;
        let stored: Vec<i64> = res
            .take(0)
            .map_err(|e| Self::map_err("resume-seq-take", &e))?;
        if let Some(max) = stored.into_iter().max() {
            NEXT_SEQ.fetch_max(max.saturating_add(1), Ordering::SeqCst);
        }
        Ok(())
    }

    /// Delete the record at `bucket:key` (no-op if absent — callers check first
    /// when NotFound semantics are required).
    pub(crate) async fn delete(&self, bucket: Bucket, key: &str) -> Result<(), PhoskError> {
        let sql = "DELETE type::thing($tb, $id)";
        self.db
            .query(sql)
            .bind(("tb", bucket.table()))
            .bind(("id", key.to_owned()))
            .await
            .map_err(|e| Self::map_err("delete", &e))?
            .check()
            .map_err(|e| Self::map_err("delete-check", &e))?;
        Ok(())
    }

    /// Count the records in `bucket` (used by the first-run seed guard).
    pub(crate) async fn count(&self, bucket: Bucket) -> Result<usize, PhoskError> {
        let sql = "SELECT VALUE doc FROM type::table($tb)";
        let mut res = self
            .db
            .query(sql)
            .bind(("tb", bucket.table()))
            .await
            .map_err(|e| Self::map_err("count", &e))?;
        let docs: Vec<String> = res.take(0).map_err(|e| Self::map_err("count-take", &e))?;
        Ok(docs.len())
    }
}
