//! In-page text search component (Find in Page)
//!
//! Provides a floating, glass-themed find bar anchored inside the content frame,
//! wired to WebKitGTK's `FindController`.

use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use webkit6::prelude::*;

pub struct FindBar {
    container: gtk4::Box,
    entry: gtk4::Entry,
    count_label: gtk4::Label,
    current_controller: Rc<RefCell<Option<webkit6::FindController>>>,
}

impl FindBar {
    /// Constructs a new `FindBar` widget
    pub fn new() -> Self {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .css_classes(["shadhin-find-bar"])
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::Start)
            .margin_top(8)
            .margin_end(16)
            .visible(false)
            .build();

        // Search text entry
        let entry = gtk4::Entry::builder()
            .placeholder_text("Find in page...")
            .css_classes(["shadhin-find-entry"])
            .width_chars(24)
            .build();
        container.append(&entry);

        // Match count indicator label
        let count_label = gtk4::Label::builder()
            .label("")
            .css_classes(["shadhin-find-count"])
            .build();
        container.append(&count_label);

        // Previous match button (Up)
        let prev_icon = gtk4::Image::from_icon_name("go-up-symbolic");
        let prev_btn = gtk4::Button::builder()
            .child(&prev_icon)
            .tooltip_text("Previous match (Shift+Enter)")
            .css_classes(["flat", "shadhin-nav-btn"])
            .build();
        container.append(&prev_btn);

        // Next match button (Down)
        let next_icon = gtk4::Image::from_icon_name("go-down-symbolic");
        let next_btn = gtk4::Button::builder()
            .child(&next_icon)
            .tooltip_text("Next match (Enter)")
            .css_classes(["flat", "shadhin-nav-btn"])
            .build();
        container.append(&next_btn);

        // Dismiss button (Close)
        let close_icon = gtk4::Image::from_icon_name("window-close-symbolic");
        let close_btn = gtk4::Button::builder()
            .child(&close_icon)
            .tooltip_text("Close find bar (Esc)")
            .css_classes(["flat", "shadhin-nav-btn"])
            .build();
        container.append(&close_btn);

        let current_controller: Rc<RefCell<Option<webkit6::FindController>>> =
            Rc::new(RefCell::new(None));

        // Connect entry text change -> search
        let fc_search = current_controller.clone();
        let lbl_search = count_label.clone();
        entry.connect_changed(move |e| {
            let text = e.text().to_string();
            if let Some(ref fc) = *fc_search.borrow() {
                if text.is_empty() {
                    fc.search_finish();
                    lbl_search.set_label("");
                } else {
                    let options = webkit6::FindOptions::CASE_INSENSITIVE.bits()
                        | webkit6::FindOptions::WRAP_AROUND.bits();
                    fc.search(&text, options, 1000);
                    fc.count_matches(&text, options, 1000);
                }
            }
        });

        // Enter key in entry goes to next match
        let fc_enter = current_controller.clone();
        entry.connect_activate(move |_| {
            if let Some(ref fc) = *fc_enter.borrow() {
                fc.search_next();
            }
        });

        // Next button click
        let fc_next = current_controller.clone();
        next_btn.connect_clicked(move |_| {
            if let Some(ref fc) = *fc_next.borrow() {
                fc.search_next();
            }
        });

        // Previous button click
        let fc_prev = current_controller.clone();
        prev_btn.connect_clicked(move |_| {
            if let Some(ref fc) = *fc_prev.borrow() {
                fc.search_previous();
            }
        });

        // Close button click
        let c_box = container.clone();
        let fc_close = current_controller.clone();
        let lbl_close = count_label.clone();
        close_btn.connect_clicked(move |_| {
            c_box.set_visible(false);
            lbl_close.set_label("");
            if let Some(ref fc) = *fc_close.borrow() {
                fc.search_finish();
            }
        });

        Self {
            container,
            entry,
            count_label,
            current_controller,
        }
    }

    /// Attaches the find bar to the active webview's FindController
    pub fn attach_webview(&self, webview: &webkit6::WebView) {
        if let Some(fc) = webview.find_controller() {
            let lbl = self.count_label.clone();
            fc.connect_counted_matches(move |_, count| {
                if count == 0 {
                    lbl.set_label("No matches");
                } else if count == 1 {
                    lbl.set_label("1 match");
                } else {
                    lbl.set_label(&format!("{count} matches"));
                }
            });

            let lbl_fail = self.count_label.clone();
            fc.connect_failed_to_find_text(move |_| {
                lbl_fail.set_label("No matches");
            });

            *self.current_controller.borrow_mut() = Some(fc);
        } else {
            *self.current_controller.borrow_mut() = None;
        }
    }

    /// Reveals the find bar and directs focus to the entry
    pub fn show_and_focus(&self) {
        self.container.set_visible(true);
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    /// Hides the find bar and clears active highlights
    pub fn close(&self) {
        self.container.set_visible(false);
        self.count_label.set_label("");
        if let Some(ref fc) = *self.current_controller.borrow() {
            fc.search_finish();
        }
    }

    /// Checks if find bar is currently visible
    pub fn is_visible(&self) -> bool {
        self.container.is_visible()
    }

    /// Root container widget
    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }
}

impl Default for FindBar {
    fn default() -> Self {
        Self::new()
    }
}
