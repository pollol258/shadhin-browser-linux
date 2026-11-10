//! Downloads popover anchored to the downloads navigation button
//!
//! Provides a compact Libadwaita popover displaying recent and active downloads,
//! live progress bars, speed, ETA, and quick actions (Open, Show in folder, Cancel, Remove).

use crate::browser::downloads::{DownloadItem, DownloadManager, DownloadStatus};
use crate::storage::StorageService;
use crate::storage::downloads::{DownloadRecord, DownloadRepository};
use gtk4::gio;
use gtk4::prelude::*;
use adw::prelude::*;
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
    const GB: f64 = MB * 1024.0;
    if bps >= GB {
        format!("{:.2} GB/s", bps / GB)
    } else if bps >= MB {
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
    session: Option<Rc<RefCell<webkit6::NetworkSession>>>,
    persistence_allowed: bool,
    is_open: Rc<RefCell<bool>>,
}

impl DownloadsPopover {
    pub fn new(
        anchor: &impl IsA<gtk4::Widget>,
        storage: StorageService,
        download_mgr: DownloadManager,
        session: Option<Rc<RefCell<webkit6::NetworkSession>>>,
        persistence_allowed: bool,
    ) -> Rc<Self> {
        let popover = gtk4::Popover::builder()
            .has_arrow(true)
            .autohide(true)
            .css_classes(["shadhin-popover"])
            .build();
        popover.set_size_request(420, -1);
        popover.set_parent(anchor);

        let main_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(10)
            .css_classes(["shadhin-popover-main"])
            .build();
        main_box.set_size_request(420, -1);

        // Header: "Downloads" title and "Clear Finished" button
        let header = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["shadhin-popover-header"])
            .build();

        let title_lbl = gtk4::Label::builder()
            .label("Downloads")
            .hexpand(true)
            .halign(gtk4::Align::Start)
            .valign(gtk4::Align::Center)
            .css_classes(["font-bold", "heading"])
            .build();
        header.append(&title_lbl);

        let clear_btn = gtk4::Button::builder()
            .label("Clear finished")
            .css_classes(["flat", "dim-label"])
            .halign(gtk4::Align::End)
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
            .max_content_height(380)
            .propagate_natural_height(true)
            .hexpand(true)
            .build();
        scrolled.set_size_request(420, -1);

        let list_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
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
            session,
            persistence_allowed,
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
            session: self.session.clone(),
            persistence_allowed: self.persistence_allowed,
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
            .hexpand(true)
            .css_classes(["downloadrow", "surface-card"])
            .build();

        let icon = gtk4::Image::from_icon_name("text-x-generic-symbolic");
        icon.set_pixel_size(24);
        icon.set_valign(gtk4::Align::Start);
        icon.set_halign(gtk4::Align::Center);
        card.append(&icon);

        let content_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .build();

        let name_lbl = gtk4::Label::builder()
            .label(&item.file_name)
            .halign(gtk4::Align::Start)
            .valign(gtk4::Align::Center)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::Middle)
            .css_classes(["font-semibold"])
            .build();
        content_box.append(&name_lbl);

        // Status subtitle
        let status_text = match item.status {
            DownloadStatus::Downloading | DownloadStatus::Pending => {
                let done = format_bytes(item.received_bytes);
                let mut parts = Vec::new();
                if item.total_bytes > 0 {
                    let total = format_bytes(item.total_bytes);
                    parts.push(format!("{done} of {total}"));
                } else {
                    parts.push(done);
                }

                if item.speed_bytes_per_sec > 0.0 {
                    parts.push(format_speed(item.speed_bytes_per_sec));
                }

                if let Some(eta_secs) = item.estimated_remaining_secs {
                    parts.push(format_eta(eta_secs));
                }

                parts.join(" · ")
            }
            DownloadStatus::Completed => {
                let size = if item.total_bytes > 0 {
                    format!(" · {}", format_bytes(item.total_bytes))
                } else if item.received_bytes > 0 {
                    format!(" · {}", format_bytes(item.received_bytes))
                } else {
                    String::new()
                };
                format!("Finished{size}")
            }
            DownloadStatus::Cancelled => "Cancelled".to_string(),
            DownloadStatus::Interrupted => {
                let err = item.error.as_deref().unwrap_or("Connection lost");
                format!("Interrupted — {err}")
            }
            DownloadStatus::Failed => {
                let err = item.error.as_deref().unwrap_or("Network error");
                format!("Failed — {err}")
            }
        };

        let status_lbl = gtk4::Label::builder()
            .label(&status_text)
            .halign(gtk4::Align::Start)
            .valign(gtk4::Align::Center)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .css_classes(["dim-label", "font-xs"])
            .build();
        content_box.append(&status_lbl);

        use std::os::unix::fs::PermissionsExt;
        let is_executable = if let Ok(meta) = std::fs::metadata(&item.file_path) {
            (meta.permissions().mode() & 0o111) != 0
                || crate::browser::downloads::is_dangerous_extension(&item.file_name)
        } else {
            crate::browser::downloads::is_dangerous_extension(&item.file_name)
        };

        if is_executable && item.status == DownloadStatus::Completed {
            let warn_box = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .spacing(4)
                .margin_top(2)
                .build();
            let warn_icon = gtk4::Image::from_icon_name("dialog-warning-symbolic");
            warn_icon.set_pixel_size(14);
            warn_icon.add_css_class("warning");
            let warn_label = gtk4::Label::builder()
                .label("Executable file — caution advised")
                .tooltip_text(
                    "This file has executable permissions or a runnable format. Exercise caution before opening.",
                )
                .css_classes(["font-xs", "dim-label"])
                .build();
            warn_box.append(&warn_icon);
            warn_box.append(&warn_label);
            content_box.append(&warn_box);
        }

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
            .spacing(8)
            .halign(gtk4::Align::Start)
            .valign(gtk4::Align::Center)
            .margin_top(4)
            .build();

        match item.status {
            DownloadStatus::Downloading | DownloadStatus::Pending => {
                let cancel_btn = gtk4::Button::builder()
                    .label("Cancel")
                    .css_classes(["flat", "destructive-action", "font-xs"])
                    .halign(gtk4::Align::Start)
                    .valign(gtk4::Align::Center)
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
                let file_name = item.file_name.clone();
                let open_btn = gtk4::Button::builder()
                    .label("Open")
                    .css_classes(["flat", "font-xs"])
                    .halign(gtk4::Align::Start)
                    .valign(gtk4::Align::Center)
                    .build();
                let fp_open = file_path.clone();
                let fn_open = file_name;
                let is_exec_click = is_executable;
                let pop_root = self.popover.clone();
                open_btn.connect_clicked(move |_| {
                    if is_exec_click {
                        let alert = adw::AlertDialog::builder()
                            .heading("Open Executable File?")
                            .body(format!(
                                "\"{fn_open}\" is an executable file or script. Opening it could allow code to execute on your computer. Are you sure you want to open it?"
                            ))
                            .build();
                        alert.add_response("cancel", "Cancel");
                        alert.add_response("open", "Open Anyway");
                        alert.set_default_response(Some("cancel"));
                        alert.set_close_response("cancel");
                        alert.set_response_appearance("open", adw::ResponseAppearance::Destructive);
                        let fp_confirmed = fp_open.clone();
                        alert.connect_response(None, move |_, resp| {
                            if resp == "open" {
                                let uri = format!("file://{}", fp_confirmed.display());
                                let _ = gio::AppInfo::launch_default_for_uri(
                                    &uri,
                                    None::<&gio::AppLaunchContext>,
                                );
                            }
                        });
                        if let Some(root) = pop_root.root()
                            && let Some(win) = root.downcast_ref::<gtk4::Window>()
                        {
                            alert.present(Some(win));
                        }
                    } else {
                        let uri = format!("file://{}", fp_open.display());
                        let _ = gio::AppInfo::launch_default_for_uri(
                            &uri,
                            None::<&gio::AppLaunchContext>,
                        );
                    }
                });
                actions.append(&open_btn);

                let folder_btn = gtk4::Button::builder()
                    .label("Show in folder")
                    .css_classes(["flat", "font-xs"])
                    .halign(gtk4::Align::Start)
                    .valign(gtk4::Align::Center)
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
                    .halign(gtk4::Align::Start)
                    .valign(gtk4::Align::Center)
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
            DownloadStatus::Interrupted | DownloadStatus::Failed => {
                let retry_btn = gtk4::Button::builder()
                    .label("Retry")
                    .css_classes(["flat", "suggested-action", "font-xs"])
                    .halign(gtk4::Align::Start)
                    .valign(gtk4::Align::Center)
                    .build();
                let id = item.id;
                let mgr = self.download_mgr.clone();
                let sess_opt = self.session.clone();
                let perm = self.persistence_allowed;
                let inst = self.clone_ref();
                retry_btn.connect_clicked(move |_| {
                    if let Some(ref s) = sess_opt {
                        mgr.retry_download(id, &s.borrow(), perm);
                    }
                    inst.refresh();
                });
                actions.append(&retry_btn);

                let remove_btn = gtk4::Button::builder()
                    .label("Remove")
                    .css_classes(["flat", "dim-label", "font-xs"])
                    .halign(gtk4::Align::Start)
                    .valign(gtk4::Align::Center)
                    .build();
                let id = item.id;
                let st = self.storage.clone();
                let inst_remove = self.clone_ref();
                remove_btn.connect_clicked(move |_| {
                    let inst_del = inst_remove.clone();
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
            DownloadStatus::Cancelled => {
                let remove_btn = gtk4::Button::builder()
                    .label("Remove")
                    .css_classes(["flat", "dim-label", "font-xs"])
                    .halign(gtk4::Align::Start)
                    .valign(gtk4::Align::Center)
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

    /// Checks if currently in open state
    pub fn is_open(&self) -> bool {
        *self.is_open.borrow()
    }

    /// Returns the underlying GtkPopover
    pub fn popover(&self) -> &gtk4::Popover {
        &self.popover
    }

    /// Explicitly dismisses and unparents the downloads popover
    pub fn cleanup(&self) {
        self.popover.popdown();
        self.popover.unparent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 2), "2.00 GB");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(0.0), "");
        assert_eq!(format_speed(-5.0), "");
        assert_eq!(format_speed(500.0), "500 B/s");
        assert_eq!(format_speed(2048.0), "2 KB/s");
        assert_eq!(format_speed(2.5 * 1024.0 * 1024.0), "2.5 MB/s");
        assert_eq!(format_speed(1.2 * 1024.0 * 1024.0 * 1024.0), "1.20 GB/s");
    }

    #[test]
    fn test_format_eta() {
        assert_eq!(format_eta(0), "0s left");
        assert_eq!(format_eta(45), "45s left");
        assert_eq!(format_eta(60), "1m 0s left");
        assert_eq!(format_eta(125), "2m 5s left");
        assert_eq!(format_eta(3600), "1h 0m left");
        assert_eq!(format_eta(3665), "1h 1m left");
    }
}
