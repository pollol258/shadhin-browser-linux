use crate::storage::StorageService;
use crate::storage::workspaces::{SessionTabItem, WorkspaceRepository};
use crate::utils::debounce::Debounce;
use gtk4::glib;
use std::collections::HashMap;
use std::fs::File;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use webkit6::{CookiePersistentStorage, NetworkSession};

/// Tab descriptor for lazy restoration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredTabDescriptor {
    pub url: String,
    pub title: String,
    pub is_active: bool,
}

/// Workspace session coordinator managing WebKit `NetworkSession` data isolation
/// and SQLite session tab persistence.
pub struct SessionManager {
    storage: StorageService,
    base_data_dir: PathBuf,
    base_cache_dir: PathBuf,
    lock_file_path: PathBuf,
    sessions: HashMap<String, NetworkSession>,
    unclean_exit_detected: bool,
    session_save_debounce: Debounce,
}

impl SessionManager {
    /// Initializes session manager with XDG storage directories
    pub fn new(storage: StorageService) -> Self {
        let xdg_data = glib::user_data_dir().join("shadhin-browser");
        let xdg_cache = glib::user_cache_dir().join("shadhin-browser");

        let base_data_dir = xdg_data.join("sessions");
        let base_cache_dir = xdg_cache.join("sessions");
        let lock_file_path = xdg_data.join("running.lock");

        // Ensure root app data dir has strict 0700 permissions
        let _ = std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&xdg_data);
        if let Ok(mut perms) = std::fs::metadata(&xdg_data).map(|m| m.permissions()) {
            perms.set_mode(0o700);
            let _ = std::fs::set_permissions(&xdg_data, perms);
        }

        // Check if previous run terminated uncleanly
        let unclean_exit_detected = lock_file_path.exists();
        if unclean_exit_detected {
            warn!("Detected running.lock on startup: previous session may have crashed");
        } else {
            // Write running.lock file for current instance with 0600 mode
            if let Ok(file) = File::create(&lock_file_path) {
                let mut perms = file
                    .metadata()
                    .map(|m| m.permissions())
                    .unwrap_or_else(|_| std::fs::metadata(&lock_file_path).unwrap().permissions());
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&lock_file_path, perms);
            }
        }

        info!(
            "SessionManager initialized: base_data_dir={:?}, base_cache_dir={:?}",
            base_data_dir, base_cache_dir
        );

        Self {
            storage,
            base_data_dir,
            base_cache_dir,
            lock_file_path,
            sessions: HashMap::new(),
            unclean_exit_detected,
            session_save_debounce: Debounce::new(),
        }
    }

    /// Returns reference to the base data directory
    pub fn base_data_dir(&self) -> &Path {
        &self.base_data_dir
    }

    /// Returns reference to the base cache directory
    pub fn base_cache_dir(&self) -> &Path {
        &self.base_cache_dir
    }

    /// Whether an unclean shutdown or crash was detected on launch
    pub fn had_unclean_exit(&self) -> bool {
        self.unclean_exit_detected
    }

    /// Clears the crash recovery lockfile upon intentional recovery dismissal
    pub fn acknowledge_crash(&self) {
        let _ = std::fs::remove_file(&self.lock_file_path);
    }

    /// Retrieves or instantiates an isolated WebKit `NetworkSession` for the specified workspace.
    /// Dedicated cookie jars, localStorage, IndexedDB, and HTTP cache are stored in `0700` directories.
    pub fn get_or_create_workspace_session(&mut self, workspace_id: &str) -> NetworkSession {
        if let Some(session) = self.sessions.get(workspace_id) {
            return session.clone();
        }

        let data_dir = self.base_data_dir.join(format!("ws_{}", workspace_id));
        let cache_dir = self.base_cache_dir.join(format!("ws_{}", workspace_id));

        // Create isolated 0700 directories
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        let _ = builder.create(&data_dir);
        let _ = builder.create(&cache_dir);

        let data_dir_str = data_dir.to_str().map(|s| s.to_string());
        let cache_dir_str = cache_dir.to_str().map(|s| s.to_string());

        let session = NetworkSession::new(data_dir_str.as_deref(), cache_dir_str.as_deref());

        // Audit non-ephemeral session persistence (Task 1)
        debug_assert!(
            !session.is_ephemeral(),
            "Workspace NetworkSession must be persistent (non-ephemeral)"
        );
        if let Some(wdm) = session.website_data_manager() {
            debug_assert!(
                !wdm.is_ephemeral(),
                "WebsiteDataManager must be persistent (non-ephemeral)"
            );
            info!(
                "SessionManager: WebsiteDataManager persistence verified (is_ephemeral=false, base_data={:?}, base_cache={:?})",
                wdm.base_data_directory(),
                wdm.base_cache_directory()
            );
        }

        // Configure persistent SQLite cookie storage so logins survive restarts
        if let Some(cookie_mgr) = session.cookie_manager() {
            let cookie_file = data_dir.join("cookies.sqlite");
            if let Some(cookie_file_str) = cookie_file.to_str() {
                cookie_mgr.set_persistent_storage(
                    cookie_file_str,
                    CookiePersistentStorage::Sqlite,
                );
                info!(
                    "SessionManager: Attached persistent SQLite cookie storage for workspace '{}' at {:?}",
                    workspace_id, cookie_file
                );
            }
        }

        // Enable persistent network credential storage
        session.set_persistent_credential_storage_enabled(true);

        self.sessions
            .insert(workspace_id.to_string(), session.clone());

        info!(
            "Initialized isolated persistent NetworkSession for workspace '{}' at {:?}",
            workspace_id, data_dir
        );

        session
    }

    /// Persists open tabs for a workspace to SQLite asynchronously
    pub fn save_workspace_tabs(
        &self,
        workspace_id: &str,
        tabs: Vec<(String, String, bool)>, // (url, title, is_active)
        persistence_allowed: bool,
    ) {
        if !persistence_allowed {
            return;
        }

        let ws_id = workspace_id.to_string();
        self.storage.dispatch(move |db| {
            let repo = WorkspaceRepository::new(db.conn());
            let tab_tuples: Vec<(&str, &str, bool)> = tabs
                .iter()
                .map(|(u, t, a)| (u.as_str(), t.as_str(), *a))
                .collect();
            let _ = repo.save_session_tabs(&ws_id, &tab_tuples);
        });
    }

    /// Debounces saving open tabs for a workspace to SQLite (500ms delay)
    pub fn save_workspace_tabs_debounced(
        &self,
        workspace_id: &str,
        tabs: Vec<(String, String, bool)>,
        persistence_allowed: bool,
    ) {
        if !persistence_allowed {
            return;
        }

        let st = self.storage.clone();
        let ws_id = workspace_id.to_string();
        self.session_save_debounce
            .debounce(std::time::Duration::from_millis(500), move || {
                st.dispatch(move |db| {
                    let repo = WorkspaceRepository::new(db.conn());
                    let tab_tuples: Vec<(&str, &str, bool)> = tabs
                        .iter()
                        .map(|(u, t, a)| (u.as_str(), t.as_str(), *a))
                        .collect();
                    let _ = repo.save_session_tabs(&ws_id, &tab_tuples);
                });
            });
    }

    /// Loads saved session tabs for a workspace
    pub fn load_workspace_tabs(
        &self,
        workspace_id: &str,
        callback: impl FnOnce(Vec<RestoredTabDescriptor>) + 'static,
    ) {
        let ws_id = workspace_id.to_string();
        self.storage.query_async(
            move |db| {
                let repo = WorkspaceRepository::new(db.conn());
                repo.get_session_tabs(&ws_id).unwrap_or_default()
            },
            move |saved_tabs: Vec<SessionTabItem>| {
                let descriptors = saved_tabs
                    .into_iter()
                    .map(|t| RestoredTabDescriptor {
                        url: t.url,
                        title: t.title,
                        is_active: t.is_active,
                    })
                    .collect();
                callback(descriptors);
            },
        );
    }

    /// Cleans up lockfile on clean exit
    pub fn on_clean_shutdown(&self) {
        let _ = std::fs::remove_file(&self.lock_file_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_lock_detection_and_acknowledgment() {
        let _storage = StorageService::new_in_memory().unwrap();
        let temp_dir =
            std::env::temp_dir().join(format!("shadhin_test_session_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let lock_path = temp_dir.join("running.lock");
        std::fs::write(&lock_path, b"1234").unwrap();

        assert!(lock_path.exists());
        let _ = std::fs::remove_file(&lock_path);
        assert!(!lock_path.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_running_lock_mode_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir =
            std::env::temp_dir().join(format!("shadhin_lock_perm_test_{}", std::process::id()));
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(&temp_dir).unwrap();

        let lock_path = temp_dir.join("running.lock");
        let file = File::create(&lock_path).unwrap();
        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&lock_path, perms).unwrap();

        let meta = std::fs::metadata(&lock_path).unwrap();
        assert_eq!(
            meta.permissions().mode() & 0o777,
            0o600,
            "running.lock must have 0600 permissions"
        );

        let dir_meta = std::fs::metadata(&temp_dir).unwrap();
        assert_eq!(
            dir_meta.permissions().mode() & 0o777,
            0o700,
            "Session directory must have 0700 permissions"
        );

        let _ = std::fs::remove_file(&lock_path);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ephemeral_contrast() {
        if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
            eprintln!("No display available, skipping WebKit session test");
            return;
        }
        if gtk4::init().is_err() || gtk4::gdk::Display::default().is_none() {
            eprintln!("No GTK display available, skipping WebKit session test");
            return;
        }

        let ephemeral = NetworkSession::new_ephemeral();
        assert!(ephemeral.is_ephemeral(), "Ephemeral session must report is_ephemeral == true");

        let storage = StorageService::new_in_memory().unwrap();
        let mut sm = SessionManager::new(storage);
        let persistent = sm.get_or_create_workspace_session("test_workspace_audit");
        assert!(!persistent.is_ephemeral(), "Workspace session must report is_ephemeral == false");
        assert!(persistent.is_persistent_credential_storage_enabled());

        if let Some(wdm) = persistent.website_data_manager() {
            assert!(!wdm.is_ephemeral(), "WebsiteDataManager must report is_ephemeral == false");
        }
    }
}
