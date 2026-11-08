//! "Clear Browsing Data" dialog with granular categories and time range selection

use crate::privacy::cookies::{ClearDataSelection, ClearTimeRange, clear_website_data};
use crate::storage::StorageService;
use crate::storage::downloads::DownloadRepository;
use crate::storage::history::HistoryRepository;
use adw::prelude::*;
use gtk4::gio;
use tracing::info;
use webkit6::NetworkSession;

/// Presents the "Clear Browsing Data" modal dialog
pub fn show_clear_browsing_data_dialog(
    parent: &impl IsA<gtk4::Widget>,
    storage: StorageService,
    session: NetworkSession,
    persistence_allowed: bool,
) {
    let dialog = adw::AlertDialog::builder()
        .heading("Clear Browsing Data")
        .body("Choose the data categories and time range you want to remove.")
        .build();

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("clear", "Clear Data");
    dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("clear"));
    dialog.set_close_response("cancel");

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(16)
        .margin_end(16)
        .build();

    // 1. Time range selector
    let range_row = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .build();

    let range_label = gtk4::Label::builder()
        .label("Time range:")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    range_row.append(&range_label);

    let time_options = ["Last hour", "Last 24 hours", "Last 7 days", "All time"];
    let time_dropdown = gtk4::DropDown::from_strings(&time_options);
    time_dropdown.set_selected(3); // Default "All time"
    range_row.append(&time_dropdown);

    content_box.append(&range_row);

    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    content_box.append(&sep);

    // 2. Category Checkboxes
    let history_check = gtk4::CheckButton::builder()
        .label("Browsing history")
        .active(true)
        .build();
    content_box.append(&history_check);

    let downloads_check = gtk4::CheckButton::builder()
        .label("Download history (does not delete downloaded files)")
        .active(true)
        .build();
    content_box.append(&downloads_check);

    let cookies_check = gtk4::CheckButton::builder()
        .label("Cookies and other site data")
        .active(true)
        .build();
    content_box.append(&cookies_check);

    let cache_check = gtk4::CheckButton::builder()
        .label("Cached images and files")
        .active(true)
        .build();
    content_box.append(&cache_check);

    let perms_check = gtk4::CheckButton::builder()
        .label("Site permissions (camera, location, etc.)")
        .active(false)
        .build();
    content_box.append(&perms_check);

    dialog.set_extra_child(Some(&content_box));

    let storage_clone = storage;
    let session_clone = session;

    dialog.choose(Some(parent), None::<&gio::Cancellable>, move |response| {
        if response != "clear" {
            return;
        }

        let time_range = match time_dropdown.selected() {
            0 => ClearTimeRange::LastHour,
            1 => ClearTimeRange::Last24Hours,
            2 => ClearTimeRange::Last7Days,
            _ => ClearTimeRange::AllTime,
        };

        let selection = ClearDataSelection {
            history: history_check.is_active(),
            downloads: downloads_check.is_active(),
            cookies_and_site_data: cookies_check.is_active(),
            cache: cache_check.is_active(),
            permissions: perms_check.is_active(),
            time_range,
        };

        let cutoff = time_range.to_cutoff_timestamp();
        info!(
            "Clear browsing data confirmed: range={:?}, cutoff={cutoff}",
            selection.time_range
        );

        // 1. Clear database history and downloads
        if persistence_allowed
            && (selection.history || selection.downloads || selection.permissions)
        {
            let clear_hist = selection.history;
            let clear_dl = selection.downloads;
            let clear_perm = selection.permissions;

            storage_clone.dispatch(move |db| {
                if clear_hist {
                    let repo = HistoryRepository::new(db.conn());
                    let _ = repo.clear_since(cutoff);
                }
                if clear_dl {
                    let repo = DownloadRepository::new(db.conn());
                    let _ = repo.clear_since(cutoff);
                }
                if clear_perm {
                    let repo = crate::browser::permissions::PermissionRepository::new(db.conn());
                    let _ = repo.clear_all_permissions(true);
                }
            });
        }

        // 2. Clear WebKit website data (cookies, storage, cache)
        if selection.cookies_and_site_data || selection.cache {
            clear_website_data(&session_clone, selection, || {
                info!("Finished clearing WebKit website data");
            });
        }
    });
}
