//! Shadhin Browser (স্বাধীন ব্রাউজার)
//! Fast · Private · Open
//!
//! Main entry point for the Linux desktop web browser.

#![allow(clippy::type_complexity)]
#![allow(clippy::suspicious_open_options)]

use gtk4::glib;
use shadhin_browser::app::{APP_NAME, APP_VERSION, ShadhinApp};
use shadhin_browser::browser::cli::{format_help_text, parse_cli_args};
use shadhin_browser::utils::logging::init_logging;

fn main() -> glib::ExitCode {
    shadhin_browser::app::init_startup_timer();
    let args: Vec<String> = std::env::args().collect();
    if let Ok(opts) = parse_cli_args(&args) {
        if opts.show_version {
            println!("{} {}", APP_NAME, APP_VERSION);
            return glib::ExitCode::SUCCESS;
        }
        if opts.show_help {
            println!("{}", format_help_text(APP_NAME, APP_VERSION));
            return glib::ExitCode::SUCCESS;
        }
    }

    if let Err(e) = init_logging() {
        eprintln!("Warning: Failed to initialize tracing: {e}");
    }

    let application = ShadhinApp::new();
    application.run()
}
