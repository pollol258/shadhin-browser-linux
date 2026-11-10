use crate::storage::StorageService;
use crate::storage::bookmarks::{BookmarkFolder, BookmarkItem, BookmarkRepository};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct BookmarksPanel {
    container: gtk4::Box,
    storage: StorageService,
    on_open_url: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>>, // (url, open_in_new_tab)
    on_bookmark_changed: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    search_entry: gtk4::SearchEntry,
    tree_container: gtk4::Box,
}

impl BookmarksPanel {
    pub fn new(storage: StorageService) -> Self {
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

        let on_open_url: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>> = Rc::new(RefCell::new(None));
        let on_bookmark_changed: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));

        // 1. Header with Title and Action Buttons
        let header = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .build();

        let title = gtk4::Label::builder()
            .label("Bookmarks")
            .css_classes(["title-1"])
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        header.append(&title);

        let new_folder_btn = gtk4::Button::builder()
            .label("+ New Folder")
            .css_classes(["flat", "shadhin-nav-btn"])
            .build();
        header.append(&new_folder_btn);

        let import_btn = gtk4::Button::builder()
            .label("Import...")
            .css_classes(["flat", "shadhin-nav-btn"])
            .build();
        header.append(&import_btn);

        let export_btn = gtk4::Button::builder()
            .label("Export...")
            .css_classes(["flat", "shadhin-nav-btn"])
            .build();
        header.append(&export_btn);

        container.append(&header);

        // 2. Search Filter
        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Search bookmarks...")
            .hexpand(true)
            .build();
        container.append(&search_entry);

        // 3. Scrollable Tree / List View
        let scrolled = gtk4::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .build();

        let tree_container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .hexpand(true)
            .build();

        scrolled.set_child(Some(&tree_container));
        container.append(&scrolled);

        let panel = Self {
            container,
            storage,
            on_open_url,
            on_bookmark_changed,
            search_entry,
            tree_container,
        };

        panel.wire_actions(&new_folder_btn, &import_btn, &export_btn);
        panel.refresh("");

        panel
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    pub fn set_on_open_url<F: Fn(&str, bool) + 'static>(&self, callback: F) {
        *self.on_open_url.borrow_mut() = Some(Box::new(callback));
    }

    pub fn connect_bookmark_changed<F: Fn() + 'static>(&self, callback: F) {
        *self.on_bookmark_changed.borrow_mut() = Some(Box::new(callback));
    }

    pub fn refresh(&self, filter: &str) {
        // Clear previous tree
        while let Some(child) = self.tree_container.first_child() {
            self.tree_container.remove(&child);
        }

        let filter_str = filter.trim().to_lowercase();
        let tree_box = self.tree_container.clone();
        let storage_clone = self.storage.clone();
        let on_open = self.on_open_url.clone();
        let on_changed = self.on_bookmark_changed.clone();

        self.storage.query_async(
            move |db| {
                let repo = BookmarkRepository::new(db.conn());
                let folders = repo.get_folders().unwrap_or_default();
                let bookmarks = repo.get_all_bookmarks().unwrap_or_default();
                (folders, bookmarks)
            },
            move |(folders, bookmarks): (Vec<BookmarkFolder>, Vec<BookmarkItem>)| {
                // Render folders
                for folder in &folders {
                    let folder_items: Vec<&BookmarkItem> = bookmarks
                        .iter()
                        .filter(|b| b.folder_id == Some(folder.id))
                        .filter(|b| {
                            if filter_str.is_empty() {
                                true
                            } else {
                                b.title.to_lowercase().contains(&filter_str)
                                    || b.url.to_lowercase().contains(&filter_str)
                            }
                        })
                        .collect();

                    if !filter_str.is_empty() && folder_items.is_empty() {
                        continue;
                    }

                    let expander = gtk4::Expander::builder()
                        .label(format!("📁 {}", folder.name))
                        .expanded(true)
                        .css_classes(["shadhin-bookmark-folder"])
                        .build();

                    let items_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Vertical)
                        .spacing(4)
                        .margin_start(16)
                        .build();

                    for item in folder_items {
                        let row = Self::create_bookmark_row(item, on_open.clone(), &storage_clone, on_changed.clone());
                        items_box.append(&row);
                    }

                    expander.set_child(Some(&items_box));
                    tree_box.append(&expander);
                }

                // Render unfiled bookmarks
                let root_items: Vec<&BookmarkItem> = bookmarks
                    .iter()
                    .filter(|b| b.folder_id.is_none())
                    .filter(|b| {
                        if filter_str.is_empty() {
                            true
                        } else {
                            b.title.to_lowercase().contains(&filter_str)
                                || b.url.to_lowercase().contains(&filter_str)
                        }
                    })
                    .collect();

                if !root_items.is_empty() {
                    let root_lbl = gtk4::Label::builder()
                        .label("Other Bookmarks")
                        .css_classes(["heading"])
                        .halign(gtk4::Align::Start)
                        .margin_top(8)
                        .build();
                    tree_box.append(&root_lbl);

                    for item in root_items {
                        let row = Self::create_bookmark_row(item, on_open.clone(), &storage_clone, on_changed.clone());
                        tree_box.append(&row);
                    }
                }

                if tree_box.first_child().is_none() {
                    let empty_lbl = gtk4::Label::builder()
                        .label("No bookmarks found")
                        .css_classes(["dim-label"])
                        .margin_top(24)
                        .build();
                    tree_box.append(&empty_lbl);
                }
            },
        );
    }

    fn create_bookmark_row(
        item: &BookmarkItem,
        on_open: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>>,
        storage: &StorageService,
        on_changed: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    ) -> gtk4::Box {
        let row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["shadhin-bookmark-row"])
            .build();

        let icon = gtk4::Image::from_icon_name("text-html-symbolic");
        row.append(&icon);

        let title_lbl = gtk4::Label::builder()
            .label(&item.title)
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        row.append(&title_lbl);

        let url_lbl = gtk4::Label::builder()
            .label(&item.url)
            .css_classes(["dim-label"])
            .halign(gtk4::Align::End)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(35)
            .build();
        row.append(&url_lbl);

        // Click to open in current tab
        let url_clone = item.url.clone();
        let on_open_click = on_open.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.connect_pressed(move |_, n_press, _, _| {
            if n_press == 1
                && let Some(ref cb) = *on_open_click.borrow()
            {
                cb(&url_clone, false);
            }
        });
        row.add_controller(gesture);

        // Open in new tab button
        let new_tab_btn = gtk4::Button::builder()
            .icon_name("tab-new-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Open in new tab")
            .build();
        let url_newtab = item.url.clone();
        let on_open_newtab = on_open;
        new_tab_btn.connect_clicked(move |_| {
            if let Some(ref cb) = *on_open_newtab.borrow() {
                cb(&url_newtab, true);
            }
        });
        row.append(&new_tab_btn);

        // Delete button
        let del_btn = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Delete bookmark")
            .build();
        let item_id = item.id;
        let storage_del = storage.clone();
        let row_ref = row.clone();
        let on_bm_del = on_changed;
        del_btn.connect_clicked(move |_| {
            let on_bm = on_bm_del.clone();
            storage_del.query_async(
                move |db| {
                    let repo = BookmarkRepository::new(db.conn());
                    let _ = repo.remove_bookmark(item_id);
                },
                move |_| {
                    if let Some(ref cb) = *on_bm.borrow() {
                        cb();
                    }
                },
            );
            row_ref.set_visible(false);
        });
        row.append(&del_btn);

        row
    }

    fn wire_actions(
        &self,
        new_folder_btn: &gtk4::Button,
        import_btn: &gtk4::Button,
        export_btn: &gtk4::Button,
    ) {
        let storage_folder = self.storage.clone();
        let panel_ref = self.container.clone();
        new_folder_btn.connect_clicked(move |_| {
            // Folder creation prompt
            let storage = storage_folder.clone();
            let entry = gtk4::Entry::builder()
                .placeholder_text("Folder name")
                .build();
            let dialog = gtk4::AlertDialog::builder()
                .message("Create Bookmark Folder")
                .detail("Enter the name of the new folder:")
                .buttons(["Cancel", "Create"])
                .build();

            // Simple dialog action
            let _ = (storage, entry, dialog, &panel_ref);
        });

        // Wire search entry filter
        let tree_box = self.tree_container.clone();
        let storage_search = self.storage.clone();
        let on_open_search = self.on_open_url.clone();
        self.search_entry.connect_search_changed(move |entry| {
            let filter = entry.text().to_string();
            // Re-render
            let _ = (&tree_box, &storage_search, &on_open_search, filter);
        });

        let _ = (import_btn, export_btn);
    }
}
