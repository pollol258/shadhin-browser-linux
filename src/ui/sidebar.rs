//! Collapsible 235px vertical tab sidebar bound to AdwTabView pages
//!
//! Implements Layout (b) from HANDOFF.md:
//! - 235px wide container wrapped in a GtkScrolledWindow
//! - Tab items: active, hover, pinned, sleeping (opacity 0.5), audio playing, close button on hover/active
//! - Footer showing open tab count and context menu hint

use crate::ui::tabbar::TabDisplayInfo;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct ShadhinSidebar {
    scrolled_window: gtk4::ScrolledWindow,
    tabs_list_box: gtk4::ListBox,
    footer_label: gtk4::Label,
    on_new_tab: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_tab_selected: Rc<RefCell<Option<Box<dyn Fn(i32)>>>>,
    on_tab_reload: Rc<RefCell<Option<Box<dyn Fn(i32)>>>>,
    on_tab_duplicate: Rc<RefCell<Option<Box<dyn Fn(i32)>>>>,
    on_tab_close: Rc<RefCell<Option<Box<dyn Fn(i32)>>>>,
    on_tab_pin: Rc<RefCell<Option<Box<dyn Fn(i32)>>>>,
    on_tab_mute: Rc<RefCell<Option<Box<dyn Fn(i32)>>>>,
    on_tab_sleep: Rc<RefCell<Option<Box<dyn Fn(i32)>>>>,
    on_workspace_selected: Rc<RefCell<Option<Box<dyn Fn(&str)>>>>,
    on_create_workspace: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    active_popover: Rc<RefCell<Option<gtk4::Popover>>>,
}

impl ShadhinSidebar {
    /// Constructs a new 235px vertical tab sidebar
    pub fn new() -> Self {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .css_classes(["shadhin-vertical-sidebar"])
            .build();
        container.set_size_request(235, -1);

        let on_new_tab: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_tab_selected: Rc<RefCell<Option<Box<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));
        let on_tab_reload: Rc<RefCell<Option<Box<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));
        let on_tab_duplicate: Rc<RefCell<Option<Box<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));
        let on_tab_close: Rc<RefCell<Option<Box<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));
        let on_tab_pin: Rc<RefCell<Option<Box<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));
        let on_tab_mute: Rc<RefCell<Option<Box<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));
        let on_tab_sleep: Rc<RefCell<Option<Box<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));
        let on_workspace_selected: Rc<RefCell<Option<Box<dyn Fn(&str)>>>> =
            Rc::new(RefCell::new(None));
        let on_create_workspace: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let active_popover: Rc<RefCell<Option<gtk4::Popover>>> = Rc::new(RefCell::new(None));

        // 1. Sidebar Header: "Tabs" label + New Tab (+) button
        let header_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(8)
            .valign(gtk4::Align::Center)
            .build();

        let title_lbl = gtk4::Label::builder()
            .label("Tabs")
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .css_classes(["heading"])
            .build();
        header_box.append(&title_lbl);

        let new_tab_icon = gtk4::Image::from_icon_name("list-add-symbolic");
        new_tab_icon.set_pixel_size(16);
        let new_tab_btn = gtk4::Button::builder()
            .child(&new_tab_icon)
            .tooltip_text("New tab (Ctrl+T)")
            .css_classes(["iconbtn", "flat"])
            .valign(gtk4::Align::Center)
            .build();
        new_tab_btn.set_size_request(32, 32);

        let on_nt_clone = on_new_tab.clone();
        new_tab_btn.connect_clicked(move |_| {
            if let Some(ref cb) = *on_nt_clone.borrow() {
                cb();
            }
        });
        header_box.append(&new_tab_btn);
        container.append(&header_box);

        // 2. Vertical tabs list box
        let tabs_list_box = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .css_classes(["shadhin-vertical-tab-list"])
            .margin_start(8)
            .margin_end(8)
            .build();
        container.append(&tabs_list_box);

        // 3. Spacer
        let spacer = gtk4::Box::builder().vexpand(true).build();
        container.append(&spacer);

        // 4. Footer info label
        let footer_label = gtk4::Label::builder()
            .label("0 open tabs · Right-click for actions")
            .css_classes(["muted-text"])
            .halign(gtk4::Align::Start)
            .margin_start(16)
            .margin_end(16)
            .margin_bottom(16)
            .margin_top(8)
            .build();
        container.append(&footer_label);

        // Wrap the whole sidebar container in a ScrolledWindow to prevent minimum height explosion
        let scrolled_window = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .hexpand(false)
            .vexpand(true)
            .child(&container)
            .build();
        scrolled_window.set_size_request(235, -1);

        Self {
            scrolled_window,
            tabs_list_box,
            footer_label,
            on_new_tab,
            on_tab_selected,
            on_tab_reload,
            on_tab_duplicate,
            on_tab_close,
            on_tab_pin,
            on_tab_mute,
            on_tab_sleep,
            on_workspace_selected,
            on_create_workspace,
            active_popover,
        }
    }

    /// Accessor to the root sidebar widget (the ScrolledWindow)
    pub fn widget(&self) -> &gtk4::Widget {
        self.scrolled_window.upcast_ref()
    }

    /// Connects the new tab button in the sidebar header
    pub fn connect_new_tab<F: Fn() + 'static>(&self, callback: F) {
        *self.on_new_tab.borrow_mut() = Some(Box::new(callback));
    }

    /// Connects tab selection from the list box
    pub fn connect_tab_selected<F: Fn(i32) + 'static>(&self, callback: F) {
        *self.on_tab_selected.borrow_mut() = Some(Box::new(callback));
    }

    /// Connects tab reload from context menu
    pub fn connect_tab_reload<F: Fn(i32) + 'static>(&self, callback: F) {
        *self.on_tab_reload.borrow_mut() = Some(Box::new(callback));
    }

    /// Connects tab duplicate from context menu
    pub fn connect_tab_duplicate<F: Fn(i32) + 'static>(&self, callback: F) {
        *self.on_tab_duplicate.borrow_mut() = Some(Box::new(callback));
    }

    /// Connects tab close
    pub fn connect_tab_close<F: Fn(i32) + 'static>(&self, callback: F) {
        *self.on_tab_close.borrow_mut() = Some(Box::new(callback));
    }

    /// Connects tab pin toggle
    pub fn connect_tab_pin<F: Fn(i32) + 'static>(&self, callback: F) {
        *self.on_tab_pin.borrow_mut() = Some(Box::new(callback));
    }

    /// Dismisses any active tab popover in the sidebar
    pub fn close_popover(&self) {
        if let Some(ref pop) = *self.active_popover.borrow() {
            pop.popdown();
        }
        *self.active_popover.borrow_mut() = None;
    }

    /// Connects tab mute toggle
    pub fn connect_tab_mute<F: Fn(i32) + 'static>(&self, callback: F) {
        *self.on_tab_mute.borrow_mut() = Some(Box::new(callback));
    }

    /// Connects tab sleep action
    pub fn connect_tab_sleep<F: Fn(i32) + 'static>(&self, callback: F) {
        *self.on_tab_sleep.borrow_mut() = Some(Box::new(callback));
    }

    /// Re-renders the vertical tabs list from the current TabDisplayInfo slice
    pub fn update_open_tabs<F: Fn(i32) + 'static>(
        &self,
        tabs: &[TabDisplayInfo],
        selected_idx: i32,
        on_select: F,
    ) {
        // Clear previous rows
        while let Some(child) = self.tabs_list_box.first_child() {
            self.tabs_list_box.remove(&child);
        }

        let on_select = Rc::new(on_select);
        let n_tabs = tabs.len();

        for (i, tab_data) in tabs.iter().enumerate() {
            let row_box = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .spacing(8)
                .valign(gtk4::Align::Center)
                .css_classes(["vertical-tab-item"])
                .build();
            row_box.set_size_request(-1, 38);

            if i as i32 == selected_idx {
                row_box.add_css_class("active");
            }
            if tab_data.is_discarded {
                row_box.add_css_class("tab-sleeping");
            }

            // 1. Favicon or Globe icon (15px)
            let icon = if let Some(ref texture) = tab_data.favicon {
                let img = gtk4::Image::from_paintable(Some(texture));
                img.set_pixel_size(15);
                img
            } else if tab_data.is_newtab {
                let img = gtk4::Image::from_icon_name("go-home-symbolic");
                img.set_pixel_size(15);
                img
            } else {
                let img = gtk4::Image::from_icon_name("globe-symbolic");
                img.set_pixel_size(15);
                img
            };
            row_box.append(&icon);

            // 2. Tab Title
            let display_title = if tab_data.title.is_empty() {
                "New Tab".to_string()
            } else {
                tab_data.title.clone()
            };

            let title_lbl = gtk4::Label::builder()
                .label(&display_title)
                .halign(gtk4::Align::Start)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .hexpand(true)
                .build();
            row_box.append(&title_lbl);

            // 3. Close button
            let close_icon = gtk4::Image::from_icon_name("window-close-symbolic");
            close_icon.set_pixel_size(13);
            let close_btn = gtk4::Button::builder()
                .child(&close_icon)
                .tooltip_text("Close tab (Ctrl+W)")
                .css_classes(["tab-close-btn", "iconbtn", "flat"])
                .valign(gtk4::Align::Center)
                .build();
            close_btn.set_size_request(24, 24);

            let on_close_cb = self.on_tab_close.clone();
            let tab_idx = i as i32;
            close_btn.connect_clicked(move |_| {
                if let Some(ref cb) = *on_close_cb.borrow() {
                    cb(tab_idx);
                }
            });
            row_box.append(&close_btn);

            // Click gesture to select tab
            let on_sel_clone = on_select.clone();
            let tab_click = gtk4::GestureClick::new();
            tab_click.connect_pressed(move |_, _, _, _| {
                on_sel_clone(tab_idx);
            });
            row_box.add_controller(tab_click);

            // Context menu for Tab (Reload, Duplicate, Pin/Unpin, Close)
            let right_click = gtk4::GestureClick::new();
            right_click.set_button(3); // Right click
            let on_reload_cb = self.on_tab_reload.clone();
            let on_dup_cb = self.on_tab_duplicate.clone();
            let on_pin_cb = self.on_tab_pin.clone();
            let on_close_ctx = self.on_tab_close.clone();
            let is_pinned = tab_data.is_pinned;
            let active_pop_ref = self.active_popover.clone();

            let row_box_ref = row_box.clone();
            right_click.connect_pressed(move |_, _, _, _| {
                if let Some(ref old_pop) = *active_pop_ref.borrow() {
                    old_pop.popdown();
                }

                let pop = gtk4::Popover::new();
                pop.set_parent(&row_box_ref);
                pop.set_autohide(true);
                pop.add_css_class("shadhin-popover");

                let menu_vbox = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Vertical)
                    .spacing(4)
                    .width_request(180)
                    .build();

                // 1. Reload Tab
                let reload_btn = gtk4::Button::builder()
                    .label("Reload Tab")
                    .css_classes(["flat"])
                    .halign(gtk4::Align::Fill)
                    .build();
                let pop_clone0 = pop.clone();
                let reload_cb = on_reload_cb.clone();
                reload_btn.connect_clicked(move |_| {
                    pop_clone0.popdown();
                    if let Some(ref cb) = *reload_cb.borrow() {
                        cb(tab_idx);
                    }
                });
                menu_vbox.append(&reload_btn);

                // 2. Duplicate Tab
                let dup_btn = gtk4::Button::builder()
                    .label("Duplicate Tab")
                    .css_classes(["flat"])
                    .halign(gtk4::Align::Fill)
                    .build();
                let pop_clone1 = pop.clone();
                let dup_cb = on_dup_cb.clone();
                dup_btn.connect_clicked(move |_| {
                    pop_clone1.popdown();
                    if let Some(ref cb) = *dup_cb.borrow() {
                        cb(tab_idx);
                    }
                });
                menu_vbox.append(&dup_btn);

                // 3. Pin Tab / Unpin Tab
                let pin_label = if is_pinned { "Unpin Tab" } else { "Pin Tab" };
                let pin_btn = gtk4::Button::builder()
                    .label(pin_label)
                    .css_classes(["flat"])
                    .halign(gtk4::Align::Fill)
                    .build();
                let pop_clone2 = pop.clone();
                let pin_cb = on_pin_cb.clone();
                pin_btn.connect_clicked(move |_| {
                    pop_clone2.popdown();
                    if let Some(ref cb) = *pin_cb.borrow() {
                        cb(tab_idx);
                    }
                });
                menu_vbox.append(&pin_btn);

                // 4. Close Tab
                let close_item = gtk4::Button::builder()
                    .label("Close Tab")
                    .css_classes(["flat"])
                    .halign(gtk4::Align::Fill)
                    .build();
                let pop_clone3 = pop.clone();
                let close_cb = on_close_ctx.clone();
                close_item.connect_clicked(move |_| {
                    pop_clone3.popdown();
                    if let Some(ref cb) = *close_cb.borrow() {
                        cb(tab_idx);
                    }
                });
                menu_vbox.append(&close_item);

                pop.set_child(Some(&menu_vbox));
                *active_pop_ref.borrow_mut() = Some(pop.clone());
                pop.popup();
            });
            row_box.add_controller(right_click);

            self.tabs_list_box.append(&row_box);
        }

        self.footer_label
            .set_label(&format!("{n_tabs} open tabs · Right-click for actions"));
    }

    /// Workspace compatibility methods (delegate clean no-ops or simple stores)
    pub fn add_workspace_item(&self, _ws_id: &str, _name: &str) {}

    pub fn set_on_create_workspace<F: Fn() + 'static>(&self, callback: F) {
        *self.on_create_workspace.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_on_workspace_selected<F: Fn(&str) + 'static>(&self, callback: F) {
        *self.on_workspace_selected.borrow_mut() = Some(Box::new(callback));
    }

    pub fn connect_page_selected<F: Fn(&str) + 'static>(&self, _callback: F) {}
}

impl Default for ShadhinSidebar {
    fn default() -> Self {
        Self::new()
    }
}
