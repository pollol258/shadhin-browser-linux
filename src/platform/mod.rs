//! Cross-platform abstraction layer for Shadhin Browser
//!
//! Separates platform-specific capabilities (content blocking, download hooks,
//! TLS state inspection, site permissions, child process sandboxing, and webview access)
//! behind pure traits, isolating Linux/WebKitGTK 6 implementations under `#[cfg(target_os = "linux")]`.

pub mod fs;
pub mod traits;

#[cfg(target_os = "linux")]
pub mod linux;

pub use fs::*;
pub use traits::*;

#[cfg(target_os = "linux")]
pub use linux::*;
