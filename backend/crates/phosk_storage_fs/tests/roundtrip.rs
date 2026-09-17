//! Integration tests for the encrypted-fs `PhotoStorage` adapter: round-trip,
//! at-rest ciphertext, EXIF strip, tamper detection, key persistence.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use phosk_adapter_storage::{PhotoStorage, StorageRef};
use phosk_core::error::PhoskError;
use phosk_storage_fs::FsPhotoStorage;

/// A minimal JPEG carrying an EXIF (APP1) segment with a secret marker, then a
/// DQT (preserved), SOS + scan data, EOI.
fn jpeg_with_exif() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&[0xFF, 0xD8]); // SOI
    // APP1, length 0x000E = 14 = 2 length bytes + 12 payload bytes.
    v.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x0E]);
    v.extend_from_slice(b"Exif\0SECRET!"); // exactly 12 bytes
    v.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x04, 0xAA, 0xBB]); // DQT preserved
    v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x04, 0x01, 0x02]); // SOS
    v.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]); // scan pixels
    v.extend_from_slice(&[0xFF, 0xD9]); // EOI
    v
}

#[tokio::test]
async fn put_get_round_trips() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsPhotoStorage::open(dir.path()).expect("open");
    let bytes = b"\x89PNG\r\n\x1a\nplain receipt bytes that are not a real image";
    let r = store.put(bytes).await.expect("put");
    // Unknown formats round-trip the pixels exactly (no strip applied).
    let got = store.get(&r).await.expect("get");
    assert_eq!(got, bytes);
}

#[tokio::test]
async fn ciphertext_on_disk_differs_from_plaintext() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsPhotoStorage::open(dir.path()).expect("open");
    let plaintext = b"super secret receipt total CHF 1234.56 at Migros Geneve".to_vec();
    let r = store.put(&plaintext).await.expect("put");

    // Find the blob file (everything in the dir except the key file).
    let mut disk_bytes = Vec::new();
    for entry in std::fs::read_dir(dir.path()).expect("read_dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        if name == "key" {
            continue;
        }
        disk_bytes = std::fs::read(entry.path()).expect("read blob");
    }
    assert!(!disk_bytes.is_empty(), "a blob file must exist on disk");
    // The plaintext must not appear verbatim in the encrypted file.
    assert!(
        !disk_bytes
            .windows(plaintext.len())
            .any(|w| w == plaintext.as_slice()),
        "plaintext must not be present in the at-rest file"
    );
    // And it must still decrypt back.
    assert_eq!(store.get(&r).await.expect("get"), plaintext);
}

#[tokio::test]
async fn exif_stripped_pixels_preserved() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsPhotoStorage::open(dir.path()).expect("open");
    let input = jpeg_with_exif();
    let r = store.put(&input).await.expect("put");
    let restored = store.get(&r).await.expect("get");

    // EXIF secret is gone from the stored (decrypted) image.
    assert!(
        !restored.windows(6).any(|w| w == b"SECRET"),
        "EXIF payload must be stripped before storage"
    );
    assert!(!restored.windows(4).any(|w| w == b"Exif"));
    // Pixels / structural data preserved: DQT payload + scan data + EOI.
    assert!(restored.windows(2).any(|w| w == [0xAA, 0xBB]));
    assert!(restored.windows(4).any(|w| w == [0x11, 0x22, 0x33, 0x44]));
    assert!(restored.starts_with(&[0xFF, 0xD8]));
    assert!(restored.ends_with(&[0xFF, 0xD9]));
    // Stripped is smaller than the original (lost the APP1 segment).
    assert!(restored.len() < input.len());
}

#[tokio::test]
async fn get_unknown_ref_is_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsPhotoStorage::open(dir.path()).expect("open");
    let err = store
        .get(&StorageRef::new("deadbeefdeadbeefdeadbeefdeadbeef"))
        .await
        .expect_err("must error");
    assert!(matches!(err, PhoskError::NotFound(_)));
}

#[tokio::test]
async fn traversal_ref_is_rejected_as_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsPhotoStorage::open(dir.path()).expect("open");
    // A ref containing a path separator / non-hex must never resolve to a path.
    for bad in ["../key", "key", "..", "a/b", "abc.def"] {
        let err = store
            .get(&StorageRef::new(bad))
            .await
            .expect_err("traversal ref must error");
        assert!(matches!(err, PhoskError::NotFound(_)), "ref {bad:?}");
    }
}

#[tokio::test]
async fn delete_removes_then_get_not_found_and_is_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsPhotoStorage::open(dir.path()).expect("open");
    let r = store.put(b"to delete").await.expect("put");
    store.delete(&r).await.expect("delete");
    let err = store.get(&r).await.expect_err("deleted");
    assert!(matches!(err, PhoskError::NotFound(_)));
    // Idempotent: deleting again is Ok.
    store.delete(&r).await.expect("second delete");
}

#[tokio::test]
async fn distinct_puts_get_distinct_refs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsPhotoStorage::open(dir.path()).expect("open");
    let a = store.put(b"alpha").await.expect("a");
    let b = store.put(b"beta").await.expect("b");
    assert_ne!(a, b);
    assert_eq!(store.get(&a).await.expect("ga"), b"alpha");
    assert_eq!(store.get(&b).await.expect("gb"), b"beta");
}

#[tokio::test]
async fn tampered_file_fails_to_decrypt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsPhotoStorage::open(dir.path()).expect("open");
    let r = store.put(b"authentic bytes").await.expect("put");

    // Flip a byte in the blob file (past the nonce, in the ciphertext/tag).
    let path = dir.path().join(r.as_str());
    let mut raw = std::fs::read(&path).expect("read");
    let last = raw.len() - 1;
    raw[last] ^= 0xFF;
    std::fs::write(&path, &raw).expect("write tampered");

    let err = store.get(&r).await.expect_err("tampered must fail auth");
    assert!(matches!(err, PhoskError::Invalid(_)));
}

#[tokio::test]
async fn key_persists_across_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r;
    {
        let store = FsPhotoStorage::open(dir.path()).expect("open 1");
        r = store.put(b"persisted across reopen").await.expect("put");
    }
    // Reopen the same dir: same key file → same key → decrypt works.
    let store2 = FsPhotoStorage::open(dir.path()).expect("open 2");
    assert_eq!(
        store2.get(&r).await.expect("get after reopen"),
        b"persisted across reopen"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn key_file_is_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let _store = FsPhotoStorage::open(dir.path()).expect("open");
    let meta = std::fs::metadata(dir.path().join("key")).expect("key meta");
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
}

#[tokio::test]
async fn empty_bytes_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FsPhotoStorage::open(dir.path()).expect("open");
    let r = store.put(b"").await.expect("put empty");
    assert_eq!(store.get(&r).await.expect("get empty"), b"");
}

#[tokio::test]
async fn usable_behind_arc_dyn() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store: Arc<dyn PhotoStorage> = Arc::new(FsPhotoStorage::open(dir.path()).expect("open"));
    let r = store.put(b"shared").await.expect("put");
    assert_eq!(store.get(&r).await.expect("get"), b"shared");
}
