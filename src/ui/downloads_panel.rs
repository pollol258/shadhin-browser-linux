#![allow(deprecated)]

use crate::browser::downloads::{DownloadItem, DownloadManager, DownloadStatus};
use crate::storage::StorageService;
use crate::storage::downloads::{DownloadRecord, DownloadRepository};
use gtk4::gio;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

fn format_bytes(bytes: i64) -> String {
    if bytes < 0 {
        return "Unknown size".to_string();
    }
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn format_speed(bps: f64) -> String {
    if bps <= 0.0 {
        return String::new();
    }
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    if bps >= MB {
        format!("{:.1} MB/s", bps / MB)
    } else if bps >= KB {
        format!("{:.0} KB/s", bps / KB)
    } else {
        format!("{:.0} B/s", bps)
    }
}

fn format_eta(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h {}m left", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m {}s left", secs / 60, secs % 60)
    } else {
        format!("{}s left", secs)
    }
}

struct DownloadRowWidgets {
    card: gtk4::Box,
    name_lbl: gtk4::Label,
    status_lbl: gtk4::Label,
    details_lbl: gtk4::Label,
    progress_bar: gtk4::ProgressBar,
    cancel_btn: gtk4::Button,
    retry_btn: gtk4::Button,
    open_btn: gtk4::Button,
    folder_btn: gtk4::Button,
    item_data: Rc<RefCell<DownloadItem>>,
}

#[deprecated(note = "DownloadsPopover is the single source of truth for downloads")]
pub struct DownloadsPanel {
    container: gtk4::Box,
    storage: StorageService,
    download_mgr: DownloadManager,
    session: Rc<RefCell<webkit6::NetworkSession>>,
    persistence_allowed: bool,
    list_container: gtk4::Box,
    search_entry: gtk4::SearchEntry,
    active_rows: Rc<RefCell<HashMap<i64, DownloadRowWidgets>>>,
    empty_label: gtk4::Label,
}

impl DownloadsPanel {
    pub fn new(
        storage: StorageService,
        download_mgr: DownloadManager,
        session: Rc<RefCell<webkit6::NetworkSession>>,
        persistence_allowed: bool,
    ) -> Self {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .margin_start(16)
            .margin_end(16)
            .margin_top(16)
            .margin_bottom(16)
            .hexpand(true)
            .vexpand(true)
            .css_classes(["shadhin-page-container"])
            .build();

        // 1. Header with Title, "Open Downloads Folder", and "Clear Finished"
        let header = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .build();

        let title = gtk4::Label::builder()
            .label("Downloads")
            .css_classes(["title-1"])
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        header.append(&title);

        let dl_dir = download_mgr.get_download_dir();
        let open_folder_btn = gtk4::Button::builder()
            .label("Open Folder")
            .icon_name("folder-symbolic")
            .css_classes(["flat", "shadhin-nav-btn"])
            .tooltip_text("Open downloads directory in system file manager")
            .build();
        open_folder_btn.connect_clicked(move |_| {
            let uri = format!("file://{}", dl_dir.display());
            let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
        });
        header.append(&open_folder_btn);

        let clear_btn = gtk4::Button::builder()
            .label("Clear Finished")
            .css_classes(["flat", "shadhin-nav-btn"])
            .tooltip_text("Clear completed, cancelled and failed downloads from history")
            .build();
        header.append(&clear_btn);
        container.append(&header);

        // 2. Search Filter
        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Search downloaded files...")
            .hexpand(true)
            .build();
        container.append(&search_entry);

        // 3. Scrollable List View
        let scrolled = gtk4::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .build();

        let list_container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .hexpand(true)
            .build();

        let empty_label = gtk4::Label::builder()
            .label("No downloads yet")
            .css_classes(["dim-label"])
            .margin_top(32)
            .visible(false)
            .build();
        list_container.append(&empty_label);

        scrolled.set_child(Some(&list_container));
        container.append(&scrolled);

        let active_rows = Rc::new(RefCell::new(HashMap::new()));

        let panel = Self {
            container,
            storage,
            download_mgr,
            session,
            persistence_allowed,
            list_container,
            search_entry,
            active_rows,
            empty_label,
        };

        panel.wire_clear_button(&clear_btn);
        panel.wire_search_entry();
        panel.wire_download_listener();
        panel.refresh("");

        panel
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    fn wire_clear_button(&self, clear_btn: &gtk4::Button) {
        let storage_clear = self.storage.clone();
        let rows_ref = self.active_rows.clone();
        let empty_lbl = self.empty_label.clone();

        clear_btn.connect_clicked(move |_| {
            storage_clear.dispatch(move |db| {
                let repo = DownloadRepository::new(db.conn());
                let _ = repo.clear_finished();
            });

            let mut rows = rows_ref.borrow_mut();
            rows.retain(|_, row| {
                let item = row.item_data.borrow();
                let is_active = item.status == DownloadStatus::Downloading
                    || item.status == DownloadStatus::Pending;
                if !is_active {
                    row.card.set_visible(false);
                }
                is_active
            });

            if rows.is_empty() {
                empty_lbl.set_visible(true);
            }
        });
    }

    fn wire_search_entry(&self) {
        let search = self.search_entry.clone();
        let rows_ref = self.active_rows.clone();
        let empty_lbl = self.empty_label.clone();

        search.connect_search_changed(move |entry| {
            let q = entry.text().trim().to_lowercase();
            let rows = rows_ref.borrow();
            let mut visible_count = 0;

            for row in rows.values() {
                let item = row.item_data.borrow();
                let matches = q.is_empty()
                    || item.file_name.to_lowercase().contains(&q)
                    || item.url.to_lowercase().contains(&q);
                row.card.set_visible(matches);
                if matches {
                    visible_count += 1;
                }
            }

            if visible_count == 0 {
                empty_lbl.set_visible(true);
            } else {
                empty_lbl.set_visible(false);
            }
        });
    }

    fn wire_download_listener(&self) {
        let rows_ref = self.active_rows.clone();
        let list_box = self.list_container.clone();
        let empty_lbl = self.empty_label.clone();
        let storage = self.storage.clone();
        let mgr = self.download_mgr.clone();
        let session = self.session.clone();
        let persistence_allowed = self.persistence_allowed;

        self.download_mgr.add_listener(move |item| {
            empty_lbl.set_visible(false);
            let mut rows = rows_ref.borrow_mut();
            if let Some(row) = rows.get(&item.id) {
                Self::update_row_widgets(row, item);
            } else {
                let row = Self::build_row(
                    item,
                    &storage,
                    &mgr,
                    &session,
                    persistence_allowed,
                    &rows_ref,
                );
                list_box.prepend(&row.card);
                rows.insert(item.id, row);
            }
        });
    }

    pub fn refresh(&self, query: &str) {
        let mut rows = self.active_rows.borrow_mut();
        for (_, row) in rows.drain() {
            self.list_container.remove(&row.card);
        }

        let q = query.trim().to_lowercase();
        let mut seen_ids = HashSet::new();

        // 1. First add live active downloads
        let active = self.download_mgr.get_active_downloads();
        for item in active {
            seen_ids.insert(item.id);
            if q.is_empty()
                || item.file_name.to_lowercase().contains(&q)
                || item.url.to_lowercase().contains(&q)
            {
                let row = Self::build_row(
                    &item,
                    &self.storage,
                    &self.download_mgr,
                    &self.session,
                    self.persistence_allowed,
                    &self.active_rows,
                );
                self.list_container.append(&row.card);
                rows.insert(item.id, row);
            }
        }
        drop(rows);

        // 2. Query persistent history from database
        let list_box = self.list_container.clone();
        let rows_ref = self.active_rows.clone();
        let empty_lbl = self.empty_label.clone();
        let storage_clone = self.storage.clone();
        let mgr_clone = self.download_mgr.clone();
        let session_clone = self.session.clone();
        let persistence_allowed = self.persistence_allowed;

        self.storage.query_async(
            move |db| {
                let repo = DownloadRepository::new(db.conn());
                repo.get_recent(100).unwrap_or_default()
            },
            move |downloads: Vec<DownloadRecord>| {
                let mut rendered = 0;
                let mut rows = rows_ref.borrow_mut();

                for dl in downloads {
                    if seen_ids.contains(&dl.id) {
                        continue;
                    }

                    if !q.is_empty()
                        && !dl.file_name.to_lowercase().contains(&q)
                        && !dl.url.to_lowercase().contains(&q)
                    {
                        continue;
                    }

                    let item = DownloadItem::from(&dl);
                    let row = Self::build_row(
                        &item,
                        &storage_clone,
                        &mgr_clone,
                        &session_clone,
                        persistence_allowed,
                        &rows_ref,
                    );
                    list_box.append(&row.card);
                    rows.insert(item.id, row);
                    rendered += 1;
                }

                if rows.is_empty() && rendered == 0 {
                    empty_lbl.set_visible(true);
                } else {
                    empty_lbl.set_visible(false);
                }
            },
        );
    }

    fn build_row(
        item: &DownloadItem,
        storage: &StorageService,
        download_mgr: &DownloadManager,
        session: &Rc<RefCell<webkit6::NetworkSession>>,
        persistence_allowed: bool,
        active_rows: &Rc<RefCell<HashMap<i64, DownloadRowWidgets>>>,
    ) -> DownloadRowWidgets {
        let card = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(6)
            .css_classes(["shadhin-download-card"])
            .build();

        let top_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(10)
            .build();

        let icon = gtk4::Image::from_icon_name("folder-download-symbolic");
        top_row.append(&icon);

        let name_lbl = gtk4::Label::builder()
            .label(&item.file_name)
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .css_classes(["bold"])
            .build();
        top_row.append(&name_lbl);

        if item.is_dangerous {
            let badge = gtk4::Label::builder()
                .label("⚠️ Executable")
                .css_classes(["warning", "caption", "shadhin-warning-badge"])
                .tooltip_text("This file type is potentially hazardous. Exercise caution.")
                .build();
            top_row.append(&badge);
        }

        let item_data = Rc::new(RefCell::new(item.clone()));

        // 1. Open Button (completed downloads)
        let open_btn = gtk4::Button::builder()
            .label("Open")
            .icon_name("document-open-symbolic")
            .css_classes(["flat", "shadhin-nav-btn", "suggested-action"])
            .tooltip_text("Open file with system default application")
            .build();
        let item_open = item_data.clone();
        open_btn.connect_clicked(move |_| {
            let p = item_open.borrow().file_path.clone();
            let uri = format!("file://{}", p.display());
            let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
        });
        top_row.append(&open_btn);

        // 2. Show in Folder Button
        let folder_btn = gtk4::Button::builder()
            .label("Show in Folder")
            .icon_name("folder-symbolic")
            .css_classes(["flat", "shadhin-nav-btn"])
            .tooltip_text("Open enclosing folder in file manager")
            .build();
        let item_folder = item_data.clone();
        folder_btn.connect_clicked(move |_| {
            let p = item_folder.borrow().file_path.clone();
            if let Some(parent) = p.parent() {
                let uri = format!("file://{}", parent.display());
                let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
            }
        });
        top_row.append(&folder_btn);

        // 3. Retry Button (failed / cancelled downloads)
        let retry_btn = gtk4::Button::builder()
            .label("Retry")
            .icon_name("view-refresh-symbolic")
            .css_classes(["flat", "shadhin-nav-btn"])
            .tooltip_text("Retry download")
            .build();
        let item_retry = item_data.clone();
        let mgr_retry = download_mgr.clone();
        let sess_retry = session.clone();
        retry_btn.connect_clicked(move |_| {
            let id = item_retry.borrow().id;
            mgr_retry.retry_download(id, &sess_retry.borrow(), persistence_allowed);
        });
        top_row.append(&retry_btn);

        // 4. Cancel Button (active downloads)
        let cancel_btn = gtk4::Button::builder()
            .label("Cancel")
            .icon_name("process-stop-symbolic")
            .css_classes(["flat", "shadhin-nav-btn", "destructive-action"])
            .tooltip_text("Cancel active download")
            .build();
        let item_cancel = item_data.clone();
        let mgr_cancel = download_mgr.clone();
        cancel_btn.connect_clicked(move |_| {
            let id = item_cancel.borrow().id;
            mgr_cancel.cancel_download(id);
        });
        top_row.append(&cancel_btn);

        // 5. Delete from History Button
        let del_btn = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Remove download record from history")
            .build();
        let item_del = item_data.clone();
        let storage_del = storage.clone();
        let card_del = card.clone();
        let active_rows_del = active_rows.clone();
        del_btn.connect_clicked(move |_| {
            let id = item_del.borrow().id;
            storage_del.dispatch(move |db| {
                let repo = DownloadRepository::new(db.conn());
                let _ = repo.delete(id);
            });
            card_del.set_visible(false);
            active_rows_del.borrow_mut().remove(&id);
        });
        top_row.append(&del_btn);

        card.append(&top_row);

        // Middle Row: Status and Details
        let status_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .build();

        let status_lbl = gtk4::Label::builder()
            .label("Starting...")
            .css_classes(["dim-label", "caption", "bold"])
            .halign(gtk4::Align::Start)
            .build();
        status_box.append(&status_lbl);

        let details_lbl = gtk4::Label::builder()
            .label("")
            .css_classes(["dim-label", "caption"])
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        status_box.append(&details_lbl);

        card.append(&status_box);

        // Bottom: Progress Bar
        let progress_bar = gtk4::ProgressBar::builder()
            .hexpand(true)
            .css_classes(["shadhin-progress-bar"])
            .build();
        card.append(&progress_bar);

        let row_widgets = DownloadRowWidgets {
            card,
            name_lbl,
            status_lbl,
            details_lbl,
            progress_bar,
            cancel_btn,
            retry_btn,
            open_btn,
            folder_btn,
            item_data,
        };

        Self::update_row_widgets(&row_widgets, item);

        row_widgets
    }

    fn update_row_widgets(widgets: &DownloadRowWidgets, item: &DownloadItem) {
        *widgets.item_data.borrow_mut() = item.clone();
        widgets.name_lbl.set_label(&item.file_name);

        match item.status {
            DownloadStatus::Pending => {
                widgets.status_lbl.set_label("Pending");
                widgets.details_lbl.set_label(&item.url);
                widgets.cancel_btn.set_visible(true);
                widgets.retry_btn.set_visible(false);
                widgets.open_btn.set_visible(false);
                widgets.folder_btn.set_visible(false);
                widgets.progress_bar.set_visible(true);
                widgets.progress_bar.pulse();
            }
            DownloadStatus::Downloading => {
                let pct = (item.progress * 100.0) as u32;
                let rec_str = format_bytes(item.received_bytes);
                let tot_str = if item.total_bytes > 0 {
                    format_bytes(item.total_bytes)
                } else {
                    "Unknown size".to_string()
                };

                let mut details = if item.total_bytes > 0 {
                    format!("{rec_str} / {tot_str} ({pct}%)")
                } else {
                    rec_str
                };

                let speed_str = format_speed(item.speed_bytes_per_sec);
                if !speed_str.is_empty() {
                    details.push_str(&format!(" • {speed_str}"));
                }

                if let Some(eta) = item.estimated_remaining_secs {
                    details.push_str(&format!(" • {}", format_eta(eta)));
                }

                widgets.status_lbl.set_label("Downloading");
                widgets.details_lbl.set_label(&details);

                widgets.cancel_btn.set_visible(true);
                widgets.retry_btn.set_visible(false);
                widgets.open_btn.set_visible(false);
                widgets.folder_btn.set_visible(false);
                widgets.progress_bar.set_visible(true);

                if item.total_bytes > 0 {
                    widgets
                        .progress_bar
                        .set_fraction(item.progress.clamp(0.0, 1.0));
                } else {
                    widgets.progress_bar.pulse();
                }
            }
            DownloadStatus::Completed => {
                let sz = format_bytes(if item.total_bytes > 0 {
                    item.total_bytes
                } else {
                    item.received_bytes
                });
                widgets.status_lbl.set_label("Completed");
                widgets
                    .details_lbl
                    .set_label(&format!("{sz} • Saved to {}", item.file_path.display()));
                widgets.cancel_btn.set_visible(false);
                widgets.retry_btn.set_visible(false);
                widgets.open_btn.set_visible(true);
                widgets.folder_btn.set_visible(true);
                widgets.progress_bar.set_visible(false);
            }
            DownloadStatus::Failed => {
                let err = item
                    .error
                    .as_deref()
                    .unwrap_or("Network or destination error");
                widgets.status_lbl.set_label("Failed");
                widgets.details_lbl.set_label(err);
                widgets.cancel_btn.set_visible(false);
                widgets.retry_btn.set_visible(true);
                widgets.open_btn.set_visible(false);
                widgets.folder_btn.set_visible(true);
                widgets.progress_bar.set_visible(false);
            }
            DownloadStatus::Cancelled => {
                widgets.status_lbl.set_label("Cancelled");
                widgets.details_lbl.set_label("Download was cancelled");
                widgets.cancel_btn.set_visible(false);
                widgets.retry_btn.set_visible(true);
                widgets.open_btn.set_visible(false);
                widgets.folder_btn.set_visible(false);
                widgets.progress_bar.set_visible(false);
            }
            DownloadStatus::Interrupted => {
                widgets.status_lbl.set_label("Interrupted");
                widgets
                    .details_lbl
                    .set_label("Download interrupted by browser restart");
                widgets.cancel_btn.set_visible(false);
                widgets.retry_btn.set_visible(true);
                widgets.open_btn.set_visible(false);
                widgets.folder_btn.set_visible(false);
                widgets.progress_bar.set_visible(false);
            }
        }
    }
}
