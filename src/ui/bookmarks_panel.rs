use crate::storage::StorageService;
use crate::storage::bookmarks::{BookmarkFolder, BookmarkItem, BookmarkRepository};
use adw::prelude::*;
use gtk4::gio;
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
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
            .label("Import…")
            .css_classes(["flat", "shadhin-nav-btn"])
            .build();
        header.append(&import_btn);

        let export_btn = gtk4::Button::builder()
            .label("Export…")
            .css_classes(["flat", "shadhin-nav-btn"])
            .build();
        header.append(&export_btn);

        container.append(&header);

        // 2. Search Filter
        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Search bookmarks…")
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
        let this_panel = self.clone();

        new_folder_btn.connect_clicked(move |_| {
            let storage = storage_folder.clone();
            let parent_win = panel_ref.root().and_then(|r| r.downcast::<gtk4::Window>().ok());

            let dialog = adw::AlertDialog::builder()
                .heading("Create Bookmark Folder")
                .body("Enter the name of the new bookmark folder:")
                .build();

            let entry = gtk4::Entry::builder()
                .placeholder_text("Folder name")
                .build();
            dialog.set_extra_child(Some(&entry));
            dialog.add_response("cancel", "Cancel");
            dialog.add_response("create", "Create");
            dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);

            let entry_clone = entry.clone();
            let rf = this_panel.clone();
            dialog.choose(parent_win.as_ref(), None::<&gio::Cancellable>, move |resp| {
                if resp == "create" {
                    let name = entry_clone.text().trim().to_string();
                    if !name.is_empty() {
                        let st = storage.clone();
                        let rf_reload = rf.clone();
                        st.dispatch(move |db| {
                            let repo = BookmarkRepository::new(db.conn());
                            let _ = repo.create_folder(&name, None);
                        });
                        glib::timeout_add_local_once(std::time::Duration::from_millis(100), move || {
                            rf_reload.refresh("");
                        });
                    }
                }
            });
        });

        // Wire search entry filter to refresh in real-time
        let this_search = self.clone();
        self.search_entry.connect_search_changed(move |entry| {
            this_search.refresh(&entry.text());
        });

        // Wire Import Bookmarks
        let storage_import = self.storage.clone();
        let this_import = self.clone();
        let panel_imp = self.container.clone();
        import_btn.connect_clicked(move |_| {
            let filter = gtk4::FileFilter::new();
            filter.set_name(Some("Bookmark Files (*.html, *.json)"));
            filter.add_pattern("*.html");
            filter.add_pattern("*.htm");
            filter.add_pattern("*.json");

            let dialog = gtk4::FileDialog::builder()
                .title("Import Bookmarks")
                .modal(true)
                .default_filter(&filter)
                .build();

            let parent_win = panel_imp.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
            let st = storage_import.clone();
            let rf = this_import.clone();
            dialog.open(parent_win.as_ref(), None::<&gio::Cancellable>, move |res| {
                if let Ok(file) = res
                    && let Some(path) = file.path()
                    && let Ok(content) = std::fs::read_to_string(&path)
                {
                    let parsed = crate::storage::import_export::parse_netscape_html(&content)
                        .or_else(|_| crate::storage::import_export::parse_json_bookmarks(&content));
                    if let Ok(bookmarks) = parsed {
                        let st_sub = st.clone();
                        let rf_sub = rf.clone();
                        st_sub.dispatch(move |db| {
                            let repo = BookmarkRepository::new(db.conn());
                            let _ = crate::storage::import_export::import_parsed_bookmarks(
                                &repo,
                                &bookmarks,
                                true,
                            );
                        });
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(150),
                            move || {
                                rf_sub.refresh("");
                            },
                        );
                    }
                }
            });
        });

        // Wire Export Bookmarks
        let storage_export = self.storage.clone();
        let panel_exp = self.container.clone();
        export_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder()
                .title("Export Bookmarks")
                .initial_name("bookmarks.html")
                .modal(true)
                .build();

            let parent_win = panel_exp.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
            let st = storage_export.clone();
            dialog.save(parent_win.as_ref(), None::<&gio::Cancellable>, move |res| {
                if let Ok(file) = res
                    && let Some(path) = file.path()
                {
                    st.query_async(
                        move |db| {
                            let repo = BookmarkRepository::new(db.conn());
                            crate::storage::import_export::export_netscape_html(&repo)
                                .unwrap_or_default()
                        },
                        move |html| {
                            if !html.is_empty() {
                                let _ = std::fs::write(&path, html);
                            }
                        },
                    );
                }
            });
        });
    }
}
