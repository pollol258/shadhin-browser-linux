//! User Interface subsystem for Shadhin Browser
//!
//! Provides the Libadwaita window chrome, glassmorphic styling, tabs,
//! address bar, sidebar navigation, New Tab dashboard, and privacy popover.

#![allow(clippy::type_complexity)]

pub mod addressbar;
pub mod bookmarks_bar;
pub mod bookmarks_panel;
pub mod clear_data;
pub mod downloads_panel;
pub mod downloads_popover;
pub mod findbar;
pub mod history_panel;
pub mod newtab;
pub mod privacy_panel;
pub mod settings;
pub mod sidebar;
pub mod tabbar;
pub mod theme;
pub mod toolbar;
pub mod window;

use gtk4::gdk::Display;
use gtk4::gio;
use gtk4::glib;
use theme::ThemeManager;
use tracing::info;

/// Initializes the GResource bundle and applies the Shadhin tokens GTK4 CSS theme.
pub fn setup_theme(initial_accent: Option<&str>, color_scheme: Option<&str>) -> ThemeManager {
    // 1. Register compiled GResource bundle
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/shadhin.gresource"));
    match gio::Resource::from_data(&glib::Bytes::from_static(bytes)) {
        Ok(resource) => {
            gio::resources_register(&resource);
            info!("Registered Shadhin GResource bundle successfully");
        }
        Err(e) => {
            tracing::error!("Failed to register embedded GResource: {e}");
        }
    }

    // 2. Load Base CSS provider from GResource or local file
    let provider = gtk4::CssProvider::new();
    let dev_mode = std::env::var("SHADHIN_DEV_CSS").is_ok();

    if dev_mode && std::path::Path::new("resources/styles/shadhin.css").exists() {
        info!("SHADHIN_DEV_CSS enabled: loading live CSS from resources/styles/shadhin.css");
        provider.load_from_path("resources/styles/shadhin.css");
    } else {
        info!("Loading CSS theme from GResource /com/shadhin/browser/styles/shadhin.css");
        provider.load_from_resource("/com/shadhin/browser/styles/shadhin.css");
    }

    // 3. Attach provider to the default display
    if let Some(display) = Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // 4. Initialize dynamic tokens provider (accent, light/dark, reduced motion)
    let theme_mgr = ThemeManager::new();
    theme_mgr.init(initial_accent, color_scheme);
    theme::set_global_theme_manager(theme_mgr.clone());
    theme_mgr
}
