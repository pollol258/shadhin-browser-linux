//! Dynamic Theme and Tokens Management for Shadhin Browser
//!
//! Maps tokens from `HANDOFF.md` and `tokens.css` into GTK CSS.
//! Manages AdwStyleManager integration, user-chosen accent color calculation,
//! readable foreground computation, and reduced-motion adherence.

use gtk4::gdk::Display;
use std::cell::RefCell;
use std::rc::Rc;
use tracing::info;

pub const DEFAULT_ACCENT: &str = "#3584e4";
pub const ACCENT_SWATCHES: [&str; 4] = ["#3584e4", "#26a269", "#9141ac", "#e66100"];

/// Computes the WCAG relative luminance of a hex color string `#rrggbb`
pub fn relative_luminance(hex: &str) -> f64 {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return 0.2; // default safe fallback
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f64 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f64 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f64 / 255.0;

    let to_linear = |c: f64| -> f64 {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };

    0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
}

/// Computes a high-contrast readable foreground color (`#ffffff` or `#000000`) for any accent
pub fn readable_foreground(accent_hex: &str) -> &'static str {
    let lum = relative_luminance(accent_hex);
    if lum > 0.45 {
        "#000000"
    } else {
        "#ffffff"
    }
}

/// Theme manager that keeps track of current style, accent, and animations
#[derive(Clone)]
pub struct ThemeManager {
    dynamic_provider: gtk4::CssProvider,
    current_accent: Rc<RefCell<String>>,
}

thread_local! {
    static GLOBAL_THEME_MGR: RefCell<Option<ThemeManager>> = const { RefCell::new(None) };
}

/// Stores the global theme manager instance for thread-local runtime access
pub fn set_global_theme_manager(mgr: ThemeManager) {
    GLOBAL_THEME_MGR.with(|cell| {
        *cell.borrow_mut() = Some(mgr);
    });
}

/// Updates the application-wide accent color at runtime
pub fn set_accent(accent: &str) {
    GLOBAL_THEME_MGR.with(|cell| {
        if let Some(ref mgr) = *cell.borrow() {
            mgr.set_accent(accent);
        }
    });
}

impl ThemeManager {
    pub fn new() -> Self {
        let dynamic_provider = gtk4::CssProvider::new();
        let current_accent = Rc::new(RefCell::new(DEFAULT_ACCENT.to_string()));

        if let Some(display) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &dynamic_provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 10,
            );
        }

        Self {
            dynamic_provider,
            current_accent,
        }
    }

    /// Sets up theme listeners and applies initial style
    pub fn init(&self, initial_accent: Option<&str>, color_scheme: Option<&str>) {
        if let Some(acc) = initial_accent {
            *self.current_accent.borrow_mut() = acc.to_string();
        }

        let style_manager = adw::StyleManager::default();

        // Apply theme mode override if specified
        match color_scheme {
            Some("dark") => style_manager.set_color_scheme(adw::ColorScheme::ForceDark),
            Some("light") => style_manager.set_color_scheme(adw::ColorScheme::ForceLight),
            _ => style_manager.set_color_scheme(adw::ColorScheme::Default),
        }

        self.refresh_tokens(&style_manager);

        // Listen for system / manual light/dark changes
        let prov_clone = self.dynamic_provider.clone();
        let accent_clone = self.current_accent.clone();
        style_manager.connect_dark_notify(move |sm| {
            let is_dark = sm.is_dark();
            let accent = accent_clone.borrow().clone();
            Self::apply_dynamic_tokens(&prov_clone, is_dark, &accent);
        });
    }

    /// Updates the accent color dynamically and refreshes GTK styles
    pub fn set_accent(&self, accent: &str) {
        *self.current_accent.borrow_mut() = accent.to_string();
        let style_manager = adw::StyleManager::default();
        self.refresh_tokens(&style_manager);
    }

    /// Recomputes and loads token definitions into CSS provider
    pub fn refresh_tokens(&self, style_manager: &adw::StyleManager) {
        let is_dark = style_manager.is_dark();
        let accent = self.current_accent.borrow().clone();
        Self::apply_dynamic_tokens(&self.dynamic_provider, is_dark, &accent);
    }

    fn apply_dynamic_tokens(provider: &gtk4::CssProvider, is_dark: bool, accent: &str) {
        let fg = readable_foreground(accent);

        let (success, warning, error, muted_text, hover_bg, border_color) = if is_dark {
            (
                "#57e389", "#f6d32d", "#ff7b63", "#b5b5b5", "#414141", "#484848",
            )
        } else {
            (
                "#18794e", "#8c6000", "#c01c28", "#606060", "#e7e7e7", "#d4d4d4",
            )
        };

        // Check if animations are globally enabled (reduced motion)
        let animations_enabled = gtk4::Settings::default()
            .map(|s| s.is_gtk_enable_animations())
            .unwrap_or(true);

        let motion_rules = if animations_enabled {
            ""
        } else {
            "* { transition: none !important; animation: none !important; }"
        };

        let css = format!(
            r#"
@define-color accent_bg_color {accent};
@define-color accent_fg_color {fg};
@define-color accent_bg {accent};
@define-color accent_fg {fg};

@define-color success {success};
@define-color warning {warning};
@define-color error {error};
@define-color muted_text {muted_text};
@define-color hover_bg {hover_bg};
@define-color border_color {border_color};

{motion_rules}
"#
        );

        provider.load_from_data(&css);
        info!(
            "Applied dynamic tokens: is_dark={is_dark}, accent={accent}, fg={fg}, animations={animations_enabled}"
        );
    }
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}
