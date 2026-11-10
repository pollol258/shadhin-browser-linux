use crate::storage::StorageService;
use crate::storage::history::{HistoryGrouped, HistoryItem, HistoryRepository};
use adw::prelude::*;
use gtk4::gio;
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::SystemTime;

#[derive(Clone)]
pub struct HistoryPanel {
    container: gtk4::Box,
    storage: StorageService,
    on_open_url: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>>,
    list_container: gtk4::Box,
}

impl HistoryPanel {
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

        // 1. Header with Title and "Clear History..."
        let header = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .build();

        let title = gtk4::Label::builder()
            .label("Browsing History")
            .css_classes(["title-1"])
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        header.append(&title);

        let clear_btn = gtk4::Button::builder()
            .label("Clear History…")
            .css_classes(["flat", "destructive-action", "shadhin-nav-btn"])
            .build();
        header.append(&clear_btn);
        container.append(&header);

        // 2. Search Filter
        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Search browsing history…")
            .hexpand(true)
            .build();
        container.append(&search_entry);

        // 3. Scrollable List View
        let scrolled = gtk4::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .build();

        let list_container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .hexpand(true)
            .build();

        scrolled.set_child(Some(&list_container));
        container.append(&scrolled);

        let panel = Self {
            container,
            storage,
            on_open_url,
            list_container,
        };

        // Connect real-time search filter
        let panel_search = panel.clone();
        search_entry.connect_search_changed(move |entry| {
            panel_search.refresh(&entry.text());
        });

        panel.wire_clear_button(&clear_btn);

        // Lazily query SQLite only when the panel is mapped / displayed
        let panel_map = panel.clone();
        panel.container.connect_map(move |_| {
            panel_map.refresh("");
        });

        panel
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    pub fn set_on_open_url<F: Fn(&str, bool) + 'static>(&self, callback: F) {
        *self.on_open_url.borrow_mut() = Some(Box::new(callback));
    }

    pub fn refresh(&self, query: &str) {
        while let Some(child) = self.list_container.first_child() {
            self.list_container.remove(&child);
        }

        let list_box = self.list_container.clone();
        let query_str = query.trim().to_lowercase();
        let on_open = self.on_open_url.clone();
        let storage_clone = self.storage.clone();

        self.storage.query_async(
            move |db| {
                let repo = HistoryRepository::new(db.conn());
                if query_str.is_empty() {
                    repo.get_grouped_history().unwrap_or(HistoryGrouped {
                        today: Vec::new(),
                        yesterday: Vec::new(),
                        earlier: Vec::new(),
                    })
                } else {
                    let results = repo.search_history(&query_str, 100).unwrap_or_default();
                    HistoryGrouped {
                        today: results,
                        yesterday: Vec::new(),
                        earlier: Vec::new(),
                    }
                }
            },
            move |grouped: HistoryGrouped| {
                let groups = [
                    ("Today", grouped.today),
                    ("Yesterday", grouped.yesterday),
                    ("Earlier", grouped.earlier),
                ];

                let mut total_rendered = 0;
                for (heading, items) in groups {
                    if items.is_empty() {
                        continue;
                    }

                    let section_lbl = gtk4::Label::builder()
                        .label(heading)
                        .css_classes(["heading", "shadhin-sidebar-section-title"])
                        .halign(gtk4::Align::Start)
                        .margin_top(8)
                        .build();
                    list_box.append(&section_lbl);

                    let section_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Vertical)
                        .spacing(2)
                        .build();

                    for item in items {
                        let row = Self::create_history_row(&item, on_open.clone(), &storage_clone);
                        section_box.append(&row);
                        total_rendered += 1;
                    }

                    list_box.append(&section_box);
                }

                if total_rendered == 0 {
                    let empty_lbl = gtk4::Label::builder()
                        .label("No browsing history found")
                        .css_classes(["dim-label"])
                        .margin_top(32)
                        .build();
                    list_box.append(&empty_lbl);
                }
            },
        );
    }

    fn create_history_row(
        item: &HistoryItem,
        on_open: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>>,
        storage: &StorageService,
    ) -> gtk4::Box {
        let row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(10)
            .css_classes(["shadhin-history-row"])
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

        // Click to navigate
        let url_clone = item.url.clone();
        let on_open_click = on_open;
        let gesture = gtk4::GestureClick::new();
        gesture.connect_pressed(move |_, n_press, _, _| {
            if n_press == 1
                && let Some(ref cb) = *on_open_click.borrow()
            {
                cb(&url_clone, false);
            }
        });
        row.add_controller(gesture);

        // Delete single entry button
        let del_btn = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Delete from history")
            .build();
        let entry_id = item.id;
        let storage_del = storage.clone();
        let row_ref = row.clone();
        del_btn.connect_clicked(move |_| {
            storage_del.dispatch(move |db| {
                let repo = HistoryRepository::new(db.conn());
                let _ = repo.delete_entry(entry_id);
            });
            row_ref.set_visible(false);
        });
        row.append(&del_btn);

        row
    }

    fn wire_clear_button(&self, clear_btn: &gtk4::Button) {
        let storage_clear = self.storage.clone();
        let panel_box = self.container.clone();
        let this_panel = self.clone();

        clear_btn.connect_clicked(move |_| {
            let storage = storage_clear.clone();
            let parent_win = panel_box
                .root()
                .and_then(|r| r.downcast::<gtk4::Window>().ok());

            let dialog = adw::AlertDialog::builder()
                .heading("Clear Browsing History")
                .body("Select the time range of browsing data to remove from your local database:")
                .build();
            dialog.add_response("cancel", "Cancel");
            dialog.add_response("hour", "Last Hour");
            dialog.add_response("day", "Last 24 Hours");
            dialog.add_response("all", "All Time");
            dialog.set_response_appearance("all", adw::ResponseAppearance::Destructive);

            let refresh_panel = this_panel.clone();
            dialog.choose(parent_win.as_ref(), None::<&gio::Cancellable>, move |resp| {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                let delete_task: Option<(i64, i64)> = match resp.as_str() {
                    "hour" => Some((now - 3600, now)),
                    "day" => Some((now - 86400, now)),
                    "all" => Some((0, now)),
                    _ => None,
                };

                if let Some((from_time, to_time)) = delete_task {
                    let st = storage.clone();
                    let rf = refresh_panel.clone();
                    st.dispatch(move |db| {
                        let repo = HistoryRepository::new(db.conn());
                        let _ = repo.delete_range(from_time, to_time);
                    });
                    glib::timeout_add_local_once(std::time::Duration::from_millis(100), move || {
                        rf.refresh("");
                    });
                }
            });
        });
    }
}
