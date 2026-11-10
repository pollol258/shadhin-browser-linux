//! Password management and desktop Keyring / Secret Service integration foundation
//!
//! Provides a secure, abstract architecture for saving, retrieving, and managing
//! web and HTTP authentication credentials. Prepared for Linux Desktop Secret Service
//! (`org.freedesktop.secrets` / `oo7` / `libsecret` / `keyring-rs`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::info;

/// Error types occurring during credential and keyring operations
#[derive(Debug, Error)]
pub enum PasswordStoreError {
    #[error("Keyring service unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("Operation failed: {0}")]
    OperationFailed(String),
    #[error("Secret service access denied: {0}")]
    AccessDenied(String),
    #[error("Keyring item not found")]
    NotFound,
}

/// Metadata describing a stored credential without exposing the plaintext secret
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedCredential {
    pub origin: String,
    pub username: String,
    pub created_at: i64,
    pub last_used_at: i64,
}

/// Abstract backend interface for secure credential persistence.
///
/// Implementations can back onto the Linux Desktop Secret Service (via `oo7` or `libsecret`),
/// in-memory test mocks, or external password managers.
pub trait KeyringBackend: Send + Sync {
    /// Human-readable identifier for this backend (e.g. "SecretService", "In-Memory Mock")
    fn name(&self) -> &'static str;

    /// Checks whether the desktop keyring service or daemon is currently reachable
    fn is_available(&self) -> bool;

    /// Stores a password securely associated with an origin and username
    fn save_password(
        &self,
        origin: &str,
        username: &str,
        secret: &str,
    ) -> Result<(), PasswordStoreError>;

    /// Retrieves a password for the specified origin and username
    fn get_password(
        &self,
        origin: &str,
        username: &str,
    ) -> Result<Option<String>, PasswordStoreError>;

    /// Deletes a stored credential
    fn delete_password(&self, origin: &str, username: &str) -> Result<bool, PasswordStoreError>;

    /// Lists stored credential metadata, optionally filtered by origin
    fn list_credentials(
        &self,
        origin: Option<&str>,
    ) -> Result<Vec<SavedCredential>, PasswordStoreError>;
}

/// In-memory keyring backend designed for unit tests and headless environments
#[derive(Debug, Default)]
pub struct MockKeyringBackend {
    store: RwLock<HashMap<(String, String), (String, SavedCredential)>>,
}

impl MockKeyringBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyringBackend for MockKeyringBackend {
    fn name(&self) -> &'static str {
        "MockKeyring"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn save_password(
        &self,
        origin: &str,
        username: &str,
        secret: &str,
    ) -> Result<(), PasswordStoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut lock = self
            .store
            .write()
            .map_err(|e| PasswordStoreError::OperationFailed(e.to_string()))?;

        let key = (origin.to_string(), username.to_string());
        let meta = SavedCredential {
            origin: origin.to_string(),
            username: username.to_string(),
            created_at: now,
            last_used_at: now,
        };

        lock.insert(key, (secret.to_string(), meta));
        Ok(())
    }

    fn get_password(
        &self,
        origin: &str,
        username: &str,
    ) -> Result<Option<String>, PasswordStoreError> {
        let mut lock = self
            .store
            .write()
            .map_err(|e| PasswordStoreError::OperationFailed(e.to_string()))?;

        let key = (origin.to_string(), username.to_string());
        if let Some((secret, meta)) = lock.get_mut(&key) {
            meta.last_used_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            Ok(Some(secret.clone()))
        } else {
            Ok(None)
        }
    }

    fn delete_password(&self, origin: &str, username: &str) -> Result<bool, PasswordStoreError> {
        let mut lock = self
            .store
            .write()
            .map_err(|e| PasswordStoreError::OperationFailed(e.to_string()))?;

        let key = (origin.to_string(), username.to_string());
        Ok(lock.remove(&key).is_some())
    }

    fn list_credentials(
        &self,
        origin: Option<&str>,
    ) -> Result<Vec<SavedCredential>, PasswordStoreError> {
        let lock = self
            .store
            .read()
            .map_err(|e| PasswordStoreError::OperationFailed(e.to_string()))?;

        let list = lock
            .values()
            .filter(|(_, meta)| origin.is_none_or(|o| meta.origin == o))
            .map(|(_, meta)| meta.clone())
            .collect();

        Ok(list)
    }
}

/// Linux Desktop Secret Service (`org.freedesktop.secrets`) backend architecture stub.
///
/// Prepared for direct integration with `oo7` or `keyring-rs` or `libsecret`.
/// Schema: `org.shadhin.BrowserCredential`
/// Attributes:
/// - `xdg:schema`: `org.shadhin.BrowserCredential`
/// - `origin`: Target web origin / host
/// - `username`: Account identifier
/// - `application`: `shadhin-browser`
pub struct SecretServiceBackendStub {
    _app_id: String,
}

impl SecretServiceBackendStub {
    pub const SCHEMA: &'static str = "org.shadhin.BrowserCredential";

    pub fn new() -> Self {
        Self {
            _app_id: "shadhin-browser".to_string(),
        }
    }
}

impl Default for SecretServiceBackendStub {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringBackend for SecretServiceBackendStub {
    fn name(&self) -> &'static str {
        "SecretService (D-Bus / org.freedesktop.secrets)"
    }

    fn is_available(&self) -> bool {
        // Checks if DBUS_SESSION_BUS_ADDRESS is present in current environment
        std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok()
    }

    fn save_password(
        &self,
        origin: &str,
        username: &str,
        _secret: &str,
    ) -> Result<(), PasswordStoreError> {
        if !self.is_available() {
            return Err(PasswordStoreError::ServiceUnavailable(
                "D-Bus session bus not available".to_string(),
            ));
        }

        info!(
            "SecretServiceBackend: Storing credential for {} ({}) under schema {}",
            origin, username, Self::SCHEMA
        );
        // Stub implementation ready for oo7 Secret Service client calls:
        // oo7::Keyring::new().await?.create_item(...)
        Ok(())
    }

    fn get_password(
        &self,
        origin: &str,
        username: &str,
    ) -> Result<Option<String>, PasswordStoreError> {
        if !self.is_available() {
            return Err(PasswordStoreError::ServiceUnavailable(
                "D-Bus session bus not available".to_string(),
            ));
        }

        info!(
            "SecretServiceBackend: Looking up credential for {} ({}) under schema {}",
            origin, username, Self::SCHEMA
        );
        // Stub implementation ready for oo7 Secret Service query:
        // oo7::Keyring::new().await?.search_items(...)
        Ok(None)
    }

    fn delete_password(&self, origin: &str, username: &str) -> Result<bool, PasswordStoreError> {
        if !self.is_available() {
            return Err(PasswordStoreError::ServiceUnavailable(
                "D-Bus session bus not available".to_string(),
            ));
        }

        info!(
            "SecretServiceBackend: Deleting credential for {} ({})",
            origin, username
        );
        Ok(false)
    }

    fn list_credentials(
        &self,
        _origin: Option<&str>,
    ) -> Result<Vec<SavedCredential>, PasswordStoreError> {
        if !self.is_available() {
            return Err(PasswordStoreError::ServiceUnavailable(
                "D-Bus session bus not available".to_string(),
            ));
        }
        Ok(Vec::new())
    }
}

/// Primary Password Manager service coordinating between WebKit authentication and secure storage
#[derive(Clone)]
pub struct PasswordManager {
    backend: Arc<dyn KeyringBackend>,
    offer_save_passwords: bool,
}

impl PasswordManager {
    /// Constructs a password manager with default desktop keyring backend
    pub fn new(offer_save_passwords: bool) -> Self {
        Self {
            backend: Arc::new(SecretServiceBackendStub::new()),
            offer_save_passwords,
        }
    }

    /// Constructs a password manager with a specified backend (e.g. for testing)
    pub fn with_backend(backend: Arc<dyn KeyringBackend>, offer_save_passwords: bool) -> Self {
        Self {
            backend,
            offer_save_passwords,
        }
    }

    /// Whether the user has enabled the "Offer to save passwords" preference
    pub fn offer_save_passwords(&self) -> bool {
        self.offer_save_passwords
    }

    /// Updates the "Offer to save passwords" preference
    pub fn set_offer_save_passwords(&mut self, enabled: bool) {
        self.offer_save_passwords = enabled;
    }

    /// Returns the active backend name
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    /// Saves a credential securely
    pub fn save_credential(
        &self,
        origin: &str,
        username: &str,
        password: &str,
    ) -> Result<(), PasswordStoreError> {
        if !self.offer_save_passwords {
            info!("Password saving skipped because 'Offer to save passwords' is disabled");
            return Ok(());
        }
        self.backend.save_password(origin, username, password)
    }

    /// Retrieves a credential secret
    pub fn get_credential(
        &self,
        origin: &str,
        username: &str,
    ) -> Result<Option<String>, PasswordStoreError> {
        self.backend.get_password(origin, username)
    }

    /// Deletes a stored credential
    pub fn delete_credential(
        &self,
        origin: &str,
        username: &str,
    ) -> Result<bool, PasswordStoreError> {
        self.backend.delete_password(origin, username)
    }

    /// Lists stored credentials without secrets
    pub fn list_credentials(
        &self,
        origin: Option<&str>,
    ) -> Result<Vec<SavedCredential>, PasswordStoreError> {
        self.backend.list_credentials(origin)
    }

    /// Configures a WebKit `AuthenticationRequest` based on the user's password saving policy.
    ///
    /// When WebKit encounters HTTP basic/digest auth or credential challenges, this hook
    /// ensures `can_save_credentials` respects the user's explicit preference.
    pub fn handle_authentication_request(&self, request: &webkit6::AuthenticationRequest) {
        let can_save = self.offer_save_passwords;
        request.set_can_save_credentials(can_save);

        if let Some(mut proposed) = request.proposed_credential() {
            info!(
                "WebKit proposed credential: username={:?}, persistence={:?}",
                proposed.username(),
                proposed.persistence()
            );
        } else {
            info!(
                "WebKit authentication request handled (can_save_credentials={})",
                can_save
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_keyring_save_and_retrieve() {
        let backend = Arc::new(MockKeyringBackend::new());
        let pm = PasswordManager::with_backend(backend, true);

        pm.save_credential("https://example.com", "user@example.com", "secret123")
            .unwrap();

        let retrieved = pm
            .get_credential("https://example.com", "user@example.com")
            .unwrap();
        assert_eq!(retrieved.as_deref(), Some("secret123"));

        let non_existent = pm
            .get_credential("https://example.com", "unknown")
            .unwrap();
        assert!(non_existent.is_none());
    }

    #[test]
    fn test_password_saving_disabled_by_preference() {
        let backend = Arc::new(MockKeyringBackend::new());
        let pm = PasswordManager::with_backend(backend, false);

        pm.save_credential("https://example.com", "alice", "p@ssword")
            .unwrap();

        let retrieved = pm.get_credential("https://example.com", "alice").unwrap();
        assert_eq!(
            retrieved, None,
            "Password should NOT be saved when offer_save_passwords is false"
        );
    }

    #[test]
    fn test_list_and_delete_credentials() {
        let backend = Arc::new(MockKeyringBackend::new());
        let pm = PasswordManager::with_backend(backend, true);

        pm.save_credential("https://site-a.com", "user1", "pass1")
            .unwrap();
        pm.save_credential("https://site-b.com", "user2", "pass2")
            .unwrap();
        pm.save_credential("https://site-a.com", "user3", "pass3")
            .unwrap();

        let site_a_creds = pm.list_credentials(Some("https://site-a.com")).unwrap();
        assert_eq!(site_a_creds.len(), 2);

        let all_creds = pm.list_credentials(None).unwrap();
        assert_eq!(all_creds.len(), 3);

        let deleted = pm
            .delete_credential("https://site-a.com", "user1")
            .unwrap();
        assert!(deleted);

        let site_a_after = pm.list_credentials(Some("https://site-a.com")).unwrap();
        assert_eq!(site_a_after.len(), 1);
        assert_eq!(site_a_after[0].username, "user3");
    }

    #[test]
    fn test_secret_service_backend_stub_metadata() {
        let stub = SecretServiceBackendStub::new();
        assert_eq!(SecretServiceBackendStub::SCHEMA, "org.shadhin.BrowserCredential");
        assert!(stub.name().contains("SecretService"));
    }
}
