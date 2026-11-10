//! WebKit WebView lifecycle and security configuration
//!
//! Enforces hardened settings on WebKitGTK 6.0 WebView instances:
//! - Disallows filesystem access from file: URLs
//! - Disallows universal access from file: URLs
//! - Restricts media autoplay via user gesture requirement
//! - Disables automatic window popup opening via JavaScript
//! - Configures hardware acceleration and smooth scrolling
//! - Leaves user agent default to prevent fingerprinting anomalies
//! - Never disables WebKit's process isolation or sandbox boundaries

use std::env;
use std::sync::Once;
use webkit6::prelude::*;

static MEMORY_SETTINGS_INIT: Once = Once::new();

/// Configures WebKit's MemoryPressureSettings adaptively based on system RAM capacity.
///
/// Sets conservative and strict memory thresholds so that WebKit's MemoryPressureMonitor
/// purges dead resources, font caches, image decoders, and triggers garbage collection
/// BEFORE the system starts paging / swapping.
pub fn configure_memory_pressure_settings() {
    let mut settings = webkit6::MemoryPressureSettings::new();
    let mem_info = crate::platform::linux::memory::MemoryInfo::read_current();

    if let Some(mem) = mem_info {
        let total_mb = mem.total_kb / 1024;
        tracing::info!("Configuring WebKit MemoryPressureSettings for system with {total_mb} MB RAM");

        if total_mb <= 8192 {
            // Low-RAM profile (<= 8GB):
            // Conservative thresholds to act early before swap thrashing.
            // Order matters: WebKit asserts conservative < strict < kill.
            settings.set_kill_threshold(0.95);
            settings.set_strict_threshold(0.80);
            settings.set_conservative_threshold(0.65);
            settings.set_poll_interval(1.5);
            settings.set_memory_limit(total_mb as u32);
        } else if total_mb <= 16384 {
            // Medium-RAM profile (8GB - 16GB):
            // Balanced thresholds for active multitasking.
            settings.set_kill_threshold(0.96);
            settings.set_strict_threshold(0.85);
            settings.set_conservative_threshold(0.70);
            settings.set_poll_interval(2.0);
            settings.set_memory_limit(total_mb as u32);
        } else {
            // High-RAM profile (> 16GB):
            // Relaxed thresholds to avoid premature tab discarding.
            settings.set_kill_threshold(0.98);
            settings.set_strict_threshold(0.90);
            settings.set_conservative_threshold(0.75);
            settings.set_poll_interval(3.0);
            settings.set_memory_limit(total_mb as u32);
        }
    } else {
        // Fallback defaults
        settings.set_kill_threshold(0.95);
        settings.set_strict_threshold(0.85);
        settings.set_conservative_threshold(0.70);
        settings.set_poll_interval(2.0);
    }

    webkit6::NetworkSession::set_memory_pressure_settings(&mut settings);
}

/// Pauses any playing HTML5 audio or video elements in the WebView
pub fn pause_media(webview: &webkit6::WebView) {
    webview.evaluate_javascript(
        "document.querySelectorAll('video, audio').forEach(function(el) { if (!el.paused) { el.pause(); } });",
        None,
        None,
        None::<&gtk4::gio::Cancellable>,
        |_| {},
    );
}

/// Purges in-memory caches on the WebKit WebsiteDataManager to prevent unbounded RAM usage
pub fn trim_website_data_cache(session: &webkit6::NetworkSession) {
    if let Some(wdm) = session.website_data_manager() {
        wdm.clear(
            webkit6::WebsiteDataTypes::MEMORY_CACHE,
            gtk4::glib::TimeSpan(0),
            Option::<&gtk4::gio::Cancellable>::None,
            |_| {},
        );
    }
}

/// Constructs a hardened `webkit6::Settings` instance adhering to Shadhin security policy
pub fn create_hardened_settings(javascript_enabled: bool) -> webkit6::Settings {
    let settings = webkit6::Settings::new();

    // 1. Scripting execution
    settings.set_enable_javascript(javascript_enabled);

    // 2. Developer extras (Web Inspector) - enabled only when SHADHIN_DEV=1 is set
    let dev_mode = env::var("SHADHIN_DEV").is_ok() || env::var("SHADHIN_DEV_CSS").is_ok();
    settings.set_enable_developer_extras(dev_mode);

    // 3. Scrolling behavior
    settings.set_enable_smooth_scrolling(true);

    // 4. Hardware acceleration
    // Under WSLg, SHM or Wayland acceleration is used; Never disable sandbox!
    settings.set_hardware_acceleration_policy(webkit6::HardwareAccelerationPolicy::Always);

    // 5. Filesystem security boundaries
    // Disallow cross-origin and file system probing from file: URLs
    settings.set_allow_file_access_from_file_urls(false);
    settings.set_allow_universal_access_from_file_urls(false);

    // 6. Media autoplay restriction
    settings.set_media_playback_requires_user_gesture(true);

    // 7. Popup prevention
    settings.set_javascript_can_open_windows_automatically(false);

    // 8. Web Audio
    settings.set_enable_webaudio(true);

    // 9. Cache and memory tuning:
    // Disabling WebKit's in-memory back-forward page cache prevents hoarding full DOM and render trees
    // across multiple inactive tabs, freeing tens of megabytes per WebProcess on modest hardware.
    settings.set_enable_page_cache(false);

    // 10. User Agent: left standard to ensure maximum web compatibility and avoid
    // unique fingerprinting tags.
    // (settings.set_user_agent is left at default)

    settings
}

/// Configures compliant BCP 47 language tags on WebKitGTK's WebContext.
///
/// Under POSIX/Linux, if `LANG` is `C` or `C.UTF-8`, WebKit transmits `Accept-Language: C`.
/// This is not a valid BCP 47 language tag (RFC 9110 / RFC 5646) and can trigger WAF
/// anomalies or CDN blocks. This function ensures compliant standard desktop language tags.
pub fn configure_preferred_languages(context: &webkit6::WebContext) {
    let mut langs: Vec<&str> = Vec::new();

    // Check system locale environment variables
    let sys_lang = env::var("LANGUAGE")
        .or_else(|_| env::var("LC_ALL"))
        .or_else(|_| env::var("LANG"))
        .unwrap_or_default();

    let lang_normalized = sys_lang.split('.').next().unwrap_or("").replace('_', "-");
    let is_c_locale = lang_normalized.is_empty()
        || lang_normalized.eq_ignore_ascii_case("c")
        || lang_normalized.eq_ignore_ascii_case("posix");

    if !is_c_locale {
        if lang_normalized.starts_with("bn") {
            langs.extend_from_slice(&["bn-BD", "bn", "en-US", "en"]);
        } else if lang_normalized.starts_with("en") {
            langs.extend_from_slice(&["en-US", "en", "bn-BD", "bn"]);
        } else {
            langs.extend_from_slice(&["en-US", "en"]);
        }
    } else {
        langs.extend_from_slice(&["en-US", "en", "bn-BD", "bn"]);
    }

    context.set_preferred_languages(&langs);
}

/// Creates a new `webkit6::WebView` configured with hardened security settings and optional NetworkSession
pub fn create_configured_webview_with_session(
    javascript_enabled: bool,
    session: Option<&webkit6::NetworkSession>,
) -> webkit6::WebView {
    // Ensure WebContext cache model is tuned for light desktop browser memory footprint
    if let Some(ctx) = webkit6::WebContext::default() {
        ctx.set_cache_model(webkit6::CacheModel::DocumentBrowser);
        configure_preferred_languages(&ctx);
    }

    // Configure WebKit's MemoryPressureSettings adaptively
    MEMORY_SETTINGS_INIT.call_once(|| {
        configure_memory_pressure_settings();
    });

    let settings = create_hardened_settings(javascript_enabled);

    let mut builder = webkit6::WebView::builder()
        .settings(&settings)
        .hexpand(true)
        .vexpand(true);

    if let Some(s) = session {
        builder = builder.network_session(s);
    }

    let webview = builder.build();
    if let Some(session) = webview.network_session()
        && let Some(wdm) = session.website_data_manager()
    {
        wdm.set_favicons_enabled(true);
    }
    webview
}

/// Creates a new `webkit6::WebView` configured with hardened security settings
pub fn create_configured_webview(javascript_enabled: bool) -> webkit6::WebView {
    create_configured_webview_with_session(javascript_enabled, None)
}

/// Helper to determine if a WebKit / GLib error corresponds to a cancelled load or policy intervention.
///
/// When an HTTP navigation decision is ignored in `connect_decide_policy`, WebKit emits
/// `connect_load_failed` with a cancellation error. Such errors must not be treated as
/// network failures or connection dropouts.
pub fn is_cancellation_error(error: &gtk4::glib::Error) -> bool {
    error.kind::<webkit6::NetworkError>() == Some(webkit6::NetworkError::Cancelled)
        || error.kind::<webkit6::PolicyError>()
            == Some(webkit6::PolicyError::FrameLoadInterruptedByPolicyChange)
        || error.kind::<gtk4::gio::IOErrorEnum>() == Some(gtk4::gio::IOErrorEnum::Cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardened_settings_creation() {
        if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
            eprintln!("No graphical display connection available, skipping WebKit GUI test");
            return;
        }

        if gtk4::init().is_err() || gtk4::gdk::Display::default().is_none() {
            eprintln!("Failed to acquire GTK display connection, skipping WebKit GUI test");
            return;
        }

        let settings = create_hardened_settings(true);
        assert!(settings.enables_javascript());
        assert!(!settings.allows_file_access_from_file_urls());
        assert!(!settings.allows_universal_access_from_file_urls());
        assert!(settings.is_media_playback_requires_user_gesture());
        assert!(!settings.is_javascript_can_open_windows_automatically());

        let settings_no_js = create_hardened_settings(false);
        assert!(!settings_no_js.enables_javascript());
    }

    #[test]
    fn test_memory_pressure_settings_order() {
        let mut settings = webkit6::MemoryPressureSettings::new();
        println!(
            "Default conservative: {}, strict: {}, kill: {}",
            settings.conservative_threshold(),
            settings.strict_threshold(),
            settings.kill_threshold()
        );
        settings.set_kill_threshold(0.95);
        settings.set_strict_threshold(0.80);
        settings.set_conservative_threshold(0.65);
        println!(
            "After setting - conservative: {}, strict: {}, kill: {}",
            settings.conservative_threshold(),
            settings.strict_threshold(),
            settings.kill_threshold()
        );
    }
}
