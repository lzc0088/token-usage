//! Vendor-credential storage via the OS keyring (T2.4 / design.md §F9).
//!
//! Credentials (OAuth tokens, API keys, cookies) never touch SQLite or disk in
//! plaintext. Each vendor is one keyring entry under service `token-usage`,
//! account = vendor id. The `Secret` is whatever string the vendor needs (API
//! key, JSON of an OAuth token, cookie header, AK+SK joined…).

use keyring::Entry;

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("keyring: {0}")]
    Keyring(String),
    #[error("credential for vendor '{0}' not found")]
    NotFound(String),
}

/// Service name under which all vendor credentials are stored.
pub const SERVICE: &str = "token-usage";

/// Store a credential for `vendor`. Overwrites any existing entry.
pub fn set(vendor: &str, secret: &str) -> Result<(), CredentialError> {
    entry(vendor)?
        .set_password(secret)
        .map_err(|e| CredentialError::Keyring(e.to_string()))
}

/// Read a vendor's credential. `NotFound` if none / never set.
pub fn get(vendor: &str) -> Result<String, CredentialError> {
    match entry(vendor)?.get_password() {
        Ok(s) => Ok(s),
        Err(keyring::Error::NoEntry) => Err(CredentialError::NotFound(vendor.to_string())),
        Err(e) => Err(CredentialError::Keyring(e.to_string())),
    }
}

/// Delete a vendor's credential. Ok if it didn't exist.
pub fn delete(vendor: &str) -> Result<(), CredentialError> {
    match entry(vendor)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CredentialError::Keyring(e.to_string())),
    }
}

/// Does a credential exist for this vendor?
pub fn exists(vendor: &str) -> Result<bool, CredentialError> {
    match get(vendor) {
        Ok(_) => Ok(true),
        Err(CredentialError::NotFound(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

fn entry(vendor: &str) -> Result<Entry, CredentialError> {
    Entry::new(SERVICE, vendor).map_err(|e| CredentialError::Keyring(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise the real OS keyring (mac Keychain / Win Credential Manager
    // / Linux Secret Service). They may need a running secrets service on
    // headless Linux; on mac/Win dev machines they pass. Vendors are namespaced
    // under a `tu_test_` prefix and cleaned up.

    fn cleanup(vendors: &[&str]) {
        for v in vendors {
            let _ = delete(v);
        }
    }

    #[test]
    #[ignore = "real OS keychain; run on a GUI session (mac Keychain / Win CredMan / \
                Linux Secret Service). Headless/SSH/CI sessions can't reliably access it."]
    fn set_get_delete_roundtrip() {
        let vendor = "tu_test_a";
        cleanup(&[vendor]);
        set(vendor, "sk-secret-123").unwrap();
        assert!(exists(vendor).unwrap());
        assert_eq!(get(vendor).unwrap(), "sk-secret-123");
        // overwrite
        set(vendor, "sk-other").unwrap();
        assert_eq!(get(vendor).unwrap(), "sk-other");
        delete(vendor).unwrap();
        assert!(!exists(vendor).unwrap());
        cleanup(&[vendor]);
    }

    #[test]
    fn get_missing_is_notfound() {
        let vendor = "tu_test_missing";
        cleanup(&[vendor]);
        match get(vendor) {
            Err(CredentialError::NotFound(v)) => assert_eq!(v, vendor),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn delete_missing_is_ok() {
        let vendor = "tu_test_missing2";
        cleanup(&[vendor]);
        assert!(delete(vendor).is_ok());
    }
}
