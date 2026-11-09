//! Linux WebKitGTK 6.0 WebView platform adapter

use crate::platform::traits::WebViewPlatformAdapter;
use webkit6::WebView;
use webkit6::prelude::*;

#[derive(Clone)]
pub struct LinuxWebView {
    webview: WebView,
}

impl LinuxWebView {
    pub fn new(webview: WebView) -> Self {
        Self { webview }
    }

    pub fn inner(&self) -> &WebView {
        &self.webview
    }
}

impl WebViewPlatformAdapter for LinuxWebView {
    type NativeHandle = WebView;

    fn with_webview<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&Self::NativeHandle) -> R,
    {
        Some(f(&self.webview))
    }

    fn load_uri(&self, uri: &str) {
        self.webview.load_uri(uri);
    }

    fn reload(&self) {
        self.webview.reload();
    }

    fn stop_loading(&self) {
        self.webview.stop_loading();
    }

    fn can_go_back(&self) -> bool {
        self.webview.can_go_back()
    }

    fn can_go_forward(&self) -> bool {
        self.webview.can_go_forward()
    }

    fn go_back(&self) {
        self.webview.go_back();
    }

    fn go_forward(&self) {
        self.webview.go_forward();
    }

    fn current_uri(&self) -> Option<String> {
        self.webview.uri().map(|u| u.to_string())
    }

    fn current_title(&self) -> Option<String> {
        self.webview.title().map(|t| t.to_string())
    }

    fn estimated_progress(&self) -> f64 {
        self.webview.estimated_load_progress()
    }

    fn is_loading(&self) -> bool {
        self.webview.is_loading()
    }
}
