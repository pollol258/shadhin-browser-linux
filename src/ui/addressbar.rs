//! Pill-shaped address bar with security shield and action buttons
//!
//! Hosts the URL entry, real-time load progress bar, security status icon,
//! interactive bookmark star with folder popover, and debounced autocomplete suggestions.

use crate::storage::StorageService;
use crate::storage::bookmarks::BookmarkRepository;
use crate::storage::history::HistoryRepository;
use crate::utils::debounce::Debounce;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub struct AddressBar {
    container: gtk4::Box,
    entry: gtk4::Entry,
    progress_bar: gtk4::ProgressBar,
    shield_icon: gtk4::Image,
    lock_btn: gtk4::Button,
    star_btn: gtk4::Button,
    star_icon: gtk4::Image,
    reader_btn: gtk4::Button,
    on_activate: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    storage: RefCell<Option<StorageService>>,
    current_url: Rc<RefCell<String>>,
    current_title: Rc<RefCell<String>>,
    suggestions_popover: gtk4::Popover,
    suggestions_box: gtk4::Box,
    autocomplete_debounce: Debounce,
    progress_hide_debounce: Debounce,
    is_programmatic_change: Rc<RefCell<bool>>,
}

impl AddressBar {
    /// Constructs a new pill address bar
    pub fn new() -> Self {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .build();

        // 1. Pill horizontal box
        let pill_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .css_classes(["address-pill"])
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .build();

        let shield_icon = gtk4::Image::from_icon_name("channel-secure-symbolic");
        shield_icon.set_pixel_size(17);
        let lock_btn = gtk4::Button::builder()
            .child(&shield_icon)
            .tooltip_text("Site information and privacy")
            .css_classes(["iconbtn", "flat"])
            .valign(gtk4::Align::Center)
            .build();
        pill_box.append(&lock_btn);

        let entry = gtk4::Entry::builder()
            .placeholder_text("Search or enter address · বাংলায় খুঁজুন")
            .hexpand(true)
            .css_classes(["shadhin-address-entry"])
            .build();
        pill_box.append(&entry);

        let star_icon = gtk4::Image::from_icon_name("non-starred-symbolic");
        star_icon.set_pixel_size(17);
        let star_btn = gtk4::Button::builder()
            .child(&star_icon)
            .tooltip_text("Bookmark this page")
            .css_classes(["iconbtn", "flat"])
            .valign(gtk4::Align::Center)
            .build();
        pill_box.append(&star_btn);

        let reader_icon = gtk4::Image::from_icon_name("view-reader-symbolic");
        reader_icon.set_pixel_size(17);
        let reader_btn = gtk4::Button::builder()
            .child(&reader_icon)
            .tooltip_text("Reader mode")
            .css_classes(["iconbtn", "flat"])
            .valign(gtk4::Align::Center)
            .build();
        pill_box.append(&reader_btn);

        container.append(&pill_box);

        // 2. Thin progress bar underneath the pill
        let progress_bar = gtk4::ProgressBar::builder()
            .css_classes(["shadhin-progress-bar"])
            .fraction(0.0)
            .visible(false)
            .build();
        container.append(&progress_bar);

        // 3. Autocomplete suggestions popover
        let suggestions_popover = gtk4::Popover::builder()
            .has_arrow(false)
            .autohide(true)
            .can_focus(false)
            .focus_on_click(false)
            .css_classes(["shadhin-suggestions-popover"])
            .build();
        suggestions_popover.set_parent(&entry);

        let suggestions_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(2)
            .width_request(450)
            .build();
        suggestions_popover.set_child(Some(&suggestions_box));

        let on_activate: Rc<RefCell<Option<Box<dyn Fn(&str)>>>> = Rc::new(RefCell::new(None));
        let on_act_clone = on_activate.clone();
        let popover_close = suggestions_popover.clone();
        entry.connect_activate(move |e| {
            popover_close.popdown();
            let text = e.text().trim().to_string();
            if !text.is_empty()
                && let Some(ref cb) = *on_act_clone.borrow()
            {
                cb(&text);
            }
        });

        let current_url = Rc::new(RefCell::new(String::new()));
        let current_title = Rc::new(RefCell::new(String::new()));
        let autocomplete_debounce = Debounce::new();
        let progress_hide_debounce = Debounce::new();
        let is_programmatic_change = Rc::new(RefCell::new(false));

        let addr = Self {
            container,
            entry,
            progress_bar,
            shield_icon,
            lock_btn,
            star_btn,
            star_icon,
            reader_btn,
            on_activate,
            storage: RefCell::new(None),
            current_url,
            current_title,
            suggestions_popover,
            suggestions_box,
            autocomplete_debounce,
            progress_hide_debounce,
            is_programmatic_change,
        };

        addr.setup_star_popover();
        addr.setup_autocomplete();

        addr
    }

    /// Associates storage service for bookmarks and history autocomplete
    pub fn set_storage(&self, storage: StorageService) {
        *self.storage.borrow_mut() = Some(storage);
    }

    /// Sets the URL or query text in the entry programmatically
    pub fn set_text(&self, text: &str) {
        *self.is_programmatic_change.borrow_mut() = true;
        self.entry.set_text(text);
        *self.is_programmatic_change.borrow_mut() = false;
    }

    /// Checks if user is currently focused/typing in the address entry
    pub fn is_focused(&self) -> bool {
        self.entry.has_focus()
    }

    /// Triggers the bookmark star button (e.g. from Ctrl+D shortcut)
    pub fn trigger_bookmark(&self) {
        self.star_btn.emit_clicked();
    }

    /// Updates the text only if the user is not actively typing/focused
    pub fn set_text_if_unfocused(&self, text: &str) {
        if !self.entry.has_focus() {
            *self.is_programmatic_change.borrow_mut() = true;
            self.entry.set_text(text);
            *self.is_programmatic_change.borrow_mut() = false;
        }
    }

    /// Updates current URL and title, and synchronizes the starred icon
    pub fn update_current_page(&self, url: &str, title: &str) {
        *self.current_url.borrow_mut() = url.to_string();
        *self.current_title.borrow_mut() = title.to_string();

        if let Some(ref storage) = *self.storage.borrow() {
            let url_check = url.to_string();
            let star_icon = self.star_icon.clone();
            let star_btn = self.star_btn.clone();

            storage.query_async(
                move |db| {
                    let repo = BookmarkRepository::new(db.conn());
                    repo.is_bookmarked(&url_check).unwrap_or(false)
                },
                move |is_bookmarked| {
                    if is_bookmarked {
                        star_icon.set_icon_name(Some("starred-symbolic"));
                        star_btn.add_css_class("active");
                    } else {
                        star_icon.set_icon_name(Some("non-starred-symbolic"));
                        star_btn.remove_css_class("active");
                    }
                },
            );
        }
    }

    /// Retrieves the current text from the entry
    pub fn text(&self) -> String {
        self.entry.text().to_string()
    }

    /// Sets the loading progress (0.0 to 1.0) with smooth hiding
    pub fn set_progress(&self, fraction: f64) {
        if fraction >= 1.0 {
            self.progress_bar.set_fraction(1.0);
            let pb = self.progress_bar.clone();
            self.progress_hide_debounce
                .debounce(Duration::from_millis(250), move || {
                    pb.set_visible(false);
                    pb.set_fraction(0.0);
                });
        } else if fraction <= 0.0 {
            self.progress_hide_debounce.cancel();
            self.progress_bar.set_visible(false);
            self.progress_bar.set_fraction(0.0);
        } else {
            self.progress_hide_debounce.cancel();
            self.progress_bar.set_visible(true);
            self.progress_bar.set_fraction(fraction);
        }
    }

    /// Updates the security shield icon indicator
    pub fn set_security_state(&self, is_secure: bool) {
        if is_secure {
            self.set_security_status("security-high-symbolic", "Connection is secure (HTTPS)");
        } else {
            self.set_security_status(
                "security-low-symbolic",
                "Not secure: Insecure connection (HTTP)",
            );
        }
    }

    /// Updates the security icon and tooltip explicitly
    pub fn set_security_status(&self, icon_name: &str, tooltip: &str) {
        self.shield_icon.set_icon_name(Some(icon_name));
        self.shield_icon.set_tooltip_text(Some(tooltip));
    }

    /// Focuses the address entry and selects all text
    pub fn focus(&self) {
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    /// Registers the callback invoked when the user submits text in the address bar
    pub fn connect_activate<F: Fn(&str) + 'static>(&self, callback: F) {
        *self.on_activate.borrow_mut() = Some(Box::new(callback));
    }

    /// Accessor to the root address bar widget
    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    /// Accessor to the star button
    pub fn star_button(&self) -> &gtk4::Button {
        &self.star_btn
    }

    fn setup_star_popover(&self) {
        let popover = gtk4::Popover::builder()
            .has_arrow(true)
            .autohide(true)
            .build();
        popover.set_parent(&self.star_btn);

        let pop_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .width_request(280)
            .build();

        let title_lbl = gtk4::Label::builder()
            .label("Edit Bookmark")
            .css_classes(["heading"])
            .halign(gtk4::Align::Start)
            .build();
        pop_box.append(&title_lbl);

        let name_entry = gtk4::Entry::builder().placeholder_text("Name").build();
        pop_box.append(&name_entry);

        let folder_combo = gtk4::DropDown::from_strings(&["Bookmarks Bar", "Other Bookmarks"]);
        pop_box.append(&folder_combo);

        let btn_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk4::Align::End)
            .margin_top(4)
            .build();

        let remove_btn = gtk4::Button::builder()
            .label("Remove")
            .css_classes(["destructive-action", "flat"])
            .build();
        btn_row.append(&remove_btn);

        let done_btn = gtk4::Button::builder()
            .label("Done")
            .css_classes(["suggested-action"])
            .build();
        btn_row.append(&done_btn);

        pop_box.append(&btn_row);
        popover.set_child(Some(&pop_box));

        let url_cur = self.current_url.clone();
        let title_cur = self.current_title.clone();
        let name_entry_clone = name_entry.clone();
        let popover_show = popover.clone();

        self.star_btn.connect_clicked(move |_| {
            name_entry_clone.set_text(&title_cur.borrow());
            popover_show.popup();
        });

        let pop_close = popover.clone();
        let star_icon = self.star_icon.clone();
        let star_btn = self.star_btn.clone();
        let url_remove = url_cur.clone();
        let storage_ref = self.storage.borrow().clone();

        remove_btn.connect_clicked(move |_| {
            let url = url_remove.borrow().clone();
            if let Some(ref st) = storage_ref {
                st.dispatch(move |db| {
                    let repo = BookmarkRepository::new(db.conn());
                    let _ = repo.remove_bookmark_by_url(&url);
                });
            }
            star_icon.set_icon_name(Some("non-starred-symbolic"));
            star_btn.remove_css_class("active");
            pop_close.popdown();
        });

        let pop_close_done = popover;
        let star_icon_done = self.star_icon.clone();
        let star_btn_done = self.star_btn.clone();
        let url_save = url_cur;
        let storage_ref_done = self.storage.borrow().clone();

        done_btn.connect_clicked(move |_| {
            let url = url_save.borrow().clone();
            let name = name_entry.text().to_string();
            if let Some(ref st) = storage_ref_done {
                st.dispatch(move |db| {
                    let repo = BookmarkRepository::new(db.conn());
                    let _ = repo.add_bookmark(&name, &url, None, None, true);
                });
            }
            star_icon_done.set_icon_name(Some("starred-symbolic"));
            star_btn_done.add_css_class("active");
            pop_close_done.popdown();
        });
    }

    fn setup_autocomplete(&self) {
        let entry_clone = self.entry.clone();
        let popover = self.suggestions_popover.clone();
        let list_box = self.suggestions_box.clone();
        let debounce = self.autocomplete_debounce.clone();
        let on_act = self.on_activate.clone();
        let storage_ref = self.storage.borrow().clone();
        let is_prog = self.is_programmatic_change.clone();

        // 1. Immediately close suggestions and cancel timer if entry loses focus
        let pop_focus = popover.clone();
        let deb_focus = debounce.clone();
        self.entry.connect_has_focus_notify(move |e| {
            if !e.has_focus() {
                deb_focus.cancel();
                pop_focus.popdown();
            }
        });

        // 2. React only to actual user typing in the entry
        self.entry.connect_changed(move |e| {
            // Guard: programmatic updates (e.g. navigation, tab switches) must never trigger autocomplete
            if *is_prog.borrow() {
                debounce.cancel();
                popover.popdown();
                return;
            }

            // Guard: do not show autocomplete if user is not actively focused on the entry
            if !e.has_focus() {
                debounce.cancel();
                popover.popdown();
                return;
            }

            let query = e.text().trim().to_string();
            if query.is_empty() {
                debounce.cancel();
                popover.popdown();
                return;
            }

            let popover_inner = popover.clone();
            let list_box_inner = list_box.clone();
            let on_act_inner = on_act.clone();
            let storage_inner = storage_ref.clone();
            let entry_inner = entry_clone.clone();

            debounce.debounce(Duration::from_millis(150), move || {
                // Verify entry still has focus when timer fires
                if !entry_inner.has_focus() {
                    popover_inner.popdown();
                    return;
                }

                while let Some(child) = list_box_inner.first_child() {
                    list_box_inner.remove(&child);
                }

                if let Some(ref st) = storage_inner {
                    let q = query.clone();
                    let pop_final = popover_inner.clone();
                    let box_final = list_box_inner.clone();
                    let on_act_final = on_act_inner.clone();
                    let entry_final = entry_inner.clone();

                    st.query_async(
                        move |db| {
                            let repo_h = HistoryRepository::new(db.conn());
                            let repo_b = BookmarkRepository::new(db.conn());
                            let history_matches =
                                repo_h.search_autocomplete(&q, 4).unwrap_or_default();
                            let bookmark_matches = repo_b.search_bookmarks(&q).unwrap_or_default();
                            (history_matches, bookmark_matches)
                        },
                        move |(history, bookmarks)| {
                            // Ensure entry is still focused before showing popover
                            if !entry_final.has_focus() {
                                pop_final.popdown();
                                return;
                            }
                            let cur_text = entry_final.text().trim().to_string();
                            if cur_text.is_empty() {
                                pop_final.popdown();
                                return;
                            }

                            // 1. Search engine query item
                            let search_url = format!(
                                "https://duckduckgo.com/?q={}",
                                glib::Uri::escape_string(&query, None, false)
                            );
                            let search_row = Self::create_suggestion_row(
                                "system-search-symbolic",
                                &format!("Search DuckDuckGo for \"{}\"", query),
                                &search_url,
                                pop_final.clone(),
                                on_act_final.clone(),
                            );
                            box_final.append(&search_row);

                            // 2. Bookmarks matches
                            for bm in bookmarks.iter().take(3) {
                                let row = Self::create_suggestion_row(
                                    "starred-symbolic",
                                    &bm.title,
                                    &bm.url,
                                    pop_final.clone(),
                                    on_act_final.clone(),
                                );
                                box_final.append(&row);
                            }

                            // 3. History matches
                            for h in history {
                                let row = Self::create_suggestion_row(
                                    "document-open-recent-symbolic",
                                    &h.title,
                                    &h.url,
                                    pop_final.clone(),
                                    on_act_final.clone(),
                                );
                                box_final.append(&row);
                            }

                            pop_final.popup();
                        },
                    );
                }
            });
        });
    }

    pub fn lock_button(&self) -> &gtk4::Button {
        &self.lock_btn
    }

    pub fn reader_button(&self) -> &gtk4::Button {
        &self.reader_btn
    }

    pub fn set_tls_state(&self, has_tls: bool, valid: bool) {
        if !has_tls {
            self.shield_icon.set_icon_name(Some("security-high-symbolic"));
            self.shield_icon.set_tooltip_text(Some("No secure site connection"));
        } else if valid {
            self.shield_icon.set_icon_name(Some("channel-secure-symbolic"));
            self.shield_icon.set_tooltip_text(Some("Connection secure"));
        } else {
            self.shield_icon.set_icon_name(Some("channel-insecure-symbolic"));
            self.shield_icon.set_tooltip_text(Some("Certificate error: connection insecure"));
        }
    }

    fn create_suggestion_row(
        icon_name: &str,
        title: &str,
        url: &str,
        popover: gtk4::Popover,
        on_act: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    ) -> gtk4::Box {
        let row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["shadhin-suggestion-row"])
            .build();

        let icon = gtk4::Image::from_icon_name(icon_name);
        row.append(&icon);

        let title_lbl = gtk4::Label::builder()
            .label(title)
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        row.append(&title_lbl);

        let url_lbl = gtk4::Label::builder()
            .label(url)
            .css_classes(["dim-label"])
            .halign(gtk4::Align::End)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(30)
            .build();
        row.append(&url_lbl);

        let target_url = url.to_string();
        let gesture = gtk4::GestureClick::new();
        gesture.connect_pressed(move |_, _, _, _| {
            popover.popdown();
            if let Some(ref cb) = *on_act.borrow() {
                cb(&target_url);
            }
        });
        row.add_controller(gesture);

        row
    }
}

impl Default for AddressBar {
    fn default() -> Self {
        Self::new()
    }
}
