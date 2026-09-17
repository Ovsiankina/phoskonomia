//! `phosk_adapter_storage` — the `PhotoStorage` **port** (ADR-005/010).
//!
//! This is the L2 contract seam between feature code and whatever persists the
//! raw receipt-photo bytes. A blob goes in, an opaque [`StorageRef`] comes back;
//! later that ref fetches or deletes the blob. The port is deliberately *dumb*:
//! it stores and returns opaque bytes and knows nothing about images, EXIF,
//! MIME, encryption, or paths. Those concerns (EXIF strip on import, encryption
//! at rest, libmagic validation) live above the port (the import pipeline) or
//! inside an L3 concrete adapter — never here.
//!
//! Per the layering rule, feature crates depend on this trait and take
//! `&dyn PhotoStorage` / `Arc<dyn PhotoStorage>`; concrete adapters (an
//! encrypted-fs store, an object store, …) are wired in only by a `bin/*`
//! composition root / the Dioxus server context and are never imported here.
//! Vendor types (file handles, paths, S3 SDK objects, cipher state) die in the
//! L3 adapter and never cross this boundary.
//!
//! The [`StorageRef`] is an **opaque** string newtype: callers must treat it as
//! a token to round-trip back to the same adapter, not as a path or URL to parse
//! or construct. Two different adapters may mint refs with totally different
//! internal shapes; only the adapter that minted a ref can resolve it.
//!
//! A small [`InMemoryStorage`] fake ships here so feature/integration tests get a
//! real, working `PhotoStorage` (correct put/get/delete semantics) without
//! touching disk or pulling in a concrete adapter.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use phosk_core::error::PhoskError;
use serde::{Deserialize, Serialize};

/// An **opaque** handle to a stored blob.
///
/// Minted by a [`PhotoStorage`] implementation on `put`, and handed back to that
/// same implementation on `get` / `delete`. The inner string is an
/// implementation detail of the adapter that produced it — callers must not
/// parse, construct, or interpret it (it is *not* a path or URL). It round-trips
/// through serde as a transparent string for the seed / persistence boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StorageRef(String);

impl StorageRef {
    /// Wrap an adapter-minted token. Only a `PhotoStorage` implementation (or
    /// persistence rehydrating a previously stored ref) should call this.
    #[inline]
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// Borrow the opaque token (e.g. to persist it alongside a `Receipt`).
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the owned opaque token.
    #[inline]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for StorageRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The opaque-blob storage port: put bytes → get a ref → fetch / delete by ref.
///
/// Object-safe and `Send + Sync` so it can live behind `Arc<dyn PhotoStorage>`
/// in the composition root and be shared across tasks. All operations are
/// fallible through [`PhoskError`]; a missing ref on `get` is
/// [`PhoskError::NotFound`], a `delete` of a missing ref is idempotent (`Ok`).
#[async_trait]
pub trait PhotoStorage: Send + Sync {
    /// Store `bytes` and return an opaque [`StorageRef`] that resolves to them.
    ///
    /// The bytes are treated as fully opaque — the port does no validation,
    /// transformation, or interpretation. Returns [`PhoskError::Invalid`] only
    /// if an adapter has a hard limit it must reject (e.g. an over-size blob).
    async fn put(&self, bytes: &[u8]) -> Result<StorageRef, PhoskError>;

    /// Fetch the bytes previously stored under `r`.
    ///
    /// Returns [`PhoskError::NotFound`] if the ref is unknown to this adapter
    /// (never seen, or already deleted).
    async fn get(&self, r: &StorageRef) -> Result<Vec<u8>, PhoskError>;

    /// Remove the blob stored under `r`.
    ///
    /// **Idempotent:** deleting an unknown / already-deleted ref is `Ok(())`,
    /// not an error — the post-condition (the ref no longer resolves) holds
    /// either way.
    async fn delete(&self, r: &StorageRef) -> Result<(), PhoskError>;
}

/// An in-process, non-persistent [`PhotoStorage`] fake for tests and local dev.
///
/// Keeps blobs in a `HashMap` behind a `Mutex` (interior mutability so the trait
/// methods stay `&self`, matching every real adapter). Refs are monotonically
/// numbered (`mem:0`, `mem:1`, …) so they are stable and easy to assert on, yet
/// still opaque to callers. Not for production: data evaporates on drop.
#[derive(Debug, Default)]
pub struct InMemoryStorage {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    blobs: HashMap<String, Vec<u8>>,
    next: u64,
}

impl InMemoryStorage {
    /// A fresh, empty in-memory store.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of distinct blobs currently held (test convenience).
    pub fn len(&self) -> Result<usize, PhoskError> {
        let inner = self.lock()?;
        Ok(inner.blobs.len())
    }

    /// Whether the store holds no blobs (test convenience).
    pub fn is_empty(&self) -> Result<bool, PhoskError> {
        Ok(self.len()? == 0)
    }

    /// Lock the inner map, mapping a poisoned mutex to an internal error rather
    /// than panicking (the no-panic rule applies even to the test fake).
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Inner>, PhoskError> {
        self.inner
            .lock()
            .map_err(|_| PhoskError::Invalid("in-memory storage mutex poisoned".to_owned()))
    }
}

#[async_trait]
impl PhotoStorage for InMemoryStorage {
    async fn put(&self, bytes: &[u8]) -> Result<StorageRef, PhoskError> {
        let key = {
            let mut inner = self.lock()?;
            let id = inner.next;
            inner.next = inner.next.wrapping_add(1);
            let key = format!("mem:{id}");
            inner.blobs.insert(key.clone(), bytes.to_vec());
            key
        };
        tracing::debug!(storage_ref = %key, len = bytes.len(), "in-memory storage put");
        Ok(StorageRef::new(key))
    }

    async fn get(&self, r: &StorageRef) -> Result<Vec<u8>, PhoskError> {
        let blob = self.lock()?.blobs.get(r.as_str()).cloned();
        blob.ok_or_else(|| PhoskError::NotFound(format!("storage ref {}", r.as_str())))
    }

    async fn delete(&self, r: &StorageRef) -> Result<(), PhoskError> {
        tracing::debug!(storage_ref = %r, "in-memory storage delete");
        self.lock()?.blobs.remove(r.as_str());
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn assert_object_safe(_s: &dyn PhotoStorage) {}

    #[test]
    fn storage_ref_is_opaque_string() {
        let r = StorageRef::new("mem:7");
        assert_eq!(r.as_str(), "mem:7");
        assert_eq!(r.to_string(), "mem:7");
        assert_eq!(r.into_inner(), "mem:7");
    }

    #[test]
    fn storage_ref_serde_is_transparent() {
        let r = StorageRef::new("abc-123");
        let json = serde_json::to_string(&r).expect("serialize ref");
        assert_eq!(json, "\"abc-123\"");
        let back: StorageRef = serde_json::from_str(&json).expect("deserialize ref");
        assert_eq!(back, r);
    }

    #[test]
    fn port_is_object_safe() {
        let store = InMemoryStorage::new();
        assert_object_safe(&store);
    }

    #[tokio::test]
    async fn put_then_get_round_trips_bytes() {
        let store = InMemoryStorage::new();
        let bytes = b"\x89PNG\r\n\x1a\n receipt photo bytes";
        let r = store.put(bytes).await.expect("put");
        let got = store.get(&r).await.expect("get");
        assert_eq!(got, bytes);
    }

    #[tokio::test]
    async fn distinct_puts_get_distinct_refs() {
        let store = InMemoryStorage::new();
        let a = store.put(b"alpha").await.expect("put a");
        let b = store.put(b"beta").await.expect("put b");
        assert_ne!(a, b);
        assert_eq!(store.get(&a).await.expect("get a"), b"alpha");
        assert_eq!(store.get(&b).await.expect("get b"), b"beta");
        assert_eq!(store.len().expect("len"), 2);
    }

    #[tokio::test]
    async fn identical_bytes_still_get_distinct_refs() {
        // Content-addressing is NOT promised by the port; two puts of the same
        // bytes are two independent blobs with two refs.
        let store = InMemoryStorage::new();
        let a = store.put(b"same").await.expect("put a");
        let b = store.put(b"same").await.expect("put b");
        assert_ne!(a, b);
        assert_eq!(store.len().expect("len"), 2);
    }

    #[tokio::test]
    async fn get_unknown_ref_is_not_found() {
        let store = InMemoryStorage::new();
        let err = store
            .get(&StorageRef::new("mem:999"))
            .await
            .expect_err("unknown ref must error");
        assert!(matches!(err, PhoskError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_removes_then_get_is_not_found() {
        let store = InMemoryStorage::new();
        let r = store.put(b"to be deleted").await.expect("put");
        store.delete(&r).await.expect("delete");
        let err = store.get(&r).await.expect_err("deleted ref must error");
        assert!(matches!(err, PhoskError::NotFound(_)));
        assert!(store.is_empty().expect("is_empty"));
    }

    #[tokio::test]
    async fn delete_is_idempotent() {
        let store = InMemoryStorage::new();
        let r = StorageRef::new("mem:never-existed");
        // Deleting an unknown ref is Ok, and a second delete is Ok too.
        store.delete(&r).await.expect("first delete");
        store.delete(&r).await.expect("second delete");
    }

    #[tokio::test]
    async fn empty_bytes_round_trip() {
        let store = InMemoryStorage::new();
        let r = store.put(b"").await.expect("put empty");
        assert_eq!(store.get(&r).await.expect("get empty"), b"");
    }

    #[tokio::test]
    async fn usable_behind_arc_dyn() {
        use std::sync::Arc;
        let store: Arc<dyn PhotoStorage> = Arc::new(InMemoryStorage::new());
        let r = store.put(b"shared").await.expect("put");
        assert_eq!(store.get(&r).await.expect("get"), b"shared");
    }
}
