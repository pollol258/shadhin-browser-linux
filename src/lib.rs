//! Shadhin Browser (স্বাধীন ব্রাউজার) core library components
//!
//! Exposes modular browser subsystems for integration testing, benchmarking, and binary execution:
//! - `app`: Main Libadwaita application lifecycle and command-line processing
//! - `browser`: Tab management, sessions, navigation sanitization, downloads, and webview settings
//! - `config`: Browser configuration persistence, themes, and sidebar state
//! - `privacy`: Content filtering, adblock, tracking strip, cookie policies, and stats
//! - `storage`: SQLite database engine, schema migrations, and asynchronous query dispatch
//! - `ui`: Libadwaita window, toolbar, tabbar, sidebar, and panels
//! - `utils`: Debounce timing, logging, and helpers

#![allow(dead_code)]
#![allow(clippy::type_complexity)]
#![allow(clippy::suspicious_open_options)]

pub mod app;
pub mod browser;
pub mod config;
pub mod platform;
pub mod privacy;
pub mod storage;
pub mod ui;
pub mod utils;
