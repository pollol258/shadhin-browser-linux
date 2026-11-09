//! Cross-platform trait boundaries and abstractions
//!
//! Provides platform-agnostic interfaces for all subsystems that interact with
//! platform-specific or web-engine-specific features:
//! - Content blocking compilation and engine attachment
//! - Download event hooks and native file streams
//! - TLS state inspection, error flags, and host exceptions
//! - Permission request classification and mediation
//! - Sandbox verification and container checks
//! - WebView engine access and lifecycle management
//! - Secure filesystem permission helpers

use std::path::Path;

/// Status of the browser web engine child process sandboxing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformSandboxStatus {
    Active,
    Inactive,
    NoChildProcessYet,
}

/// Category of site permission requested by web content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformPermissionKind {
    Geolocation,
    Notifications,
    Camera,
    Microphone,
    CameraAndMicrophone,
    Clipboard,
    DeviceInfo,
    Other,
}

impl PlatformPermissionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Geolocation => "geolocation",
            Self::Notifications => "notifications",
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::CameraAndMicrophone => "camera_and_microphone",
            Self::Clipboard => "clipboard",
            Self::DeviceInfo => "device_info",
            Self::Other => "other",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Geolocation => "Location Access",
            Self::Notifications => "Notifications",
            Self::Camera => "Camera Access",
            Self::Microphone => "Microphone Access",
            Self::CameraAndMicrophone => "Camera and Microphone",
            Self::Clipboard => "Clipboard Access",
            Self::DeviceInfo => "Device Information",
            Self::Other => "Hardware Access",
        }
    }
}

/// State decision for a permission request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformPermissionDecision {
    Allow,
    Block,
    Ask,
}

/// Normalized TLS certificate and error description
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformTlsInfo {
    pub host: String,
    pub is_valid: bool,
    pub has_tls_errors: bool,
    pub certificate_summary: Option<String>,
    pub error_flags_description: Option<String>,
}

/// Normalized download event reported from the platform engine
#[derive(Debug, Clone)]
pub enum PlatformDownloadEvent {
    Started {
        download_id: i64,
        suggested_filename: String,
        url: String,
        mime_type: Option<String>,
        total_bytes: i64,
    },
    Progress {
        download_id: i64,
        received_bytes: i64,
        total_bytes: i64,
    },
    Finished {
        download_id: i64,
        destination_path: String,
    },
    Failed {
        download_id: i64,
        error_message: String,
    },
    Cancelled {
        download_id: i64,
    },
}

/// Platform interface for content filter compilation and attachment
pub trait ContentBlockerPlatform {
    fn compile_rules_from_json(&self, store_dir: &Path, filter_id: &str, json_rules: &str) -> Result<(), String>;
    fn is_filter_active(&self, filter_id: &str) -> bool;
}

/// Platform interface for download hooks
pub trait DownloadHookPlatform {
    fn cancel_download(&self, download_id: i64);
}

/// Platform interface for TLS state inspection and host exceptions
pub trait TlsStateProvider {
    fn get_current_tls_info(&self) -> Option<PlatformTlsInfo>;
    fn allow_tls_certificate_for_host(&self, host: &str);
}

/// Platform interface for permission mediation
pub trait PermissionMediator {
    fn identify_request(&self) -> (PlatformPermissionKind, String);
    fn allow(&self);
    fn deny(&self);
}

/// Platform interface for sandbox verification
pub trait SandboxChecker {
    fn check_sandbox_status(&self) -> PlatformSandboxStatus;
}

/// Platform interface for webview widget access and lifecycle
pub trait WebViewPlatformAdapter {
    type NativeHandle;

    fn with_webview<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&Self::NativeHandle) -> R;

    fn load_uri(&self, uri: &str);
    fn reload(&self);
    fn stop_loading(&self);
    fn can_go_back(&self) -> bool;
    fn can_go_forward(&self) -> bool;
    fn go_back(&self);
    fn go_forward(&self);
    fn current_uri(&self) -> Option<String>;
    fn current_title(&self) -> Option<String>;
    fn estimated_progress(&self) -> f64;
    fn is_loading(&self) -> bool;
}
