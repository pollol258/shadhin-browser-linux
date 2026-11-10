//! Pill-shaped address bar with security shield and action buttons
//!
//! Hosts the URL entry, security status icon,
//! interactive bookmark star with folder popover, debounced autocomplete suggestions,
//! keyboard navigation (Arrow keys, Alt+Enter, Escape), and paste sanitization.

use crate::storage::StorageService;
use crate::storage::bookmarks::BookmarkRepository;
use crate::storage::history::HistoryRepository;
use crate::utils::debounce::Debounce;
use gtk4::gdk::{Key, ModifierType};
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct SuggestionItem {
    pub title: String,
    pub url: String,
    pub row_widget: gtk4::Box,
}

pub struct AddressBar {
    container: gtk4::Box,
    entry: gtk4::Entry,
    shield_icon: gtk4::Image,
    lock_btn: gtk4::Button,
    star_btn: gtk4::Button,
    star_icon: gtk4::Image,
    reader_btn: gtk4::Button,
    reload_btn: gtk4::Button,
    reload_icon: gtk4::Image,
    is_loading: Rc<RefCell<bool>>,
    on_activate: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>>,
    on_bookmark_changed: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    storage: RefCell<Option<StorageService>>,
    current_url: Rc<RefCell<String>>,
    current_title: Rc<RefCell<String>>,
    suggestions_popover: gtk4::Popover,
    suggestions_box: gtk4::Box,
    suggestions_items: Rc<RefCell<Vec<SuggestionItem>>>,
    selected_suggestion_idx: Rc<RefCell<Option<usize>>>,
    typed_text_backup: Rc<RefCell<String>>,
    autocomplete_debounce: Debounce,
    is_programmatic_change: Rc<RefCell<bool>>,
    was_focused_before_click: Rc<RefCell<bool>>,
    star_popover: Rc<RefCell<Option<gtk4::Popover>>>,
}

impl AddressBar {
    /// Constructs a new pill address bar
    pub fn new() -> Self {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();

        // 1. Pill horizontal box
        let pill_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .css_classes(["address-pill"])
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();

        let shield_icon = gtk4::Image::from_icon_name("security-low-symbolic");
        shield_icon.set_pixel_size(16);
        shield_icon.set_halign(gtk4::Align::Center);
        shield_icon.set_valign(gtk4::Align::Center);
        let lock_btn = gtk4::Button::builder()
            .child(&shield_icon)
            .tooltip_text("Site information and privacy")
            .css_classes(["iconbtn", "flat"])
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();
        lock_btn.set_size_request(28, 28);
        pill_box.append(&lock_btn);

        let entry = gtk4::Entry::builder()
            .placeholder_text("Search or enter address · বাংলায় খুঁজুন")
            .tooltip_text("Address and search bar · Type a URL or search query")
            .hexpand(true)
            .css_classes(["shadhin-address-entry"])
            .visible(true)
            .build();
        EditableExt::set_alignment(&entry, 0.5);
        entry.update_property(&[gtk4::accessible::Property::Label("Address and search bar")]);
        pill_box.append(&entry);

        let star_icon = gtk4::Image::from_icon_name("non-starred-symbolic");
        star_icon.set_pixel_size(16);
        star_icon.set_halign(gtk4::Align::Center);
        star_icon.set_valign(gtk4::Align::Center);
        let star_btn = gtk4::Button::builder()
            .child(&star_icon)
            .tooltip_text("Bookmark this page (Ctrl+D)")
            .css_classes(["iconbtn", "flat"])
            .valign(gtk4::Align::Center)
            .visible(false)
            .build();
        star_btn.set_size_request(28, 28);
        pill_box.append(&star_btn);

        let reader_icon = gtk4::Image::from_icon_name("view-reader-symbolic");
        reader_icon.set_pixel_size(16);
        reader_icon.set_halign(gtk4::Align::Center);
        reader_icon.set_valign(gtk4::Align::Center);
        let reader_btn = gtk4::Button::builder()
            .child(&reader_icon)
            .tooltip_text("Reader mode")
            .css_classes(["iconbtn", "flat"])
            .valign(gtk4::Align::Center)
            .visible(false)
            .build();
        reader_btn.set_size_request(28, 28);
        pill_box.append(&reader_btn);

        let reload_icon = gtk4::Image::from_icon_name("view-refresh-symbolic");
        reload_icon.set_pixel_size(16);
        reload_icon.set_halign(gtk4::Align::Center);
        reload_icon.set_valign(gtk4::Align::Center);
        let reload_btn = gtk4::Button::builder()
            .child(&reload_icon)
            .tooltip_text("Reload page (Ctrl+R)")
            .css_classes(["iconbtn", "flat"])
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();
        reload_btn.set_size_request(28, 28);
        pill_box.append(&reload_btn);

        container.append(&pill_box);

        // 2. Autocomplete suggestions popover
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
        suggestions_box.update_property(&[gtk4::accessible::Property::Label(
            "Search and address suggestions",
        )]);
        suggestions_popover.set_child(Some(&suggestions_box));

        let on_activate: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>> = Rc::new(RefCell::new(None));
        let on_bookmark_changed: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let current_url = Rc::new(RefCell::new(String::new()));
        let current_title = Rc::new(RefCell::new(String::new()));
        let autocomplete_debounce = Debounce::new();
        let is_programmatic_change = Rc::new(RefCell::new(false));
        let was_focused_before_click = Rc::new(RefCell::new(false));

        let suggestions_items: Rc<RefCell<Vec<SuggestionItem>>> = Rc::new(RefCell::new(Vec::new()));
        let selected_suggestion_idx: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));
        let typed_text_backup = Rc::new(RefCell::new(String::new()));

        let is_loading = Rc::new(RefCell::new(false));

        let addr = Self {
            container,
            entry,
            shield_icon,
            lock_btn,
            star_btn,
            star_icon,
            reader_btn,
            reload_btn,
            reload_icon,
            is_loading,
            on_activate,
            on_bookmark_changed,
            storage: RefCell::new(None),
            current_url,
            current_title,
            suggestions_popover,
            suggestions_box,
            suggestions_items,
            selected_suggestion_idx,
            typed_text_backup,
            autocomplete_debounce,
            is_programmatic_change,
            was_focused_before_click,
            star_popover: Rc::new(RefCell::new(None)),
        };

        addr.setup_star_popover();
        addr.setup_autocomplete();
        addr.setup_key_and_focus_controller();

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
        if !self.entry.has_focus() {
            EditableExt::set_alignment(&self.entry, 0.5);
        }
    }

    /// Checks if user is currently focused/typing in the address entry
    pub fn is_focused(&self) -> bool {
        self.entry.has_focus()
    }

    /// Triggers the bookmark star button (e.g. from Ctrl+D shortcut)
    pub fn trigger_bookmark(&self) {
        self.star_btn.set_visible(true);
        self.star_btn.emit_clicked();
    }

    /// Updates the text only if the user is not actively typing/focused
    pub fn set_text_if_unfocused(&self, text: &str) {
        if !self.entry.has_focus() {
            *self.is_programmatic_change.borrow_mut() = true;
            self.entry.set_text(text);
            EditableExt::set_alignment(&self.entry, 0.5);
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
            let entry_focus = self.entry.clone();

            storage.query_async(
                move |db| {
                    let repo = BookmarkRepository::new(db.conn());
                    repo.is_bookmarked(&url_check).unwrap_or(false)
                },
                move |is_bookmarked| {
                    if is_bookmarked {
                        star_icon.set_icon_name(Some("starred-symbolic"));
                        star_btn.add_css_class("active");
                        star_btn.set_visible(true);
                    } else {
                        star_icon.set_icon_name(Some("non-starred-symbolic"));
                        star_btn.remove_css_class("active");
                        star_btn.set_visible(entry_focus.has_focus());
                    }
                },
            );
        }
    }

    /// Retrieves the current text from the entry
    pub fn text(&self) -> String {
        self.entry.text().to_string()
    }

    /// Connects a listener notified when a bookmark is added or removed
    pub fn connect_bookmark_changed<F: Fn() + 'static>(&self, callback: F) {
        *self.on_bookmark_changed.borrow_mut() = Some(Box::new(callback));
    }

    /// Emits the bookmark changed notification
    pub fn emit_bookmark_changed(&self) {
        if let Some(ref cb) = *self.on_bookmark_changed.borrow() {
            cb();
        }
    }

    /// Updates the security shield icon indicator based on TLS validity
    pub fn set_security_state(&self, is_secure: bool) {
        if is_secure {
            self.set_security_status("channel-secure-symbolic", "Connection is secure (HTTPS)");
        } else {
            self.set_security_status(
                "channel-insecure-symbolic",
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
        EditableExt::set_alignment(&self.entry, 0.0);
        let ent = self.entry.clone();
        glib::idle_add_local_once(move || {
            ent.select_region(0, -1);
        });
    }

    /// Registers the callback invoked when the user submits text in the address bar
    /// Signature provides `(input_text, new_tab)` where `new_tab` is true for Alt+Enter
    pub fn connect_activate<F: Fn(&str, bool) + 'static>(&self, callback: F) {
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

    /// Accessor to the address entry
    pub fn entry(&self) -> &gtk4::Entry {
        &self.entry
    }

    pub fn lock_button(&self) -> &gtk4::Button {
        &self.lock_btn
    }

    pub fn reader_button(&self) -> &gtk4::Button {
        &self.reader_btn
    }

    /// Accessor to the reload button
    pub fn reload_button(&self) -> &gtk4::Button {
        &self.reload_btn
    }

    /// Toggles Reload / Stop appearance
    pub fn set_is_loading(&self, loading: bool) {
        *self.is_loading.borrow_mut() = loading;
        if loading {
            self.reload_icon
                .set_icon_name(Some("process-stop-symbolic"));
            self.reload_btn.set_tooltip_text(Some("Stop loading"));
        } else {
            self.reload_icon
                .set_icon_name(Some("view-refresh-symbolic"));
            self.reload_btn
                .set_tooltip_text(Some("Reload page (Ctrl+R)"));
        }
    }

    /// Connects the reload/stop button clicked signal
    pub fn connect_reload<F: Fn(bool) + 'static>(&self, callback: F) {
        let is_loading = self.is_loading.clone();
        self.reload_btn.connect_clicked(move |_| {
            let loading = *is_loading.borrow();
            callback(loading);
        });
    }

    /// Closes the suggestions popover if open
    pub fn close_suggestions(&self) {
        self.suggestions_popover.popdown();
    }

    /// Closes the star bookmark popover if open
    pub fn close_star_popover(&self) {
        if let Some(ref pop) = *self.star_popover.borrow() {
            pop.popdown();
        }
    }

    /// Explicitly unparents suggestions and bookmark popovers
    pub fn cleanup(&self) {
        self.close_suggestions();
        self.suggestions_popover.unparent();
        if let Some(pop) = self.star_popover.borrow_mut().take() {
            pop.popdown();
            pop.unparent();
        }
    }

    /// Accessor to the suggestions popover
    pub fn suggestions_popover(&self) -> &gtk4::Popover {
        &self.suggestions_popover
    }

    fn setup_key_and_focus_controller(&self) {
        let entry = self.entry.clone();
        let popover = self.suggestions_popover.clone();
        let on_act = self.on_activate.clone();
        let cur_url = self.current_url.clone();
        let is_prog = self.is_programmatic_change.clone();
        let items_cell = self.suggestions_items.clone();
        let sel_idx_cell = self.selected_suggestion_idx.clone();
        let backup_text_cell = self.typed_text_backup.clone();

        // 1. Key controller for Enter, Alt+Enter, Escape, and Up/Down suggestion selection
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let entry_k = entry.clone();
        let pop_k = popover.clone();
        let is_prog_k = is_prog.clone();
        let items_k = items_cell.clone();
        let sel_idx_k = sel_idx_cell.clone();
        let backup_k = backup_text_cell.clone();
        let on_act_k = on_act.clone();
        let cur_url_k = cur_url.clone();

        key_controller.connect_key_pressed(move |_, key, _code, state| {
            let alt = state.contains(ModifierType::ALT_MASK);
            let ctrl = state.contains(ModifierType::CONTROL_MASK);

            // Enter, Alt+Enter, or Ctrl+Enter
            if key == Key::Return || key == Key::KP_Enter || key == Key::ISO_Enter {
                pop_k.popdown();
                let mut text = {
                    let items = items_k.borrow();
                    if let Some(idx) = *sel_idx_k.borrow()
                        && let Some(item) = items.get(idx)
                    {
                        item.url.clone()
                    } else {
                        entry_k.text().trim().to_string()
                    }
                };

                // Ctrl+Enter on bare word expands to https://www.word.com
                if ctrl && !text.is_empty() {
                    let expanded = crate::browser::navigation::expand_ctrl_enter(&text);
                    if expanded != text {
                        text = expanded;
                        *is_prog_k.borrow_mut() = true;
                        entry_k.set_text(&text);
                        *is_prog_k.borrow_mut() = false;
                    }
                }

                tracing::info!("AddressBar: Key::Return captured in Capture phase: '{text}' (alt={alt})");
                if !text.is_empty() {
                    if let Some(ref cb) = *on_act_k.borrow() {
                        cb(&text, alt);
                    } else {
                        tracing::warn!("AddressBar: on_activate callback is None!");
                    }
                }
                if !alt
                    && let Some(root) = entry_k.root()
                    && let Ok(win) = root.downcast::<gtk4::Window>()
                {
                    gtk4::prelude::RootExt::set_focus(&win, None::<&gtk4::Widget>);
                }
                return glib::Propagation::Stop;
            }

            // Down Arrow: Move suggestion selection downwards
            if key == Key::Down {
                let items = items_k.borrow();
                if !items.is_empty() {
                    let curr = *sel_idx_k.borrow();
                    let next = match curr {
                        None => {
                            *backup_k.borrow_mut() = entry_k.text().to_string();
                            Some(0)
                        }
                        Some(idx) if idx + 1 < items.len() => Some(idx + 1),
                        Some(_) => None, // cycle back to user typed text
                    };
                    *sel_idx_k.borrow_mut() = next;

                    for (i, item) in items.iter().enumerate() {
                        if Some(i) == next {
                            item.row_widget.add_css_class("selected");
                        } else {
                            item.row_widget.remove_css_class("selected");
                        }
                    }

                    *is_prog_k.borrow_mut() = true;
                    if let Some(idx) = next {
                        entry_k.set_text(&items[idx].url);
                    } else {
                        entry_k.set_text(&backup_k.borrow());
                    }
                    *is_prog_k.borrow_mut() = false;

                    return glib::Propagation::Stop;
                }
            }

            // Up Arrow: Move suggestion selection upwards
            if key == Key::Up {
                let items = items_k.borrow();
                if !items.is_empty() {
                    let curr = *sel_idx_k.borrow();
                    let prev = match curr {
                        None => {
                            *backup_k.borrow_mut() = entry_k.text().to_string();
                            Some(items.len() - 1)
                        }
                        Some(0) => None, // cycle back to user typed text
                        Some(idx) => Some(idx - 1),
                    };
                    *sel_idx_k.borrow_mut() = prev;

                    for (i, item) in items.iter().enumerate() {
                        if Some(i) == prev {
                            item.row_widget.add_css_class("selected");
                        } else {
                            item.row_widget.remove_css_class("selected");
                        }
                    }

                    *is_prog_k.borrow_mut() = true;
                    if let Some(idx) = prev {
                        entry_k.set_text(&items[idx].url);
                    } else {
                        entry_k.set_text(&backup_k.borrow());
                    }
                    *is_prog_k.borrow_mut() = false;

                    return glib::Propagation::Stop;
                }
            }

            // Escape: first dismiss suggestions / revert text, second unfocus entry
            if key == Key::Escape {
                // If suggestions popover is currently active
                if pop_k.is_visible() {
                    pop_k.popdown();
                    if !backup_k.borrow().is_empty() {
                        *is_prog_k.borrow_mut() = true;
                        entry_k.set_text(&backup_k.borrow());
                        *is_prog_k.borrow_mut() = false;
                    }
                    return glib::Propagation::Stop;
                }

                // If text was modified from the current page URL, revert text
                let page_url = cur_url_k.borrow().clone();
                let current_input = entry_k.text().to_string();
                if current_input != page_url && !page_url.is_empty() {
                    *is_prog_k.borrow_mut() = true;
                    entry_k.set_text(&page_url);
                    *is_prog_k.borrow_mut() = false;
                    return glib::Propagation::Stop;
                }

                // Second Escape: unfocus address bar
                if let Some(root) = entry_k.root()
                    && let Ok(win) = root.downcast::<gtk4::Window>()
                {
                    gtk4::prelude::RootExt::set_focus(&win, None::<&gtk4::Widget>);
                }
                return glib::Propagation::Stop;
            }

            glib::Propagation::Proceed
        });
        self.entry.add_controller(key_controller);

        // Direct GtkEntry activate signal connection as canonical submit fallback
        let on_act_native = on_act.clone();
        let items_native = items_cell.clone();
        let sel_native = sel_idx_cell.clone();
        let pop_native = popover.clone();
        self.entry.connect_activate(move |entry| {
            pop_native.popdown();
            let text = {
                let items = items_native.borrow();
                if let Some(idx) = *sel_native.borrow()
                    && let Some(item) = items.get(idx)
                {
                    item.url.clone()
                } else {
                    entry.text().trim().to_string()
                }
            };
            tracing::info!("AddressBar: GtkEntry 'activate' signal triggered: '{text}'");
            if !text.is_empty() {
                if let Some(ref cb) = *on_act_native.borrow() {
                    cb(&text, false);
                } else {
                    tracing::warn!("AddressBar: activate signal received but on_activate callback is None!");
                }
            }
            if let Some(root) = entry.root()
                && let Ok(win) = root.downcast::<gtk4::Window>()
            {
                gtk4::prelude::RootExt::set_focus(&win, None::<&gtk4::Widget>);
            }
        });

        // 2. Click behavior: first click selects all, subsequent clicks place cursor
        let gesture_click = gtk4::GestureClick::new();
        gesture_click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let was_foc = self.was_focused_before_click.clone();
        let entry_c = self.entry.clone();
        gesture_click.connect_pressed(move |_, _, _, _| {
            let already_focused = *was_foc.borrow();
            if !already_focused {
                let ent = entry_c.clone();
                glib::idle_add_local_once(move || {
                    ent.select_region(0, -1);
                });
            }
        });
        self.entry.add_controller(gesture_click);

        // 3. Focus tracking and blur restore behavior
        let cur_url_f = self.current_url.clone();
        let entry_f = self.entry.clone();
        let is_prog_f = self.is_programmatic_change.clone();
        let was_foc_notify = self.was_focused_before_click.clone();
        let pop_f = self.suggestions_popover.clone();
        let deb_f = self.autocomplete_debounce.clone();
        let star_btn_f = self.star_btn.clone();

        self.entry.connect_has_focus_notify(move |e| {
            if e.has_focus() {
                *was_foc_notify.borrow_mut() = true;
                EditableExt::set_alignment(e, 0.0);
                star_btn_f.set_visible(true);
            } else {
                *was_foc_notify.borrow_mut() = false;
                EditableExt::set_alignment(e, 0.5);
                if !star_btn_f.has_css_class("active") {
                    star_btn_f.set_visible(false);
                }
                deb_f.cancel();
                pop_f.popdown();

                // Clicking away without submitting restores the active page URL
                let active_url = cur_url_f.borrow().clone();
                if !active_url.is_empty() && e.text() != active_url {
                    *is_prog_f.borrow_mut() = true;
                    entry_f.set_text(&active_url);
                    *is_prog_f.borrow_mut() = false;
                    EditableExt::set_alignment(&entry_f, 0.5);
                }
            }
        });
    }

    fn setup_star_popover(&self) {
        let popover = gtk4::Popover::builder()
            .has_arrow(true)
            .autohide(true)
            .build();
        popover.set_parent(&self.star_btn);
        *self.star_popover.borrow_mut() = Some(popover.clone());

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
        let entry_bm_remove = self.entry.clone();
        let url_remove = url_cur.clone();
        let storage_ref = self.storage.borrow().clone();
        let on_bm_remove = self.on_bookmark_changed.clone();

        remove_btn.connect_clicked(move |_| {
            let url = url_remove.borrow().clone();
            let on_bm = on_bm_remove.clone();
            if let Some(ref st) = storage_ref {
                st.query_async(
                    move |db| {
                        let repo = BookmarkRepository::new(db.conn());
                        let _ = repo.remove_bookmark_by_url(&url);
                    },
                    move |_| {
                        if let Some(ref cb) = *on_bm.borrow() {
                            cb();
                        }
                    },
                );
            }
            star_icon.set_icon_name(Some("non-starred-symbolic"));
            star_btn.remove_css_class("active");
            if !entry_bm_remove.has_focus() {
                star_btn.set_visible(false);
            }
            pop_close.popdown();
        });

        let pop_close_done = popover;
        let star_icon_done = self.star_icon.clone();
        let star_btn_done = self.star_btn.clone();
        let url_save = url_cur;
        let storage_ref_done = self.storage.borrow().clone();
        let on_bm_done = self.on_bookmark_changed.clone();

        done_btn.connect_clicked(move |_| {
            let url = url_save.borrow().clone();
            let name = name_entry.text().to_string();
            let on_bm = on_bm_done.clone();
            if let Some(ref st) = storage_ref_done {
                st.query_async(
                    move |db| {
                        let repo = BookmarkRepository::new(db.conn());
                        let _ = repo.add_bookmark(&name, &url, None, None, true);
                    },
                    move |_| {
                        if let Some(ref cb) = *on_bm.borrow() {
                            cb();
                        }
                    },
                );
            }
            star_icon_done.set_icon_name(Some("starred-symbolic"));
            star_btn_done.add_css_class("active");
            star_btn_done.set_visible(true);
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
        let items_cell = self.suggestions_items.clone();
        let sel_idx_cell = self.selected_suggestion_idx.clone();
        let shield_icon = self.shield_icon.clone();

        self.entry.connect_changed(move |e| {
            // Guard: programmatic updates must not trigger autocomplete
            if *is_prog.borrow() {
                debounce.cancel();
                popover.popdown();
                return;
            }

            // Paste and insertion cleaning: sanitize newlines/tabs and prevent > 10,000 chars freeze
            let raw_text = e.text();
            if raw_text.contains('\r')
                || raw_text.contains('\n')
                || raw_text.contains('\t')
                || raw_text.len() > 10000
            {
                let sanitized: String = raw_text
                    .chars()
                    .filter(|&c| c != '\r' && c != '\n')
                    .map(|c| if c == '\t' { ' ' } else { c })
                    .take(10000)
                    .collect();
                if sanitized != raw_text {
                    *is_prog.borrow_mut() = true;
                    e.set_text(&sanitized);
                    e.set_position(sanitized.len() as i32);
                    *is_prog.borrow_mut() = false;
                    return;
                }
            }

            // Guard: do not show autocomplete if user is not actively focused on the entry
            if !e.has_focus() {
                debounce.cancel();
                popover.popdown();
                return;
            }

            // While user is actively typing, indicator displays neutral search icon
            shield_icon.set_icon_name(Some("system-search-symbolic"));
            shield_icon.set_tooltip_text(Some("Search or enter address"));

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
            let items_inner = items_cell.clone();
            let sel_idx_inner = sel_idx_cell.clone();

            // Debounce autocomplete search at ~100ms
            debounce.debounce(Duration::from_millis(100), move || {
                if !entry_inner.has_focus() {
                    popover_inner.popdown();
                    return;
                }

                while let Some(child) = list_box_inner.first_child() {
                    list_box_inner.remove(&child);
                }
                items_inner.borrow_mut().clear();
                *sel_idx_inner.borrow_mut() = None;

                if let Some(ref st) = storage_inner {
                    let q = query.clone();
                    let pop_final = popover_inner.clone();
                    let box_final = list_box_inner.clone();
                    let on_act_final = on_act_inner.clone();
                    let entry_final = entry_inner.clone();
                    let items_final = items_inner.clone();

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
                            if !entry_final.has_focus() {
                                pop_final.popdown();
                                return;
                            }
                            let cur_text = entry_final.text().trim().to_string();
                            if cur_text.is_empty() {
                                pop_final.popdown();
                                return;
                            }

                            let mut items_vec = Vec::new();

                            // 1. Search engine query item
                            let search_url = format!(
                                "https://duckduckgo.com/?q={}",
                                crate::browser::navigation::percent_encode_query(&query)
                            );
                            let search_title = format!("Search DuckDuckGo for \"{}\"", query);
                            let search_row = Self::create_suggestion_row(
                                "system-search-symbolic",
                                &search_title,
                                &search_url,
                                pop_final.clone(),
                                on_act_final.clone(),
                            );
                            box_final.append(&search_row);
                            items_vec.push(SuggestionItem {
                                title: search_title,
                                url: search_url,
                                row_widget: search_row,
                            });

                            // 2. Bookmarks matches (ranked)
                            for bm in bookmarks.iter().take(3) {
                                let row = Self::create_suggestion_row(
                                    "starred-symbolic",
                                    &bm.title,
                                    &bm.url,
                                    pop_final.clone(),
                                    on_act_final.clone(),
                                );
                                box_final.append(&row);
                                items_vec.push(SuggestionItem {
                                    title: bm.title.clone(),
                                    url: bm.url.clone(),
                                    row_widget: row,
                                });
                            }

                            // 3. History matches (ranked by frequency and recency)
                            for h in history {
                                let row = Self::create_suggestion_row(
                                    "document-open-recent-symbolic",
                                    &h.title,
                                    &h.url,
                                    pop_final.clone(),
                                    on_act_final.clone(),
                                );
                                box_final.append(&row);
                                items_vec.push(SuggestionItem {
                                    title: h.title.clone(),
                                    url: h.url.clone(),
                                    row_widget: row,
                                });
                            }

                            *items_final.borrow_mut() = items_vec;
                            pop_final.popup();
                        },
                    );
                }
            });
        });
    }

    fn create_suggestion_row(
        icon_name: &str,
        title: &str,
        url: &str,
        popover: gtk4::Popover,
        on_act: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>>,
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
        let popover_action = popover;
        gesture.connect_pressed(move |_, _, _, _| {
            if let Some(root) = popover_action.root()
                && let Ok(win) = root.downcast::<gtk4::Window>()
            {
                gtk4::prelude::RootExt::set_focus(&win, None::<&gtk4::Widget>);
            }
            popover_action.popdown();
            if let Some(ref cb) = *on_act.borrow() {
                cb(&target_url, false);
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
