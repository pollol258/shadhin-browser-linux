//! Linux WebKitGTK 6.0 TLS state inspection and certificate exception management

use crate::platform::traits::{PlatformTlsInfo, TlsStateProvider};
use gtk4::gio;
use webkit6::WebView;
use webkit6::prelude::*;

pub struct LinuxTlsStateProvider {
    webview: WebView,
}

impl LinuxTlsStateProvider {
    pub fn new(webview: WebView) -> Self {
        Self { webview }
    }
}

impl TlsStateProvider for LinuxTlsStateProvider {
    fn get_current_tls_info(&self) -> Option<PlatformTlsInfo> {
        let uri = self.webview.uri().map(|u| u.to_string())?;
        let host = crate::browser::navigation::extract_display_host(&uri).unwrap_or_default();

        if !uri.starts_with("https://") {
            return Some(PlatformTlsInfo {
                host,
                is_valid: false,
                has_tls_errors: false,
                certificate_summary: None,
                error_flags_description: None,
            });
        }

        if let Some((cert, flags)) = self.webview.tls_info() {
            let has_errors = !flags.is_empty();
            let mut flag_descs = Vec::new();
            if flags.contains(gio::TlsCertificateFlags::UNKNOWN_CA) {
                flag_descs.push("UNKNOWN_CA");
            }
            if flags.contains(gio::TlsCertificateFlags::BAD_IDENTITY) {
                flag_descs.push("BAD_IDENTITY");
            }
            if flags.contains(gio::TlsCertificateFlags::EXPIRED) {
                flag_descs.push("EXPIRED");
            }
            if flags.contains(gio::TlsCertificateFlags::REVOKED) {
                flag_descs.push("REVOKED");
            }
            if flags.contains(gio::TlsCertificateFlags::INSECURE) {
                flag_descs.push("INSECURE");
            }

            let cert_summary = cert.issuer_name().map(|n| n.to_string());
            let err_desc = if flag_descs.is_empty() {
                None
            } else {
                Some(flag_descs.join(", "))
            };

            Some(PlatformTlsInfo {
                host,
                is_valid: !has_errors,
                has_tls_errors: has_errors,
                certificate_summary: cert_summary,
                error_flags_description: err_desc,
            })
        } else {
            // HTTPS without TLS info yet (e.g. connecting or failed before handshake)
            Some(PlatformTlsInfo {
                host,
                is_valid: false,
                has_tls_errors: true,
                certificate_summary: None,
                error_flags_description: Some("NO_TLS_INFO".to_string()),
            })
        }
    }

    fn allow_tls_certificate_for_host(&self, host: &str) {
        if let Some(session) = self.webview.network_session()
            && let Some((cert, _flags)) = self.webview.tls_info()
        {
            session.allow_tls_certificate_for_host(&cert, host);
        }
    }
}
