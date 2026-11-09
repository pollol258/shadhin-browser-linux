//! Downloads popover anchored to the downloads navigation button
//!
//! Provides a compact Libadwaita popover displaying recent and active downloads,
//! live progress bars, speed, ETA, and quick actions (Open, Show in folder, Cancel, Remove).

use crate::browser::downloads::{DownloadItem, DownloadManager, DownloadStatus};
use crate::storage::StorageService;
use crate::storage::downloads::{DownloadRecord, DownloadRepository};
use gtk4::gio;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
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

pub struct DownloadsPopover {
    popover: gtk4::Popover,
    list_box: gtk4::Box,
    empty_box: gtk4::Box,
    storage: StorageService,
    download_mgr: DownloadManager,
    is_open: Rc<RefCell<bool>>,
}

impl DownloadsPopover {
    pub fn new(
        anchor: &impl IsA<gtk4::Widget>,
        storage: StorageService,
        download_mgr: DownloadManager,
    ) -> Rc<Self> {
        let popover = gtk4::Popover::builder()
            .has_arrow(true)
            .autohide(true)
            .css_classes(["shadhin-popover"])
            .build();
        popover.set_parent(anchor);

        let main_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .width_request(380)
            .build();

        // Header: "Downloads" title and "Clear Finished" button
        let header = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["shadhin-popover-header"])
            .build();

        let title_lbl = gtk4::Label::builder()
            .label("Downloads")
            .hexpand(true)
            .halign(gtk4::Align::Start)
            .css_classes(["font-bold", "heading"])
            .build();
        header.append(&title_lbl);

        let clear_btn = gtk4::Button::builder()
            .label("Clear finished")
            .css_classes(["flat", "dim-label"])
            .valign(gtk4::Align::Center)
            .build();
        header.append(&clear_btn);
        main_box.append(&header);

        let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        main_box.append(&sep);

        // Empty state container
        let empty_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Center)
            .margin_top(24)
            .margin_bottom(24)
            .build();

        let empty_icon = gtk4::Image::from_icon_name("folder-download-symbolic");
        empty_icon.set_pixel_size(32);
        empty_icon.set_opacity(0.6);
        empty_box.append(&empty_icon);

        let empty_label = gtk4::Label::builder()
            .label("No downloads yet")
            .css_classes(["dim-label"])
            .build();
        empty_box.append(&empty_label);
        main_box.append(&empty_box);

        // Scrolled list of download cards
        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .max_content_height(360)
            .propagate_natural_height(true)
            .hexpand(true)
            .build();

        let list_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(6)
            .hexpand(true)
            .build();
        scrolled.set_child(Some(&list_box));
        main_box.append(&scrolled);

        popover.set_child(Some(&main_box));

        let is_open = Rc::new(RefCell::new(false));
        let is_open_clone = is_open.clone();
        popover.connect_closed(move |_| {
            *is_open_clone.borrow_mut() = false;
        });

        let inst = Rc::new(Self {
            popover,
            list_box,
            empty_box,
            storage,
            download_mgr,
            is_open,
        });

        // Wire Clear Finished button
        let inst_clear = inst.clone();
        clear_btn.connect_clicked(move |_| {
            let st = inst_clear.storage.clone();
            let pop_refresh = inst_clear.clone();
            st.query_async(
                |db| {
                    let repo = DownloadRepository::new(db.conn());
                    let _ = repo.clear_finished();
                },
                move |()| {
                    pop_refresh.refresh();
                },
            );
        });

        // Wire live progress updates while open
        let inst_listener = inst.clone();
        inst.download_mgr.add_listener(move |_| {
            if *inst_listener.is_open.borrow() {
                inst_listener.refresh();
            }
        });

        let inst_comp = inst.clone();
        inst.download_mgr.add_completion_listener(move |_| {
            if *inst_comp.is_open.borrow() {
                inst_comp.refresh();
            }
        });

        inst
    }

    /// Refreshes the list of active and recent downloads
    pub fn refresh(&self) {
        // Clear existing list items
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        let active_downloads = self.download_mgr.get_active_downloads();
        let mut seen_ids = HashSet::new();
        let mut has_items = false;

        for item in &active_downloads {
            seen_ids.insert(item.id);
            has_items = true;
            let card = self.build_row(item);
            self.list_box.append(&card);
        }

        let list_box = self.list_box.clone();
        let empty_box = self.empty_box.clone();
        let inst = self.clone_ref();

        self.storage.query_async(
            |db| {
                let repo = DownloadRepository::new(db.conn());
                repo.get_recent(20).unwrap_or_default()
            },
            move |records: Vec<DownloadRecord>| {
                let mut added = false;
                for rec in records {
                    if seen_ids.contains(&rec.id) {
                        continue;
                    }
                    let item = DownloadItem::from(&rec);
                    let card = inst.build_row(&item);
                    list_box.append(&card);
                    added = true;
                }

                if has_items || added {
                    empty_box.set_visible(false);
                } else {
                    empty_box.set_visible(true);
                }
            },
        );

        if has_items {
            self.empty_box.set_visible(false);
        }
    }

    fn clone_ref(&self) -> Rc<Self> {
        Rc::new(Self {
            popover: self.popover.clone(),
            list_box: self.list_box.clone(),
            empty_box: self.empty_box.clone(),
            storage: self.storage.clone(),
            download_mgr: self.download_mgr.clone(),
            is_open: self.is_open.clone(),
        })
    }

    fn build_row(&self, item: &DownloadItem) -> gtk4::Box {
        let card = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .margin_start(4)
            .margin_end(4)
            .margin_top(4)
            .margin_bottom(4)
            .css_classes(["downloadrow", "surface-card"])
            .build();

        let icon = gtk4::Image::from_icon_name("text-x-generic-symbolic");
        icon.set_pixel_size(24);
        icon.set_valign(gtk4::Align::Start);
        card.append(&icon);

        let content_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .build();

        let name_lbl = gtk4::Label::builder()
            .label(&item.file_name)
            .halign(gtk4::Align::Start)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .css_classes(["font-semibold"])
            .build();
        content_box.append(&name_lbl);

        // Status subtitle
        let status_text = match item.status {
            DownloadStatus::Downloading | DownloadStatus::Pending => {
                let done = format_bytes(item.received_bytes);
                let total = format_bytes(item.total_bytes);
                let speed = format_speed(item.speed_bytes_per_sec);
                let eta = item
                    .estimated_remaining_secs
                    .map(format_eta)
                    .unwrap_or_default();
                format!("{done} of {total} · {speed} {eta}")
            }
            DownloadStatus::Completed => "Finished".to_string(),
            DownloadStatus::Cancelled => "Cancelled".to_string(),
            DownloadStatus::Interrupted => "Interrupted".to_string(),
            DownloadStatus::Failed => {
                let err = item.error.as_deref().unwrap_or("Network error");
                format!("Failed — {err}")
            }
        };

        let status_lbl = gtk4::Label::builder()
            .label(&status_text)
            .halign(gtk4::Align::Start)
            .css_classes(["dim-label", "font-xs"])
            .build();
        content_box.append(&status_lbl);

        if matches!(item.status, DownloadStatus::Downloading | DownloadStatus::Pending) {
            let pbar = gtk4::ProgressBar::builder()
                .fraction(item.progress)
                .css_classes(["shadhin-progress"])
                .build();
            content_box.append(&pbar);
        }

        // Action buttons
        let actions = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .margin_top(2)
            .build();

        match item.status {
            DownloadStatus::Downloading | DownloadStatus::Pending => {
                let cancel_btn = gtk4::Button::builder()
                    .label("Cancel")
                    .css_classes(["flat", "destructive-action", "font-xs"])
                    .build();
                let mgr = self.download_mgr.clone();
                let id = item.id;
                let inst = self.clone_ref();
                cancel_btn.connect_clicked(move |_| {
                    mgr.cancel_download(id);
                    inst.refresh();
                });
                actions.append(&cancel_btn);
            }
            DownloadStatus::Completed => {
                let file_path = item.file_path.clone();
                let open_btn = gtk4::Button::builder()
                    .label("Open")
                    .css_classes(["flat", "font-xs"])
                    .build();
                let fp_open = file_path.clone();
                open_btn.connect_clicked(move |_| {
                    let uri = format!("file://{}", fp_open.display());
                    let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
                });
                actions.append(&open_btn);

                let folder_btn = gtk4::Button::builder()
                    .label("Show in folder")
                    .css_classes(["flat", "font-xs"])
                    .build();
                let fp_folder = file_path;
                folder_btn.connect_clicked(move |_| {
                    if let Some(parent) = fp_folder.parent() {
                        let uri = format!("file://{}", parent.display());
                        let _ = gio::AppInfo::launch_default_for_uri(
                            &uri,
                            None::<&gio::AppLaunchContext>,
                        );
                    }
                });
                actions.append(&folder_btn);

                let remove_btn = gtk4::Button::builder()
                    .label("Remove")
                    .css_classes(["flat", "dim-label", "font-xs"])
                    .build();
                let id = item.id;
                let st = self.storage.clone();
                let inst = self.clone_ref();
                remove_btn.connect_clicked(move |_| {
                    let inst_del = inst.clone();
                    st.query_async(
                        move |db| {
                            let repo = DownloadRepository::new(db.conn());
                            let _ = repo.delete(id);
                        },
                        move |()| {
                            inst_del.refresh();
                        },
                    );
                });
                actions.append(&remove_btn);
            }
            DownloadStatus::Cancelled | DownloadStatus::Failed | DownloadStatus::Interrupted => {
                let remove_btn = gtk4::Button::builder()
                    .label("Remove")
                    .css_classes(["flat", "dim-label", "font-xs"])
                    .build();
                let id = item.id;
                let st = self.storage.clone();
                let inst = self.clone_ref();
                remove_btn.connect_clicked(move |_| {
                    let inst_del = inst.clone();
                    st.query_async(
                        move |db| {
                            let repo = DownloadRepository::new(db.conn());
                            let _ = repo.delete(id);
                        },
                        move |()| {
                            inst_del.refresh();
                        },
                    );
                });
                actions.append(&remove_btn);
            }
        }

        content_box.append(&actions);
        card.append(&content_box);
        card
    }

    /// Toggles popup visibility
    pub fn toggle(&self) {
        if self.popover.is_visible() {
            self.popdown();
        } else {
            self.refresh();
            *self.is_open.borrow_mut() = true;
            self.popover.popup();
        }
    }

    /// Dismisses the popover
    pub fn popdown(&self) {
        *self.is_open.borrow_mut() = false;
        self.popover.popdown();
    }

    /// Checks if currently visible
    pub fn is_visible(&self) -> bool {
        self.popover.is_visible()
    }

    /// Returns the underlying GtkPopover
    pub fn popover(&self) -> &gtk4::Popover {
        &self.popover
    }
}
