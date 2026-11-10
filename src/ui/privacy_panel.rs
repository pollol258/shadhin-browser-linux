//! Privacy Shield popover and settings panel
//!
//! Provides the primary user-facing security control center. Toggles bind to
//! `BrowserConfig` (`~/.config/shadhin-browser/config.toml`) and live WebKit filter states.

use crate::config::BrowserConfig;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use tracing::info;

pub struct PrivacyPopover {
    popover: gtk4::Popover,
    current_domain: Rc<RefCell<Option<String>>>,
    domain_label: gtk4::Label,
    site_switch: gtk4::Switch,
    site_status_label: gtk4::Label,
    filter_status_label: gtk4::Label,
    trackers_count_label: gtk4::Label,
    trackers_box: gtk4::Box,
    on_private_window: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_clear_data: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_open_settings: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_toggle_site: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>>,
    on_toggle_master: Rc<RefCell<Option<Box<dyn Fn(bool)>>>>,
}

impl PrivacyPopover {
    /// Creates a new Privacy Shield popover anchored to a parent toolbar button
    pub fn new(anchor: &impl IsA<gtk4::Widget>) -> Self {
        let popover = gtk4::Popover::builder()
            .has_arrow(true)
            .autohide(true)
            .css_classes(["shadhin-popover"])
            .build();

        popover.set_parent(anchor);

        let config = Rc::new(RefCell::new(BrowserConfig::load()));
        let current_domain = Rc::new(RefCell::new(None::<String>));

        let main_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .width_request(290)
            .build();

        // 1. Header with Master Switch
        let header_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["shadhin-popover-header"])
            .build();

        let shield_icon = gtk4::Image::from_icon_name("security-high-symbolic");
        header_row.append(&shield_icon);

        let title = gtk4::Label::builder()
            .label("Privacy Shield")
            .hexpand(true)
            .halign(gtk4::Align::Start)
            .css_classes(["shadhin-popover-header"])
            .build();
        header_row.append(&title);

        let master_switch = gtk4::Switch::builder()
            .active(config.borrow().privacy.shield_enabled)
            .valign(gtk4::Align::Center)
            .tooltip_text("Master switch: enable or disable all privacy protections")
            .build();

        header_row.append(&master_switch);
        main_box.append(&header_row);

        // 2. Per-Site Section
        let site_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .margin_top(4)
            .margin_bottom(4)
            .css_classes(["shadhin-card"])
            .build();

        let domain_label = gtk4::Label::builder()
            .label("No active site")
            .halign(gtk4::Align::Start)
            .css_classes(["shadhin-popover-label"])
            .build();
        site_box.append(&domain_label);

        let site_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .build();

        let site_status_label = gtk4::Label::builder()
            .label("Shield active on this site")
            .hexpand(true)
            .halign(gtk4::Align::Start)
            .css_classes(["shadhin-stat-label"])
            .build();
        site_row.append(&site_status_label);

        let site_switch = gtk4::Switch::builder()
            .active(true)
            .valign(gtk4::Align::Center)
            .tooltip_text("Turn shield on or off specifically for this site")
            .build();
        site_row.append(&site_switch);

        site_box.append(&site_row);

        let filter_status_label = gtk4::Label::builder()
            .label("Content Filter: Active")
            .halign(gtk4::Align::Start)
            .css_classes(["font-xs", "dim-label"])
            .build();
        site_box.append(&filter_status_label);

        let trackers_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .margin_top(2)
            .build();

        let track_icon = gtk4::Image::from_icon_name("security-high-symbolic");
        track_icon.set_pixel_size(14);
        trackers_box.append(&track_icon);

        let trackers_count_label = gtk4::Label::builder()
            .label("Trackers blocked: 0")
            .halign(gtk4::Align::Start)
            .css_classes(["font-xs"])
            .build();
        trackers_box.append(&trackers_count_label);
        site_box.append(&trackers_box);

        main_box.append(&site_box);

        // 3. Toggles Group
        let rows = [
            (
                "Block Trackers",
                "block_trackers",
                "Blocks cross-site tracking scripts and telemetry beacons",
            ),
            (
                "Block Ads",
                "block_ads",
                "Blocks advertisements and commercial banner domains",
            ),
            (
                "Fingerprinting Protection (limited)",
                "block_fingerprinting",
                "Restricts known canvas and hardware probing; Brave-style randomization not supported on WebKitGTK",
            ),
            (
                "HTTPS-Only Mode",
                "https_only",
                "Upgrades insecure HTTP main-frame navigations to HTTPS",
            ),
            (
                "Cookie Protection",
                "block_third_party_cookies",
                "Blocks third-party and cross-site tracking cookies",
            ),
        ];

        for (label_text, key, tooltip) in rows {
            let row = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .spacing(12)
                .css_classes(["shadhin-popover-row"])
                .tooltip_text(tooltip)
                .build();

            let label = gtk4::Label::builder()
                .label(label_text)
                .hexpand(true)
                .halign(gtk4::Align::Start)
                .css_classes(["shadhin-popover-label"])
                .build();
            row.append(&label);

            let initial_state = match key {
                "block_trackers" => config.borrow().privacy.block_trackers,
                "block_ads" => config.borrow().privacy.block_ads,
                "block_fingerprinting" => config.borrow().privacy.block_fingerprinting,
                "https_only" => config.borrow().privacy.https_only,
                "block_third_party_cookies" => config.borrow().privacy.block_third_party_cookies,
                _ => true,
            };

            let sw = gtk4::Switch::builder()
                .active(initial_state)
                .valign(gtk4::Align::Center)
                .build();

            let cfg_row = config.clone();
            let k = key.to_string();
            let lbl = label_text.to_string();
            sw.connect_active_notify(move |switch_widget| {
                let active = switch_widget.is_active();
                let mut c = cfg_row.borrow_mut();
                match k.as_str() {
                    "block_trackers" => c.privacy.block_trackers = active,
                    "block_ads" => c.privacy.block_ads = active,
                    "block_fingerprinting" => c.privacy.block_fingerprinting = active,
                    "https_only" => c.privacy.https_only = active,
                    "block_third_party_cookies" => c.privacy.block_third_party_cookies = active,
                    _ => {}
                }
                let _ = c.save();
                info!("Privacy setting '{lbl}' toggled: {active}");
            });

            row.append(&sw);
            main_box.append(&row);
        }

        // 4. Action Buttons
        let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        main_box.append(&sep);

        let on_private_window: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_clear_data: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_open_settings: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_toggle_site: Rc<RefCell<Option<Box<dyn Fn(&str, bool)>>>> =
            Rc::new(RefCell::new(None));
        let on_toggle_master: Rc<RefCell<Option<Box<dyn Fn(bool)>>>> = Rc::new(RefCell::new(None));

        // Connect master switch
        let cfg_master = config.clone();
        let otm = on_toggle_master.clone();
        master_switch.connect_active_notify(move |sw| {
            let active = sw.is_active();
            cfg_master.borrow_mut().privacy.shield_enabled = active;
            let _ = cfg_master.borrow().save();
            info!("Privacy Shield master switch toggled: {active}");
            if let Some(ref cb) = *otm.borrow() {
                cb(active);
            }
        });

        // Connect site switch
        let ots = on_toggle_site.clone();
        let cur_dom = current_domain.clone();
        let ssl = site_status_label.clone();
        let fsl_clone = filter_status_label.clone();
        site_switch.connect_active_notify(move |sw| {
            let active = sw.is_active();
            if active {
                ssl.set_label("Shield active on this site");
                fsl_clone.set_label("Content Filter: Active");
            } else {
                ssl.set_label("Shield off for this site");
                fsl_clone.set_label("Content Filter: Disabled for this site");
            }

            if let Some(ref domain) = *cur_dom.borrow()
                && let Some(ref cb) = *ots.borrow()
            {
                cb(domain, active);
            }
        });

        // Private Window action
        let pvt_btn = gtk4::Button::builder()
            .label("Private Window (Ctrl+Shift+N)")
            .halign(gtk4::Align::Fill)
            .css_classes(["shadhin-popover-action"])
            .build();
        let opw = on_private_window.clone();
        let pop_hide1 = popover.clone();
        pvt_btn.connect_clicked(move |_| {
            pop_hide1.popdown();
            if let Some(ref cb) = *opw.borrow() {
                cb();
            }
        });
        main_box.append(&pvt_btn);

        // Clear Data action
        let clr_btn = gtk4::Button::builder()
            .label("Clear Browsing Data… (Ctrl+Shift+Del)")
            .halign(gtk4::Align::Fill)
            .css_classes(["shadhin-popover-action"])
            .build();
        let ocd = on_clear_data.clone();
        let pop_hide2 = popover.clone();
        clr_btn.connect_clicked(move |_| {
            pop_hide2.popdown();
            if let Some(ref cb) = *ocd.borrow() {
                cb();
            }
        });
        main_box.append(&clr_btn);

        // Privacy Settings action
        let set_btn = gtk4::Button::builder()
            .label("Privacy Settings")
            .halign(gtk4::Align::Fill)
            .css_classes(["shadhin-popover-action"])
            .build();
        let ops = on_open_settings.clone();
        let pop_hide3 = popover.clone();
        set_btn.connect_clicked(move |_| {
            pop_hide3.popdown();
            if let Some(ref cb) = *ops.borrow() {
                cb();
            }
        });
        main_box.append(&set_btn);

        popover.set_child(Some(&main_box));

        Self {
            popover,
            current_domain,
            domain_label,
            site_switch,
            site_status_label,
            filter_status_label,
            trackers_count_label,
            trackers_box,
            on_private_window,
            on_clear_data,
            on_open_settings,
            on_toggle_site,
            on_toggle_master,
        }
    }

    /// Updates site-specific labels and switch state when the active tab navigates
    pub fn update_for_site(&self, domain_opt: Option<&str>, is_shield_enabled: bool) {
        self.update_block_status(domain_opt, is_shield_enabled, 0, is_shield_enabled);
    }

    /// Updates site-specific block status and tracker counts based on real content filter
    pub fn update_block_status(
        &self,
        domain_opt: Option<&str>,
        is_shield_enabled: bool,
        trackers_blocked: u32,
        filter_active: bool,
    ) {
        if let Some(domain) = domain_opt {
            *self.current_domain.borrow_mut() = Some(domain.to_string());
            self.domain_label.set_label(domain);
            self.site_switch.set_active(is_shield_enabled);
            if is_shield_enabled && filter_active {
                self.site_status_label
                    .set_label("Shield active on this site");
                self.filter_status_label
                    .set_label("Content Filter: Active");
            } else if !is_shield_enabled {
                self.site_status_label
                    .set_label("Shield off for this site");
                self.filter_status_label
                    .set_label("Content Filter: Disabled for this site");
            } else {
                self.site_status_label
                    .set_label("Shield active (Filter paused)");
                self.filter_status_label
                    .set_label("Content Filter: Paused");
            }

            self.trackers_count_label
                .set_label(&format!("Trackers blocked: {trackers_blocked}"));
            self.trackers_box.set_visible(true);
        } else {
            *self.current_domain.borrow_mut() = None;
            self.domain_label.set_label("No active site");
            self.site_status_label.set_label("Internal page");
            self.filter_status_label.set_label("Content Filter: N/A");
            self.site_switch.set_active(true);
            self.trackers_box.set_visible(false);
        }
    }

    pub fn set_on_private_window<F: Fn() + 'static>(&self, f: F) {
        *self.on_private_window.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_clear_data<F: Fn() + 'static>(&self, f: F) {
        *self.on_clear_data.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_open_settings<F: Fn() + 'static>(&self, f: F) {
        *self.on_open_settings.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_toggle_site<F: Fn(&str, bool) + 'static>(&self, f: F) {
        *self.on_toggle_site.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_toggle_master<F: Fn(bool) + 'static>(&self, f: F) {
        *self.on_toggle_master.borrow_mut() = Some(Box::new(f));
    }

    /// Toggles the popover visibility
    pub fn toggle(&self) {
        if self.popover.is_visible() {
            self.popover.popdown();
        } else {
            self.popover.popup();
        }
    }

    /// Accessor to the underlying GTK popover widget
    pub fn popover(&self) -> &gtk4::Popover {
        &self.popover
    }

    /// Explicitly dismisses and unparents the privacy popover
    pub fn cleanup(&self) {
        self.popover.popdown();
        self.popover.unparent();
    }
}
