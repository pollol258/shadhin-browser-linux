//! Navigation toolbar / header bar containing navigation, address pill, downloads, and app menu
//!
//! Styled according to HANDOFF.md (56 px tall, icon buttons 36x36 px, icon 17 px, radius 8 px).
//! Layout: [sidebar toggle] [back] [forward] ..... [reload/stop][address pill] ..... [download] [menu]

use crate::browser::downloads::DownloadManager;
use crate::storage::StorageService;
use crate::ui::addressbar::AddressBar;
use crate::ui::downloads_popover::DownloadsPopover;
use crate::ui::privacy_panel::PrivacyPopover;
use adw::prelude::*;
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;

pub struct ShadhinToolbar {
    container: adw::HeaderBar,
    center_clamp: adw::Clamp,
    back_btn: gtk4::Button,
    forward_btn: gtk4::Button,
    sidebar_toggle_btn: gtk4::Button,
    address_bar: Rc<AddressBar>,
    privacy_popover: Rc<PrivacyPopover>,
    download_btn: gtk4::Button,
    downloads_badge: gtk4::Label,
    downloads_popover: Rc<RefCell<Option<Rc<DownloadsPopover>>>>,
    menu_btn: gtk4::MenuButton,
}

impl ShadhinToolbar {
    /// Constructs a new 50px navigation toolbar matching prototype header
    pub fn new() -> Self {
        let container = adw::HeaderBar::builder()
            .css_classes(["shadhin-header"])
            .show_title(true)
            .visible(true)
            .build();
        container.set_size_request(-1, 50);

        // --- 1. Left Cluster: Sidebar Toggle, Back, Forward ---
        let left_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();

        let sidebar_icon = gtk4::Image::from_icon_name("sidebar-show-symbolic");
        sidebar_icon.set_pixel_size(16);
        sidebar_icon.set_halign(gtk4::Align::Center);
        sidebar_icon.set_valign(gtk4::Align::Center);
        let sidebar_toggle_btn = gtk4::Button::builder()
            .child(&sidebar_icon)
            .tooltip_text("Toggle tab sidebar")
            .css_classes(["iconbtn", "nav-btn", "shadhin-nav-btn", "sidebar-toggle", "flat"])
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();
        sidebar_toggle_btn.set_size_request(32, 32);
        left_box.append(&sidebar_toggle_btn);

        let back_icon = gtk4::Image::from_icon_name("go-previous-symbolic");
        back_icon.set_pixel_size(16);
        back_icon.set_halign(gtk4::Align::Center);
        back_icon.set_valign(gtk4::Align::Center);
        let back_btn = gtk4::Button::builder()
            .child(&back_icon)
            .tooltip_text("Go back (Alt+Left)")
            .css_classes(["iconbtn", "nav-btn", "shadhin-nav-btn", "flat"])
            .valign(gtk4::Align::Center)
            .sensitive(false)
            .visible(true)
            .build();
        back_btn.set_size_request(32, 32);
        left_box.append(&back_btn);

        let forward_icon = gtk4::Image::from_icon_name("go-next-symbolic");
        forward_icon.set_pixel_size(16);
        forward_icon.set_halign(gtk4::Align::Center);
        forward_icon.set_valign(gtk4::Align::Center);
        let forward_btn = gtk4::Button::builder()
            .child(&forward_icon)
            .tooltip_text("Go forward (Alt+Right)")
            .css_classes(["iconbtn", "nav-btn", "shadhin-nav-btn", "flat"])
            .valign(gtk4::Align::Center)
            .sensitive(false)
            .visible(true)
            .build();
        forward_btn.set_size_request(32, 32);
        left_box.append(&forward_btn);

        container.pack_start(&left_box);

        // --- 2. Center: [address pill with internal reload] wrapped in adw::Clamp ---
        let address_bar = Rc::new(AddressBar::new());
        let addr_widget = address_bar.widget();
        addr_widget.set_hexpand(true);
        addr_widget.set_size_request(360, 35);
        addr_widget.set_visible(true);

        let center_clamp = adw::Clamp::builder()
            .maximum_size(680)
            .tightening_threshold(360)
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();
        center_clamp.set_child(Some(addr_widget));

        container.set_title_widget(Some(&center_clamp));

        // Anchor Privacy Popover to the Lock button inside the Address Bar
        let privacy_popover = Rc::new(PrivacyPopover::new(address_bar.lock_button()));

        // --- 3. Right Cluster: Downloads, Menu (No "+" in header bar!) ---
        let right_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();

        // Downloads button with badge overlay
        let dl_icon = gtk4::Image::from_icon_name("folder-download-symbolic");
        dl_icon.set_pixel_size(16);
        dl_icon.set_halign(gtk4::Align::Center);
        dl_icon.set_valign(gtk4::Align::Center);
        let download_btn = gtk4::Button::builder()
            .child(&dl_icon)
            .tooltip_text("Downloads (Ctrl+J)")
            .css_classes(["iconbtn", "nav-btn", "shadhin-nav-btn", "flat"])
            .valign(gtk4::Align::Center)
            .build();
        download_btn.set_size_request(32, 32);

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

        // Main Menu button
        let menu_icon = gtk4::Image::from_icon_name("open-menu-symbolic");
        menu_icon.set_pixel_size(16);
        menu_icon.set_halign(gtk4::Align::Center);
        menu_icon.set_valign(gtk4::Align::Center);
        let menu_btn = gtk4::MenuButton::builder()
            .child(&menu_icon)
            .tooltip_text("Main menu")
            .css_classes(["iconbtn", "nav-btn", "shadhin-nav-btn", "flat"])
            .always_show_arrow(false)
            .valign(gtk4::Align::Center)
            .build();
        menu_btn.set_size_request(32, 32);
        right_box.append(&menu_btn);

        container.pack_end(&right_box);

        let downloads_popover: Rc<RefCell<Option<Rc<DownloadsPopover>>>> =
            Rc::new(RefCell::new(None));

        // Wire popover mutual exclusivity
        let dl_pop_priv = downloads_popover.clone();
        let menu_priv = menu_btn.clone();
        let addr_priv = address_bar.clone();
        let priv_clone = privacy_popover.clone();
        address_bar.lock_button().connect_clicked(move |_| {
            menu_priv.popdown();
            addr_priv.close_suggestions();
            if let Some(ref dl) = *dl_pop_priv.borrow() {
                dl.popdown();
            }
            priv_clone.toggle();
        });

        let dl_pop_click = downloads_popover.clone();
        let priv_dl = privacy_popover.clone();
        let menu_dl = menu_btn.clone();
        let addr_dl = address_bar.clone();
        download_btn.connect_clicked(move |_| {
            priv_dl.popover().popdown();
            menu_dl.popdown();
            addr_dl.close_suggestions();
            if let Some(ref dl) = *dl_pop_click.borrow() {
                dl.toggle();
            }
        });

        let dl_pop_menu = downloads_popover.clone();
        let priv_menu = privacy_popover.clone();
        let addr_menu = address_bar.clone();
        menu_btn.connect_active_notify(move |mb| {
            if mb.is_active() {
                priv_menu.popover().popdown();
                addr_menu.close_suggestions();
                if let Some(ref dl) = *dl_pop_menu.borrow() {
                    dl.popdown();
                }
            }
        });

        Self {
            container,
            center_clamp,
            back_btn,
            forward_btn,
            sidebar_toggle_btn,
            address_bar,
            privacy_popover,
            download_btn,
            downloads_badge,
            downloads_popover,
            menu_btn,
        }
    }

    /// Attaches the downloads popover with live download manager and storage services
    pub fn attach_downloads(
        &self,
        storage: StorageService,
        download_mgr: DownloadManager,
        session: Option<Rc<RefCell<webkit6::NetworkSession>>>,
        persistence_allowed: bool,
    ) {
        let pop = DownloadsPopover::new(
            &self.download_btn,
            storage,
            download_mgr,
            session,
            persistence_allowed,
        );
        *self.downloads_popover.borrow_mut() = Some(pop);
    }

    /// Toggles the downloads popover
    pub fn toggle_downloads_popover(&self) {
        self.privacy_popover.popover().popdown();
        self.menu_btn.popdown();
        self.address_bar.close_suggestions();
        if let Some(ref dl) = *self.downloads_popover.borrow() {
            dl.toggle();
        }
    }

    /// Dynamically recalculates and sets address pill width: min(680px, 58vw)
    pub fn update_address_pill_width(&self, window_width: i32) {
        let effective_width = if window_width >= 360 { window_width } else { 1280 };
        let target = ((effective_width as f64 * 0.58).round() as i32).clamp(360, 680);
        self.center_clamp.set_maximum_size(target);
        self.address_bar.widget().set_size_request(target.min(460), 35);
    }

    /// Closes all open popovers (main menu, downloads, privacy, address suggestions)
    pub fn close_all_popovers(&self) {
        self.privacy_popover.popover().popdown();
        if let Some(ref dl) = *self.downloads_popover.borrow() {
            dl.popdown();
        }
        self.menu_btn.popdown();
        self.address_bar.close_suggestions();
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
        self.address_bar.set_is_loading(loading);
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
        self.address_bar.connect_reload(callback);
    }

    /// Accessor to the reload button
    pub fn reload_button(&self) -> &gtk4::Button {
        self.address_bar.reload_button()
    }

    /// Connects the sidebar toggle button clicked signal
    pub fn connect_sidebar_toggle<F: Fn() + 'static>(&self, callback: F) {
        self.sidebar_toggle_btn.connect_clicked(move |_| callback());
    }

    /// Connects the download button clicked signal
    pub fn connect_downloads<F: Fn() + 'static>(&self, callback: F) {
        self.download_btn.connect_clicked(move |_| callback());
    }

    /// Connects the new tab button clicked signal (no-op since header bar has no '+' button)
    pub fn connect_new_tab<F: Fn() + 'static>(&self, _callback: F) {}

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

    /// Accessor to the downloads popover
    pub fn downloads_popover(&self) -> Option<Rc<DownloadsPopover>> {
        self.downloads_popover.borrow().clone()
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
