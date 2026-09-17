//! `phosk_storage_fs` — the **encrypted-filesystem** [`PhotoStorage`] adapter
//! (L3, ADR-005/010).
//!
//! A concrete implementation of the `phosk_adapter_storage::PhotoStorage` port
//! that persists receipt photos to files under a configurable data directory,
//! **encrypted at rest** and **EXIF-stripped on import** (ADR §0 security model:
//! photos are encrypted at rest, EXIF stripped on import, telemetry zero).
//!
//! Pipeline per `put`:
//! 1. **Strip metadata** in pure Rust (see [`exif`]) — EXIF/XMP/ICC/comment for
//!    JPEG, text/`eXIf` chunks for PNG — leaving pixels untouched.
//! 2. **Encrypt** the stripped bytes with XChaCha20-Poly1305 under a per-store
//!    data key, using a fresh random 24-byte nonce.
//! 3. **Write** `nonce ‖ ciphertext` to a file named by a random token; that
//!    token is the opaque [`StorageRef`].
//!
//! `get` reads the file, splits off the nonce, and decrypts (authenticated — a
//! tampered file fails to decrypt). `delete` removes the file (idempotent).
//!
//! ### Key management
//! The 32-byte data key lives in a `key` file inside the data dir, created with
//! `0600` permissions on first use and reused thereafter. This is a local,
//! single-user desktop threat model: the key sits next to the data, so the
//! encryption defends against *off-device* exposure (a stolen disk image, a
//! backup, a synced folder) rather than a live local attacker. A future KDF /
//! OS-keyring upgrade is an adapter-internal change that never touches the port.
//!
//! ### Layering
//! This crate is wired in **only** by a composition root (`bin/*` or the Dioxus
//! server context); feature crates depend on the L2 `PhotoStorage` trait and
//! never import it. Vendor types (the cipher, file handles, nonces) die here.

mod exif;

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use phosk_adapter_storage::{PhotoStorage, StorageRef};
use phosk_core::error::PhoskError;
use rand::RngCore;

/// XChaCha20-Poly1305 nonce length (bytes).
const NONCE_LEN: usize = 24;
/// Data-key length (bytes).
const KEY_LEN: usize = 32;
/// File name of the at-rest data key inside the data dir.
const KEY_FILE: &str = "key";
/// Length of the random hex token used for blob filenames (16 bytes → 32 hex).
const REF_TOKEN_BYTES: usize = 16;

/// An encrypted-at-rest, EXIF-stripping [`PhotoStorage`] backed by a data dir.
///
/// Construct with [`FsPhotoStorage::open`], pointing at a directory the process
/// owns. The directory is created if missing; the data key is generated on first
/// use (`0600`) and loaded thereafter.
#[derive(Debug)]
pub struct FsPhotoStorage {
    dir: PathBuf,
    key: [u8; KEY_LEN],
}

impl FsPhotoStorage {
    /// Open (creating if needed) an encrypted store rooted at `dir`.
    ///
    /// Creates the directory and, on first use, generates the `0600` data key.
    /// Returns [`PhoskError::Invalid`] if the directory or key cannot be
    /// established (carrying a non-PII reason).
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, PhoskError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)
            .map_err(|e| PhoskError::Invalid(format!("create data dir: {e}")))?;
        let key = load_or_create_key(&dir)?;
        tracing::debug!(dir = %dir.display(), "opened encrypted fs photo storage");
        Ok(Self { dir, key })
    }

    /// Build the cipher from the loaded data key.
    fn cipher(&self) -> Result<XChaCha20Poly1305, PhoskError> {
        XChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| PhoskError::Invalid("invalid storage key length".to_owned()))
    }

    /// Resolve a ref to its on-disk path, rejecting any token that is not the
    /// flat random filename we mint (defence against path traversal — a ref must
    /// never contain a separator, `.` component, or anything but hex).
    fn path_for(&self, r: &StorageRef) -> Result<PathBuf, PhoskError> {
        let token = r.as_str();
        let valid =
            !token.is_empty() && token.len() <= 128 && token.bytes().all(|b| b.is_ascii_hexdigit());
        if !valid {
            return Err(PhoskError::NotFound(format!("storage ref {token}")));
        }
        Ok(self.dir.join(token))
    }
}

/// Load the data key from `dir/key`, or generate + persist a fresh one (`0600`).
fn load_or_create_key(dir: &Path) -> Result<[u8; KEY_LEN], PhoskError> {
    let key_path = dir.join(KEY_FILE);
    match std::fs::read(&key_path) {
        Ok(bytes) => {
            let arr: [u8; KEY_LEN] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| PhoskError::Invalid("corrupt storage key file".to_owned()))?;
            Ok(arr)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut key = [0u8; KEY_LEN];
            rand::rngs::OsRng.fill_bytes(&mut key);
            write_key_0600(&key_path, &key)?;
            tracing::debug!("generated new at-rest storage key");
            Ok(key)
        }
        Err(e) => Err(PhoskError::Invalid(format!("read storage key: {e}"))),
    }
}

/// Write the key file with owner-only `0600` permissions.
#[cfg(unix)]
fn write_key_0600(path: &Path, key: &[u8; KEY_LEN]) -> Result<(), PhoskError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| PhoskError::Invalid(format!("create key file: {e}")))?;
    f.write_all(key)
        .map_err(|e| PhoskError::Invalid(format!("write key file: {e}")))?;
    f.flush()
        .map_err(|e| PhoskError::Invalid(format!("flush key file: {e}")))?;
    Ok(())
}

/// Non-unix fallback: write the key, then best-effort tighten permissions.
#[cfg(not(unix))]
fn write_key_0600(path: &Path, key: &[u8; KEY_LEN]) -> Result<(), PhoskError> {
    std::fs::write(path, key).map_err(|e| PhoskError::Invalid(format!("write key file: {e}")))?;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_readonly(false);
        let _ = std::fs::set_permissions(path, perms);
    }
    Ok(())
}

/// Mint a fresh random hex token for a blob filename.
fn fresh_ref_token() -> String {
    let mut raw = [0u8; REF_TOKEN_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    let mut s = String::with_capacity(REF_TOKEN_BYTES * 2);
    for b in raw {
        // Lowercase hex; `is_ascii_hexdigit` accepts it on the read path.
        s.push(char::from(HEX[(b >> 4) as usize]));
        s.push(char::from(HEX[(b & 0x0f) as usize]));
    }
    s
}

const HEX: &[u8; 16] = b"0123456789abcdef";

#[async_trait]
impl PhotoStorage for FsPhotoStorage {
    async fn put(&self, bytes: &[u8]) -> Result<StorageRef, PhoskError> {
        // 1. EXIF / metadata strip (pixels untouched).
        let stripped = exif::strip_metadata(bytes);

        // 2. Encrypt with a fresh random nonce.
        let cipher = self.cipher()?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, stripped.as_ref())
            .map_err(|_| PhoskError::Invalid("encrypt failed".to_owned()))?;

        // 3. Lay out `nonce ‖ ciphertext` and write to a random-named file.
        let mut on_disk = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        on_disk.extend_from_slice(&nonce_bytes);
        on_disk.extend_from_slice(&ciphertext);

        // Retry on the (astronomically unlikely) name collision so we never
        // clobber an existing blob.
        let token = loop {
            let token = fresh_ref_token();
            let path = self.dir.join(&token);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    use std::io::Write;
                    f.write_all(&on_disk)
                        .map_err(|e| PhoskError::Invalid(format!("write blob: {e}")))?;
                    f.flush()
                        .map_err(|e| PhoskError::Invalid(format!("flush blob: {e}")))?;
                    break token;
                }
                // Name collision (astronomically unlikely): loop and re-mint.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(PhoskError::Invalid(format!("create blob: {e}"))),
            }
        };

        tracing::debug!(len = bytes.len(), "stored encrypted photo");
        Ok(StorageRef::new(token))
    }

    async fn get(&self, r: &StorageRef) -> Result<Vec<u8>, PhoskError> {
        let path = self.path_for(r)?;
        let on_disk = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(PhoskError::NotFound(format!("storage ref {}", r.as_str())));
            }
            Err(e) => return Err(PhoskError::Invalid(format!("read blob: {e}"))),
        };
        if on_disk.len() < NONCE_LEN {
            return Err(PhoskError::Invalid("blob too short".to_owned()));
        }
        let (nonce_bytes, ciphertext) = on_disk.split_at(NONCE_LEN);
        let cipher = self.cipher()?;
        let nonce = XNonce::from_slice(nonce_bytes);
        let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
            PhoskError::Invalid("decrypt failed (tampered or wrong key)".to_owned())
        })?;
        Ok(plaintext)
    }

    async fn delete(&self, r: &StorageRef) -> Result<(), PhoskError> {
        // An invalid/unknown-shaped ref simply has nothing to delete.
        let Ok(path) = self.path_for(r) else {
            return Ok(());
        };
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(PhoskError::Invalid(format!("delete blob: {e}"))),
        }
    }
}
