//! Native New Tab dashboard with search, shortcuts, and privacy statistics
//!
//! Hosts the centered DuckDuckGo search bar, SQLite-backed shortcut cards,
//! recently visited sites, and the Privacy Shield statistics indicator.

use crate::storage::StorageService;
use crate::storage::history::{HistoryItem, HistoryRepository};
use crate::storage::shortcuts::{ShortcutItem, ShortcutRepository};
use adw::prelude::*;
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct ShadhinNewTab {
    scrolled_window: gtk4::ScrolledWindow,
    shortcuts_box: gtk4::Box,
    recent_box: gtk4::Box,
    on_navigate: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    storage: Rc<RefCell<Option<StorageService>>>,
}

impl ShadhinNewTab {
    /// Constructs a new New Tab page component
    pub fn new() -> Self {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(20)
            .valign(gtk4::Align::Start)
            .halign(gtk4::Align::Center)
            .margin_top(32)
            .margin_bottom(32)
            .margin_start(16)
            .margin_end(16)
            .css_classes(["shadhin-newtab-container"])
            .build();

        let scrolled_window = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&container)
            .build();

        let on_navigate: Rc<RefCell<Option<Box<dyn Fn(&str)>>>> = Rc::new(RefCell::new(None));

        // 1. Shadhin Branding Logo
        let logo_icon = gtk4::Image::from_icon_name("web-browser-symbolic");
        logo_icon.set_pixel_size(64);
        logo_icon.add_css_class("shadhin-brand-logo");
        container.append(&logo_icon);

        // 2. Title & Tagline
        let title_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .build();

        let brand_label = gtk4::Label::builder()
            .label("স্বাধীন ব্রাউজার")
            .css_classes(["shadhin-brand-title"])
            .build();
        title_box.append(&brand_label);

        let tagline = gtk4::Label::builder()
            .label("Sovereign, fast, and respectful of your digital freedom")
            .css_classes(["shadhin-brand-subtitle"])
            .build();
        title_box.append(&tagline);
        container.append(&title_box);

        // 3. Centered Pill Search Box
        let search_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["shadhin-newtab-search-box"])
            .halign(gtk4::Align::Center)
            .build();

        let search_icon = gtk4::Image::from_icon_name("system-search-symbolic");
        search_icon.set_opacity(0.7);
        search_box.append(&search_icon);

        let search_entry = gtk4::Entry::builder()
            .placeholder_text("Search the web privately")
            .hexpand(true)
            .css_classes(["shadhin-newtab-search-entry"])
            .build();
        search_box.append(&search_entry);

        let on_nav_clone = on_navigate.clone();
        search_entry.connect_activate(move |entry| {
            let query = entry.text().trim().to_string();
            if !query.is_empty() {
                let url = if query.starts_with("http://")
                    || query.starts_with("https://")
                    || query.contains('.') && !query.contains(' ')
                {
                    if !query.starts_with("http://") && !query.starts_with("https://") {
                        format!("https://{query}")
                    } else {
                        query
                    }
                } else {
                    let encoded = glib::Uri::escape_string(&query, None, false);
                    format!("https://duckduckgo.com/?q={encoded}")
                };

                if let Some(ref cb) = *on_nav_clone.borrow() {
                    cb(&url);
                }
            }
        });
        container.append(&search_box);

        // 4. Shortcut Tiles Grid
        let shortcuts_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk4::Align::Center)
            .css_classes(["shadhin-shortcut-grid"])
            .build();
        container.append(&shortcuts_box);

        // 5. Recently Visited Sites Section
        let recent_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk4::Align::Center)
            .css_classes(["shadhin-recent-sites-grid"])
            .build();
        container.append(&recent_box);

        // 6. Honest Privacy Shield Stats Bar (SPEC.md 2.8)
        let stats_card = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .halign(gtk4::Align::Center)
            .css_classes(["shadhin-stats-card"])
            .build();

        let header_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .build();

        let shield_icon = gtk4::Image::from_icon_name("security-high-symbolic");
        header_row.append(&shield_icon);

        let stats_title = gtk4::Label::builder()
            .label("Privacy Shield — Protection Active")
            .css_classes(["shadhin-stats-title"])
            .hexpand(true)
            .halign(gtk4::Align::Start)
            .build();
        header_row.append(&stats_title);
        stats_card.append(&header_row);

        let grid = gtk4::Grid::builder()
            .column_spacing(20)
            .row_spacing(4)
            .halign(gtk4::Align::Center)
            .build();

        let items = [
            ("—", "Trackers Blocked", 0),
            ("—", "Ads Blocked", 1),
            ("—", "Fingerprints Blocked", 2),
            ("—", "Data Saved", 3),
        ];

        for (num, lbl, col) in items {
            let num_lbl = gtk4::Label::builder()
                .label(num)
                .css_classes(["shadhin-stat-num"])
                .tooltip_text("Not measurable on this WebKitGTK version")
                .build();
            let text_lbl = gtk4::Label::builder()
                .label(lbl)
                .css_classes(["shadhin-stat-label"])
                .tooltip_text("Not measurable on this WebKitGTK version")
                .build();

            grid.attach(&num_lbl, col, 0, 1, 1);
            grid.attach(&text_lbl, col, 1, 1, 1);
        }

        stats_card.append(&grid);
        container.append(&stats_card);

        let storage: Rc<RefCell<Option<StorageService>>> = Rc::new(RefCell::new(None));

        Self {
            scrolled_window,
            shortcuts_box,
            recent_box,
            on_navigate,
            storage,
        }
    }

    /// Sets the navigation callback triggered when the user enters a search or clicks a shortcut
    pub fn set_on_navigate<F: Fn(&str) + 'static>(&self, callback: F) {
        *self.on_navigate.borrow_mut() = Some(Box::new(callback));
    }

    /// Populates shortcuts and recently visited sites from storage asynchronously
    pub fn load_data(&self, storage: StorageService) {
        *self.storage.borrow_mut() = Some(storage.clone());

        let sc_box = self.shortcuts_box.clone();
        let rc_box = self.recent_box.clone();
        let on_nav = self.on_navigate.clone();
        let storage_add = storage.clone();
        let this_clone = self.clone();

        storage.query_async(
            move |db| {
                let repo_sc = ShortcutRepository::new(db.conn());
                let repo_h = HistoryRepository::new(db.conn());
                let shortcuts = repo_sc.get_all().unwrap_or_default();
                let recent = repo_h.get_top_sites(5).unwrap_or_default();
                (shortcuts, recent)
            },
            move |(shortcuts, recent): (Vec<ShortcutItem>, Vec<HistoryItem>)| {
                // 1. Populate Shortcuts
                while let Some(child) = sc_box.first_child() {
                    sc_box.remove(&child);
                }

                let sc_ids: Vec<i64> = shortcuts.iter().map(|s| s.id).collect();
                let total_sc = shortcuts.len();

                for (idx, sc) in shortcuts.into_iter().enumerate() {
                    let card_wrapper = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .spacing(0)
                        .build();

                    let btn = gtk4::Button::builder()
                        .css_classes(["shadhin-shortcut-card"])
                        .build();

                    let card_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Vertical)
                        .spacing(6)
                        .halign(gtk4::Align::Center)
                        .valign(gtk4::Align::Center)
                        .build();

                    let icon = gtk4::Image::from_icon_name("text-html-symbolic");
                    icon.set_pixel_size(24);
                    card_box.append(&icon);

                    let label = gtk4::Label::builder()
                        .label(&sc.title)
                        .css_classes(["shadhin-shortcut-label"])
                        .ellipsize(gtk4::pango::EllipsizeMode::End)
                        .max_width_chars(12)
                        .build();
                    card_box.append(&label);

                    btn.set_child(Some(&card_box));

                    let target = sc.url.clone();
                    let on_nav_card = on_nav.clone();
                    btn.connect_clicked(move |_| {
                        if let Some(ref cb) = *on_nav_card.borrow() {
                            cb(&target);
                        }
                    });

                    // Action buttons column (edit, delete, move)
                    let actions_col = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Vertical)
                        .spacing(2)
                        .valign(gtk4::Align::Start)
                        .build();

                    // Small delete button for shortcut
                    let del_btn = gtk4::Button::builder()
                        .icon_name("window-close-symbolic")
                        .css_classes(["flat", "circular", "shadhin-delete-shortcut-btn"])
                        .tooltip_text("Remove shortcut")
                        .build();

                    let sc_id = sc.id;
                    let st_del = storage_add.clone();
                    let sc_box_del = sc_box.clone();
                    let card_del = card_wrapper.clone();
                    del_btn.connect_clicked(move |_| {
                        let st = st_del.clone();
                        let box_ref = sc_box_del.clone();
                        let c_ref = card_del.clone();
                        st.dispatch(move |db| {
                            let repo = ShortcutRepository::new(db.conn());
                            let _ = repo.delete(sc_id);
                        });
                        box_ref.remove(&c_ref);
                    });
                    actions_col.append(&del_btn);

                    // Edit button with popover
                    let edit_btn = gtk4::Button::builder()
                        .icon_name("document-edit-symbolic")
                        .css_classes(["flat", "circular", "shadhin-delete-shortcut-btn"])
                        .tooltip_text("Edit shortcut")
                        .build();

                    let edit_pop = gtk4::Popover::builder().autohide(true).build();
                    edit_pop.set_parent(&edit_btn);

                    let edit_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Vertical)
                        .spacing(6)
                        .margin_top(8)
                        .margin_bottom(8)
                        .margin_start(8)
                        .margin_end(8)
                        .width_request(200)
                        .build();

                    let edit_title_entry = gtk4::Entry::builder().text(&sc.title).build();
                    let edit_url_entry = gtk4::Entry::builder().text(&sc.url).build();
                    let edit_save_btn = gtk4::Button::builder()
                        .label("Update")
                        .css_classes(["suggested-action"])
                        .halign(gtk4::Align::End)
                        .build();

                    edit_box.append(&gtk4::Label::builder().label("Edit Shortcut").build());
                    edit_box.append(&edit_title_entry);
                    edit_box.append(&edit_url_entry);
                    edit_box.append(&edit_save_btn);
                    edit_pop.set_child(Some(&edit_box));

                    let st_edit = storage_add.clone();
                    let this_edit = this_clone.clone();
                    let pop_edit_ref = edit_pop.clone();
                    let e_title = edit_title_entry.clone();
                    let e_url = edit_url_entry.clone();
                    edit_save_btn.connect_clicked(move |_| {
                        let t = e_title.text().trim().to_string();
                        let u = e_url.text().trim().to_string();
                        if !t.is_empty() && !u.is_empty() {
                            let st = st_edit.clone();
                            let this_rf = this_edit.clone();
                            st.dispatch(move |db| {
                                let repo = ShortcutRepository::new(db.conn());
                                let _ = repo.update(sc_id, &t, &u);
                            });
                            pop_edit_ref.popdown();
                            let st_r = st_edit.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(150),
                                move || {
                                    this_rf.load_data(st_r);
                                },
                            );
                        }
                    });

                    edit_btn.connect_clicked(move |_| {
                        edit_pop.popup();
                    });
                    actions_col.append(&edit_btn);

                    // Reorder: Move Left (if not first)
                    if idx > 0 {
                        let move_l_btn = gtk4::Button::builder()
                            .icon_name("go-previous-symbolic")
                            .css_classes(["flat", "circular", "shadhin-delete-shortcut-btn"])
                            .tooltip_text("Move left")
                            .build();

                        let mut new_order = sc_ids.clone();
                        new_order.swap(idx, idx - 1);
                        let st_reorder = storage_add.clone();
                        let this_reorder = this_clone.clone();
                        move_l_btn.connect_clicked(move |_| {
                            let order = new_order.clone();
                            let st = st_reorder.clone();
                            let this_rf = this_reorder.clone();
                            st.dispatch(move |db| {
                                let repo = ShortcutRepository::new(db.conn());
                                let _ = repo.reorder(&order);
                            });
                            let st_r = st_reorder.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(150),
                                move || {
                                    this_rf.load_data(st_r);
                                },
                            );
                        });
                        actions_col.append(&move_l_btn);
                    }

                    // Reorder: Move Right (if not last)
                    if idx + 1 < total_sc {
                        let move_r_btn = gtk4::Button::builder()
                            .icon_name("go-next-symbolic")
                            .css_classes(["flat", "circular", "shadhin-delete-shortcut-btn"])
                            .tooltip_text("Move right")
                            .build();

                        let mut new_order = sc_ids.clone();
                        new_order.swap(idx, idx + 1);
                        let st_reorder = storage_add.clone();
                        let this_reorder = this_clone.clone();
                        move_r_btn.connect_clicked(move |_| {
                            let order = new_order.clone();
                            let st = st_reorder.clone();
                            let this_rf = this_reorder.clone();
                            st.dispatch(move |db| {
                                let repo = ShortcutRepository::new(db.conn());
                                let _ = repo.reorder(&order);
                            });
                            let st_r = st_reorder.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(150),
                                move || {
                                    this_rf.load_data(st_r);
                                },
                            );
                        });
                        actions_col.append(&move_r_btn);
                    }

                    card_wrapper.append(&btn);
                    card_wrapper.append(&actions_col);
                    sc_box.append(&card_wrapper);
                }

                // Add "+ Add Shortcut" card with input popover
                let add_btn = gtk4::Button::builder()
                    .css_classes(["shadhin-shortcut-card"])
                    .build();

                let add_box = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Vertical)
                    .spacing(6)
                    .halign(gtk4::Align::Center)
                    .valign(gtk4::Align::Center)
                    .build();

                let add_icon = gtk4::Image::from_icon_name("list-add-symbolic");
                add_icon.set_pixel_size(24);
                add_box.append(&add_icon);

                let add_lbl = gtk4::Label::builder()
                    .label("Add Shortcut")
                    .css_classes(["shadhin-shortcut-label"])
                    .build();
                add_box.append(&add_lbl);
                add_btn.set_child(Some(&add_box));

                let popover = gtk4::Popover::builder()
                    .has_arrow(true)
                    .autohide(true)
                    .build();
                popover.set_parent(&add_btn);

                let pop_box = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Vertical)
                    .spacing(8)
                    .margin_top(8)
                    .margin_bottom(8)
                    .margin_start(8)
                    .margin_end(8)
                    .width_request(240)
                    .build();

                let pop_title = gtk4::Label::builder()
                    .label("Add Shortcut")
                    .css_classes(["title-4"])
                    .halign(gtk4::Align::Start)
                    .build();
                pop_box.append(&pop_title);

                let title_entry = gtk4::Entry::builder()
                    .placeholder_text("Title (e.g. GitHub)")
                    .build();
                pop_box.append(&title_entry);

                let url_entry = gtk4::Entry::builder()
                    .placeholder_text("URL (https://...)")
                    .text("https://")
                    .build();
                pop_box.append(&url_entry);

                let save_btn = gtk4::Button::builder()
                    .label("Save")
                    .css_classes(["suggested-action"])
                    .halign(gtk4::Align::End)
                    .build();
                pop_box.append(&save_btn);

                popover.set_child(Some(&pop_box));

                let pop_click = popover.clone();
                add_btn.connect_clicked(move |_| {
                    pop_click.popup();
                });

                let st_save = storage_add.clone();
                let pop_save = popover.clone();
                let title_e = title_entry.clone();
                let url_e = url_entry.clone();
                let this_refresh = this_clone.clone();

                save_btn.connect_clicked(move |_| {
                    let title = title_e.text().trim().to_string();
                    let mut url = url_e.text().trim().to_string();
                    if title.is_empty() || url.is_empty() {
                        return;
                    }
                    if !url.starts_with("http://") && !url.starts_with("https://") {
                        url = format!("https://{url}");
                    }

                    let t = title.clone();
                    let u = url.clone();
                    let st = st_save.clone();
                    let refresh_self = this_refresh.clone();

                    st.dispatch(move |db| {
                        let repo = ShortcutRepository::new(db.conn());
                        let _ = repo.add(&t, &u);
                    });

                    pop_save.popdown();
                    title_e.set_text("");
                    url_e.set_text("https://");

                    let st_reload = st_save.clone();
                    glib::timeout_add_local_once(
                        std::time::Duration::from_millis(150),
                        move || {
                            refresh_self.load_data(st_reload);
                        },
                    );
                });

                sc_box.append(&add_btn);

                // 2. Populate Recently Visited Sites
                while let Some(child) = rc_box.first_child() {
                    rc_box.remove(&child);
                }

                for item in recent {
                    let chip = gtk4::Button::builder()
                        .css_classes(["flat", "shadhin-recent-chip"])
                        .build();

                    let chip_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .spacing(6)
                        .build();

                    let chip_icon = gtk4::Image::from_icon_name("document-open-recent-symbolic");
                    chip_box.append(&chip_icon);

                    let chip_lbl = gtk4::Label::builder()
                        .label(&item.title)
                        .ellipsize(gtk4::pango::EllipsizeMode::End)
                        .max_width_chars(18)
                        .build();
                    chip_box.append(&chip_lbl);

                    chip.set_child(Some(&chip_box));

                    let target = item.url.clone();
                    let on_nav_chip = on_nav.clone();
                    chip.connect_clicked(move |_| {
                        if let Some(ref cb) = *on_nav_chip.borrow() {
                            cb(&target);
                        }
                    });

                    rc_box.append(&chip);
                }
            },
        );
    }

    /// Accessor to the root widget
    pub fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.scrolled_window
    }
}

impl Default for ShadhinNewTab {
    fn default() -> Self {
        Self::new()
    }
}
