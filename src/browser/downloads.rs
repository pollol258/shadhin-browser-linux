use crate::storage::StorageService;
use crate::storage::downloads::{DownloadRecord, DownloadRepository};
use gtk4::glib;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};
use webkit6::prelude::*;

const MAX_FILENAME_BYTES: usize = 240;

/// List of executable or dangerous file extensions requiring caution badges
pub const DANGEROUS_EXTENSIONS: &[&str] = &[
    "sh", "bash", "desktop", "appimage", "exe", "bat", "cmd", "run", "bin", "msi", "apk", "deb",
    "rpm", "jar", "vbs", "ps1",
];

/// Checks whether a filename ends with a potentially hazardous executable extension
pub fn is_dangerous_extension(filename: &str) -> bool {
    let lower = filename.trim().to_lowercase();
    if let Some(pos) = lower.rfind('.') {
        let ext = &lower[pos + 1..];
        DANGEROUS_EXTENSIONS.contains(&ext)
    } else {
        false
    }
}

/// Sanitizes a suggested filename to prevent directory traversal, control character injection,
/// leading dots, and filesystem overflow, while preserving Unicode characters and file extensions.
pub fn sanitize_download_filename(raw_name: &str) -> String {
    let raw = raw_name.trim();
    if raw.is_empty() {
        return "download".to_string();
    }

    // Normalize Windows backslashes so path traversal is consistently neutralized across platforms
    let normalized = raw.replace('\\', "/");

    // Extract basename in case a full path was passed
    let base = Path::new(&normalized)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| normalized.to_string());

    // Filter control characters, slashes, backslashes, colons, NUL bytes, and filesystem-illegal characters
    let mut clean: String = base
        .chars()
        .filter(|c| {
            !c.is_control()
                && *c != '/'
                && *c != '\\'
                && *c != ':'
                && *c != '\0'
                && *c != '*'
                && *c != '?'
                && *c != '<'
                && *c != '>'
                && *c != '|'
                && *c != '"'
        })
        .collect();

    // Strip leading dots to prevent creating hidden files
    clean = clean.trim_start_matches('.').trim().to_string();

    if clean.is_empty() {
        return "download".to_string();
    }

    // Check byte length and truncate if necessary, preserving extension
    if clean.len() > MAX_FILENAME_BYTES {
        let path = Path::new(&clean);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let ext_with_dot = if ext.is_empty() {
            String::new()
        } else {
            format!(".{}", ext)
        };

        let target_stem_len = MAX_FILENAME_BYTES.saturating_sub(ext_with_dot.len());
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(&clean);

        // Safely truncate UTF-8 string
        let mut truncated_stem = String::new();
        for ch in stem.chars() {
            if truncated_stem.len() + ch.len_utf8() > target_stem_len {
                break;
            }
            truncated_stem.push(ch);
        }

        clean = format!("{}{}", truncated_stem, ext_with_dot);
    }

    if clean.is_empty() {
        "download".to_string()
    } else {
        clean
    }
}

/// Finds an available filename by appending " (1)", " (2)" before the extension
/// if the destination already exists in the target directory.
pub fn resolve_collision_path(target_dir: &Path, file_name: &str) -> PathBuf {
    let candidate = target_dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    for i in 1..10_000 {
        let new_name = if ext.is_empty() {
            format!("{} ({})", stem, i)
        } else {
            format!("{} ({}).{}", stem, i, ext)
        };
        let p = target_dir.join(new_name);
        if !p.exists() {
            return p;
        }
    }

    // Ultimate fallback with timestamp
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    target_dir.join(format!("{}_{}", ts, file_name))
}

/// Sets strict non-executable file mode (0600 = rw-------)
pub fn set_safe_file_permissions(path: &Path) {
    if let Ok(file) = File::open(path)
        && let Ok(mut perms) = file.metadata().map(|m| m.permissions())
    {
        perms.set_mode(0o600);
        let _ = std::fs::set_permissions(path, perms);
    }
}

/// Download lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl DownloadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Downloading => "downloading",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn from_state_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "downloading" => Self::Downloading,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Interrupted,
        }
    }
}

impl std::str::FromStr for DownloadStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_state_str(s))
    }
}

/// Comprehensive representation of an active or historical download item
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadItem {
    pub id: i64,
    pub url: String,
    pub file_path: PathBuf,
    pub file_name: String,
    pub mime_type: Option<String>,
    pub total_bytes: i64,
    pub received_bytes: i64,
    pub status: DownloadStatus,
    pub progress: f64, // 0.0 to 1.0
    pub speed_bytes_per_sec: f64,
    pub estimated_remaining_secs: Option<u64>,
    pub error: Option<String>,
    pub started_at: u64,
    pub is_dangerous: bool,
}

impl From<&DownloadRecord> for DownloadItem {
    fn from(rec: &DownloadRecord) -> Self {
        let progress = if rec.total_bytes > 0 {
            (rec.received_bytes as f64 / rec.total_bytes as f64).clamp(0.0, 1.0)
        } else if rec.state == "completed" {
            1.0
        } else {
            0.0
        };

        Self {
            id: rec.id,
            url: rec.url.clone(),
            file_path: PathBuf::from(&rec.file_path),
            file_name: rec.file_name.clone(),
            mime_type: rec.mime.clone(),
            total_bytes: rec.total_bytes,
            received_bytes: rec.received_bytes,
            status: DownloadStatus::from_state_str(&rec.state),
            progress,
            speed_bytes_per_sec: 0.0,
            estimated_remaining_secs: None,
            error: rec.error.clone(),
            started_at: rec.started_at as u64,
            is_dangerous: is_dangerous_extension(&rec.file_name),
        }
    }
}

struct ActiveDownloadEntry {
    item: DownloadItem,
    webkit_download: Option<webkit6::Download>,
    last_ui_update: Instant,
    last_db_update: Instant,
    last_speed_sample_time: Instant,
    last_speed_sample_bytes: i64,
    cancelled_by_user: bool,
    persistence_allowed: bool,
}

struct DownloadManagerInner {
    storage: StorageService,
    download_dir: PathBuf,
    active_downloads: HashMap<i64, ActiveDownloadEntry>,
    next_id: AtomicI64,
    listeners: Vec<Rc<dyn Fn(&DownloadItem)>>,
    start_listeners: Vec<Rc<dyn Fn(&DownloadItem)>>,
    completion_listeners: Vec<Rc<dyn Fn(&DownloadItem)>>,
    attached_sessions: HashSet<usize>,
}

/// Browser Download Manager coordinator
#[derive(Clone)]
pub struct DownloadManager {
    inner: Rc<RefCell<DownloadManagerInner>>,
}

impl DownloadManager {
    pub fn new(storage: StorageService, download_dir: PathBuf) -> Self {
        if !download_dir.exists() {
            let _ = std::fs::create_dir_all(&download_dir);
        }

        Self {
            inner: Rc::new(RefCell::new(DownloadManagerInner {
                storage,
                download_dir,
                active_downloads: HashMap::new(),
                next_id: AtomicI64::new(1),
                listeners: Vec::new(),
                start_listeners: Vec::new(),
                completion_listeners: Vec::new(),
                attached_sessions: HashSet::new(),
            })),
        }
    }

    /// Attaches to WebKit network session to handle download requests securely
    pub fn attach_session(&self, session: &webkit6::NetworkSession, persistence_allowed: bool) {
        let session_key = session.as_ptr() as usize;
        if self.inner.borrow().attached_sessions.contains(&session_key) {
            return;
        }

        let mgr = self.clone();
        session.connect_download_started(move |_session, download| {
            mgr.handle_download(download, persistence_allowed);
        });

        self.inner
            .borrow_mut()
            .attached_sessions
            .insert(session_key);
        info!("DownloadManager attached to NetworkSession ({session_key:#x})");
    }

    /// Handles a new download initiated by WebKitGTK
    pub fn handle_download(&self, download: &webkit6::Download, persistence_allowed: bool) {
        let raw_id = self.inner.borrow().next_id.fetch_add(1, Ordering::SeqCst);
        let url = download
            .request()
            .and_then(|r| r.uri())
            .map(|u| u.to_string())
            .unwrap_or_default();

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let initial_item = DownloadItem {
            id: raw_id,
            url: url.clone(),
            file_path: self.inner.borrow().download_dir.clone(),
            file_name: "downloading...".to_string(),
            mime_type: None,
            total_bytes: 0,
            received_bytes: 0,
            status: DownloadStatus::Pending,
            progress: 0.0,
            speed_bytes_per_sec: 0.0,
            estimated_remaining_secs: None,
            error: None,
            started_at: now_secs,
            is_dangerous: false,
        };

        let entry = ActiveDownloadEntry {
            item: initial_item.clone(),
            webkit_download: Some(download.clone()),
            last_ui_update: Instant::now(),
            last_db_update: Instant::now(),
            last_speed_sample_time: Instant::now(),
            last_speed_sample_bytes: 0,
            cancelled_by_user: false,
            persistence_allowed,
        };

        self.inner
            .borrow_mut()
            .active_downloads
            .insert(raw_id, entry);
        self.notify_listeners(&initial_item);

        // 1. decide-destination
        let mgr_dest = self.clone();
        let dl_dir = self.inner.borrow().download_dir.clone();
        download.connect_decide_destination(move |dl, suggested_filename| {
            if !dl_dir.exists() {
                let _ = std::fs::create_dir_all(&dl_dir);
            }

            let clean_name = sanitize_download_filename(suggested_filename);
            let dest_path = resolve_collision_path(&dl_dir, &clean_name);
            let final_name = dest_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| clean_name.clone());
            let dest_str = dest_path.to_string_lossy().to_string();

            info!(
                "Download {raw_id}: suggested='{suggested_filename}', sanitized='{clean_name}', destination='{dest_str}'"
            );
            // CRITICAL: WebKitGTK 6.0 asserts g_path_is_absolute(destination). Must pass absolute filesystem path, NOT file:// URI!
            dl.set_destination(&dest_str);

            let mime = dl
                .response()
                .and_then(|r| r.mime_type())
                .map(|m| m.to_string());
            let total = dl
                .response()
                .map(|r| r.content_length())
                .unwrap_or(0) as i64;

            mgr_dest.update_on_destination(
                raw_id,
                &dest_path,
                &final_name,
                mime.as_deref(),
                total,
            );
            true
        });

        // 2. received-data
        let mgr_data = self.clone();
        download.connect_received_data(move |dl, _data_len| {
            mgr_data.update_on_progress(raw_id, dl);
        });

        // 3. finished
        let mgr_finish = self.clone();
        download.connect_finished(move |dl| {
            mgr_finish.update_on_finished(raw_id, dl);
        });

        // 4. failed
        let mgr_fail = self.clone();
        download.connect_failed(move |_dl, err| {
            mgr_fail.update_on_failed(raw_id, err);
        });
    }

    fn update_on_destination(
        &self,
        id: i64,
        dest_path: &Path,
        file_name: &str,
        mime: Option<&str>,
        total_bytes: i64,
    ) {
        let mut updated_item = None;
        let storage = self.inner.borrow().storage.clone();
        let persistence_allowed = {
            let mut inner = self.inner.borrow_mut();
            if let Some(entry) = inner.active_downloads.get_mut(&id) {
                entry.item.file_path = dest_path.to_path_buf();
                entry.item.file_name = file_name.to_string();
                entry.item.mime_type = mime.map(|m| m.to_string());
                entry.item.total_bytes = total_bytes;
                entry.item.status = DownloadStatus::Downloading;
                entry.item.is_dangerous = is_dangerous_extension(file_name);
                updated_item = Some(entry.item.clone());
                entry.persistence_allowed
            } else {
                false
            }
        };

        if let Some(ref item) = updated_item {
            self.notify_listeners(item);
            self.notify_start(item);

            if persistence_allowed {
                let url = item.url.clone();
                let path_str = dest_path.to_string_lossy().to_string();
                let name_str = file_name.to_string();
                let mime_str = mime.map(|m| m.to_string());
                let mgr_clone = self.clone();

                storage.query_async(
                    move |db| {
                        let repo = DownloadRepository::new(db.conn());
                        repo.insert(
                            &url,
                            &path_str,
                            &name_str,
                            mime_str.as_deref(),
                            total_bytes,
                            true,
                        )
                    },
                    move |res| {
                        if let Ok(db_id) = res {
                            mgr_clone.set_db_id(id, db_id);
                        }
                    },
                );
            }
        }
    }

    fn set_db_id(&self, synthetic_id: i64, db_id: i64) {
        let mut inner = self.inner.borrow_mut();
        if let Some(mut entry) = inner.active_downloads.remove(&synthetic_id) {
            entry.item.id = db_id;
            inner.active_downloads.insert(db_id, entry);
        }
    }

    fn update_on_progress(&self, id: i64, dl: &webkit6::Download) {
        let received = dl.received_data_length() as i64;
        let progress = dl.estimated_progress();
        let elapsed = dl.elapsed_time();

        let now = Instant::now();
        let mut item_to_notify = None;

        {
            let mut inner = self.inner.borrow_mut();
            let key = inner
                .active_downloads
                .iter()
                .find(|(k, v)| **k == id || v.item.id == id)
                .map(|(k, _)| *k);

            if let Some(k) = key
                && let Some(entry) = inner.active_downloads.get_mut(&k)
            {
                if entry.item.status != DownloadStatus::Downloading {
                    entry.item.status = DownloadStatus::Downloading;
                }

                if entry.item.total_bytes <= 0
                    && let Some(resp) = dl.response()
                {
                    let len = resp.content_length() as i64;
                    if len > 0 {
                        entry.item.total_bytes = len;
                    }
                }

                entry.item.received_bytes = received;
                entry.item.progress = progress;

                let delta_t = now.duration_since(entry.last_speed_sample_time).as_secs_f64();
                if delta_t >= 0.25 {
                    let delta_bytes = (received - entry.last_speed_sample_bytes).max(0);
                    let instant_speed = delta_bytes as f64 / delta_t;
                    let smoothed_speed = if entry.item.speed_bytes_per_sec > 0.0 {
                        0.7 * entry.item.speed_bytes_per_sec + 0.3 * instant_speed
                    } else {
                        instant_speed
                    };
                    entry.item.speed_bytes_per_sec = smoothed_speed;
                    entry.last_speed_sample_time = now;
                    entry.last_speed_sample_bytes = received;
                } else if elapsed > 0.1 && entry.item.speed_bytes_per_sec <= 0.0 {
                    entry.item.speed_bytes_per_sec = received as f64 / elapsed;
                }

                if entry.item.total_bytes > received && entry.item.speed_bytes_per_sec > 1.0 {
                    let remaining = (entry.item.total_bytes - received) as f64
                        / entry.item.speed_bytes_per_sec;
                    entry.item.estimated_remaining_secs = Some(remaining.round() as u64);
                } else if entry.item.total_bytes > 0 && entry.item.total_bytes <= received {
                    entry.item.estimated_remaining_secs = Some(0);
                } else {
                    entry.item.estimated_remaining_secs = None;
                }

                if now.duration_since(entry.last_ui_update) >= Duration::from_millis(100) {
                    entry.last_ui_update = now;
                    item_to_notify = Some(entry.item.clone());
                }

                if entry.persistence_allowed
                    && now.duration_since(entry.last_db_update) >= Duration::from_secs(2)
                {
                    entry.last_db_update = now;
                    let db_id = entry.item.id;
                    let tot = entry.item.total_bytes;
                    let storage = inner.storage.clone();
                    storage.dispatch(move |db| {
                        let repo = DownloadRepository::new(db.conn());
                        let _ = repo.update_progress(db_id, received, tot, true);
                    });
                }
            }
        }

        if let Some(ref item) = item_to_notify {
            self.notify_listeners(item);
        }
    }

    fn update_on_finished(&self, id: i64, dl: &webkit6::Download) {
        let mut finished_item = None;
        let storage = self.inner.borrow().storage.clone();

        {
            let mut inner = self.inner.borrow_mut();
            let key = inner
                .active_downloads
                .iter()
                .find(|(k, v)| **k == id || v.item.id == id)
                .map(|(k, _)| *k);

            if let Some(k) = key
                && let Some(entry) = inner.active_downloads.get_mut(&k)
            {
                set_safe_file_permissions(&entry.item.file_path);

                entry.item.status = DownloadStatus::Completed;
                entry.item.progress = 1.0;
                let received = dl.received_data_length() as i64;
                if received > 0 {
                    entry.item.received_bytes = received;
                }
                if entry.item.total_bytes <= 0 {
                    entry.item.total_bytes = entry.item.received_bytes;
                }
                entry.item.speed_bytes_per_sec = 0.0;
                entry.item.estimated_remaining_secs = None;

                finished_item = Some(entry.item.clone());

                if entry.persistence_allowed {
                    let db_id = entry.item.id;
                    let tot = entry.item.total_bytes;
                    let rec = entry.item.received_bytes;
                    storage.dispatch(move |db| {
                        let repo = DownloadRepository::new(db.conn());
                        let _ = repo.update_progress(db_id, rec, tot, true);
                        let _ = repo.update_state(db_id, "completed", true, None, true);
                    });
                }
            }
        }

        if let Some(ref item) = finished_item {
            info!(
                "Download completed: '{}' ({})",
                item.file_name,
                item.file_path.display()
            );
            self.notify_listeners(item);
            self.notify_completion(item);
        }
    }

    fn update_on_failed(&self, id: i64, error: &glib::Error) {
        let mut failed_item = None;
        let storage = self.inner.borrow().storage.clone();

        {
            let mut inner = self.inner.borrow_mut();
            let key = inner
                .active_downloads
                .iter()
                .find(|(k, v)| **k == id || v.item.id == id)
                .map(|(k, _)| *k);

            if let Some(k) = key
                && let Some(entry) = inner.active_downloads.get_mut(&k)
            {
                let is_cancelled = entry.cancelled_by_user
                    || error.kind::<webkit6::NetworkError>()
                        == Some(webkit6::NetworkError::Cancelled);

                if is_cancelled {
                    entry.item.status = DownloadStatus::Cancelled;
                    entry.item.error = Some("Download cancelled".to_string());
                } else {
                    entry.item.status = DownloadStatus::Failed;
                    entry.item.error = Some(error.message().to_string());
                }

                entry.item.speed_bytes_per_sec = 0.0;
                entry.item.estimated_remaining_secs = None;

                failed_item = Some(entry.item.clone());

                if entry.persistence_allowed {
                    let db_id = entry.item.id;
                    let status_str = entry.item.status.as_str().to_string();
                    let err_msg = entry.item.error.clone();
                    storage.dispatch(move |db| {
                        let repo = DownloadRepository::new(db.conn());
                        let _ =
                            repo.update_state(db_id, &status_str, true, err_msg.as_deref(), true);
                    });
                }
            }
        }

        if let Some(ref item) = failed_item {
            warn!(
                "Download failed/cancelled: '{}' ({:?})",
                item.file_name, item.error
            );
            self.notify_listeners(item);
        }
    }

    /// Cancels an in-flight download
    pub fn cancel_download(&self, id: i64) {
        let mut dl_to_cancel = None;
        let mut item_to_notify = None;
        let storage = self.inner.borrow().storage.clone();

        {
            let mut inner = self.inner.borrow_mut();
            let key = inner
                .active_downloads
                .iter()
                .find(|(k, v)| **k == id || v.item.id == id)
                .map(|(k, _)| *k);

            if let Some(k) = key
                && let Some(entry) = inner.active_downloads.get_mut(&k)
            {
                entry.cancelled_by_user = true;
                entry.item.status = DownloadStatus::Cancelled;
                entry.item.error = Some("Download cancelled by user".to_string());
                dl_to_cancel = entry.webkit_download.clone();
                item_to_notify = Some(entry.item.clone());

                if entry.persistence_allowed {
                    let db_id = entry.item.id;
                    storage.dispatch(move |db| {
                        let repo = DownloadRepository::new(db.conn());
                        let _ = repo.update_state(
                            db_id,
                            "cancelled",
                            true,
                            Some("Cancelled by user"),
                            true,
                        );
                    });
                }
            }
        }

        if let Some(dl) = dl_to_cancel {
            dl.cancel();
        }

        if let Some(ref item) = item_to_notify {
            self.notify_listeners(item);
        }
    }

    /// Retries a previously failed or cancelled download
    pub fn retry_download(
        &self,
        id: i64,
        session: &webkit6::NetworkSession,
        persistence_allowed: bool,
    ) {
        let url_opt = {
            let inner = self.inner.borrow();
            let key = inner
                .active_downloads
                .iter()
                .find(|(k, v)| **k == id || v.item.id == id)
                .map(|(k, _)| *k);
            key.and_then(|k| inner.active_downloads.get(&k))
                .map(|e| e.item.url.clone())
        };

        if let Some(url) = url_opt {
            self.download_uri(session, &url, persistence_allowed);
        } else {
            let storage = self.inner.borrow().storage.clone();
            let mgr = self.clone();
            let session_clone = session.clone();
            storage.query_async(
                move |db| {
                    db.conn()
                        .query_row(
                            "SELECT url FROM downloads WHERE id = ?1;",
                            rusqlite::params![id],
                            |row| row.get::<_, String>(0),
                        )
                        .ok()
                },
                move |res| {
                    if let Some(url) = res {
                        mgr.download_uri(&session_clone, &url, persistence_allowed);
                    }
                },
            );
        }
    }

    /// Programmatically initiates a download for a given URI
    pub fn download_uri(
        &self,
        session: &webkit6::NetworkSession,
        uri: &str,
        persistence_allowed: bool,
    ) -> Option<webkit6::Download> {
        info!("Programmatic download requested for: {uri}");
        self.attach_session(session, persistence_allowed);
        session.download_uri(uri)
    }

    /// Registers a listener for real-time download status updates
    pub fn add_listener<F: Fn(&DownloadItem) + 'static>(&self, listener: F) {
        self.inner.borrow_mut().listeners.push(Rc::new(listener));
    }

    /// Registers a listener for download start notifications
    pub fn add_start_listener<F: Fn(&DownloadItem) + 'static>(&self, listener: F) {
        self.inner
            .borrow_mut()
            .start_listeners
            .push(Rc::new(listener));
    }

    /// Registers a listener for completed download notifications
    pub fn add_completion_listener<F: Fn(&DownloadItem) + 'static>(&self, listener: F) {
        self.inner
            .borrow_mut()
            .completion_listeners
            .push(Rc::new(listener));
    }

    /// Returns a list of currently active / in-flight downloads
    pub fn get_active_downloads(&self) -> Vec<DownloadItem> {
        self.inner
            .borrow()
            .active_downloads
            .values()
            .map(|e| e.item.clone())
            .collect()
    }

    /// Returns the target download directory
    pub fn get_download_dir(&self) -> PathBuf {
        self.inner.borrow().download_dir.clone()
    }

    fn notify_listeners(&self, item: &DownloadItem) {
        let listeners: Vec<Rc<dyn Fn(&DownloadItem)>> = self.inner.borrow().listeners.clone();
        for listener in listeners {
            listener(item);
        }
    }

    fn notify_start(&self, item: &DownloadItem) {
        let listeners: Vec<Rc<dyn Fn(&DownloadItem)>> =
            self.inner.borrow().start_listeners.clone();
        for listener in listeners {
            listener(item);
        }
    }

    fn notify_completion(&self, item: &DownloadItem) {
        let listeners: Vec<Rc<dyn Fn(&DownloadItem)>> =
            self.inner.borrow().completion_listeners.clone();
        for listener in listeners {
            listener(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_download_filename_hostile_inputs() {
        // Path traversal attempts
        assert_eq!(sanitize_download_filename("../../etc/passwd"), "passwd");
        assert_eq!(
            sanitize_download_filename("..\\..\\windows\\system32\\calc.exe"),
            "calc.exe"
        );
        assert_eq!(sanitize_download_filename("/var/log/syslog"), "syslog");

        // Leading dots (prevent hidden file creation)
        assert_eq!(sanitize_download_filename(".bashrc"), "bashrc");
        assert_eq!(sanitize_download_filename("...hidden.txt"), "hidden.txt");

        // Control characters and forbidden separators
        assert_eq!(
            sanitize_download_filename("test\0bad\x07file.pdf"),
            "testbadfile.pdf"
        );
        assert_eq!(
            sanitize_download_filename("my:report:2026.docx"),
            "myreport2026.docx"
        );

        // Empty fallbacks
        assert_eq!(sanitize_download_filename(""), "download");
        assert_eq!(sanitize_download_filename("   "), "download");
        assert_eq!(sanitize_download_filename("..."), "download");

        // Unicode / Bangla filenames
        assert_eq!(
            sanitize_download_filename("নথি_পত্র_২০২৬.pdf"),
            "নথি_পত্র_২০২৬.pdf"
        );

        // Extremely long filename truncation preserving extension
        let long_stem = "a".repeat(300);
        let long_file = format!("{}.tar.gz", long_stem);
        let sanitized = sanitize_download_filename(&long_file);
        assert!(sanitized.len() <= MAX_FILENAME_BYTES);
        assert!(sanitized.ends_with(".gz"));
    }

    #[test]
    fn test_collision_evasion() {
        let temp_dir = std::env::temp_dir().join("shadhin_test_collisions");
        let _ = std::fs::create_dir_all(&temp_dir);

        let f1 = temp_dir.join("document.pdf");
        std::fs::write(&f1, b"first").unwrap();

        let resolved = resolve_collision_path(&temp_dir, "document.pdf");
        assert_eq!(
            resolved.file_name().unwrap().to_str().unwrap(),
            "document (1).pdf"
        );

        std::fs::write(&resolved, b"second").unwrap();
        let resolved2 = resolve_collision_path(&temp_dir, "document.pdf");
        assert_eq!(
            resolved2.file_name().unwrap().to_str().unwrap(),
            "document (2).pdf"
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_dangerous_extensions_detection() {
        assert!(is_dangerous_extension("install.sh"));
        assert!(is_dangerous_extension("launcher.desktop"));
        assert!(is_dangerous_extension("app.AppImage"));
        assert!(is_dangerous_extension("setup.exe"));
        assert!(is_dangerous_extension("script.bat"));
        assert!(is_dangerous_extension("payload.bin"));

        // Safe files
        assert!(!is_dangerous_extension("photo.png"));
        assert!(!is_dangerous_extension("document.pdf"));
        assert!(!is_dangerous_extension("notes.txt"));
        assert!(!is_dangerous_extension("archive.tar.gz"));
    }
}
