//! URL navigation policy, scheme validation, and address bar classification
//!
//! Enforces strict security boundaries on address bar inputs, rejecting dangerous
//! pseudo-schemes (javascript:, data:) and translating bare domains,
//! IP addresses, internal pages, and search phrases into validated targets.
//!
//! This module has ZERO dependencies on GTK or WebKit, ensuring full unit testability.

use std::net::IpAddr;

/// Result of classifying a user's address bar input
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationTarget {
    /// Valid web or local URL (HTTP, HTTPS, or explicitly typed file:)
    Url(String),
    /// Search engine query with encoded search URL
    Search(String),
    /// Safe internal application page (e.g., shadhin:newtab, shadhin:settings, about:blank)
    Internal(String),
    /// Prohibited or dangerous scheme rejected by security policy
    Blocked { scheme: String, reason: String },
}

/// Alias for NavigationTarget for testable input classification
pub type InputKind = NavigationTarget;

impl NavigationTarget {
    pub fn is_url(&self) -> bool {
        matches!(self, Self::Url(_))
    }

    pub fn is_search(&self) -> bool {
        matches!(self, Self::Search(_))
    }

    pub fn is_internal(&self) -> bool {
        matches!(self, Self::Internal(_))
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked { .. })
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            Self::Url(u) => Some(u),
            _ => None,
        }
    }
}

/// Permitted internal schemes and page targets
pub const ALLOWED_INTERNAL_PAGES: &[&str] = &[
    "shadhin:newtab",
    "shadhin:settings",
    "shadhin:bookmarks",
    "shadhin:history",
    "shadhin:downloads",
    "shadhin:error",
    "about:blank",
    "about:newtab",
    "about:bookmarks",
    "about:history",
    "about:downloads",
];

/// Prohibited dangerous schemes that must never be executed from the address bar
pub const BLOCKED_SCHEMES: &[(&str, &str)] = &[
    (
        "javascript",
        "JavaScript execution from address bar is prohibited for security (Self-XSS protection)",
    ),
    (
        "data",
        "Data URLs in address bar are blocked to prevent phishing and script injection attacks",
    ),
    ("ftp", "FTP protocol is deprecated and insecure"),
    (
        "blob",
        "Blob URLs cannot be directly navigated from address bar",
    ),
];

/// Configuration settings controlling address bar input classification
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassificationSettings {
    /// URL template for search engine queries containing `%s`
    pub search_template: String,
    /// Whether file: scheme is allowed (true when typed explicitly by user in omnibox)
    pub allow_file: bool,
    /// Whether developer extras are enabled (allows view-source:)
    pub developer_extras: bool,
}

impl Default for ClassificationSettings {
    fn default() -> Self {
        Self {
            search_template: "https://duckduckgo.com/?q=%s".to_string(),
            allow_file: false,
            developer_extras: false,
        }
    }
}

impl From<&str> for ClassificationSettings {
    fn from(template: &str) -> Self {
        Self {
            search_template: template.to_string(),
            allow_file: false,
            developer_extras: false,
        }
    }
}

impl From<&String> for ClassificationSettings {
    fn from(template: &String) -> Self {
        Self {
            search_template: template.clone(),
            allow_file: false,
            developer_extras: false,
        }
    }
}

impl From<String> for ClassificationSettings {
    fn from(template: String) -> Self {
        Self {
            search_template: template,
            allow_file: false,
            developer_extras: false,
        }
    }
}

impl From<&ClassificationSettings> for ClassificationSettings {
    fn from(s: &ClassificationSettings) -> Self {
        s.clone()
    }
}

/// Classifies user address bar input into a safe `NavigationTarget`
///
/// Accepts any type converting into `ClassificationSettings` (including `&str` search template)
pub fn classify_input(raw_input: &str, settings: impl Into<ClassificationSettings>) -> NavigationTarget {
    let settings = settings.into();
    classify_input_internal(raw_input, &settings)
}

fn classify_input_internal(raw_input: &str, settings: &ClassificationSettings) -> NavigationTarget {
    // Reject illegal null bytes immediately to prevent C FFI panics and injection
    if raw_input.contains('\0') {
        return NavigationTarget::Blocked {
            scheme: "null-byte".to_string(),
            reason: "Input contains prohibited null byte".to_string(),
        };
    }

    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        return NavigationTarget::Internal("shadhin:newtab".to_string());
    }

    // 1. Check for exact internal pages
    for &page in ALLOWED_INTERNAL_PAGES {
        if trimmed.eq_ignore_ascii_case(page) {
            return NavigationTarget::Internal(page.to_string());
        }
    }

    // 2. If the input contains whitespace, it's definitely a search query
    if trimmed.contains(char::is_whitespace) {
        return build_search_target(trimmed, &settings.search_template);
    }

    // 3. Check for localhost or localhost:port
    if is_localhost(trimmed) {
        return NavigationTarget::Url(format!("http://{trimmed}"));
    }

    // 4. Check for IP address (IPv4 or IPv6 literal)
    if is_ip_address_or_host(trimmed) {
        if is_bare_ipv6(trimmed) {
            return NavigationTarget::Url(format!("http://[{trimmed}]"));
        }
        return NavigationTarget::Url(format!("http://{trimmed}"));
    }

    // 5. Check for scheme prefix (e.g., http://, https://, javascript:, data:, file:, view-source:)
    if let Some(colon_pos) = trimmed.find(':') {
        let after_colon = &trimmed[colon_pos + 1..];
        let port_candidate = after_colon.split('/').next().unwrap_or(after_colon);

        // If after colon is pure digits, it's a host:port (handled below), not a scheme!
        let is_port =
            !port_candidate.is_empty() && port_candidate.chars().all(|c| c.is_ascii_digit());

        if !is_port {
            let scheme_raw = &trimmed[..colon_pos];
            let scheme_lower = scheme_raw.to_ascii_lowercase();

            // Blocked schemes check
            for &(blocked, reason) in BLOCKED_SCHEMES {
                if scheme_lower == blocked {
                    return NavigationTarget::Blocked {
                        scheme: scheme_lower,
                        reason: reason.to_string(),
                    };
                }
            }

            // File scheme handling: allowed only when explicitly enabled (typed by user in bar)
            if scheme_lower == "file" {
                if settings.allow_file {
                    if trimmed.starts_with("file://") || trimmed.starts_with("file:/") {
                        return NavigationTarget::Url(trimmed.to_string());
                    } else {
                        return NavigationTarget::Blocked {
                            scheme: "file".to_string(),
                            reason: "Malformed file URI syntax. Expected file:///path".to_string(),
                        };
                    }
                } else {
                    return NavigationTarget::Blocked {
                        scheme: "file".to_string(),
                        reason: "Local filesystem access is prohibited from address bar".to_string(),
                    };
                }
            }

            // View-source scheme handling: allowed only if developer extras are on
            if scheme_lower == "view-source" {
                if settings.developer_extras {
                    let inner = &trimmed[colon_pos + 1..];
                    if inner.starts_with("http://")
                        || inner.starts_with("https://")
                        || inner.starts_with("file://")
                    {
                        return NavigationTarget::Url(trimmed.to_string());
                    } else {
                        return NavigationTarget::Blocked {
                            scheme: "view-source".to_string(),
                            reason: "view-source target must be a valid http, https, or file URL"
                                .to_string(),
                        };
                    }
                } else {
                    return NavigationTarget::Blocked {
                        scheme: "view-source".to_string(),
                        reason: "View source requires Developer Extras to be enabled".to_string(),
                    };
                }
            }

            // HTTP / HTTPS allowed schemes
            if scheme_lower == "http" || scheme_lower == "https" {
                if trimmed.len() > colon_pos + 2 && trimmed[colon_pos..].starts_with("://") {
                    let after_slash = &trimmed[colon_pos + 3..];
                    if !after_slash.is_empty() && !after_slash.starts_with('/') {
                        return NavigationTarget::Url(trimmed.to_string());
                    }
                }
                // Malformed http:/ or http: without host
                return NavigationTarget::Blocked {
                    scheme: scheme_lower,
                    reason: "Malformed URL scheme syntax".to_string(),
                };
            }

            // Allowed custom internal scheme (shadhin:)
            if scheme_lower == "shadhin" {
                if ALLOWED_INTERNAL_PAGES
                    .iter()
                    .any(|&p| p.eq_ignore_ascii_case(trimmed))
                {
                    return NavigationTarget::Internal(trimmed.to_string());
                } else {
                    return NavigationTarget::Blocked {
                        scheme: scheme_lower,
                        reason: "Unknown or prohibited internal page".to_string(),
                    };
                }
            }

            // Allowed custom internal scheme (about:)
            if scheme_lower == "about" {
                if ALLOWED_INTERNAL_PAGES
                    .iter()
                    .any(|&p| p.eq_ignore_ascii_case(trimmed))
                {
                    return NavigationTarget::Internal(trimmed.to_string());
                } else {
                    return NavigationTarget::Blocked {
                        scheme: scheme_lower,
                        reason: "Unknown or prohibited about: page".to_string(),
                    };
                }
            }

            // Any other custom or unknown scheme is blocked safely
            return NavigationTarget::Blocked {
                scheme: scheme_lower,
                reason: "Unsupported or unverified URI scheme".to_string(),
            };
        }
    }

    // 6. Check if input looks like a valid domain name or domain with path
    if looks_like_domain(trimmed) {
        return NavigationTarget::Url(format!("https://{trimmed}"));
    }

    // 7. Fallback: Search engine query
    build_search_target(trimmed, &settings.search_template)
}

/// Constructs search target with pure Rust percent-encoding
fn build_search_target(query: &str, template: &str) -> NavigationTarget {
    let escaped = percent_encode_query(query);
    let search_url = if template.contains("%s") {
        template.replace("%s", &escaped)
    } else {
        format!("https://duckduckgo.com/?q={escaped}")
    };
    NavigationTarget::Search(search_url)
}

const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";

/// Pure Rust RFC 3986 percent-encoding for query strings
pub fn percent_encode_query(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len() * 3);
    for &byte in input.as_bytes() {
        match byte {
            // Unreserved characters (RFC 3986 section 2.3)
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            // Space encoded as %20
            b' ' => encoded.push_str("%20"),
            // All other bytes are percent-encoded
            b => {
                encoded.push('%');
                encoded.push(HEX_CHARS[(b >> 4) as usize] as char);
                encoded.push(HEX_CHARS[(b & 0x0F) as usize] as char);
            }
        }
    }
    encoded
}

/// Expands a bare word on Ctrl+Enter (word -> https://www.word.com)
pub fn expand_ctrl_enter(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // If it already has a scheme, slash, space, or dot, it is not a bare word
    if trimmed.contains("://") || trimmed.contains('/') || trimmed.contains(' ') || trimmed.contains('.') {
        trimmed.to_string()
    } else {
        format!("https://www.{trimmed}.com")
    }
}

/// Checks if token is localhost or a private IPv4 / IPv6 network address
/// Covers: 127.0.0.0/8, ::1, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
pub fn is_localhost_or_private_ip(host: &str) -> bool {
    let host_part = host.split('/').next().unwrap_or(host);
    let clean_host = if host_part.starts_with('[') {
        if let Some(end) = host_part.find(']') {
            &host_part[1..end]
        } else {
            host_part.trim_start_matches('[').trim_end_matches(']')
        }
    } else if is_bare_ipv6(host_part) {
        host_part
    } else {
        host_part.split(':').next().unwrap_or(host_part)
    };

    if is_localhost(clean_host) {
        return true;
    }

    if let Ok(ip) = clean_host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => {
                let octets = v4.octets();
                // 127.0.0.0/8 (Loopback)
                if octets[0] == 127 {
                    return true;
                }
                // 10.0.0.0/8 (Private)
                if octets[0] == 10 {
                    return true;
                }
                // 172.16.0.0/12 (Private: 172.16.0.0 - 172.31.255.255)
                if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                    return true;
                }
                // 192.168.0.0/16 (Private)
                if octets[0] == 192 && octets[1] == 168 {
                    return true;
                }
                // 169.254.0.0/16 (Link-local)
                if octets[0] == 169 && octets[1] == 254 {
                    return true;
                }
                false
            }
            std::net::IpAddr::V6(v6) => {
                // ::1 (Loopback) or Unique Local Address (fc00::/7)
                v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
            }
        }
    } else {
        false
    }
}

/// Classification of network load failures
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkFailureKind {
    /// Connection-level failure (connection refused, no HTTPS listener on 443, timed out, unreachable)
    ConnectionRefusedOrTimedOut,
    /// TLS or Certificate error (expired, unknown CA, wrong host, revoked)
    TlsCertificateError,
    /// User or policy cancellation
    Cancelled,
    /// Other error (e.g. DNS resolution failure, malformed headers)
    Other,
}

/// Classifies network failure by domain, code, and error message
pub fn classify_network_failure_by_reason(
    error_domain: &str,
    error_code: i32,
    error_message: &str,
) -> NetworkFailureKind {
    // 1. Cancellation check (WebKit NetworkError::Cancelled = 302, PolicyError::FrameLoadInterrupted = 102)
    if error_code == 302 || error_code == 102 || error_message.to_lowercase().contains("cancelled") {
        return NetworkFailureKind::Cancelled;
    }

    let msg_lower = error_message.to_lowercase();
    let dom_lower = error_domain.to_lowercase();

    // 2. TLS / Certificate error check
    if dom_lower.contains("tls")
        || dom_lower.contains("certificate")
        || dom_lower.contains("ssl")
        || msg_lower.contains("certificate")
        || msg_lower.contains("tls")
        || msg_lower.contains("ssl")
        || msg_lower.contains("expired")
        || msg_lower.contains("unknown ca")
        || msg_lower.contains("bad identity")
        || msg_lower.contains("revoked")
    {
        return NetworkFailureKind::TlsCertificateError;
    }

    // 3. Connection-level failure (connection refused, timed out, host unreachable, no listener on 443)
    if msg_lower.contains("connection refused")
        || msg_lower.contains("timed out")
        || msg_lower.contains("timeout")
        || msg_lower.contains("cant_connect")
        || msg_lower.contains("cannot connect")
        || msg_lower.contains("no route to host")
        || msg_lower.contains("network is unreachable")
        || error_code == 4 // SOUP_STATUS_CANT_CONNECT
        || error_code == 7 // SOUP_STATUS_IO_ERROR
    {
        return NetworkFailureKind::ConnectionRefusedOrTimedOut;
    }

    NetworkFailureKind::Other
}

/// Determines whether a load failure is eligible for HTTP fallback
///
/// CRITICAL INVARIANT: NEVER fall back to HTTP after a TLS or certificate error!
/// HTTP fallback is only permitted for pure connection-level failures (connection refused, timed out).
pub fn is_eligible_for_http_fallback(failure_kind: NetworkFailureKind, is_tls_error: bool) -> bool {
    if is_tls_error {
        return false;
    }
    failure_kind == NetworkFailureKind::ConnectionRefusedOrTimedOut
}

/// Checks if token is localhost or localhost:port
fn is_localhost(token: &str) -> bool {
    let host_part = token.split('/').next().unwrap_or(token);
    let host_only = host_part.split(':').next().unwrap_or(host_part);
    host_only.eq_ignore_ascii_case("localhost")
        || host_only.eq_ignore_ascii_case("ip6-localhost")
        || host_only.eq_ignore_ascii_case("localhost.localdomain")
}

/// Checks if string is a bare IPv6 address (without brackets)
fn is_bare_ipv6(token: &str) -> bool {
    let host_part = token.split('/').next().unwrap_or(token);
    if host_part.starts_with('[') {
        return false;
    }
    if host_part.contains(':') && !host_part.contains('.') {
        return host_part.parse::<std::net::Ipv6Addr>().is_ok();
    }
    false
}

/// Checks if token is a valid IPv4 or IPv6 address (with optional port and path)
fn is_ip_address_or_host(token: &str) -> bool {
    let host_part = token.split('/').next().unwrap_or(token);

    // IPv6 literal enclosed in brackets [::1]:8080 or [::1]
    if host_part.starts_with('[') {
        if let Some(end_bracket) = host_part.find(']') {
            let ip_str = &host_part[1..end_bracket];
            return ip_str.parse::<IpAddr>().is_ok();
        }
        return false;
    }

    // Bare IPv6 like ::1
    if is_bare_ipv6(token) {
        return true;
    }

    // IPv4 with optional port
    let host_only = host_part.split(':').next().unwrap_or(host_part);
    host_only.parse::<std::net::Ipv4Addr>().is_ok()
}

/// Checks if string has domain-like structure (e.g., example.com, test.co.uk, বাংলা.ভারত)
fn looks_like_domain(token: &str) -> bool {
    let host_part = token.split('/').next().unwrap_or(token);
    let host_only = host_part.split(':').next().unwrap_or(host_part);

    if host_only.is_empty() || host_only.starts_with('.') || host_only.ends_with('.') {
        return false;
    }

    let parts: Vec<&str> = host_only.split('.').collect();
    if parts.len() < 2 {
        return false;
    }

    // Validate that labels are non-empty
    for part in &parts {
        if part.is_empty() {
            return false;
        }
        if part.starts_with('-') || part.ends_with('-') {
            return false;
        }
    }

    // Check last label (TLD)
    let Some(tld) = parts.last() else {
        return false;
    };
    // Valid TLD: at least 2 chars, letters, digits, hyphens or non-ascii unicode
    tld.chars().count() >= 2
        && tld
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || !c.is_ascii())
}

/// Extracts a clean display host from a URL for UI presentation without GLib
pub fn extract_display_host(url: &str) -> Option<String> {
    let without_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };

    let host_part = without_scheme.split('/').next().unwrap_or(without_scheme);
    let host_part = host_part.split('?').next().unwrap_or(host_part);
    let host_part = host_part.split('#').next().unwrap_or(host_part);

    // IPv6 literal enclosed in brackets
    if host_part.starts_with('[')
        && let Some(end) = host_part.find(']')
    {
        return Some(host_part[..=end].to_string());
    }

    let host_only = host_part.split(':').next().unwrap_or(host_part);
    if host_only.is_empty() {
        None
    } else {
        Some(host_only.to_string())
    }
}

/// Unicode script category classification for IDN homograph defense
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum UnicodeScript {
    Latin,
    Cyrillic,
    Greek,
    Bengali,
    Arabic,
    Devanagari,
    Cjk,
    Common, // digits, hyphens, periods
    Other,
}

/// Classifies a character into a Unicode script category
pub fn char_script(c: char) -> UnicodeScript {
    match c {
        'a'..='z' | 'A'..='Z' => UnicodeScript::Latin,
        '0'..='9' | '-' | '.' | '_' => UnicodeScript::Common,
        '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}' => UnicodeScript::Cyrillic,
        '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}' => UnicodeScript::Greek,
        '\u{0980}'..='\u{09FF}' => UnicodeScript::Bengali,
        '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' => UnicodeScript::Arabic,
        '\u{0900}'..='\u{097F}' => UnicodeScript::Devanagari,
        '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' => UnicodeScript::Cjk,
        _ => UnicodeScript::Other,
    }
}

/// Evaluates if an Internationalized Domain Name (IDN) is safe to display in Unicode
///
/// Returns true if the domain consists of a single script (plus common ASCII digits/hyphens)
/// and does not perform cross-script homoglyph spoofing (such as mixing Cyrillic with Latin).
pub fn is_safe_idn(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    for label in labels {
        if label.is_empty() {
            continue;
        }

        // Punycode prefixes are handled: if label is already Punycode, check decoded or keep Punycode
        if label.to_ascii_lowercase().starts_with("xn--") {
            // Keep punycode representation for raw xn-- labels if not conclusively safe
            return false;
        }

        let mut has_latin = false;
        let mut has_cyrillic = false;
        let mut has_greek = false;

        for c in label.chars() {
            match char_script(c) {
                UnicodeScript::Latin => has_latin = true,
                UnicodeScript::Cyrillic => has_cyrillic = true,
                UnicodeScript::Greek => has_greek = true,
                _ => {}
            }
        }

        // Dangerous homograph: mixing Latin with Cyrillic or Greek in the same label
        if has_latin && (has_cyrillic || has_greek) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_TMPL: &str = "https://duckduckgo.com/?q=%s";

    #[test]
    fn test_empty_and_whitespace() {
        assert_eq!(
            classify_input("", SEARCH_TMPL),
            NavigationTarget::Internal("shadhin:newtab".to_string())
        );
        assert_eq!(
            classify_input("   ", SEARCH_TMPL),
            NavigationTarget::Internal("shadhin:newtab".to_string())
        );
    }

    #[test]
    fn test_internal_pages() {
        assert_eq!(
            classify_input("shadhin:newtab", SEARCH_TMPL),
            NavigationTarget::Internal("shadhin:newtab".to_string())
        );
        assert_eq!(
            classify_input("shadhin:settings", SEARCH_TMPL),
            NavigationTarget::Internal("shadhin:settings".to_string())
        );
        assert_eq!(
            classify_input("about:blank", SEARCH_TMPL),
            NavigationTarget::Internal("about:blank".to_string())
        );
        assert_eq!(
            classify_input("about:newtab", SEARCH_TMPL),
            NavigationTarget::Internal("about:newtab".to_string())
        );
    }

    // Rule 1: Full valid URL
    #[test]
    fn test_rule_1_full_valid_urls() {
        assert_eq!(
            classify_input("https://example.com/path?q=1", SEARCH_TMPL),
            NavigationTarget::Url("https://example.com/path?q=1".to_string())
        );
        assert_eq!(
            classify_input("http://example.com", SEARCH_TMPL),
            NavigationTarget::Url("http://example.com".to_string())
        );
        assert_eq!(
            classify_input("https://wikipedia.org/wiki/Rust", SEARCH_TMPL),
            NavigationTarget::Url("https://wikipedia.org/wiki/Rust".to_string())
        );
        assert_eq!(
            classify_input("https://sub.domain.co.uk:8443/path?arg=1#top", SEARCH_TMPL),
            NavigationTarget::Url("https://sub.domain.co.uk:8443/path?arg=1#top".to_string())
        );
    }

    // Rule 2: No scheme (defaults to https://)
    #[test]
    fn test_rule_2_bare_domains() {
        assert_eq!(
            classify_input("example.com", SEARCH_TMPL),
            NavigationTarget::Url("https://example.com".to_string())
        );
        assert_eq!(
            classify_input("www.example.com", SEARCH_TMPL),
            NavigationTarget::Url("https://www.example.com".to_string())
        );
        assert_eq!(
            classify_input("example.com/path", SEARCH_TMPL),
            NavigationTarget::Url("https://example.com/path".to_string())
        );
        assert_eq!(
            classify_input("crates.io/crates/webkit6", SEARCH_TMPL),
            NavigationTarget::Url("https://crates.io/crates/webkit6".to_string())
        );
    }

    // Rule 3: localhost and private IP addresses
    #[test]
    fn test_rule_3_localhost_and_private_ips() {
        assert_eq!(
            classify_input("localhost", SEARCH_TMPL),
            NavigationTarget::Url("http://localhost".to_string())
        );
        assert_eq!(
            classify_input("localhost:3000", SEARCH_TMPL),
            NavigationTarget::Url("http://localhost:3000".to_string())
        );
        assert_eq!(
            classify_input("127.0.0.1", SEARCH_TMPL),
            NavigationTarget::Url("http://127.0.0.1".to_string())
        );
        assert_eq!(
            classify_input("127.0.0.1:8080", SEARCH_TMPL),
            NavigationTarget::Url("http://127.0.0.1:8080".to_string())
        );
        assert_eq!(
            classify_input("[::1]", SEARCH_TMPL),
            NavigationTarget::Url("http://[::1]".to_string())
        );
        assert_eq!(
            classify_input("[::1]:8000", SEARCH_TMPL),
            NavigationTarget::Url("http://[::1]:8000".to_string())
        );
        assert_eq!(
            classify_input("::1", SEARCH_TMPL),
            NavigationTarget::Url("http://[::1]".to_string())
        );
        assert_eq!(
            classify_input("192.168.1.1", SEARCH_TMPL),
            NavigationTarget::Url("http://192.168.1.1".to_string())
        );
        assert_eq!(
            classify_input("192.168.0.254:8080", SEARCH_TMPL),
            NavigationTarget::Url("http://192.168.0.254:8080".to_string())
        );
        assert_eq!(
            classify_input("10.0.0.1", SEARCH_TMPL),
            NavigationTarget::Url("http://10.0.0.1".to_string())
        );
        assert_eq!(
            classify_input("10.10.10.10:9000/api", SEARCH_TMPL),
            NavigationTarget::Url("http://10.10.10.10:9000/api".to_string())
        );
        assert_eq!(
            classify_input("172.16.0.1", SEARCH_TMPL),
            NavigationTarget::Url("http://172.16.0.1".to_string())
        );
        assert_eq!(
            classify_input("172.31.255.255:4000", SEARCH_TMPL),
            NavigationTarget::Url("http://172.31.255.255:4000".to_string())
        );
    }

    // Rule 4: Text with spaces or invalid host is a search query with proper percent-encoding
    #[test]
    fn test_rule_4_search_queries_with_special_characters() {
        assert_eq!(
            classify_input("rust programming language", SEARCH_TMPL),
            NavigationTarget::Search(
                "https://duckduckgo.com/?q=rust%20programming%20language".to_string()
            )
        );
        assert_eq!(
            classify_input("\"exact search\" 'quotes' & <tags>?", SEARCH_TMPL),
            NavigationTarget::Search(
                "https://duckduckgo.com/?q=%22exact%20search%22%20%27quotes%27%20%26%20%3Ctags%3E%3F".to_string()
            )
        );
        assert_eq!(
            classify_input("not a valid host!", SEARCH_TMPL),
            NavigationTarget::Search(
                "https://duckduckgo.com/?q=not%20a%20valid%20host%21".to_string()
            )
        );
    }

    // Rule 5: Single word without a dot searches by default
    #[test]
    fn test_rule_5_single_word_search_default() {
        assert_eq!(
            classify_input("intranet", SEARCH_TMPL),
            NavigationTarget::Search("https://duckduckgo.com/?q=intranet".to_string())
        );
        assert_eq!(
            classify_input("bangladesh", SEARCH_TMPL),
            NavigationTarget::Search("https://duckduckgo.com/?q=bangladesh".to_string())
        );
        assert_eq!(
            classify_input("singleword", SEARCH_TMPL),
            NavigationTarget::Search("https://duckduckgo.com/?q=singleword".to_string())
        );
        // Known localhost navigates
        assert_eq!(
            classify_input("localhost", SEARCH_TMPL),
            NavigationTarget::Url("http://localhost".to_string())
        );
        assert_eq!(
            classify_input("ip6-localhost", SEARCH_TMPL),
            NavigationTarget::Url("http://ip6-localhost".to_string())
        );
    }

    // Rule 6: Bangla text and mixed queries
    #[test]
    fn test_rule_6_bangla_and_mixed_search_queries() {
        let bangla_search = classify_input("বাংলাদেশ", SEARCH_TMPL);
        assert!(bangla_search.is_search());
        assert_eq!(
            bangla_search,
            NavigationTarget::Search(format!(
                "https://duckduckgo.com/?q={}",
                percent_encode_query("বাংলাদেশ")
            ))
        );

        let mixed = classify_input("shadhin ব্রাউজার বৈশিষ্ট্য", SEARCH_TMPL);
        assert!(mixed.is_search());
        assert_eq!(
            mixed,
            NavigationTarget::Search(format!(
                "https://duckduckgo.com/?q={}",
                percent_encode_query("shadhin ব্রাউজার বৈশিষ্ট্য")
            ))
        );
    }

    // Rule 7: Allowed and blocked schemes
    #[test]
    fn test_rule_7_scheme_policies() {
        // http and https allowed
        assert_eq!(
            classify_input("https://shadhin.org", SEARCH_TMPL),
            NavigationTarget::Url("https://shadhin.org".to_string())
        );
        assert_eq!(
            classify_input("http://plain.org", SEARCH_TMPL),
            NavigationTarget::Url("http://plain.org".to_string())
        );

        // javascript: blocked with clear security reason
        match classify_input("javascript:alert(1)", SEARCH_TMPL) {
            NavigationTarget::Blocked { scheme, reason } => {
                assert_eq!(scheme, "javascript");
                assert!(reason.contains("prohibited"));
            }
            other => panic!("Expected Blocked, got {other:?}"),
        }

        // data: blocked with clear security reason
        match classify_input("data:text/html,<h1>Phishing</h1>", SEARCH_TMPL) {
            NavigationTarget::Blocked { scheme, reason } => {
                assert_eq!(scheme, "data");
                assert!(reason.contains("blocked"));
            }
            other => panic!("Expected Blocked, got {other:?}"),
        }

        // file: blocked by default
        match classify_input("file:///etc/passwd", SEARCH_TMPL) {
            NavigationTarget::Blocked { scheme, .. } => assert_eq!(scheme, "file"),
            other => panic!("Expected Blocked, got {other:?}"),
        }

        // file: allowed when allow_file is explicitly set
        let file_settings = ClassificationSettings {
            search_template: SEARCH_TMPL.to_string(),
            allow_file: true,
            developer_extras: false,
        };
        assert_eq!(
            classify_input("file:///home/user/document.pdf", &file_settings),
            NavigationTarget::Url("file:///home/user/document.pdf".to_string())
        );

        // view-source: blocked when developer extras are false
        match classify_input("view-source:https://example.com", SEARCH_TMPL) {
            NavigationTarget::Blocked { scheme, .. } => assert_eq!(scheme, "view-source"),
            other => panic!("Expected Blocked, got {other:?}"),
        }

        // view-source: allowed when developer extras are enabled
        let dev_settings = ClassificationSettings {
            search_template: SEARCH_TMPL.to_string(),
            allow_file: false,
            developer_extras: true,
        };
        assert_eq!(
            classify_input("view-source:https://example.com", &dev_settings),
            NavigationTarget::Url("view-source:https://example.com".to_string())
        );

        // Unknown schemes show clear message, do not crash
        match classify_input("steam://rungameid/730", SEARCH_TMPL) {
            NavigationTarget::Blocked { scheme, reason } => {
                assert_eq!(scheme, "steam");
                assert!(reason.contains("Unsupported"));
            }
            other => panic!("Expected Blocked, got {other:?}"),
        }
    }

    // Rule 8: Leading/trailing whitespace
    #[test]
    fn test_rule_8_whitespace_handling() {
        assert_eq!(
            classify_input("   https://example.com   ", SEARCH_TMPL),
            NavigationTarget::Url("https://example.com".to_string())
        );
        assert_eq!(
            classify_input("  rust lang  ", SEARCH_TMPL),
            NavigationTarget::Search("https://duckduckgo.com/?q=rust%20lang".to_string())
        );
    }

    // Rule 16: IDN homograph checks
    #[test]
    fn test_rule_16_idn_homograph_safety() {
        // Pure Bengali domain is safe
        assert!(is_safe_idn("বাংলা.ভারত"));
        // Pure Latin domain is safe
        assert!(is_safe_idn("example.com"));

        // Dangerous homograph: Latin 'p' and 'y' mixed with Cyrillic 'а' (U+0430)
        let spoofed = "p\u{0430}ypal.com";
        assert!(!is_safe_idn(spoofed));

        // Punycode representation is detected
        assert!(!is_safe_idn("xn--54b7fta0cc.xn--54b7fta0cc"));
    }

    #[test]
    fn test_extract_display_host() {
        assert_eq!(
            extract_display_host("https://example.com/test"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_display_host("http://localhost:8080/app"),
            Some("localhost".to_string())
        );
        assert_eq!(
            extract_display_host("https://[::1]:9000/app"),
            Some("[::1]".to_string())
        );
        assert_eq!(
            extract_display_host("https://sub.domain.co.uk:9000/"),
            Some("sub.domain.co.uk".to_string())
        );
    }

    #[test]
    fn test_expand_ctrl_enter_bare_word() {
        // Bare words get expanded to https://www.word.com
        assert_eq!(expand_ctrl_enter("apple"), "https://www.apple.com");
        assert_eq!(expand_ctrl_enter("rust-lang"), "https://www.rust-lang.com");
        assert_eq!(expand_ctrl_enter("  wikipedia  "), "https://www.wikipedia.com");

        // Already formatted domains or URLs are NOT mangled
        assert_eq!(expand_ctrl_enter("example.org"), "example.org");
        assert_eq!(expand_ctrl_enter("https://example.com"), "https://example.com");
        assert_eq!(expand_ctrl_enter("spaced query"), "spaced query");
        assert_eq!(expand_ctrl_enter(""), "");
    }

    #[test]
    fn test_is_localhost_or_private_ip() {
        // Localhost and loopback
        assert!(is_localhost_or_private_ip("localhost"));
        assert!(is_localhost_or_private_ip("localhost:8080"));
        assert!(is_localhost_or_private_ip("127.0.0.1"));
        assert!(is_localhost_or_private_ip("127.0.0.254:3000"));
        assert!(is_localhost_or_private_ip("[::1]"));
        assert!(is_localhost_or_private_ip("[::1]:8000"));

        // Private IP ranges (10/8, 172.16/12, 192.168/16)
        assert!(is_localhost_or_private_ip("10.0.0.1"));
        assert!(is_localhost_or_private_ip("10.254.254.1:8080"));
        assert!(is_localhost_or_private_ip("172.16.0.1"));
        assert!(is_localhost_or_private_ip("172.31.255.255"));
        assert!(is_localhost_or_private_ip("192.168.1.1"));
        assert!(is_localhost_or_private_ip("192.168.0.100:9000"));

        // Public IPs and domains are NOT localhost or private
        assert!(!is_localhost_or_private_ip("example.com"));
        assert!(!is_localhost_or_private_ip("172.32.0.1")); // outside 172.16/12
        assert!(!is_localhost_or_private_ip("8.8.8.8"));
        assert!(!is_localhost_or_private_ip("1.1.1.1"));
    }

    #[test]
    fn test_failure_classification_and_http_fallback_eligibility() {
        // 1. Connection-level failures are eligible for HTTP fallback
        let conn_refused = classify_network_failure_by_reason(
            "soup-error-quark",
            4,
            "Could not connect: Connection refused",
        );
        assert_eq!(conn_refused, NetworkFailureKind::ConnectionRefusedOrTimedOut);
        assert!(is_eligible_for_http_fallback(conn_refused, false));

        let timed_out = classify_network_failure_by_reason(
            "soup-error-quark",
            7,
            "Connection timed out",
        );
        assert_eq!(timed_out, NetworkFailureKind::ConnectionRefusedOrTimedOut);
        assert!(is_eligible_for_http_fallback(timed_out, false));

        // 2. TLS and Certificate errors are NEVER eligible for HTTP fallback!
        let tls_expired = classify_network_failure_by_reason(
            "g-tls-error-quark",
            1,
            "SSL/TLS Certificate has expired",
        );
        assert_eq!(tls_expired, NetworkFailureKind::TlsCertificateError);
        assert!(!is_eligible_for_http_fallback(tls_expired, false));
        assert!(!is_eligible_for_http_fallback(tls_expired, true));

        let tls_unknown_ca = classify_network_failure_by_reason(
            "g-tls-certificate-error-quark",
            2,
            "Certificate authority is unknown or self-signed",
        );
        assert_eq!(tls_unknown_ca, NetworkFailureKind::TlsCertificateError);
        assert!(!is_eligible_for_http_fallback(tls_unknown_ca, false));

        let tls_wrong_host = classify_network_failure_by_reason(
            "g-tls-certificate-error-quark",
            3,
            "Certificate bad identity: hostname mismatch",
        );
        assert_eq!(tls_wrong_host, NetworkFailureKind::TlsCertificateError);
        assert!(!is_eligible_for_http_fallback(tls_wrong_host, false));

        // If is_tls_error flag is true, fallback is ALWAYS blocked
        assert!(!is_eligible_for_http_fallback(conn_refused, true));

        // 3. User cancellation is NEVER eligible for HTTP fallback
        let cancelled = classify_network_failure_by_reason(
            "webkit-network-error-quark",
            302,
            "Load request was cancelled",
        );
        assert_eq!(cancelled, NetworkFailureKind::Cancelled);
        assert!(!is_eligible_for_http_fallback(cancelled, false));
    }
}
