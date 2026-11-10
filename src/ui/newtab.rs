//! Native New Tab dashboard with search, shortcuts, and privacy statistics
//!
//! Hosts the centered search bar, SQLite-backed static shortcut cards with context menus,
//! recently visited sites, and the Privacy Shield indicator.

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
    shortcuts_flow: gtk4::FlowBox,
    recent_box: gtk4::Box,
    on_navigate: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    storage: Rc<RefCell<Option<StorageService>>>,
}

impl ShadhinNewTab {
    /// Constructs a new New Tab page component
    pub fn new() -> Self {
        let clamp = adw::Clamp::builder()
            .maximum_size(780)
            .tightening_threshold(520)
            .hexpand(true)
            .valign(gtk4::Align::Start)
            .halign(gtk4::Align::Fill)
            .margin_top(40)
            .margin_bottom(40)
            .margin_start(20)
            .margin_end(20)
            .build();

        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(28)
            .valign(gtk4::Align::Start)
            .halign(gtk4::Align::Fill)
            .css_classes(["shadhin-newtab-container"])
            .build();
        clamp.set_child(Some(&container));

        let scrolled_window = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&clamp)
            .build();

        let on_navigate: Rc<RefCell<Option<Box<dyn Fn(&str)>>>> = Rc::new(RefCell::new(None));

        // 1. Shadhin Branding Logo & Headings
        let brand_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(10)
            .halign(gtk4::Align::Center)
            .build();

        let logo_icon = gtk4::Image::from_icon_name("web-browser-symbolic");
        logo_icon.set_pixel_size(48);
        logo_icon.set_halign(gtk4::Align::Center);
        logo_icon.set_valign(gtk4::Align::Center);

        let logo_badge = gtk4::Box::builder()
            .css_classes(["newtab-brandmark"])
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Center)
            .build();
        logo_badge.append(&logo_icon);
        brand_box.append(&logo_badge);

        let brand_label = gtk4::Label::builder()
            .label("স্বাধীন ব্রাউজার")
            .css_classes(["shadhin-brand-title"])
            .halign(gtk4::Align::Center)
            .build();
        brand_box.append(&brand_label);

        let tagline = gtk4::Label::builder()
            .label("Sovereign, fast, and respectful of your digital freedom")
            .css_classes(["shadhin-brand-subtitle"])
            .halign(gtk4::Align::Center)
            .build();
        brand_box.append(&tagline);
        container.append(&brand_box);

        // 2. Centered Pill Search Box
        let search_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(10)
            .css_classes(["newtab-search"])
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .build();
        search_box.set_size_request(520, 48);

        let search_icon = gtk4::Image::from_icon_name("system-search-symbolic");
        search_icon.set_pixel_size(16);
        search_icon.set_opacity(0.65);
        search_box.append(&search_icon);

        let search_entry = gtk4::Entry::builder()
            .placeholder_text("Search the web privately or type a URL")
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
                    || (query.contains('.') && !query.contains(' '))
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

        // 3. Shortcuts Static Grid (gtk4::FlowBox)
        let shortcuts_section = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .build();

        let shortcuts_flow = gtk4::FlowBox::builder()
            .valign(gtk4::Align::Start)
            .halign(gtk4::Align::Center)
            .selection_mode(gtk4::SelectionMode::None)
            .max_children_per_line(6)
            .min_children_per_line(3)
            .column_spacing(16)
            .row_spacing(16)
            .css_classes(["shadhin-shortcut-flow"])
            .build();
        shortcuts_section.append(&shortcuts_flow);
        container.append(&shortcuts_section);

        // 4. Recently Visited Sites Section
        let recent_container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .halign(gtk4::Align::Center)
            .build();

        let recent_lbl = gtk4::Label::builder()
            .label("Recently Visited")
            .css_classes(["dim-label"])
            .halign(gtk4::Align::Center)
            .build();
        recent_container.append(&recent_lbl);

        let recent_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk4::Align::Center)
            .build();
        recent_container.append(&recent_box);
        container.append(&recent_container);

        // 5. Honest Privacy Shield Status Card
        let stats_card = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(10)
            .halign(gtk4::Align::Center)
            .css_classes(["shadhin-stats-card"])
            .build();
        stats_card.set_size_request(520, -1);

        let header_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk4::Align::Center)
            .build();

        let shield_icon = gtk4::Image::from_icon_name("security-high-symbolic");
        shield_icon.set_pixel_size(18);
        header_row.append(&shield_icon);

        let stats_title = gtk4::Label::builder()
            .label("Privacy Shield — Protection Active")
            .css_classes(["shadhin-stats-title", "heading"])
            .build();
        header_row.append(&stats_title);
        stats_card.append(&header_row);

        let badges_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk4::Align::Center)
            .build();

        let badges = [
            "EasyList Ad Blocking",
            "EasyPrivacy Tracking Protection",
            "HTTPS-Only Upgrade",
            "Sandbox Isolation",
        ];

        for badge in badges {
            let badge_lbl = gtk4::Label::builder()
                .label(format!("✓ {badge}"))
                .css_classes(["shadhin-stat-badge"])
                .build();
            badges_box.append(&badge_lbl);
        }

        stats_card.append(&badges_box);
        container.append(&stats_card);

        let storage: Rc<RefCell<Option<StorageService>>> = Rc::new(RefCell::new(None));

        Self {
            scrolled_window,
            shortcuts_flow,
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

        let sc_flow = self.shortcuts_flow.clone();
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
                // 1. Clear existing FlowBox children
                while let Some(child) = sc_flow.first_child() {
                    sc_flow.remove(&child);
                }

                let sc_ids: Vec<i64> = shortcuts.iter().map(|s| s.id).collect();
                let total_sc = shortcuts.len();

                for (idx, sc) in shortcuts.into_iter().enumerate() {
                    let overlay = gtk4::Overlay::new();

                    let btn = gtk4::Button::builder()
                        .css_classes(["speed-dial-tile", "flat"])
                        .tooltip_text(&sc.url)
                        .build();

                    let card_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Vertical)
                        .spacing(8)
                        .halign(gtk4::Align::Center)
                        .valign(gtk4::Align::Center)
                        .build();

                    let icon_box = gtk4::Box::builder()
                        .css_classes(["speed-dial-icon"])
                        .halign(gtk4::Align::Center)
                        .valign(gtk4::Align::Center)
                        .build();

                    let first_char = sc.title.chars().next().unwrap_or('W').to_uppercase().to_string();
                    let letter_lbl = gtk4::Label::builder()
                        .label(&first_char)
                        .halign(gtk4::Align::Center)
                        .valign(gtk4::Align::Center)
                        .build();
                    icon_box.append(&letter_lbl);
                    card_box.append(&icon_box);

                    let title_lbl = gtk4::Label::builder()
                        .label(&sc.title)
                        .css_classes(["speed-dial-label"])
                        .ellipsize(gtk4::pango::EllipsizeMode::End)
                        .max_width_chars(11)
                        .halign(gtk4::Align::Center)
                        .build();
                    card_box.append(&title_lbl);

                    btn.set_child(Some(&card_box));

                    let target = sc.url.clone();
                    let on_nav_card = on_nav.clone();
                    btn.connect_clicked(move |_| {
                        if let Some(ref cb) = *on_nav_card.borrow() {
                            cb(&target);
                        }
                    });

                    overlay.set_child(Some(&btn));

                    // Discreet options button on top right of the tile
                    let more_btn = gtk4::Button::builder()
                        .icon_name("view-more-symbolic")
                        .css_classes(["flat", "circular", "speed-dial-more-btn"])
                        .halign(gtk4::Align::End)
                        .valign(gtk4::Align::Start)
                        .margin_top(4)
                        .margin_end(4)
                        .tooltip_text("Shortcut options")
                        .build();
                    more_btn.set_size_request(24, 24);
                    overlay.add_overlay(&more_btn);

                    // Context Popover Menu
                    let menu_pop = gtk4::Popover::builder()
                        .has_arrow(true)
                        .autohide(true)
                        .build();
                    menu_pop.set_parent(&more_btn);

                    let menu_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Vertical)
                        .spacing(4)
                        .margin_top(6)
                        .margin_bottom(6)
                        .margin_start(6)
                        .margin_end(6)
                        .width_request(160)
                        .build();

                    // --- Edit Popover & Button ---
                    let edit_pop = gtk4::Popover::builder()
                        .has_arrow(true)
                        .autohide(true)
                        .build();
                    edit_pop.set_parent(&btn);

                    let edit_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Vertical)
                        .spacing(8)
                        .margin_top(10)
                        .margin_bottom(10)
                        .margin_start(10)
                        .margin_end(10)
                        .width_request(240)
                        .build();

                    let edit_title_lbl = gtk4::Label::builder()
                        .label("Edit Shortcut")
                        .css_classes(["heading"])
                        .halign(gtk4::Align::Start)
                        .build();
                    edit_box.append(&edit_title_lbl);

                    let edit_title_entry = gtk4::Entry::builder().text(&sc.title).placeholder_text("Name").build();
                    let edit_url_entry = gtk4::Entry::builder().text(&sc.url).placeholder_text("URL").build();
                    edit_box.append(&edit_title_entry);
                    edit_box.append(&edit_url_entry);

                    let edit_save_btn = gtk4::Button::builder()
                        .label("Update")
                        .css_classes(["suggested-action"])
                        .halign(gtk4::Align::End)
                        .build();
                    edit_box.append(&edit_save_btn);
                    edit_pop.set_child(Some(&edit_box));

                    let sc_id = sc.id;
                    let st_edit = storage_add.clone();
                    let this_edit = this_clone.clone();
                    let pop_edit_ref = edit_pop.clone();
                    let e_title = edit_title_entry.clone();
                    let e_url = edit_url_entry.clone();
                    edit_save_btn.connect_clicked(move |_| {
                        let t = e_title.text().trim().to_string();
                        let mut u = e_url.text().trim().to_string();
                        if !t.is_empty() && !u.is_empty() {
                            if !u.starts_with("http://") && !u.starts_with("https://") {
                                u = format!("https://{u}");
                            }
                            let st = st_edit.clone();
                            let this_rf = this_edit.clone();
                            st.dispatch(move |db| {
                                let repo = ShortcutRepository::new(db.conn());
                                let _ = repo.update(sc_id, &t, &u);
                            });
                            pop_edit_ref.popdown();
                            let st_r = st_edit.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(100),
                                move || {
                                    this_rf.load_data(st_r);
                                },
                            );
                        }
                    });

                    let menu_pop_close_edit = menu_pop.clone();
                    let edit_pop_open = edit_pop.clone();
                    let edit_opt_btn = gtk4::Button::builder()
                        .label("Edit Shortcut…")
                        .icon_name("document-edit-symbolic")
                        .css_classes(["flat"])
                        .halign(gtk4::Align::Fill)
                        .build();
                    edit_opt_btn.connect_clicked(move |_| {
                        menu_pop_close_edit.popdown();
                        edit_pop_open.popup();
                    });
                    menu_box.append(&edit_opt_btn);

                    // --- Move Left Button (if idx > 0) ---
                    if idx > 0 {
                        let move_l_btn = gtk4::Button::builder()
                            .label("Move Left")
                            .icon_name("go-previous-symbolic")
                            .css_classes(["flat"])
                            .halign(gtk4::Align::Fill)
                            .build();

                        let mut new_order = sc_ids.clone();
                        new_order.swap(idx, idx - 1);
                        let st_reorder = storage_add.clone();
                        let this_reorder = this_clone.clone();
                        let menu_pop_close_ml = menu_pop.clone();
                        move_l_btn.connect_clicked(move |_| {
                            menu_pop_close_ml.popdown();
                            let order = new_order.clone();
                            let st = st_reorder.clone();
                            let this_rf = this_reorder.clone();
                            st.dispatch(move |db| {
                                let repo = ShortcutRepository::new(db.conn());
                                let _ = repo.reorder(&order);
                            });
                            let st_r = st_reorder.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(100),
                                move || {
                                    this_rf.load_data(st_r);
                                },
                            );
                        });
                        menu_box.append(&move_l_btn);
                    }

                    // --- Move Right Button (if idx + 1 < total_sc) ---
                    if idx + 1 < total_sc {
                        let move_r_btn = gtk4::Button::builder()
                            .label("Move Right")
                            .icon_name("go-next-symbolic")
                            .css_classes(["flat"])
                            .halign(gtk4::Align::Fill)
                            .build();

                        let mut new_order = sc_ids.clone();
                        new_order.swap(idx, idx + 1);
                        let st_reorder = storage_add.clone();
                        let this_reorder = this_clone.clone();
                        let menu_pop_close_mr = menu_pop.clone();
                        move_r_btn.connect_clicked(move |_| {
                            menu_pop_close_mr.popdown();
                            let order = new_order.clone();
                            let st = st_reorder.clone();
                            let this_rf = this_reorder.clone();
                            st.dispatch(move |db| {
                                let repo = ShortcutRepository::new(db.conn());
                                let _ = repo.reorder(&order);
                            });
                            let st_r = st_reorder.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(100),
                                move || {
                                    this_rf.load_data(st_r);
                                },
                            );
                        });
                        menu_box.append(&move_r_btn);
                    }

                    // --- Remove Button ---
                    let del_opt_btn = gtk4::Button::builder()
                        .label("Remove")
                        .icon_name("user-trash-symbolic")
                        .css_classes(["flat", "destructive-action"])
                        .halign(gtk4::Align::Fill)
                        .build();

                    let st_del = storage_add.clone();
                    let this_del = this_clone.clone();
                    let menu_pop_close_del = menu_pop.clone();
                    del_opt_btn.connect_clicked(move |_| {
                        menu_pop_close_del.popdown();
                        let st = st_del.clone();
                        let this_rf = this_del.clone();
                        st.dispatch(move |db| {
                            let repo = ShortcutRepository::new(db.conn());
                            let _ = repo.delete(sc_id);
                        });
                        let st_r = st_del.clone();
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(100),
                            move || {
                                this_rf.load_data(st_r);
                            },
                        );
                    });
                    menu_box.append(&del_opt_btn);
                    menu_pop.set_child(Some(&menu_box));

                    // Right click (secondary click) opens context menu
                    let gesture_rc = gtk4::GestureClick::new();
                    gesture_rc.set_button(3);
                    let menu_pop_rc = menu_pop.clone();
                    gesture_rc.connect_pressed(move |_, _, _, _| {
                        menu_pop_rc.popup();
                    });
                    btn.add_controller(gesture_rc);

                    // Clicking the "..." more button opens context menu
                    let menu_pop_more = menu_pop.clone();
                    more_btn.connect_clicked(move |_| {
                        menu_pop_more.popup();
                    });

                    sc_flow.append(&overlay);
                }

                // Add "+ Add Shortcut" card with input popover
                let add_btn = gtk4::Button::builder()
                    .css_classes(["speed-dial-tile", "flat"])
                    .tooltip_text("Add a new shortcut")
                    .build();

                let add_box = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Vertical)
                    .spacing(8)
                    .halign(gtk4::Align::Center)
                    .valign(gtk4::Align::Center)
                    .build();

                let add_icon_box = gtk4::Box::builder()
                    .css_classes(["speed-dial-icon"])
                    .halign(gtk4::Align::Center)
                    .valign(gtk4::Align::Center)
                    .build();

                let add_icon = gtk4::Image::from_icon_name("list-add-symbolic");
                add_icon.set_pixel_size(20);
                add_icon_box.append(&add_icon);
                add_box.append(&add_icon_box);

                let add_lbl = gtk4::Label::builder()
                    .label("Add Shortcut")
                    .css_classes(["speed-dial-label"])
                    .halign(gtk4::Align::Center)
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
                    .margin_top(10)
                    .margin_bottom(10)
                    .margin_start(10)
                    .margin_end(10)
                    .width_request(240)
                    .build();

                let pop_title = gtk4::Label::builder()
                    .label("Add Shortcut")
                    .css_classes(["heading"])
                    .halign(gtk4::Align::Start)
                    .build();
                pop_box.append(&pop_title);

                let title_entry = gtk4::Entry::builder()
                    .placeholder_text("Name (e.g. GitHub)")
                    .build();
                pop_box.append(&title_entry);

                let url_entry = gtk4::Entry::builder()
                    .placeholder_text("URL (https://...)")
                    .text("https://")
                    .build();
                pop_box.append(&url_entry);

                let save_btn = gtk4::Button::builder()
                    .label("Add")
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
                        std::time::Duration::from_millis(100),
                        move || {
                            refresh_self.load_data(st_reload);
                        },
                    );
                });

                sc_flow.append(&add_btn);

                // 2. Populate Recently Visited Sites
                while let Some(child) = rc_box.first_child() {
                    rc_box.remove(&child);
                }

                for item in recent {
                    let chip = gtk4::Button::builder()
                        .css_classes(["flat", "shadhin-recent-chip"])
                        .tooltip_text(&item.url)
                        .build();

                    let chip_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .spacing(6)
                        .build();

                    let chip_icon = gtk4::Image::from_icon_name("document-open-recent-symbolic");
                    chip_icon.set_pixel_size(14);
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
