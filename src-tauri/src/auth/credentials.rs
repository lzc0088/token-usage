//! Vendor-credential storage via an encrypted local blob (design.md §F9).
//!
//! Replaces the OS keyring — on macOS an unsigned app triggers a Keychain
//! authorization prompt on every access, which is unusable. Instead, secrets
//! are encrypted with AES-256-GCM under a machine-derived key and stored in
//! the `app_config` SQLite table (key `cred:<vendor>`).
//!
//! The master key is `SHA256(app-salt || hostname || username)` — derived once
//! per process and cached in a `OnceCell`. This is a deliberate trade-off:
//! zero authorization prompts + cross-platform uniformity, at the cost of
//! theoretical same-machine exposure (another process can derive the same key).
//! For a local-first tool this beats both plaintext (.env) and the keyring UX.
//! After code-signing ships, the key can move back to the keyring (one prompt).

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::config;
use crate::storage::StorageError;

/// Per-vendor keyring entry is one `app_config` row keyed `cred:<vendor>`.
const KEY_PREFIX: &str = "cred:";
/// App-specific salt mixed into the master-key derivation.
const APP_SALT: &[u8] = b"token-usage::credentials::v1";
/// AES-GCM nonce size (bytes).
const NONCE_LEN: usize = 12;

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    #[error("encryption failed: {0}")]
    Encrypt(String),
    #[error("decryption failed: {0}")]
    Decrypt(String),
    #[error("credential for vendor '{0}' not found")]
    NotFound(String),
}

fn cred_key(vendor: &str) -> String {
    format!("{KEY_PREFIX}{vendor}")
}

/// Machine-bound 32-byte AES-256 key. Derived once, cached for the process.
/// Uses a `Mutex` (not `OnceCell`) because the rsproxy toolchain omits
/// `std::sync::OnceCell`. SHA-256 derivation runs at most once.
static MASTER_KEY: std::sync::Mutex<Option<[u8; 32]>> = std::sync::Mutex::new(None);

fn master_key() -> [u8; 32] {
    let mut guard = MASTER_KEY.lock().expect("master key mutex poisoned");
    if let Some(k) = guard.as_ref() {
        return *k;
    }
    let mut hasher = Sha256::new();
    hasher.update(APP_SALT);
    // Use the local data directory path as a stable machine+user identifier.
    // On macOS this is `$HOME/Library/Application Support`, which depends on
    // the user but NOT on the network (unlike `hostname::get()`, which can
    // return an IP address when HostName is unset, breaking decryption across
    // network changes).
    if let Some(dir) = dirs::data_local_dir() {
        hasher.update(dir.to_string_lossy().as_bytes());
    }
    if let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("USERNAME")) {
        hasher.update(user.as_bytes());
    }
    let key: [u8; 32] = hasher.finalize().into();
    *guard = Some(key);
    key
}

/// Encrypt a plaintext secret → `base64(nonce || ciphertext+tag)`.
fn encrypt(plaintext: &str) -> Result<String, CredentialError> {
    let key_bytes = master_key();
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| CredentialError::Encrypt(e.to_string()))?;
    let mut combined = Vec::with_capacity(NONCE_LEN + ct.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ct);
    Ok(STANDARD.encode(&combined))
}

/// Decrypt `base64(nonce || ciphertext+tag)` → plaintext.
fn decrypt(blob: &str) -> Result<String, CredentialError> {
    let combined = STANDARD
        .decode(blob)
        .map_err(|e| CredentialError::Decrypt(e.to_string()))?;
    if combined.len() < NONCE_LEN {
        return Err(CredentialError::Decrypt("blob too short".into()));
    }
    let (nonce_bytes, ct) = combined.split_at(NONCE_LEN);
    let key_bytes = master_key();
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct)
        .map_err(|e| CredentialError::Decrypt(e.to_string()))?;
    String::from_utf8(pt).map_err(|e| CredentialError::Decrypt(e.to_string()))
}

/// Store an encrypted credential for `vendor`. Overwrites any existing entry.
pub fn set(conn: &Connection, vendor: &str, secret: &str) -> Result<(), CredentialError> {
    let blob = encrypt(secret)?;
    config::set_raw(conn, &cred_key(vendor), &blob)?;
    Ok(())
}

/// Read a vendor's decrypted credential. `NotFound` if none / never set.
pub fn get(conn: &Connection, vendor: &str) -> Result<String, CredentialError> {
    match config::get_raw(conn, &cred_key(vendor))? {
        Some(blob) => decrypt(&blob),
        None => Err(CredentialError::NotFound(vendor.to_string())),
    }
}

/// Delete a vendor's credential. Ok if it didn't exist.
pub fn delete(conn: &Connection, vendor: &str) -> Result<(), CredentialError> {
    conn.execute(
        "DELETE FROM app_config WHERE key = ?",
        rusqlite::params![cred_key(vendor)],
    )
    .map_err(|e| CredentialError::Storage(StorageError::Sqlite(e)))?;
    Ok(())
}

/// Does an (decryptable) credential exist for this vendor?
pub fn exists(conn: &Connection, vendor: &str) -> Result<bool, CredentialError> {
    match get(conn, vendor) {
        Ok(_) => Ok(true),
        Err(CredentialError::NotFound(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let blob = encrypt("sk-secret-123").unwrap();
        assert_eq!(decrypt(&blob).unwrap(), "sk-secret-123");
    }

    #[test]
    fn set_get_delete_roundtrip() {
        let conn = mem();
        set(&conn, "test_vendor", "sk-secret-123").unwrap();
        assert!(exists(&conn, "test_vendor").unwrap());
        assert_eq!(get(&conn, "test_vendor").unwrap(), "sk-secret-123");
        // overwrite
        set(&conn, "test_vendor", "sk-other").unwrap();
        assert_eq!(get(&conn, "test_vendor").unwrap(), "sk-other");
        // delete
        delete(&conn, "test_vendor").unwrap();
        assert!(!exists(&conn, "test_vendor").unwrap());
    }

    #[test]
    fn stored_value_is_ciphertext_not_plaintext() {
        let conn = mem();
        set(&conn, "test_vendor2", "sk-secret-123").unwrap();
        let raw: String = conn
            .query_row(
                "SELECT value FROM app_config WHERE key = ?",
                rusqlite::params![cred_key("test_vendor2")],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!raw.contains("sk-secret-123"));
        assert!(STANDARD.decode(&raw).is_ok()); // valid base64
    }

    #[test]
    fn get_missing_is_notfound() {
        let conn = mem();
        match get(&conn, "nope") {
            Err(CredentialError::NotFound(v)) => assert_eq!(v, "nope"),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn multibyte_json_roundtrip() {
        // multi-field vendors store a JSON string; ensure UTF-8 survives.
        let conn = mem();
        let payload = r#"{"key":"sk-x","orgid":"o-1","projid":"p-1"}"#;
        set(&conn, "zai_team", payload).unwrap();
        assert_eq!(get(&conn, "zai_team").unwrap(), payload);
    }
}
