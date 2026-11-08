//! Browser subsystem module
//!
//! Encompasses webview management, navigation lifecycle, multi-tab coordination,
//! download workflows, runtime permissions, and session persistence.

pub mod cli;
pub mod downloads;
pub mod navigation;
pub mod permissions;
pub mod sandbox;
pub mod session;
pub mod tabs;
pub mod webview;
