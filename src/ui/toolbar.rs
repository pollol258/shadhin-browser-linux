//! Navigation toolbar / header bar containing navigation, address pill, downloads, and app menu
//!
//! Styled according to HANDOFF.md (56 px tall, icon buttons 36x36 px, icon 17 px, radius 8 px).

use crate::ui::addressbar::AddressBar;
use crate::ui::privacy_panel::PrivacyPopover;
use adw::prelude::*;
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;

pub struct ShadhinToolbar {
    container: adw::HeaderBar,
    back_btn: gtk4::Button,
    forward_btn: gtk4::Button,
    reload_btn: gtk4::Button,
    reload_icon: gtk4::Image,
    sidebar_toggle_btn: gtk4::Button,
    address_bar: Rc<AddressBar>,
    privacy_popover: Rc<PrivacyPopover>,
    download_btn: gtk4::Button,
    downloads_badge: gtk4::Label,
    new_tab_btn: gtk4::Button,
    menu_btn: gtk4::MenuButton,
    is_loading: Rc<RefCell<bool>>,
}

impl ShadhinToolbar {
    /// Constructs a new 56px navigation toolbar matching prototype header
    pub fn new() -> Self {
        let container = adw::HeaderBar::builder()
            .css_classes(["shadhin-header"])
            .show_title(false)
            .build();
        container.set_size_request(-1, 56);

        // --- 1. Left Cluster: Sidebar Toggle, Back, Forward, Reload/Stop ---
        let left_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk4::Align::Center)
            .build();

        let sidebar_icon = gtk4::Image::from_icon_name("view-sidebar-symbolic");
        sidebar_icon.set_pixel_size(17);
        let sidebar_toggle_btn = gtk4::Button::builder()
            .child(&sidebar_icon)
            .tooltip_text("Toggle tab sidebar")
            .css_classes(["iconbtn", "shadhin-nav-btn", "flat"])
            .valign(gtk4::Align::Center)
            .build();
        sidebar_toggle_btn.set_size_request(36, 36);
        left_box.append(&sidebar_toggle_btn);

        let back_icon = gtk4::Image::from_icon_name("go-previous-symbolic");
        back_icon.set_pixel_size(17);
        let back_btn = gtk4::Button::builder()
            .child(&back_icon)
            .tooltip_text("Go back (Alt+Left)")
            .css_classes(["iconbtn", "shadhin-nav-btn", "flat"])
            .valign(gtk4::Align::Center)
            .sensitive(false)
            .build();
        back_btn.set_size_request(36, 36);
        left_box.append(&back_btn);

        let forward_icon = gtk4::Image::from_icon_name("go-next-symbolic");
        forward_icon.set_pixel_size(17);
        let forward_btn = gtk4::Button::builder()
            .child(&forward_icon)
            .tooltip_text("Go forward (Alt+Right)")
            .css_classes(["iconbtn", "shadhin-nav-btn", "flat"])
            .valign(gtk4::Align::Center)
            .sensitive(false)
            .build();
        forward_btn.set_size_request(36, 36);
        left_box.append(&forward_btn);

        let reload_icon = gtk4::Image::from_icon_name("view-refresh-symbolic");
        reload_icon.set_pixel_size(17);
        let reload_btn = gtk4::Button::builder()
            .child(&reload_icon)
            .tooltip_text("Reload page (Ctrl+R)")
            .css_classes(["iconbtn", "shadhin-nav-btn", "flat"])
            .valign(gtk4::Align::Center)
            .build();
        reload_btn.set_size_request(36, 36);
        left_box.append(&reload_btn);

        container.pack_start(&left_box);

        // --- 2. Center: Centered Pill Address Bar (min(680px, 58vw)) ---
        let address_bar = Rc::new(AddressBar::new());
        let addr_widget = address_bar.widget();
        addr_widget.set_hexpand(true);
        addr_widget.set_halign(gtk4::Align::Center);
        addr_widget.set_size_request(460, 39);
        container.set_title_widget(Some(addr_widget));

        // Anchor Privacy Popover to the Lock button inside the Address Bar
        let privacy_popover = Rc::new(PrivacyPopover::new(address_bar.lock_button()));
        let pop_clone = privacy_popover.clone();
        address_bar.lock_button().connect_clicked(move |_| {
            pop_clone.toggle();
        });

        // --- 3. Right Cluster: Downloads, New Tab, Menu ---
        let right_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk4::Align::Center)
            .build();

        // Downloads button with badge overlay
        let dl_icon = gtk4::Image::from_icon_name("folder-download-symbolic");
        dl_icon.set_pixel_size(17);
        let download_btn = gtk4::Button::builder()
            .child(&dl_icon)
            .tooltip_text("Downloads (Ctrl+J)")
            .css_classes(["iconbtn", "shadhin-nav-btn", "flat"])
            .valign(gtk4::Align::Center)
            .build();
        download_btn.set_size_request(36, 36);

        let downloads_badge = gtk4::Label::builder()
            .css_classes(["count-badge"])
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::Start)
            .visible(false)
            .build();

        let dl_overlay = gtk4::Overlay::new();
        dl_overlay.set_child(Some(&download_btn));
        dl_overlay.add_overlay(&downloads_badge);
        right_box.append(&dl_overlay);

        // New Tab button (+)
        let new_tab_icon = gtk4::Image::from_icon_name("list-add-symbolic");
        new_tab_icon.set_pixel_size(17);
        let new_tab_btn = gtk4::Button::builder()
            .child(&new_tab_icon)
            .tooltip_text("New tab (Ctrl+T)")
            .css_classes(["iconbtn", "shadhin-nav-btn", "flat"])
            .valign(gtk4::Align::Center)
            .build();
        new_tab_btn.set_size_request(36, 36);
        right_box.append(&new_tab_btn);

        // Main Menu button
        let menu_btn = gtk4::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Main menu")
            .css_classes(["iconbtn", "shadhin-nav-btn", "flat"])
            .always_show_arrow(false)
            .valign(gtk4::Align::Center)
            .build();
        menu_btn.set_size_request(36, 36);
        right_box.append(&menu_btn);

        container.pack_end(&right_box);

        let is_loading = Rc::new(RefCell::new(false));

        Self {
            container,
            back_btn,
            forward_btn,
            reload_btn,
            reload_icon,
            sidebar_toggle_btn,
            address_bar,
            privacy_popover,
            download_btn,
            downloads_badge,
            new_tab_btn,
            menu_btn,
            is_loading,
        }
    }

    /// Updates back button sensitivity
    pub fn set_can_go_back(&self, can: bool) {
        self.back_btn.set_sensitive(can);
    }

    /// Updates forward button sensitivity
    pub fn set_can_go_forward(&self, can: bool) {
        self.forward_btn.set_sensitive(can);
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

    /// Connects the back button clicked signal
    pub fn connect_back<F: Fn() + 'static>(&self, callback: F) {
        self.back_btn.connect_clicked(move |_| callback());
    }

    /// Connects the forward button clicked signal
    pub fn connect_forward<F: Fn() + 'static>(&self, callback: F) {
        self.forward_btn.connect_clicked(move |_| callback());
    }

    /// Connects the reload/stop button clicked signal
    pub fn connect_reload<F: Fn(bool) + 'static>(&self, callback: F) {
        let is_loading = self.is_loading.clone();
        self.reload_btn.connect_clicked(move |_| {
            let loading = *is_loading.borrow();
            callback(loading);
        });
    }

    /// Connects the sidebar toggle button clicked signal
    pub fn connect_sidebar_toggle<F: Fn() + 'static>(&self, callback: F) {
        self.sidebar_toggle_btn.connect_clicked(move |_| callback());
    }

    /// Connects the download button clicked signal
    pub fn connect_downloads<F: Fn() + 'static>(&self, callback: F) {
        self.download_btn.connect_clicked(move |_| callback());
    }

    /// Connects the new tab button clicked signal
    pub fn connect_new_tab<F: Fn() + 'static>(&self, callback: F) {
        self.new_tab_btn.connect_clicked(move |_| callback());
    }

    /// Updates the download count badge
    pub fn set_download_count(&self, count: usize) {
        if count > 0 {
            self.downloads_badge.set_text(&count.to_string());
            self.downloads_badge.set_visible(true);
        } else {
            self.downloads_badge.set_visible(false);
        }
    }

    /// Accessor to the address bar
    pub fn address_bar(&self) -> &AddressBar {
        &self.address_bar
    }

    /// Accessor to the privacy shield popover
    pub fn privacy_popover(&self) -> &PrivacyPopover {
        &self.privacy_popover
    }

    /// Updates the sidebar toggle button active state
    pub fn update_sidebar_active(&self, active: bool) {
        if active {
            self.sidebar_toggle_btn.add_css_class("active");
        } else {
            self.sidebar_toggle_btn.remove_css_class("active");
        }
    }

    /// Toggles visual private browsing style on the toolbar
    pub fn set_private_mode(&self, is_private: bool) {
        if is_private {
            self.container.add_css_class("shadhin-private-toolbar");
        } else {
            self.container.remove_css_class("shadhin-private-toolbar");
        }
    }

    /// Accessor to the root toolbar / header bar widget
    pub fn widget(&self) -> &adw::HeaderBar {
        &self.container
    }

    /// Accessor to the download button
    pub fn download_button(&self) -> &gtk4::Button {
        &self.download_btn
    }

    /// Accessor to the new tab button
    pub fn new_tab_button(&self) -> &gtk4::Button {
        &self.new_tab_btn
    }

    /// Accessor to the main menu button
    pub fn menu_button(&self) -> &gtk4::MenuButton {
        &self.menu_btn
    }

    /// Sets the menu model for the main menu button
    pub fn set_menu_model<M: glib::object::IsA<gtk4::gio::MenuModel>>(&self, model: &M) {
        self.menu_btn.set_menu_model(Some(model));
    }
}

impl Default for ShadhinToolbar {
    fn default() -> Self {
        Self::new()
    }
}
