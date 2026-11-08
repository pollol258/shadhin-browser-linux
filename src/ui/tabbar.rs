//! Tab management and tab strip component
//!
//! Synchronizes the visual tab buttons in the title bar with an `adw::TabView`,
//! handling tab creation, selection, title updates, drag-and-drop reordering,
//! and tab context menus (duplicate, close others, close).

use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct TabDisplayInfo {
    pub title: String,
    pub favicon: Option<gtk4::gdk::Texture>,
    pub is_newtab: bool,
    pub is_discarded: bool,
}

pub struct ShadhinTabStrip {
    container: gtk4::Box,
    tabs_box: gtk4::Box,
    new_tab_btn: gtk4::Button,
    tab_view: adw::TabView,
    on_new_tab: Rc<RefCell<Option<Box<dyn Fn(Option<String>)>>>>,
    on_duplicate_tab: Rc<RefCell<Option<Box<dyn Fn(i32)>>>>,
    on_close_others: Rc<RefCell<Option<Box<dyn Fn(i32)>>>>,
}

impl ShadhinTabStrip {
    /// Constructs a new tab strip bound to an `adw::TabView`
    pub fn new(tab_view: &adw::TabView) -> Self {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk4::Align::Center)
            .css_classes(["shadhin-tab-strip"])
            .build();

        let tabs_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk4::Align::Center)
            .build();
        container.append(&tabs_box);

        let new_tab_icon = gtk4::Image::from_icon_name("list-add-symbolic");
        let new_tab_btn = gtk4::Button::builder()
            .child(&new_tab_icon)
            .tooltip_text("Open new tab (Ctrl+T)")
            .css_classes(["shadhin-tab-new-btn"])
            .valign(gtk4::Align::Center)
            .build();
        container.append(&new_tab_btn);

        let on_new_tab: Rc<RefCell<Option<Box<dyn Fn(Option<String>)>>>> =
            Rc::new(RefCell::new(None));
        let on_new_clone = on_new_tab.clone();
        new_tab_btn.connect_clicked(move |_| {
            if let Some(ref cb) = *on_new_clone.borrow() {
                cb(None);
            }
        });

        let on_duplicate_tab: Rc<RefCell<Option<Box<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));
        let on_close_others: Rc<RefCell<Option<Box<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));

        Self {
            container,
            tabs_box,
            new_tab_btn,
            tab_view: tab_view.clone(),
            on_new_tab,
            on_duplicate_tab,
            on_close_others,
        }
    }

    /// Re-renders the tab buttons based on the `adw::TabView` pages and display metadata
    pub fn refresh(&self, tabs_info: &[TabDisplayInfo]) {
        // Clear existing visual tab buttons
        while let Some(child) = self.tabs_box.first_child() {
            self.tabs_box.remove(&child);
        }

        let n_pages = self.tab_view.n_pages();
        let selected_page = self.tab_view.selected_page();

        for i in 0..n_pages {
            let page = self.tab_view.nth_page(i);
            let is_selected = selected_page.as_ref() == Some(&page);
            let info = tabs_info.get(i as usize);

            let tab_btn = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .spacing(8)
                .css_classes(["shadhin-tab"])
                .valign(gtk4::Align::Center)
                .build();

            if is_selected {
                tab_btn.add_css_class("active");
            }

            if let Some(tab_data) = info
                && tab_data.is_discarded
            {
                tab_btn.add_css_class("tab-sleeping");
                tab_btn.set_opacity(0.5);
            }

            // Real favicon or fallback icon
            let icon = if let Some(tab_data) = info {
                if let Some(ref texture) = tab_data.favicon {
                    let img = gtk4::Image::from_paintable(Some(texture));
                    img.set_pixel_size(16);
                    img
                } else if tab_data.is_newtab {
                    let img = gtk4::Image::from_icon_name("go-home-symbolic");
                    img.set_pixel_size(16);
                    img.set_opacity(0.85);
                    img
                } else {
                    let img = gtk4::Image::from_icon_name("globe-symbolic");
                    img.set_pixel_size(16);
                    img.set_opacity(0.85);
                    img
                }
            } else {
                let img = gtk4::Image::from_icon_name("globe-symbolic");
                img.set_pixel_size(16);
                img.set_opacity(0.85);
                img
            };
            icon.add_css_class("shadhin-tab-favicon");
            tab_btn.append(&icon);

            // Tab title
            let display_title = if let Some(tab_data) = info {
                if tab_data.title.is_empty() {
                    "New Tab".to_string()
                } else {
                    tab_data.title.clone()
                }
            } else {
                let raw_title = page.title();
                if raw_title.is_empty() {
                    "New Tab".to_string()
                } else {
                    raw_title.to_string()
                }
            };

            let title_lbl = gtk4::Label::builder()
                .label(&display_title)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .max_width_chars(18)
                .hexpand(true)
                .halign(gtk4::Align::Start)
                .css_classes(["shadhin-tab-title"])
                .build();
            tab_btn.append(&title_lbl);

            // Close button (x)
            let close_icon = gtk4::Image::from_icon_name("window-close-symbolic");
            let close_btn = gtk4::Button::builder()
                .child(&close_icon)
                .css_classes(["flat", "shadhin-tab-close"])
                .valign(gtk4::Align::Center)
                .tooltip_text("Close tab (Ctrl+W)")
                .build();

            let tv_close = self.tab_view.clone();
            let page_close = page.clone();
            close_btn.connect_clicked(move |_| {
                tv_close.close_page(&page_close);
            });
            tab_btn.append(&close_btn);

            // Left Click: Switch tab
            let gesture = gtk4::GestureClick::new();
            let tv_select = self.tab_view.clone();
            let page_select = page.clone();
            gesture.connect_pressed(move |_, _, _, _| {
                tv_select.set_selected_page(&page_select);
            });
            tab_btn.add_controller(gesture);

            // Tab Context Menu (Right-click popover)
            let popover = gtk4::Popover::new();
            popover.set_parent(&tab_btn);

            let menu_box = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .spacing(4)
                .margin_top(4)
                .margin_bottom(4)
                .margin_start(4)
                .margin_end(4)
                .build();

            let dup_btn = gtk4::Button::builder()
                .label("Duplicate Tab")
                .css_classes(["flat"])
                .build();
            let on_dup_clone = self.on_duplicate_tab.clone();
            let pop_dup = popover.clone();
            let dup_idx = i;
            dup_btn.connect_clicked(move |_| {
                pop_dup.popdown();
                if let Some(ref cb) = *on_dup_clone.borrow() {
                    cb(dup_idx);
                }
            });
            menu_box.append(&dup_btn);

            let close_others_btn = gtk4::Button::builder()
                .label("Close Other Tabs")
                .css_classes(["flat"])
                .build();
            let on_co_clone = self.on_close_others.clone();
            let pop_co = popover.clone();
            let co_idx = i;
            close_others_btn.connect_clicked(move |_| {
                pop_co.popdown();
                if let Some(ref cb) = *on_co_clone.borrow() {
                    cb(co_idx);
                }
            });
            menu_box.append(&close_others_btn);

            let close_tab_menu_btn = gtk4::Button::builder()
                .label("Close Tab")
                .css_classes(["flat"])
                .build();
            let pop_close = popover.clone();
            let tv_ctx_close = self.tab_view.clone();
            let page_ctx_close = page.clone();
            close_tab_menu_btn.connect_clicked(move |_| {
                pop_close.popdown();
                tv_ctx_close.close_page(&page_ctx_close);
            });
            menu_box.append(&close_tab_menu_btn);

            popover.set_child(Some(&menu_box));

            let right_click = gtk4::GestureClick::new();
            right_click.set_button(3); // Right click button
            let pop_popup = popover.clone();
            right_click.connect_pressed(move |_, _, _, _| {
                pop_popup.popup();
            });
            tab_btn.add_controller(right_click);

            // Drag and drop tab reordering
            let drag = gtk4::DragSource::new();
            drag.set_actions(gtk4::gdk::DragAction::MOVE);
            let page_idx = i;
            drag.connect_prepare(move |_, _, _| {
                Some(gtk4::gdk::ContentProvider::for_value(&page_idx.to_value()))
            });
            tab_btn.add_controller(drag);

            let drop =
                gtk4::DropTarget::new(gtk4::glib::types::Type::I32, gtk4::gdk::DragAction::MOVE);
            let tv_reorder = self.tab_view.clone();
            let target_page = page.clone();
            drop.connect_drop(move |_, val, _, _| {
                if let Ok(from_idx) = val.get::<i32>() {
                    let to_idx = tv_reorder.page_position(&target_page);
                    if from_idx != to_idx && from_idx >= 0 && from_idx < tv_reorder.n_pages() {
                        let from_page = tv_reorder.nth_page(from_idx);
                        tv_reorder.reorder_page(&from_page, to_idx);
                        return true;
                    }
                }
                false
            });
            tab_btn.add_controller(drop);

            self.tabs_box.append(&tab_btn);
        }
    }

    /// Connects the callback when the user requests a new tab
    pub fn connect_new_tab<F: Fn(Option<String>) + 'static>(&self, callback: F) {
        *self.on_new_tab.borrow_mut() = Some(Box::new(callback));
    }

    /// Programmatically triggers opening a new tab
    pub fn trigger_new_tab(&self, initial_url: Option<String>) {
        if let Some(ref cb) = *self.on_new_tab.borrow() {
            cb(initial_url);
        }
    }

    /// Connects the callback when duplicating a tab via context menu
    pub fn connect_duplicate_tab<F: Fn(i32) + 'static>(&self, callback: F) {
        *self.on_duplicate_tab.borrow_mut() = Some(Box::new(callback));
    }

    /// Connects the callback when closing all other tabs via context menu
    pub fn connect_close_other_tabs<F: Fn(i32) + 'static>(&self, callback: F) {
        *self.on_close_others.borrow_mut() = Some(Box::new(callback));
    }

    /// Accessor to the root container widget
    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    /// Accessor to the new tab button
    pub fn new_tab_button(&self) -> &gtk4::Button {
        &self.new_tab_btn
    }
}
