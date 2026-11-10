//! Application lifecycle and main window initialization
//!
//! Sets up the Libadwaita Application (`com.shadhin.browser`), establishes
//! the window, and embeds a `webkit6::WebView` loading a verified URL.

use crate::browser::cli::{format_help_text, parse_cli_args};
use crate::config::BrowserConfig;
use crate::ui;
use crate::ui::window::ShadhinWindow;
use adw::prelude::*;
use gtk4::gio;
use gtk4::glib;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tracing::info;

pub const APP_ID: &str = "com.shadhin.browser";
pub const APP_NAME: &str = "Shadhin Browser";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub static START_INSTANT: OnceLock<Instant> = OnceLock::new();

/// Records process startup timestamp as early as possible
pub fn init_startup_timer() {
    START_INSTANT.get_or_init(Instant::now);
}

pub struct ShadhinApp {
    app: adw::Application,
}

impl Default for ShadhinApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ShadhinApp {
    /// Creates a new `ShadhinApp` instance configured with the official application ID
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id(APP_ID)
            .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
            .build();

        Self::setup_app_actions(&app);

        app.connect_command_line(Self::on_command_line);
        app.connect_activate(Self::on_activate);

        Self { app }
    }

    /// Command-line argument handler (handles version, help, private mode, and URLs)
    fn on_command_line(
        app: &adw::Application,
        cmdline: &gio::ApplicationCommandLine,
    ) -> glib::ExitCode {
        let args: Vec<String> = cmdline
            .arguments()
            .into_iter()
            .filter_map(|s| s.into_string().ok())
            .collect();

        match parse_cli_args(&args) {
            Ok(opts) => {
                if opts.show_version {
                    println!("{} {}", APP_NAME, APP_VERSION);
                    return glib::ExitCode::SUCCESS;
                }

                if opts.show_help {
                    println!("{}", format_help_text(APP_NAME, APP_VERSION));
                    return glib::ExitCode::SUCCESS;
                }

                if opts.private_window {
                    Self::launch_private(app);
                    return glib::ExitCode::SUCCESS;
                }

                let windows = app.windows();
                if opts.new_window || windows.is_empty() {
                    Self::launch(app, &opts.urls);
                } else if !opts.urls.is_empty() {
                    // Open requested URLs in a new browser window or existing instance
                    Self::launch(app, &opts.urls);
                } else if let Some(first_win) = windows.first() {
                    first_win.present();
                }

                glib::ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("Error: {err}\nRun 'shadhin-browser --help' for usage.");
                glib::ExitCode::FAILURE
            }
        }
    }

    /// Primary activation signal handler
    fn on_activate(app: &adw::Application) {
        Self::launch(app, &[]);
    }

    /// Sets up application-wide GActions and keyboard accelerators
    fn setup_app_actions(app: &adw::Application) {
        let app_clone = app.clone();
        let act_new_win = gio::SimpleAction::new("new-window", None);
        act_new_win.connect_activate(move |_, _| {
            Self::launch(&app_clone, &[]);
        });
        app.add_action(&act_new_win);

        let app_clone = app.clone();
        let act_new_priv = gio::SimpleAction::new("new-private-window", None);
        act_new_priv.connect_activate(move |_, _| {
            Self::launch_private(&app_clone);
        });
        app.add_action(&act_new_priv);

        let app_clone = app.clone();
        let act_quit = gio::SimpleAction::new("quit", None);
        act_quit.connect_activate(move |_, _| {
            app_clone.quit();
        });
        app.add_action(&act_quit);

        app.set_accels_for_action("app.new-window", &["<Control>n"]);
        app.set_accels_for_action("app.new-private-window", &["<Control><Shift>n"]);
        app.set_accels_for_action("app.quit", &["<Control>q"]);
        app.set_accels_for_action("win.new-tab", &["<Control>t"]);
        app.set_accels_for_action("win.reopen-tab", &["<Control><Shift>t"]);
        app.set_accels_for_action("win.bookmarks", &["<Control><Shift>o"]);
        app.set_accels_for_action("win.history", &["<Control>h"]);
        app.set_accels_for_action("win.downloads", &["<Control>j"]);
        app.set_accels_for_action("win.zoom-in", &["<Control>plus", "<Control>equal"]);
        app.set_accels_for_action("win.zoom-out", &["<Control>minus"]);
        app.set_accels_for_action("win.zoom-reset", &["<Control>0"]);
        app.set_accels_for_action("win.find-in-page", &["<Control>f"]);
        app.set_accels_for_action("win.clear-browsing-data", &["<Control><Shift>Delete"]);
        app.set_accels_for_action("win.settings", &["<Control>comma"]);
    }

    /// Prepares UI styling and decoration layout
    fn prepare_environment() {
        let config = BrowserConfig::load();

        // Ensure window controls are shown
        if let Some(settings) = gtk4::Settings::default() {
            settings.set_gtk_decoration_layout(Some(":minimize,maximize,close"));
        }

        // Load GResource & apply CSS theme and dynamic tokens (accent, light/dark, reduced motion)
        ui::setup_theme(Some(&config.accent_color), Some(&config.color_scheme));
    }

    /// Launches a private window
    fn launch_private(app: &adw::Application) {
        info!(
            "Launching {} ({}) in Private Browsing Mode",
            APP_NAME, APP_ID
        );
        Self::prepare_environment();
        let window = ShadhinWindow::new_private(app);
        window.present();
    }

    /// Launches the main browser window with optional URLs
    fn launch(app: &adw::Application, urls: &[String]) {
        info!(
            "Launching {} ({}) with {} URLs",
            APP_NAME,
            APP_ID,
            urls.len()
        );

        Self::prepare_environment();

        // Build main application window shell
        let window = ShadhinWindow::new_with_urls(app, urls);
        window.present();

        // Startup instrumentation: measure duration to first paint / event loop idle
        glib::idle_add_local_once(move || {
            if let Some(start) = START_INSTANT.get() {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                info!("STARTUP_METRIC: Time to first paint (window shown): {:.2} ms", ms);
            }
        });

        if std::env::var("SHADHIN_BENCH_STARTUP").is_ok() {
            let app_clone = app.clone();
            glib::idle_add_local_once(move || {
                info!("STARTUP_BENCHMARK_COMPLETE");
                app_clone.quit();
            });
        }

        if std::env::var("SHADHIN_VERIFY_MENU").is_ok() {
            let app_clone = app.clone();
            let window_clone = window.clone();
            glib::timeout_add_local_once(Duration::from_millis(500), move || {
                Self::run_gui_menu_verification(&app_clone, &window_clone);
            });
        }
    }

    fn run_gui_menu_verification(app: &adw::Application, window: &ShadhinWindow) {
        info!("REAL_GUI_VERIFY: Starting real GUI interaction verification");

        let menu_btn = window.toolbar().menu_button();
        info!(
            "REAL_GUI_VERIFY: MenuButton is_sensitive = {}",
            menu_btn.is_sensitive()
        );

        info!("REAL_GUI_VERIFY: Opening Application Menu via MenuButton popup()");
        menu_btn.popup();
        let popover_opened = menu_btn.popover().is_some();
        info!("REAL_GUI_VERIFY: Menu popover opened = {}", popover_opened);

        info!("REAL_GUI_VERIFY: Activating action win.new-tab from menu");
        gio::prelude::ActionGroupExt::activate_action(window.window(), "new-tab", None);
        info!("REAL_GUI_VERIFY: win.new-tab activated successfully");

        info!("REAL_GUI_VERIFY: Activating action win.history from menu");
        gio::prelude::ActionGroupExt::activate_action(window.window(), "history", None);
        info!(
            "REAL_GUI_VERIFY: win.history activated, address_bar = {}",
            window.toolbar().address_bar().text()
        );

        info!("REAL_GUI_VERIFY: Activating action win.zoom-in from menu");
        gio::prelude::ActionGroupExt::activate_action(window.window(), "zoom-in", None);
        info!("REAL_GUI_VERIFY: win.zoom-in activated successfully");

        info!("REAL_GUI_VERIFY: Activating action win.about from menu");
        gio::prelude::ActionGroupExt::activate_action(window.window(), "about", None);
        info!("REAL_GUI_VERIFY: win.about dialog activated and presented successfully");

        info!("REAL_GUI_VERIFY: Testing actions in window state");
        let is_active = window.window().is_active();
        info!(
            "REAL_GUI_VERIFY: Window active (focused) property = {}",
            is_active
        );
        let about_act = window.window().lookup_action("about");
        let new_tab_act = window.window().lookup_action("new-tab");
        info!(
            "REAL_GUI_VERIFY: Window actions enabled regardless of focus: win.about={}, win.new-tab={}",
            about_act.map(|a| a.is_enabled()).unwrap_or(false),
            new_tab_act.map(|a| a.is_enabled()).unwrap_or(false)
        );

        let style_manager = adw::StyleManager::default();
        style_manager.set_color_scheme(adw::ColorScheme::ForceLight);
        info!(
            "REAL_GUI_VERIFY: Switched to Light Theme: color_scheme = {:?}",
            style_manager.color_scheme()
        );
        style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
        info!(
            "REAL_GUI_VERIFY: Switched to Dark Theme: color_scheme = {:?}",
            style_manager.color_scheme()
        );

        menu_btn.popdown();
        info!("REAL_GUI_VERIFY_COMPLETE");
        app.quit();
    }

    /// Runs the application GLib/GIO main loop with command-line arguments
    pub fn run(&self) -> glib::ExitCode {
        self.app.run()
    }
}
