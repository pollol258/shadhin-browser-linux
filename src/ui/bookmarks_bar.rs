//! Horizontal bookmarks bar located beneath the navigation toolbar
//!
//! Provides quick-access bookmark chips for frequently visited sites,
//! synchronized with the bookmarks storage repository.

use crate::storage::StorageService;
use crate::storage::bookmarks::{BookmarkItem, BookmarkRepository};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct BookmarksBar {
    container: gtk4::Box,
    chips_box: gtk4::Box,
    on_navigate: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    storage: StorageService,
}

impl BookmarksBar {
    pub fn new(storage: StorageService) -> Self {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .css_classes(["shadhin-bookmarks-bar"])
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();
        container.set_size_request(-1, 32);

        let folder_icon = gtk4::Image::from_icon_name("user-bookmarks-symbolic");
        folder_icon.set_pixel_size(14);
        folder_icon.set_opacity(0.7);
        folder_icon.set_visible(true);
        container.append(&folder_icon);

        let chips_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .hexpand(true)
            .visible(true)
            .build();
        container.append(&chips_box);

        // Pre-populate with default empty label so bar is never zero-height or invisible before DB query returns
        let empty_lbl = gtk4::Label::builder()
            .label("No bookmarks yet (Ctrl+D to bookmark)")
            .css_classes(["shadhin-bookmarks-bar-empty"])
            .visible(true)
            .build();
        chips_box.append(&empty_lbl);

        let on_navigate: Rc<RefCell<Option<Box<dyn Fn(&str)>>>> = Rc::new(RefCell::new(None));

        let bar = Self {
            container,
            chips_box,
            on_navigate,
            storage,
        };

        bar.refresh();
        bar
    }

    /// Accessor to the root widget
    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    /// Toggles visibility of the bookmarks bar (Ctrl+Shift+B)
    pub fn toggle_visibility(&self) {
        self.container.set_visible(!self.container.is_visible());
    }

    /// Connects navigation callback when a bookmark is clicked
    pub fn connect_navigate<F: Fn(&str) + 'static>(&self, callback: F) {
        *self.on_navigate.borrow_mut() = Some(Box::new(callback));
    }

    /// Refreshes bookmarks from SQLite database
    pub fn refresh(&self) {
        let chips = self.chips_box.clone();
        let on_nav = self.on_navigate.clone();

        self.storage.query_async(
            move |db| {
                let repo = BookmarkRepository::new(db.conn());
                repo.get_all_bookmarks().unwrap_or_default()
            },
            move |items: Vec<BookmarkItem>| {
                while let Some(child) = chips.first_child() {
                    chips.remove(&child);
                }

                if items.is_empty() {
                    let empty_lbl = gtk4::Label::builder()
                        .label("No bookmarks yet (Ctrl+D to bookmark)")
                        .css_classes(["shadhin-bookmarks-bar-empty"])
                        .visible(true)
                        .build();
                    chips.append(&empty_lbl);
                    return;
                }

                for item in items.into_iter().take(12) {
                    let btn = gtk4::Button::builder()
                        .css_classes(["flat", "shadhin-bookmarks-bar-chip"])
                        .tooltip_text(&item.url)
                        .visible(true)
                        .build();

                    let row = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .spacing(4)
                        .visible(true)
                        .build();

                    let icon = gtk4::Image::from_icon_name("text-html-symbolic");
                    icon.set_pixel_size(14);
                    icon.set_visible(true);
                    row.append(&icon);

                    let title = if item.title.is_empty() {
                        item.url.clone()
                    } else {
                        item.title.clone()
                    };

                    let lbl = gtk4::Label::builder()
                        .label(&title)
                        .ellipsize(gtk4::pango::EllipsizeMode::End)
                        .max_width_chars(16)
                        .visible(true)
                        .build();
                    row.append(&lbl);

                    btn.set_child(Some(&row));

                    let target = item.url.clone();
                    let on_nav_item = on_nav.clone();
                    btn.connect_clicked(move |_| {
                        if let Some(ref cb) = *on_nav_item.borrow() {
                            cb(&target);
                        }
                    });

                    chips.append(&btn);
                }
            },
        );
    }
}
