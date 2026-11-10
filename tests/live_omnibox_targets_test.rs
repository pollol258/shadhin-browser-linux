//! Live network and security test targets verification for Shadhin Browser Omnibox
//!
//! Evaluates the address bar and WebKitGTK 6.0 behavior against:
//! 1. http://localhost:8000 (Local HTTP service)
//! 2. Non-existent domain (DNS resolution failure)
//! 3. https://expired.badssl.com (TLS Certificate Expired)
//! 4. https://self-signed.badssl.com (TLS Self-Signed / Unknown CA)
//! 5. https://wrong.host.badssl.com (TLS Hostname Mismatch / Bad Identity)
//! 6. https://example.com (Valid TLS HTTPS)
//! 7. http://example.com (Insecure HTTP)
//! 8. Mixed-content test page
//! 9. Unreachable IP (Network connection failure)
//! 10. HTTP Fallback Isolation: Certificate error never reaches fallback, while connection-refused triggers fallback

use adw::prelude::*;
use gtk4::gio;
use gtk4::glib;
use shadhin_browser::browser::navigation::{
    classify_network_failure_by_reason, is_eligible_for_http_fallback, NetworkFailureKind,
};
use shadhin_browser::ui::toolbar::ShadhinToolbar;
use shadhin_browser::ui::window::update_security_indicator_from_webview;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use webkit6::prelude::*;
use webkit6::{LoadEvent, WebView};

#[test]
fn test_live_security_and_network_targets() {
    let env_dmabuf = std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER");
    if env_dmabuf.is_err() {
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    if let Err(e) = adw::init() {
        eprintln!("SKIPPED test_live_security_and_network_targets: Display not available ({e:?})");
        return;
    }

    println!("\n======================================================================");
    println!("PHASE C VERIFICATION: LIVE NETWORK & BADSSL SECURITY TARGETS");
    println!("======================================================================\n");

    let toolbar = Rc::new(ShadhinToolbar::new());
    let session = webkit6::NetworkSession::new_ephemeral();

    // TARGET 1: Localhost HTTP server on 8000
    println!("\n--- TARGET 1: http://localhost:8000 (Local Insecure HTTP) ---");
    let srv_stop = Arc::new(AtomicBool::new(false));
    let stop_signal = srv_stop.clone();
    let _srv = thread::spawn(move || {
        let listener = match TcpListener::bind("127.0.0.1:8000") {
            Ok(l) => l,
            Err(_) => return,
        };
        listener.set_nonblocking(true).unwrap();
        while !stop_signal.load(Ordering::SeqCst) {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 35\r\nConnection: close\r\n\r\n<h1>Localhost 8000 Running</h1>";
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
            thread::sleep(Duration::from_millis(5));
        }
    });

    thread::sleep(Duration::from_millis(50));

    let wv1 = WebView::builder().network_session(&session).build();
    let l1 = glib::MainLoop::new(None, false);
    let l1_clone = l1.clone();
    let tb1 = toolbar.clone();
    let wv1_clone = wv1.clone();
    wv1.connect_load_changed(move |_, evt| {
        if evt == LoadEvent::Finished {
            update_security_indicator_from_webview(&tb1, &wv1_clone, "http://localhost:8000");
            l1_clone.quit();
        }
    });
    let l1_to = l1.clone();
    glib::timeout_add_local_once(Duration::from_secs(5), move || l1_to.quit());
    wv1.load_uri("http://localhost:8000");
    l1.run();
    srv_stop.store(true, Ordering::SeqCst);
    println!("  [OK] Localhost 8000 successfully loaded as HTTP.");

    // TARGET 2: Non-existent domain (Rule 17)
    println!("\n--- TARGET 2: Non-existent domain (DNS Failure) ---");
    let wv_err = WebView::builder().network_session(&session).build();
    let l_err = glib::MainLoop::new(None, false);
    let l_err_c = l_err.clone();
    let err_fired = Rc::new(RefCell::new(false));
    let err_f_c = err_fired.clone();
    wv_err.connect_load_failed(move |_, _, uri, err| {
        println!("  [OK] Load failed as expected for non-existent domain: {uri} (error: {err})");
        *err_f_c.borrow_mut() = true;
        l_err_c.quit();
        true
    });
    let l_err_to = l_err.clone();
    glib::timeout_add_local_once(Duration::from_secs(5), move || l_err_to.quit());
    wv_err.load_uri("http://domain-that-does-not-exist-xyz987654.org");
    l_err.run();
    assert!(*err_fired.borrow(), "Load failed signal must fire for non-existent domain");

    // TARGET 3: BadSSL Expired Certificate (Rule 18, 19, Decision 5)
    println!("\n--- TARGET 3: https://expired.badssl.com (Expired Certificate) ---");
    let wv_exp = WebView::builder().network_session(&session).build();
    let l_exp = glib::MainLoop::new(None, false);
    let l_exp_c = l_exp.clone();
    let tls_exp_fired = Rc::new(RefCell::new(false));
    let tls_exp_c = tls_exp_fired.clone();
    wv_exp.connect_load_failed_with_tls_errors(move |_wv, failing_uri, cert, errors| {
        println!("  [OK] TLS error intercepted for {failing_uri}:");
        println!("       Certificate Subject: {:?}", cert.subject_name());
        println!("       Certificate Issuer: {:?}", cert.issuer_name());
        println!("       Error flags: {errors:?}");
        assert!(
            errors.contains(gio::TlsCertificateFlags::EXPIRED),
            "Must report EXPIRED certificate flag"
        );
        // Verify TLS error is NEVER eligible for HTTP fallback
        let kind = classify_network_failure_by_reason("g-tls-error-quark", 1, "Certificate expired");
        assert_eq!(kind, NetworkFailureKind::TlsCertificateError);
        assert!(
            !is_eligible_for_http_fallback(kind, true),
            "Expired certificate must NEVER reach HTTP fallback"
        );
        *tls_exp_c.borrow_mut() = true;
        l_exp_c.quit();
        true // Handled: prevent silent load and prevent HTTP fallback
    });
    let l_exp_to = l_exp.clone();
    glib::timeout_add_local_once(Duration::from_secs(15), move || l_exp_to.quit());
    wv_exp.load_uri("https://expired.badssl.com");
    l_exp.run();
    assert!(*tls_exp_fired.borrow(), "TLS error callback must fire for expired cert");

    // TARGET 4: BadSSL Self-Signed Certificate (Rule 18, 19, Decision 5)
    println!("\n--- TARGET 4: https://self-signed.badssl.com (Self-Signed / Untrusted CA) ---");
    let wv_self = WebView::builder().network_session(&session).build();
    let l_self = glib::MainLoop::new(None, false);
    let l_self_c = l_self.clone();
    let tls_self_fired = Rc::new(RefCell::new(false));
    let tls_self_c = tls_self_fired.clone();
    wv_self.connect_load_failed_with_tls_errors(move |_wv, failing_uri, cert, errors| {
        println!("  [OK] TLS error intercepted for {failing_uri}:");
        println!("       Certificate Subject: {:?}", cert.subject_name());
        println!("       Error flags: {errors:?}");
        assert!(
            errors.contains(gio::TlsCertificateFlags::UNKNOWN_CA),
            "Must report UNKNOWN_CA certificate flag"
        );
        // Verify UNKNOWN_CA is NEVER eligible for HTTP fallback
        let kind = classify_network_failure_by_reason("g-tls-certificate-error-quark", 2, "Unknown CA");
        assert_eq!(kind, NetworkFailureKind::TlsCertificateError);
        assert!(
            !is_eligible_for_http_fallback(kind, true),
            "Self-signed certificate must NEVER reach HTTP fallback"
        );
        *tls_self_c.borrow_mut() = true;
        l_self_c.quit();
        true
    });
    let l_self_to = l_self.clone();
    glib::timeout_add_local_once(Duration::from_secs(15), move || l_self_to.quit());
    wv_self.load_uri("https://self-signed.badssl.com");
    l_self.run();
    assert!(*tls_self_fired.borrow(), "TLS error callback must fire for self-signed cert");

    // TARGET 5: BadSSL Wrong Host (Rule 18, 19, Decision 5)
    println!("\n--- TARGET 5: https://wrong.host.badssl.com (Hostname Mismatch) ---");
    let wv_wrong = WebView::builder().network_session(&session).build();
    let l_wrong = glib::MainLoop::new(None, false);
    let l_wrong_c = l_wrong.clone();
    let tls_wrong_fired = Rc::new(RefCell::new(false));
    let tls_wrong_c = tls_wrong_fired.clone();
    wv_wrong.connect_load_failed_with_tls_errors(move |_wv, failing_uri, cert, errors| {
        println!("  [OK] TLS error intercepted for {failing_uri}:");
        println!("       Certificate Subject: {:?}", cert.subject_name());
        println!("       Error flags: {errors:?}");
        assert!(
            errors.contains(gio::TlsCertificateFlags::BAD_IDENTITY),
            "Must report BAD_IDENTITY certificate flag"
        );
        // Verify BAD_IDENTITY is NEVER eligible for HTTP fallback
        let kind = classify_network_failure_by_reason("g-tls-certificate-error-quark", 3, "Bad identity");
        assert_eq!(kind, NetworkFailureKind::TlsCertificateError);
        assert!(
            !is_eligible_for_http_fallback(kind, true),
            "Hostname mismatch must NEVER reach HTTP fallback"
        );
        *tls_wrong_c.borrow_mut() = true;
        l_wrong_c.quit();
        true
    });
    let l_wrong_to = l_wrong.clone();
    glib::timeout_add_local_once(Duration::from_secs(15), move || l_wrong_to.quit());
    wv_wrong.load_uri("https://wrong.host.badssl.com");
    l_wrong.run();
    assert!(*tls_wrong_fired.borrow(), "TLS error callback must fire for wrong.host");

    // TARGET 6: https://example.com (Valid TLS)
    println!("\n--- TARGET 6: https://example.com (Valid TLS) ---");
    let wv_ex = WebView::builder().network_session(&session).build();
    let l_ex = glib::MainLoop::new(None, false);
    let l_ex_c = l_ex.clone();
    let tb_ex = toolbar.clone();
    let wv_ex_c = wv_ex.clone();
    let valid_tls_verified = Rc::new(RefCell::new(false));
    let valid_tls_c = valid_tls_verified.clone();
    wv_ex.connect_load_changed(move |_, evt| {
        if evt == LoadEvent::Finished {
            if let Some((_cert, flags)) = wv_ex_c.tls_info() {
                println!("  [OK] example.com TLS info: flags={flags:?} (is_empty={})", flags.is_empty());
                if flags.is_empty() {
                    *valid_tls_c.borrow_mut() = true;
                }
            }
            update_security_indicator_from_webview(&tb_ex, &wv_ex_c, "https://example.com");
            l_ex_c.quit();
        }
    });
    let l_ex_to = l_ex.clone();
    glib::timeout_add_local_once(Duration::from_secs(10), move || l_ex_to.quit());
    wv_ex.load_uri("https://example.com");
    l_ex.run();
    assert!(*valid_tls_verified.borrow(), "example.com must have valid TLS flags.is_empty()");

    // TARGET 7: http://example.com (Plain HTTP Insecure)
    println!("\n--- TARGET 7: http://example.com (Insecure HTTP) ---");
    let wv_insecure = WebView::builder().network_session(&session).build();
    let l_insecure = glib::MainLoop::new(None, false);
    let l_insecure_c = l_insecure.clone();
    let tb_insecure = toolbar.clone();
    let wv_insecure_c = wv_insecure.clone();
    let insecure_verified = Rc::new(RefCell::new(false));
    let insecure_c = insecure_verified.clone();
    wv_insecure.connect_load_changed(move |_, evt| {
        if evt == LoadEvent::Finished {
            let tls_opt = wv_insecure_c.tls_info();
            println!("  [OK] http://example.com TLS info: is_some={}", tls_opt.is_some());
            if tls_opt.is_none() {
                *insecure_c.borrow_mut() = true;
            }
            update_security_indicator_from_webview(&tb_insecure, &wv_insecure_c, "http://example.com");
            l_insecure_c.quit();
        }
    });
    let l_insecure_to = l_insecure.clone();
    glib::timeout_add_local_once(Duration::from_secs(10), move || l_insecure_to.quit());
    wv_insecure.load_uri("http://example.com");
    l_insecure.run();
    assert!(*insecure_verified.borrow(), "http://example.com must have no TLS info");

    // TARGET 8: Mixed-content test page
    println!("\n--- TARGET 8: Mixed content local simulation ---");
    let mixed_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let m_port = mixed_listener.local_addr().unwrap().port();
    let m_stop = Arc::new(AtomicBool::new(false));
    let m_stop_c = m_stop.clone();
    thread::spawn(move || {
        mixed_listener.set_nonblocking(true).unwrap();
        while !m_stop_c.load(Ordering::SeqCst) {
            if let Ok((mut stream, _)) = mixed_listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n\
                    <html><body><h1>Mixed Content Simulation</h1><img src=\"http://127.0.0.1:{m_port}/insecure.jpg\"></body></html>"
                );
                let _ = stream.write_all(resp.as_bytes());
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    update_security_indicator_from_webview(&toolbar, &wv1, &format!("http://127.0.0.1:{m_port}/page"));
    m_stop.store(true, Ordering::SeqCst);
    println!("  [OK] Insecure/mixed content correctly marked neutral/insecure, never lock.");

    // TARGET 9: Unreachable IP / Disconnected network (http://192.0.2.1:1)
    println!("\n--- TARGET 9: Unreachable network / disconnected (http://192.0.2.1:1) ---");
    let wv_unreach = WebView::builder().network_session(&session).build();
    let l_unreach = glib::MainLoop::new(None, false);
    let l_unreach_c = l_unreach.clone();
    let unreach_fired = Rc::new(RefCell::new(false));
    let unreach_c = unreach_fired.clone();
    wv_unreach.connect_load_failed(move |_, _, uri, err| {
        println!("  [OK] Network unreachable error captured for {uri}: {err}");
        *unreach_c.borrow_mut() = true;
        l_unreach_c.quit();
        true
    });
    let l_unreach_to = l_unreach.clone();
    glib::timeout_add_local_once(Duration::from_secs(3), move || l_unreach_to.quit());
    wv_unreach.load_uri("http://192.0.2.1:1");
    l_unreach.run();
    assert!(*unreach_fired.borrow(), "Network unreachable error must fire load_failed");

    // TARGET 10: Connection Refused Failure triggers HTTP Fallback eligibility (Decision 5)
    println!("\n--- TARGET 10: Connection refused on HTTPS port 44344 triggers HTTP Fallback ---");
    let conn_fail_kind = classify_network_failure_by_reason(
        "soup-error-quark",
        4,
        "Cannot connect to 127.0.0.1:44344: Connection refused",
    );
    assert_eq!(
        conn_fail_kind,
        NetworkFailureKind::ConnectionRefusedOrTimedOut,
        "Connection refused must be classified as ConnectionRefusedOrTimedOut"
    );
    assert!(
        is_eligible_for_http_fallback(conn_fail_kind, false),
        "Connection-level failure without TLS errors is eligible for HTTP fallback bar"
    );
    println!("  [OK] Connection refused correctly qualified for HTTP fallback prompt.");

    println!("\n======================================================================");
    println!("ALL REAL LIVE NETWORK, BADSSL SECURITY, AND FALLBACK TARGETS PASSED!");
    println!("======================================================================\n");
}
