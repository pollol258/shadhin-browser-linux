//! Main Libadwaita application window with custom styling and engine integration
//!
//! Orchestrates the client-side titlebar with embedded tabs, navigation toolbar,
//! collapsible burgundy sidebar, tab management via `TabManager` and `adw::TabView`,
//! and hardened `webkit6::WebView` instances inside a rounded content frame.

use crate::browser::downloads::DownloadManager;
use crate::browser::navigation::{NavigationTarget, classify_input};
use crate::browser::permissions::{
    PermissionRepository, PermissionState, identify_permission_request,
};
use crate::browser::session::SessionManager;
use crate::browser::tabs::{TAB_DISCARD_THRESHOLD, TabId, TabManager};
use crate::browser::webview::create_configured_webview_with_session;
use crate::config::BrowserConfig;
use crate::privacy::adblock::{ContentFilterManager, SiteExceptionRepository};
use crate::privacy::cookies::{apply_cookie_policy, apply_itp};
use crate::privacy::https_only::{HttpsMode, HttpsUpgradeManager, extract_host_from_uri};
use crate::storage::StorageService;
use crate::storage::history::HistoryRepository;
use crate::storage::workspaces::WorkspaceRepository;
use crate::ui::bookmarks_bar::BookmarksBar;
use crate::ui::bookmarks_panel::BookmarksPanel;
use crate::ui::findbar::FindBar;
use crate::ui::history_panel::HistoryPanel;
use crate::ui::newtab::ShadhinNewTab;
use crate::ui::settings::SettingsWindow;
use crate::ui::sidebar::ShadhinSidebar;
use crate::ui::tabbar::{ShadhinTabStrip, TabDisplayInfo};
use crate::ui::toolbar::ShadhinToolbar;
use crate::utils::debounce::Debounce;
use adw::prelude::*;
use gtk4::gdk::Key;
use gtk4::gio;
use gtk4::glib;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use tracing::info;
use webkit6::prelude::*;
use webkit6::{
    NavigationPolicyDecision, NetworkSession, PolicyDecisionType, ResponsePolicyDecision,
};

/// Representation of visual UI widgets for a browser tab
pub struct TabUiItem {
    pub stack: gtk4::Stack,
    pub webview: RefCell<Option<webkit6::WebView>>,
    pub newtab: ShadhinNewTab,
    pub bookmarks_panel: BookmarksPanel,
    pub history_panel: HistoryPanel,
    pub storage: StorageService,
    pub error_box: gtk4::Box,
    pub error_label: gtk4::Label,
    pub tls_box: gtk4::Box,
    pub tls_host_label: gtk4::Label,
    pub tls_detail_label: gtk4::Label,
    pub tls_advanced_revealer: gtk4::Revealer,
    pub tls_advanced_btn: gtk4::Button,
    pub tls_error_flags_label: gtk4::Label,
    pub tls_proceed_btn: gtk4::Button,
    pub active_tls_cert: RefCell<Option<(gio::TlsCertificate, String, String)>>,
    pub permission_revealer: gtk4::Revealer,
    pub permission_label: gtk4::Label,
    pub permission_icon: gtk4::Image,
    pub permission_remember: gtk4::CheckButton,
    pub permission_allow_btn: gtk4::Button,
    pub permission_block_btn: gtk4::Button,
    pub active_permission_request: RefCell<Option<webkit6::PermissionRequest>>,
    pub https_box: gtk4::Box,
    pub https_host_label: gtk4::Label,
    pub https_desc_label: gtk4::Label,
    pub https_continue_btn: gtk4::Button,
    pub https_fallback_url: RefCell<Option<String>>,
}

#[derive(Clone)]
pub struct ShadhinWindow {
    window: adw::ApplicationWindow,
    split_view: adw::OverlaySplitView,
    toolbar: Rc<ShadhinToolbar>,
    bookmarks_bar: Rc<BookmarksBar>,
    sidebar: Rc<ShadhinSidebar>,
    tab_view: adw::TabView,
    tab_bar: adw::TabBar,
    tab_strip: Rc<ShadhinTabStrip>,
    load_line: gtk4::ProgressBar,
    sandbox_banner: adw::Banner,
    find_bar: Rc<FindBar>,
    tab_manager: Rc<RefCell<TabManager>>,
    tab_items: Rc<RefCell<Vec<Rc<TabUiItem>>>>,
    config: Rc<RefCell<BrowserConfig>>,
    storage: StorageService,
    session_mgr: Rc<RefCell<SessionManager>>,
    download_mgr: DownloadManager,
    current_workspace: Rc<RefCell<String>>,
    https_manager: Rc<RefCell<HttpsUpgradeManager>>,
    active_network_session: Rc<RefCell<NetworkSession>>,
    nav_ctx: Rc<NavigationContext>,
    persistence_allowed: bool,
    fallback_bar: gtk4::Box,
    fallback_target: Rc<RefCell<Option<(TabId, String)>>>,
}

/// Navigation execution context consolidating cross-component dependencies
#[derive(Clone)]
pub struct NavigationContext {
    pub window: adw::ApplicationWindow,
    pub tab_manager: Rc<RefCell<TabManager>>,
    pub config: Rc<RefCell<BrowserConfig>>,
    pub tab_items: Rc<RefCell<Vec<Rc<TabUiItem>>>>,
    pub tab_view: adw::TabView,
    pub tab_strip: Rc<ShadhinTabStrip>,
    pub sidebar: Rc<ShadhinSidebar>,
    pub toolbar: Rc<ShadhinToolbar>,
    pub find_bar: Rc<FindBar>,
    pub storage: StorageService,
    pub session: Rc<RefCell<NetworkSession>>,
    pub filter_manager: ContentFilterManager,
    pub https_manager: Rc<RefCell<HttpsUpgradeManager>>,
    pub download_mgr: DownloadManager,
    pub persistence_allowed: bool,
    pub load_line: gtk4::ProgressBar,
    pub sandbox_banner: adw::Banner,
    pub pending_bare_domain_fallbacks: Rc<RefCell<std::collections::HashMap<TabId, String>>>,
    pub fallback_bar: gtk4::Box,
    pub fallback_target: Rc<RefCell<Option<(TabId, String)>>>,
}

/// Updates address bar security shield based on real WebView TLS state
pub fn update_security_indicator_from_webview(
    tb: &ShadhinToolbar,
    wv: &webkit6::WebView,
    uri: &str,
) {
    if uri.is_empty() || uri == "about:newtab" || uri == "shadhin:newtab" || uri == "about:blank" {
        tb.address_bar()
            .set_security_status("security-low-symbolic", "Internal page: New Tab");
        return;
    }

    if uri.starts_with("about:") || uri.starts_with("shadhin:") {
        tb.address_bar()
            .set_security_status("security-low-symbolic", "Internal application page");
        return;
    }

    if uri.starts_with("file://") {
        tb.address_bar()
            .set_security_status("security-low-symbolic", "Local file system");
        return;
    }

    if uri.starts_with("http://") {
        tb.address_bar().set_security_status(
            "channel-insecure-symbolic",
            "Not secure: Insecure connection (HTTP)",
        );
        return;
    }

    if uri.starts_with("https://") {
        if let Some((_cert, flags)) = wv.tls_info() {
            if flags.is_empty() {
                tb.address_bar().set_security_status(
                    "channel-secure-symbolic",
                    "Connection is secure (HTTPS)",
                );
            } else {
                tb.address_bar().set_security_status(
                    "channel-insecure-symbolic",
                    "Certificate error: Connection is not private",
                );
            }
        } else {
            tb.address_bar().set_security_status(
                "security-low-symbolic",
                "Connection state unverified",
            );
        }
        return;
    }

    tb.address_bar()
        .set_security_status("security-low-symbolic", "Unverified URI scheme");
}

impl ShadhinWindow {
    /// Constructs and initializes the main Shadhin Browser application window
    pub fn new(app: &adw::Application) -> Self {
        Self::new_internal(app, &[], false)
    }

    /// Constructs and initializes the main Shadhin Browser window with specified URLs
    pub fn new_with_urls(app: &adw::Application, initial_urls: &[String]) -> Self {
        Self::new_internal(app, initial_urls, false)
    }

    /// Constructs and initializes a private browsing window with ephemeral session
    pub fn new_private(app: &adw::Application) -> Self {
        Self::new_internal(app, &[], true)
    }

    fn new_internal(app: &adw::Application, initial_urls: &[String], is_private: bool) -> Self {
        let config = Rc::new(RefCell::new(BrowserConfig::load()));

        // 1. Build main Libadwaita window
        let title = if is_private {
            "Shadhin Browser (Private Browsing)"
        } else {
            "Shadhin Browser"
        };
        let css_classes = if is_private {
            vec!["shadhin-window", "shadhin-private-window"]
        } else {
            vec!["shadhin-window"]
        };

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(title)
            .default_width(1280)
            .default_height(800)
            .css_classes(css_classes)
            .build();
        window.set_size_request(600, 450);

        // 2. Tab management and UI components
        let tab_view = adw::TabView::new();
        let tab_strip = Rc::new(ShadhinTabStrip::new(&tab_view));
        let toolbar = Rc::new(ShadhinToolbar::new());
        if is_private {
            toolbar.set_private_mode(true);
        }

        let sidebar = Rc::new(ShadhinSidebar::new());
        let find_bar = Rc::new(FindBar::new());

        let tab_manager = Rc::new(RefCell::new(TabManager::new()));
        let tab_items: Rc<RefCell<Vec<Rc<TabUiItem>>>> = Rc::new(RefCell::new(Vec::new()));

        // TabBar for top strip layout (Layout a)
        let tab_bar = adw::TabBar::builder()
            .view(&tab_view)
            .autohide(false)
            .css_classes(["shadhin-top-tab-bar"])
            .build();

        let tab_bar_new_btn = gtk4::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("New tab (Ctrl+T)")
            .css_classes(["iconbtn", "flat"])
            .valign(gtk4::Align::Center)
            .margin_end(6)
            .build();
        tab_bar_new_btn.set_size_request(32, 32);
        tab_bar.set_end_action_widget(Some(&tab_bar_new_btn));

        // 2px load progress line directly under header
        let load_line = gtk4::ProgressBar::builder()
            .css_classes(["shadhin-load-line"])
            .fraction(0.0)
            .visible(false)
            .hexpand(true)
            .build();
        load_line.set_size_request(-1, 2);

        // Real WebKit child process sandbox warning banner
        let sandbox_banner = adw::Banner::builder()
            .title("Sandbox is not active. Web content may be less isolated.")
            .revealed(false)
            .build();

        tab_view.set_hexpand(true);
        tab_view.set_vexpand(true);

        // 4. Persistence, sessions, and downloads initialization
        let (storage, persistence_allowed) = if is_private {
            let st = StorageService::new_in_memory().unwrap_or_else(|e| {
                tracing::error!("Failed to initialize in-memory storage: {e}");
                StorageService::open_default().unwrap()
            });
            (st, false)
        } else {
            let st = StorageService::open_default().unwrap_or_else(|e| {
                tracing::warn!("Failed to open default storage ({e}); using in-memory database");
                StorageService::new_in_memory().unwrap_or_else(|e2| {
                    tracing::error!("Failed to create in-memory database: {e2}");
                    panic!("Cannot initialize any storage backend: {e2}");
                })
            });
            (st, true)
        };

        let session_mgr = Rc::new(RefCell::new(SessionManager::new(storage.clone())));
        let active_sess_inst = if is_private {
            NetworkSession::new_ephemeral()
        } else {
            session_mgr
                .borrow_mut()
                .get_or_create_workspace_session("default")
        };
        let active_network_session = Rc::new(RefCell::new(active_sess_inst));

        let dl_dir = PathBuf::from(&config.borrow().download_dir);
        let download_mgr = DownloadManager::new(storage.clone(), dl_dir);
        download_mgr.attach_session(&active_network_session.borrow(), persistence_allowed);

        apply_cookie_policy(
            &active_network_session.borrow(),
            crate::privacy::cookies::CookiePolicy::BlockThirdParty,
        );
        apply_itp(&active_network_session.borrow(), true);

        let filter_manager = ContentFilterManager::new();
        filter_manager.initialize(|| {
            info!("Bundled content filter lists compiled & ready");
        });

        let https_manager = Rc::new(RefCell::new(HttpsUpgradeManager::new()));

        toolbar.address_bar().set_storage(storage.clone());
        toolbar.attach_downloads(
            storage.clone(),
            download_mgr.clone(),
            Some(active_network_session.clone()),
            persistence_allowed,
        );
        toolbar.update_address_pill_width(1280);

        let tb_resize = toolbar.clone();
        window.connect_default_width_notify(move |win| {
            let w = if win.width() > 0 {
                win.width()
            } else if win.default_width() > 0 {
                win.default_width()
            } else {
                1280
            };
            tb_resize.update_address_pill_width(w);
        });

        let tb_blur = toolbar.clone();
        let ts_blur = tab_strip.clone();
        let sb_blur = sidebar.clone();
        window.connect_is_active_notify(move |win| {
            if !win.is_active() {
                tb_blur.close_all_popovers();
                ts_blur.close_popover();
                sb_blur.close_popover();
            }
        });
        let bookmarks_bar = Rc::new(BookmarksBar::new(storage.clone()));
        let current_workspace = Rc::new(RefCell::new("default".to_string()));

        // 5. Overlay Split View (Sidebar + Content)
        let split_view = adw::OverlaySplitView::new();
        split_view.set_sidebar(Some(sidebar.widget()));
        split_view.set_content(Some(&tab_view));
        split_view.set_hexpand(true);
        split_view.set_vexpand(true);

        // 6. Central Root Box hosting LoadLine, TabBar, BookmarksBar, FindBar, Banners, and SplitView
        let root_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();

        // Inline HTTP fallback bar under address bar
        let fallback_bar = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["shadhin-http-fallback-bar"])
            .visible(false)
            .build();

        let fallback_icon = gtk4::Image::from_icon_name("dialog-warning-symbolic");
        fallback_icon.set_pixel_size(18);
        fallback_bar.append(&fallback_icon);

        let fallback_label = gtk4::Label::builder()
            .label("This site does not support HTTPS. Open it over an insecure connection?")
            .hexpand(true)
            .xalign(0.0)
            .build();
        fallback_bar.append(&fallback_label);

        let fallback_continue_btn = gtk4::Button::builder()
            .label("Continue")
            .css_classes(["destructive-action"])
            .build();
        fallback_bar.append(&fallback_continue_btn);

        let fallback_cancel_btn = gtk4::Button::builder()
            .label("Cancel")
            .css_classes(["flat"])
            .build();
        fallback_bar.append(&fallback_cancel_btn);

        root_box.append(toolbar.widget());
        root_box.append(&load_line);
        root_box.append(&fallback_bar);
        root_box.append(&tab_bar);
        bookmarks_bar.widget().set_visible(true);
        root_box.append(bookmarks_bar.widget());
        root_box.append(find_bar.widget());

        // Banners
        if is_private {
            let pvt_banner = adw::Banner::builder()
                .title("Private Browsing: Shadhin won't save browsing history, cookies, or site data. Downloaded files remain on disk.")
                .revealed(true)
                .build();
            root_box.append(&pvt_banner);
        } else if session_mgr.borrow().had_unclean_exit() {
            let banner = adw::Banner::builder()
                .title("Shadhin Browser closed unexpectedly.")
                .button_label("Restore Session")
                .revealed(true)
                .build();

            let sm_click = session_mgr.clone();
            let banner_ref = banner.clone();
            banner.connect_button_clicked(move |_| {
                sm_click.borrow().acknowledge_crash();
                banner_ref.set_revealed(false);
            });
            root_box.append(&banner);
        }

        root_box.append(&sandbox_banner);
        root_box.append(&split_view);

        // Tab layout configuration: "top" vs "vertical"
        let initial_layout = config.borrow().tab_layout.clone();
        if initial_layout == "vertical" {
            tab_bar.set_visible(false);
            split_view.set_show_sidebar(true);
            toolbar.update_sidebar_active(true);
        } else {
            tab_bar.set_visible(true);
            split_view.set_show_sidebar(false);
            toolbar.update_sidebar_active(false);
        }

        // Responsive breakpoint: for < 800px collapses sidebar to hidden
        let bp_compact = adw::Breakpoint::new(
            adw::BreakpointCondition::parse("max-width: 800px").expect("valid condition"),
        );
        let sv_bp = split_view.clone();
        let tb_bp = toolbar.clone();
        bp_compact.connect_apply(move |_| {
            sv_bp.set_show_sidebar(false);
            tb_bp.update_sidebar_active(false);
        });
        window.add_breakpoint(bp_compact);

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&root_box));
        window.set_content(Some(&toast_overlay));

        let toast_ov = toast_overlay.clone();
        let win_for_dl = window.clone();
        download_mgr.add_completion_listener(move |item| {
            let toast = adw::Toast::new(&format!("Download completed: {}", item.file_name));
            toast.set_button_label(Some("Open"));
            let file_path = item.file_path.clone();
            let file_name = item.file_name.clone();
            let win_click = win_for_dl.clone();
            toast.connect_button_clicked(move |_| {
                use std::os::unix::fs::PermissionsExt;
                let is_exec = if let Ok(meta) = std::fs::metadata(&file_path) {
                    (meta.permissions().mode() & 0o111) != 0
                        || crate::browser::downloads::is_dangerous_extension(&file_name)
                } else {
                    crate::browser::downloads::is_dangerous_extension(&file_name)
                };

                if is_exec {
                    let alert = adw::AlertDialog::builder()
                        .heading("Open Executable File?")
                        .body(format!(
                            "\"{file_name}\" is an executable file or script. Opening it could allow code to execute on your computer. Are you sure you want to open it?"
                        ))
                        .build();
                    alert.add_response("cancel", "Cancel");
                    alert.add_response("open", "Open Anyway");
                    alert.set_default_response(Some("cancel"));
                    alert.set_close_response("cancel");
                    alert.set_response_appearance("open", adw::ResponseAppearance::Destructive);
                    let fp_confirmed = file_path.clone();
                    alert.connect_response(None, move |_, resp| {
                        if resp == "open" {
                            let uri = format!("file://{}", fp_confirmed.display());
                            let _ = gio::AppInfo::launch_default_for_uri(
                                &uri,
                                None::<&gio::AppLaunchContext>,
                            );
                        }
                    });
                    alert.present(Some(&win_click));
                } else {
                    let uri = format!("file://{}", file_path.display());
                    let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
                }
            });
            toast_ov.add_toast(toast);
        });

        let pending_bare_domain_fallbacks =
            Rc::new(RefCell::new(std::collections::HashMap::new()));
        let fallback_target: Rc<RefCell<Option<(TabId, String)>>> = Rc::new(RefCell::new(None));

        // Connect Continue and Cancel on HTTP fallback bar
        let fb_bar_c = fallback_bar.clone();
        let fb_target_c = fallback_target.clone();
        let items_fb = tab_items.clone();
        let tb_c = toolbar.clone();
        fallback_continue_btn.connect_clicked(move |_| {
            fb_bar_c.set_visible(false);
            if let Some((tab_id, http_url)) = fb_target_c.borrow_mut().take() {
                let items = items_fb.borrow();
                if let Some(item) = items.get(tab_id)
                    && let Some(ref wv) = *item.webview.borrow()
                {
                    tb_c.address_bar().set_text(&http_url);
                    tb_c.address_bar().set_security_status(
                        "channel-insecure-symbolic",
                        "Not secure: Insecure connection (HTTP)",
                    );
                    wv.load_uri(&http_url);
                }
            }
        });

        let fb_bar_cancel = fallback_bar.clone();
        let fb_target_cancel = fallback_target.clone();
        fallback_cancel_btn.connect_clicked(move |_| {
            fb_bar_cancel.set_visible(false);
            fb_target_cancel.borrow_mut().take();
        });

        let nav_ctx = Rc::new(NavigationContext {
            window: window.clone(),
            tab_manager: tab_manager.clone(),
            config: config.clone(),
            tab_items: tab_items.clone(),
            tab_view: tab_view.clone(),
            tab_strip: tab_strip.clone(),
            sidebar: sidebar.clone(),
            toolbar: toolbar.clone(),
            find_bar: find_bar.clone(),
            storage: storage.clone(),
            session: active_network_session.clone(),
            filter_manager: filter_manager.clone(),
            https_manager: https_manager.clone(),
            download_mgr: download_mgr.clone(),
            persistence_allowed,
            load_line: load_line.clone(),
            sandbox_banner: sandbox_banner.clone(),
            pending_bare_domain_fallbacks,
            fallback_bar: fallback_bar.clone(),
            fallback_target: fallback_target.clone(),
        });

        // Wire top TabBar '+' new tab button
        let nav_nt_tb = nav_ctx.clone();
        tab_bar_new_btn.connect_clicked(move |_| {
            nav_nt_tb.create_tab(None, None);
        });

        // Wire sidebar new tab button
        let nav_nt_sb = nav_ctx.clone();
        sidebar.connect_new_tab(move || {
            nav_nt_sb.create_tab(None, None);
        });

        // Wire sidebar tab select
        let tv_sb_sel = tab_view.clone();
        sidebar.connect_tab_selected(move |idx| {
            if idx >= 0 && idx < tv_sb_sel.n_pages() {
                let page = tv_sb_sel.nth_page(idx);
                tv_sb_sel.set_selected_page(&page);
            }
        });

        // Wire sidebar tab close
        let tv_sb_close = tab_view.clone();
        sidebar.connect_tab_close(move |idx| {
            if idx >= 0 && idx < tv_sb_close.n_pages() {
                let page = tv_sb_close.nth_page(idx);
                tv_sb_close.close_page(&page);
            }
        });

        // Wire sidebar tab pin
        let tm_sb_pin = tab_manager.clone();
        let nav_sb_pin = nav_ctx.clone();
        let tv_sb_pin = tab_view.clone();
        sidebar.connect_tab_pin(move |idx| {
            if idx >= 0 {
                let tm = tm_sb_pin.borrow_mut();
                if let Some(tab_cell) = tm.get_tab(idx as usize) {
                    let mut tab = tab_cell.borrow_mut();
                    tab.pinned = !tab.pinned;
                    if idx < tv_sb_pin.n_pages() {
                        let page = tv_sb_pin.nth_page(idx);
                        tv_sb_pin.set_page_pinned(&page, tab.pinned);
                    }
                }
                drop(tm);
                nav_sb_pin.sync_tabs();
            }
        });

        // Wire sidebar tab sleep
        let tm_sb_sleep = tab_manager.clone();
        let nav_sb_sleep = nav_ctx.clone();
        sidebar.connect_tab_sleep(move |idx| {
            if idx >= 0 {
                let tm = tm_sb_sleep.borrow_mut();
                if let Some(tab_cell) = tm.get_tab(idx as usize) {
                    let mut tab = tab_cell.borrow_mut();
                    tab.is_discarded = true;
                    tab.has_active_webview = false;
                }
                drop(tm);
                nav_sb_sleep.sync_tabs();
            }
        });

        let win = Self {
            window,
            split_view,
            toolbar,
            bookmarks_bar,
            sidebar,
            tab_view,
            tab_bar,
            tab_strip,
            load_line,
            sandbox_banner,
            find_bar,
            tab_manager,
            tab_items,
            config,
            storage,
            session_mgr,
            download_mgr,
            current_workspace,
            https_manager,
            active_network_session,
            nav_ctx,
            persistence_allowed,
            fallback_bar,
            fallback_target,
        };

        win.setup_signals();
        win.setup_shortcuts();
        win.setup_actions();

        if persistence_allowed {
            let storage_clean = win.storage.clone();
            let retention_debounce = Debounce::new();
            retention_debounce.debounce(Duration::from_secs(5), move || {
                storage_clean.dispatch(|db| {
                    let repo = HistoryRepository::new(db.conn());
                    let _ = repo.cleanup_retention(90);
                });
            });
        }

        // Open initial tabs (either landing New Tab or requested URLs)
        if initial_urls.is_empty() {
            win.create_tab(None, None);
        } else {
            for url in initial_urls {
                win.create_tab(Some(url), None);
            }
        }

        win
    }

    /// Creates and configures a new tab page with lazy WebView allocation
    pub fn create_tab(&self, initial_url: Option<&str>, parent_id: Option<TabId>) -> usize {
        self.nav_ctx.create_tab(initial_url, parent_id)
    }

    /// Standalone helper to trigger UI tab strip and sidebar synchronization
    fn sync_tabs_external(
        tm_cell: &Rc<RefCell<TabManager>>,
        ts: &Rc<ShadhinTabStrip>,
        sb: &Rc<ShadhinSidebar>,
        tv: &adw::TabView,
        tb: &Rc<ShadhinToolbar>,
    ) {
        let tm = tm_cell.borrow();
        let tabs_info: Vec<TabDisplayInfo> = tm
            .tabs()
            .iter()
            .map(|t| {
                let b = t.borrow();
                TabDisplayInfo {
                    title: b.title.clone(),
                    favicon: b.favicon.clone(),
                    is_newtab: b.is_newtab(),
                    is_discarded: b.is_discarded,
                    is_pinned: b.pinned,
                }
            })
            .collect();

        ts.refresh(&tabs_info);

        let sel_idx = tm.selected_index();
        let tv_clone = tv.clone();
        sb.update_open_tabs(&tabs_info, sel_idx as i32, move |idx| {
            if idx >= 0 && idx < tv_clone.n_pages() {
                let page = tv_clone.nth_page(idx);
                tv_clone.set_selected_page(&page);
            }
        });

        // Reflect real discarded / sleeping tab state on AdwTabPage indicator
        for (i, info) in tabs_info.iter().enumerate() {
            if (i as i32) < tv.n_pages() {
                let page = tv.nth_page(i as i32);
                if info.is_discarded {
                    page.set_indicator_icon(Some(&gio::ThemedIcon::new("weather-clear-night-symbolic")));
                    page.set_indicator_tooltip("Sleeping tab (RAM discarded)");
                } else {
                    page.set_indicator_icon(None::<&gio::ThemedIcon>);
                    page.set_indicator_tooltip("");
                }
            }
        }

        if let Some(active_tab_cell) = tm.active_tab() {
            let active = active_tab_cell.borrow();
            tb.set_is_loading(active.is_loading);
            if active.is_newtab() {
                tb.address_bar().set_text_if_unfocused("");
                tb.address_bar().set_security_status("security-low-symbolic", "New Tab");
            } else {
                tb.address_bar().set_text_if_unfocused(&active.url);
                tb.address_bar().update_current_page(&active.url, &active.title);
                if active.url.starts_with("http://") {
                    tb.address_bar().set_security_status(
                        "channel-insecure-symbolic",
                        "Not secure: Insecure connection (HTTP)",
                    );
                } else if active.url.starts_with("about:") || active.url.starts_with("shadhin:") {
                    tb.address_bar().set_security_status(
                        "security-low-symbolic",
                        "Internal application page",
                    );
                }
            }
        }
    }
}

impl NavigationContext {
    pub fn create_tab(&self, initial_url: Option<&str>, parent_id: Option<TabId>) -> usize {
        let (tab_idx, _tab_model) = self
            .tab_manager
            .borrow_mut()
            .new_tab(initial_url, parent_id);

        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        stack.set_hexpand(true);
        stack.set_vexpand(true);

        // 1. Native New Tab view
        let newtab = ShadhinNewTab::new();
        stack.add_named(newtab.widget(), Some("newtab"));

        // 2. Styled Network Error view
        let error_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .valign(gtk4::Align::Center)
            .halign(gtk4::Align::Center)
            .css_classes(["shadhin-error-box"])
            .build();

        let error_icon = gtk4::Image::from_icon_name("network-error-symbolic");
        error_icon.set_pixel_size(48);
        error_box.append(&error_icon);

        let error_title = gtk4::Label::builder()
            .label("Unable to Connect")
            .css_classes(["shadhin-error-title"])
            .build();
        error_box.append(&error_title);

        let error_label = gtk4::Label::builder()
            .label("Check your network connection and try again.")
            .css_classes(["shadhin-error-desc"])
            .wrap(true)
            .justify(gtk4::Justification::Center)
            .build();
        error_box.append(&error_label);

        let retry_btn = gtk4::Button::builder()
            .label("Try Again")
            .css_classes(["suggested-action", "shadhin-nav-btn"])
            .margin_top(8)
            .build();
        error_box.append(&retry_btn);
        stack.add_named(&error_box, Some("error"));

        // 3. Styled TLS Certificate Warning view
        let tls_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .valign(gtk4::Align::Center)
            .halign(gtk4::Align::Center)
            .css_classes(["shadhin-tls-box"])
            .build();

        let tls_icon = gtk4::Image::from_icon_name("dialog-warning-symbolic");
        tls_icon.set_pixel_size(52);
        tls_box.append(&tls_icon);

        let tls_title = gtk4::Label::builder()
            .label("Your connection is not private")
            .css_classes(["shadhin-tls-title"])
            .build();
        tls_box.append(&tls_title);

        let tls_host_label = gtk4::Label::builder()
            .label("")
            .css_classes(["shadhin-tls-desc"])
            .build();
        tls_box.append(&tls_host_label);

        let tls_detail_label = gtk4::Label::builder()
            .label(
                "The security certificate presented by this website has errors or is not trusted.",
            )
            .css_classes(["shadhin-tls-desc"])
            .wrap(true)
            .justify(gtk4::Justification::Center)
            .build();
        tls_box.append(&tls_detail_label);

        let tls_actions = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk4::Align::Center)
            .margin_top(12)
            .build();

        let back_safety_btn = gtk4::Button::builder()
            .label("Go Back to Safety")
            .css_classes(["suggested-action", "shadhin-nav-btn"])
            .build();
        let tls_advanced_btn = gtk4::Button::builder()
            .label("Advanced")
            .css_classes(["flat", "shadhin-nav-btn"])
            .build();
        tls_actions.append(&back_safety_btn);
        tls_actions.append(&tls_advanced_btn);
        tls_box.append(&tls_actions);

        let tls_advanced_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();

        let advanced_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .margin_top(12)
            .halign(gtk4::Align::Center)
            .build();

        let tls_error_flags_label = gtk4::Label::builder()
            .label("Certificate error details")
            .css_classes(["dim-label"])
            .wrap(true)
            .justify(gtk4::Justification::Center)
            .build();
        advanced_box.append(&tls_error_flags_label);

        let tls_proceed_btn = gtk4::Button::builder()
            .label("Accept Risk and Continue (Unsafe)")
            .css_classes(["destructive-action", "flat"])
            .halign(gtk4::Align::Center)
            .build();
        advanced_box.append(&tls_proceed_btn);

        tls_advanced_revealer.set_child(Some(&advanced_box));
        tls_box.append(&tls_advanced_revealer);
        stack.add_named(&tls_box, Some("tls"));

        // 4. HTTPS-Only Fallback Interstitial view
        let https_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(16)
            .valign(gtk4::Align::Center)
            .halign(gtk4::Align::Center)
            .css_classes(["shadhin-https-box"])
            .build();

        let https_icon = gtk4::Image::from_icon_name("security-low-symbolic");
        https_icon.set_pixel_size(48);
        https_box.append(&https_icon);

        let https_title = gtk4::Label::builder()
            .label("Secure Connection Not Available")
            .css_classes(["shadhin-https-title"])
            .build();
        https_box.append(&https_title);

        let https_host_label = gtk4::Label::builder()
            .css_classes(["shadhin-https-host"])
            .build();
        https_box.append(&https_host_label);

        let https_desc = gtk4::Label::builder()
            .label("HTTPS-Only Mode is enabled. The site you are attempting to visit does not support HTTPS.")
            .css_classes(["shadhin-https-desc"])
            .wrap(true)
            .justify(gtk4::Justification::Center)
            .build();
        https_box.append(&https_desc);

        let https_actions = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk4::Align::Center)
            .margin_top(8)
            .build();

        let https_back_btn = gtk4::Button::builder()
            .label("Go Back")
            .css_classes(["suggested-action", "shadhin-nav-btn"])
            .build();
        let https_continue_btn = gtk4::Button::builder()
            .label("Continue to HTTP Site")
            .css_classes(["destructive-action", "shadhin-nav-btn"])
            .build();
        https_actions.append(&https_back_btn);
        https_actions.append(&https_continue_btn);
        https_box.append(&https_actions);
        stack.add_named(&https_box, Some("https_error"));

        // 5. Non-modal Permission Request Bar
        let permission_revealer = gtk4::Revealer::new();
        permission_revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        let perm_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["shadhin-permission-bar"])
            .build();
        let permission_icon = gtk4::Image::from_icon_name("dialog-question-symbolic");
        permission_icon.set_pixel_size(20);
        let permission_label = gtk4::Label::builder()
            .label("This site wants access to a device feature")
            .hexpand(true)
            .xalign(0.0)
            .build();
        let permission_remember = gtk4::CheckButton::with_label("Remember decision");
        permission_remember.set_active(true);
        let permission_allow_btn = gtk4::Button::builder()
            .label("Allow")
            .css_classes(["suggested-action", "pill"])
            .build();
        let permission_block_btn = gtk4::Button::builder()
            .label("Block")
            .css_classes(["pill"])
            .build();
        perm_box.append(&permission_icon);
        perm_box.append(&permission_label);
        perm_box.append(&permission_remember);
        perm_box.append(&permission_allow_btn);
        perm_box.append(&permission_block_btn);
        permission_revealer.set_child(Some(&perm_box));
        permission_revealer.set_reveal_child(false);

        // 6. Native Bookmarks, History, Downloads Panels
        let bookmarks_panel = BookmarksPanel::new(self.storage.clone());
        stack.add_named(bookmarks_panel.widget(), Some("bookmarks"));

        let history_panel = HistoryPanel::new(self.storage.clone());
        stack.add_named(history_panel.widget(), Some("history"));

        newtab.load_data(self.storage.clone());

        let ui_item = Rc::new(TabUiItem {
            stack: stack.clone(),
            webview: RefCell::new(None),
            newtab,
            bookmarks_panel,
            history_panel,
            storage: self.storage.clone(),
            error_box,
            error_label,
            tls_box,
            tls_host_label,
            tls_detail_label,
            tls_advanced_revealer: tls_advanced_revealer.clone(),
            tls_advanced_btn: tls_advanced_btn.clone(),
            tls_error_flags_label,
            tls_proceed_btn: tls_proceed_btn.clone(),
            active_tls_cert: RefCell::new(None),
            permission_revealer: permission_revealer.clone(),
            permission_label,
            permission_icon,
            permission_remember: permission_remember.clone(),
            permission_allow_btn: permission_allow_btn.clone(),
            permission_block_btn: permission_block_btn.clone(),
            active_permission_request: RefCell::new(None),
            https_box,
            https_host_label: https_host_label.clone(),
            https_desc_label: https_desc.clone(),
            https_continue_btn: https_continue_btn.clone(),
            https_fallback_url: RefCell::new(None),
        });

        self.tab_items.borrow_mut().push(ui_item.clone());

        let page_container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        page_container.append(&permission_revealer);
        page_container.append(&stack);

        let page = self.tab_view.append(&page_container);
        page.set_title("New Tab");
        self.tab_view.set_selected_page(&page);

        // Wire NewTab navigation callback
        let nav_nt = self.clone();
        let page_nt = page.clone();
        ui_item.newtab.set_on_navigate(move |input| {
            info!("NewTab search/navigation initiated");
            let is_dev = std::env::var("SHADHIN_DEV").is_ok()
                || std::env::var("SHADHIN_DEV_CSS").is_ok();
            let settings = crate::browser::navigation::ClassificationSettings {
                search_template: nav_nt.config.borrow().search_engine_url.clone(),
                allow_file: true,
                developer_extras: is_dev,
            };
            let target = classify_input(input, &settings);
            nav_nt.execute_navigation(tab_idx, &target, &page_nt);
        });

        // If initial_url was provided and not a newtab page, initialize the WebView and load
        if let Some(url) = initial_url {
            if url != "about:newtab" && url != "shadhin:newtab" {
                let is_dev = std::env::var("SHADHIN_DEV").is_ok()
                    || std::env::var("SHADHIN_DEV_CSS").is_ok()
                    || url.starts_with("view-source:");
                let settings = crate::browser::navigation::ClassificationSettings {
                    search_template: self.config.borrow().search_engine_url.clone(),
                    allow_file: true,
                    developer_extras: is_dev,
                };
                let target = classify_input(url, &settings);
                self.execute_navigation(tab_idx, &target, &page);
            } else {
                stack.set_visible_child_name("newtab");
                self.toolbar.address_bar().set_text("");
                self.toolbar
                    .address_bar()
                    .set_security_status("security-low-symbolic", "New Tab");
                self.toolbar.address_bar().focus();
            }
        } else {
            stack.set_visible_child_name("newtab");
            self.toolbar.address_bar().set_text("");
            self.toolbar
                .address_bar()
                .set_security_status("security-low-symbolic", "New Tab");
            self.toolbar.address_bar().focus();
        }

        // Wire Retry button on Error view
        let nav_retry = self.clone();
        let page_retry = page.clone();
        retry_btn.connect_clicked(move |_| {
            let tm = nav_retry.tab_manager.borrow();
            if let Some(tab_cell) = tm.get_tab(tab_idx) {
                let url = tab_cell.borrow().url.clone();
                let is_dev = std::env::var("SHADHIN_DEV").is_ok()
                    || std::env::var("SHADHIN_DEV_CSS").is_ok();
                let settings = crate::browser::navigation::ClassificationSettings {
                    search_template: nav_retry.config.borrow().search_engine_url.clone(),
                    allow_file: true,
                    developer_extras: is_dev,
                };
                let target = classify_input(&url, &settings);
                drop(tm);
                nav_retry.execute_navigation(tab_idx, &target, &page_retry);
            }
        });

        // Wire Advanced button toggle and Accept Risk button on TLS view
        let rev_tls = tls_advanced_revealer;
        tls_advanced_btn.connect_clicked(move |_| {
            rev_tls.set_reveal_child(!rev_tls.reveals_child());
        });

        let nav_tls_proceed = self.clone();
        let ui_item_proceed = ui_item.clone();
        let stack_proceed = stack.clone();
        tls_proceed_btn.connect_clicked(move |_| {
            if let Some((cert, host, failing_uri)) =
                ui_item_proceed.active_tls_cert.borrow_mut().take()
            {
                info!("User accepted TLS certificate risk for host: {host}");
                nav_tls_proceed
                    .session
                    .borrow()
                    .allow_tls_certificate_for_host(&cert, &host);
                if let Some(ref wv) = *ui_item_proceed.webview.borrow() {
                    stack_proceed.set_visible_child_name("web");
                    wv.load_uri(&failing_uri);
                }
            }
        });

        // Wire Safety button on TLS view (Go back or open New Tab)
        let nav_tls_back = self.clone();
        let stack_tls = stack.clone();
        back_safety_btn.connect_clicked(move |_| {
            let items = nav_tls_back.tab_items.borrow();
            if let Some(item) = items.get(tab_idx)
                && let Some(ref wv) = *item.webview.borrow()
                && wv.can_go_back()
            {
                wv.go_back();
                stack_tls.set_visible_child_name("web");
                return;
            }
            let tm = nav_tls_back.tab_manager.borrow();
            if let Some(tab_cell) = tm.get_tab(tab_idx) {
                let mut tab = tab_cell.borrow_mut();
                tab.url = "about:newtab".to_string();
                tab.title = "New Tab".to_string();
            }
            stack_tls.set_visible_child_name("newtab");
        });

        // Wire HTTPS Go Back button
        let nav_hb = self.clone();
        let stack_hb = stack.clone();
        https_back_btn.connect_clicked(move |_| {
            let items = nav_hb.tab_items.borrow();
            if let Some(item) = items.get(tab_idx)
                && let Some(ref wv) = *item.webview.borrow()
                && wv.can_go_back()
            {
                wv.go_back();
                stack_hb.set_visible_child_name("web");
                return;
            }
            let tm = nav_hb.tab_manager.borrow();
            if let Some(tab_cell) = tm.get_tab(tab_idx) {
                let mut tab = tab_cell.borrow_mut();
                tab.url = "about:newtab".to_string();
                tab.title = "New Tab".to_string();
            }
            stack_hb.set_visible_child_name("newtab");
        });

        // Wire HTTPS Continue to HTTP Site button
        let nav_hc = self.clone();
        let ui_hc = ui_item.clone();
        let host_hc = https_host_label.clone();
        https_continue_btn.connect_clicked(move |_| {
            let fallback_url_opt = ui_hc.https_fallback_url.borrow().clone();
            let url_to_load = if let Some(ref fallback) = fallback_url_opt {
                fallback.clone()
            } else {
                let host = host_hc.text().to_string();
                if host.is_empty() {
                    return;
                }
                format!("http://{host}")
            };

            if let Some(host) = extract_host_from_uri(&url_to_load) {
                nav_hc
                    .https_manager
                    .borrow_mut()
                    .add_session_exemption(&host);
            }

            if let Some(ref wv) = *ui_hc.webview.borrow() {
                wv.load_uri(&url_to_load);
                ui_hc.stack.set_visible_child_name("web");
            }
        });

        // Wire Permission Allow button
        let nav_pa = self.clone();
        let ui_pa = ui_item.clone();
        let rev_pa = permission_revealer.clone();
        permission_allow_btn.connect_clicked(move |_| {
            if let Some(req) = ui_pa.active_permission_request.borrow_mut().take() {
                let (ptype, _) = identify_permission_request(&req);
                let remember = ui_pa.permission_remember.is_active();
                if remember
                    && nav_pa.persistence_allowed
                    && let Some(ref wv) = *ui_pa.webview.borrow()
                    && let Some(uri) = wv.uri()
                    && let Some(host) = extract_host_from_uri(uri.as_str())
                {
                    nav_pa.storage.dispatch(move |db| {
                        let repo = PermissionRepository::new(db.conn());
                        let _ = repo.set_permission(
                            &host,
                            ptype,
                            PermissionState::Allow,
                            nav_pa.persistence_allowed,
                        );
                    });
                }
                req.allow();
                rev_pa.set_reveal_child(false);
            }
        });

        // Wire Permission Block button
        let nav_pb = self.clone();
        let ui_pb = ui_item.clone();
        let rev_pb = permission_revealer.clone();
        permission_block_btn.connect_clicked(move |_| {
            if let Some(req) = ui_pb.active_permission_request.borrow_mut().take() {
                let (ptype, _) = identify_permission_request(&req);
                let remember = ui_pb.permission_remember.is_active();
                if remember
                    && nav_pb.persistence_allowed
                    && let Some(ref wv) = *ui_pb.webview.borrow()
                    && let Some(uri) = wv.uri()
                    && let Some(host) = extract_host_from_uri(uri.as_str())
                {
                    nav_pb.storage.dispatch(move |db| {
                        let repo = PermissionRepository::new(db.conn());
                        let _ = repo.set_permission(
                            &host,
                            ptype,
                            PermissionState::Block,
                            nav_pb.persistence_allowed,
                        );
                    });
                }
                req.deny();
                rev_pb.set_reveal_child(false);
            }
        });

        self.sync_tabs();
        tab_idx
    }

    pub fn setup_webview_for_tab(&self, tab_idx: usize, page: &adw::TabPage) -> webkit6::WebView {
        let js_enabled = self.config.borrow().privacy.enable_javascript;
        let session = self.session.borrow();
        let webview = create_configured_webview_with_session(js_enabled, Some(&*session));

        // Attach content filter rules
        let ucm = webview.user_content_manager().expect("UserContentManager");
        let shield_enabled = self.config.borrow().privacy.shield_enabled;
        let ads_enabled = self.config.borrow().privacy.block_ads;
        let trackers_enabled = self.config.borrow().privacy.block_trackers;
        self.filter_manager
            .apply_filters(&ucm, shield_enabled, ads_enabled, trackers_enabled);

        // Attach to stack safely with bounds check
        let items = self.tab_items.borrow();
        let Some(ui_item) = items.get(tab_idx).cloned() else {
            tracing::warn!(
                "tab_items index {tab_idx} out of bounds (len: {})",
                items.len()
            );
            return webview;
        };
        webview.set_visible(true);
        ui_item.stack.add_named(&webview, Some("web"));
        *ui_item.webview.borrow_mut() = Some(webview.clone());
        let storage_lc = ui_item.storage.clone();
        drop(items);

        let Some(tab_c) = self.tab_manager.borrow().get_tab(tab_idx) else {
            tracing::warn!("tab_manager index {tab_idx} out of bounds");
            return webview;
        };
        let tab_id = tab_c.borrow().id;

        self.find_bar.attach_webview(&webview);

        // 1. connect_load_changed
        let nav_lc = self.clone();
        let page_lc = page.clone();
        let tab_c_lc = tab_c.clone();
        let tb_lc = self.toolbar.clone();

        webview.connect_load_changed(move |wv, event| {
            let mut tab = tab_c_lc.borrow_mut();
            match event {
                webkit6::LoadEvent::Started => {
                    tab.is_loading = true;
                    tb_lc.set_is_loading(true);
                    nav_lc.load_line.set_fraction(0.05);
                    nav_lc.load_line.set_visible(true);
                    if let Some(uri) = wv.uri() {
                        let uri_str = uri.as_str();
                        tab.url = uri_str.to_string();
                        tb_lc.address_bar().set_text_if_unfocused(uri_str);
                    }
                }
                webkit6::LoadEvent::Committed => {
                    nav_lc
                        .https_manager
                        .borrow_mut()
                        .clear_upgrade_attempt(tab_id);
                    nav_lc
                        .pending_bare_domain_fallbacks
                        .borrow_mut()
                        .remove(&tab_id);
                    nav_lc.fallback_bar.set_visible(false);
                    nav_lc.fallback_target.borrow_mut().take();
                    if let Some(uri) = wv.uri() {
                        let uri_str = uri.as_str();
                        tab.url = uri_str.to_string();
                        tb_lc.address_bar().set_text_if_unfocused(uri_str);
                        update_security_indicator_from_webview(&tb_lc, wv, uri_str);
                        tb_lc.address_bar().update_current_page(uri_str, &tab.title);
                        if let Some(host) = extract_host_from_uri(uri_str) {
                            tb_lc.privacy_popover().update_for_site(Some(&host), true);
                        }
                    }
                    tab.can_go_back = wv.can_go_back();
                    tab.can_go_forward = wv.can_go_forward();
                    tb_lc.set_can_go_back(tab.can_go_back);
                    tb_lc.set_can_go_forward(tab.can_go_forward);
                }
                webkit6::LoadEvent::Finished => {
                    tab.is_loading = false;
                    tab.progress = 0.0;
                    tb_lc.set_is_loading(false);
                    tb_lc.address_bar().set_progress(0.0);
                    nav_lc.load_line.set_visible(false);
                    nav_lc.load_line.set_fraction(0.0);

                    // Real WebKit child process sandbox check
                    let sb_status = crate::browser::sandbox::check_webkit_child_sandbox();
                    if sb_status == crate::browser::sandbox::SandboxStatus::Inactive {
                        nav_lc.sandbox_banner.set_revealed(true);
                    } else {
                        nav_lc.sandbox_banner.set_revealed(false);
                    }

                    if let Some(title) = wv.title() {
                        let title_str = title.to_string();
                        tab.title = title_str.clone();
                        page_lc.set_title(&title_str);
                    }
                    tab.favicon = wv.favicon();
                    tab.can_go_back = wv.can_go_back();
                    tab.can_go_forward = wv.can_go_forward();
                    tb_lc.set_can_go_back(tab.can_go_back);
                    tb_lc.set_can_go_forward(tab.can_go_forward);

                    let current_url = tab.url.clone();
                    let current_title = tab.title.clone();

                    drop(tab);

                    if let Some(uri) = wv.uri() {
                        let uri_str = uri.as_str();
                        update_security_indicator_from_webview(&tb_lc, wv, uri_str);
                        tb_lc.address_bar().update_current_page(uri_str, &current_title);
                    }

                    crate::utils::net_diagnostics::record_load_finished(&current_url);

                    // Record visit to history for web URLs if persistence is allowed
                    if nav_lc.persistence_allowed
                        && (current_url.starts_with("http://")
                            || current_url.starts_with("https://"))
                    {
                        let st = storage_lc.clone();
                        st.dispatch(move |db| {
                            let repo = HistoryRepository::new(db.conn());
                            let _ = repo.record_visit(&current_url, &current_title, true);
                        });
                    }

                    nav_lc.sync_tabs();
                }
                _ => {}
            }
        });

        // 1b. connect_uri_notify (follows fragment and client pushState changes)
        let tb_uri = self.toolbar.clone();
        let tab_c_uri = tab_c.clone();
        webview.connect_uri_notify(move |wv| {
            if let Some(uri) = wv.uri() {
                let uri_str = uri.as_str();
                let mut tab = tab_c_uri.borrow_mut();
                tab.url = uri_str.to_string();
                let title = tab.title.clone();
                drop(tab);
                tb_uri.address_bar().set_text_if_unfocused(uri_str);
                tb_uri.address_bar().update_current_page(uri_str, &title);
            }
        });

        // 2. connect_title_notify
        let nav_title = self.clone();
        let page_title = page.clone();
        let tab_c_title = tab_c.clone();
        webview.connect_title_notify(move |wv| {
            if let Some(t) = wv.title() {
                let s = t.to_string();
                tab_c_title.borrow_mut().title = s.clone();
                page_title.set_title(&s);
                nav_title.sync_tabs();
            }
        });

        // 3. connect_favicon_notify
        let nav_fav = self.clone();
        let tab_c_fav = tab_c.clone();
        webview.connect_favicon_notify(move |wv| {
            tab_c_fav.borrow_mut().favicon = wv.favicon();
            nav_fav.sync_tabs();
        });

        // 4. connect_estimated_load_progress_notify
        let tb_prog = self.toolbar.clone();
        let tab_c_prog = tab_c.clone();
        let ll_prog = self.load_line.clone();
        webview.connect_estimated_load_progress_notify(move |wv| {
            let progress = wv.estimated_load_progress();
            tab_c_prog.borrow_mut().progress = progress;
            tb_prog.address_bar().set_progress(progress);
            if progress > 0.0 && progress < 1.0 {
                ll_prog.set_fraction(progress);
                ll_prog.set_visible(true);
            } else {
                ll_prog.set_visible(false);
                ll_prog.set_fraction(0.0);
            }
        });

        // 5. connect_decide_policy (HTTPS-Only Upgrade interceptor & Net Diagnostics)
        let https_mgr_dp = self.https_manager.clone();
        let cfg_dp = self.config.clone();
        let ui_dp = ui_item.clone();
        let tab_c_dp = tab_c.clone();
        let tb_dp = self.toolbar.clone();
        let page_dp = page.clone();

        webview.connect_decide_policy(move |wv, decision, decision_type| {
            if decision_type == PolicyDecisionType::Response
                && let Ok(resp_decision) = decision.clone().downcast::<ResponsePolicyDecision>()
            {
                let is_main_frame = resp_decision.is_main_frame_main_resource();
                if let Some(resp) = resp_decision.response() {
                    crate::utils::net_diagnostics::record_response_decision(&resp, is_main_frame);
                }

                let is_supported = resp_decision.is_mime_type_supported();
                let is_attachment = resp_decision
                    .response()
                    .and_then(|r| r.http_headers())
                    .map(|h| {
                        if let Some((disposition, _)) = h.content_disposition() {
                            disposition.eq_ignore_ascii_case("attachment")
                        } else if let Some(cd) = h.one("Content-Disposition") {
                            cd.to_ascii_lowercase().contains("attachment")
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);

                if !is_supported || is_attachment {
                    info!(
                        "ResponsePolicyDecision: routing to download manager (supported={is_supported}, attachment={is_attachment})"
                    );
                    decision.download();
                    return true;
                }
            }

            if decision_type == PolicyDecisionType::NavigationAction
                && let Ok(nav_decision) = decision.clone().downcast::<NavigationPolicyDecision>()
                && let Some(action) = nav_decision.navigation_action()
                && let Some(req) = action.request()
                && let Some(uri) = req.uri()
            {
                let is_redirect = action.is_redirect();
                crate::utils::net_diagnostics::record_navigation_request(
                    &req,
                    is_redirect,
                    None,
                );

                let uri_str = uri.as_str();
                // Block web content from navigating to internal application schemes or local filesystem
                if uri_str.starts_with("shadhin:") || uri_str.starts_with("file:") {
                    tracing::warn!("Blocked attempt by web content to navigate to internal/local URI: {uri_str}");
                    decision.ignore();
                    return true;
                }

                if uri_str.starts_with("http://")
                    && let Some(host) = extract_host_from_uri(uri_str)
                {
                    let mode = cfg_dp.borrow().privacy.resolved_https_mode();

                    // Redirect loop detection: if this tab is already attempting an upgrade
                    // and the site redirects back to the original insecure URI, stop and show interstitial
                    if https_mgr_dp.borrow().is_redirect_loop(tab_id, uri_str) {
                        tracing::warn!(
                            "Detected HTTPS upgrade redirect loop for tab {tab_id} at {uri_str}"
                        );
                        decision.ignore();
                        https_mgr_dp.borrow_mut().clear_upgrade_attempt(tab_id);

                        ui_dp
                            .https_fallback_url
                            .borrow_mut()
                            .replace(uri_str.to_string());
                        ui_dp.https_host_label.set_label(&host);

                        if mode == HttpsMode::Strict {
                            ui_dp.https_desc_label.set_label(
                                "HTTPS-Only Mode is set to Strict. Insecure HTTP connections are blocked and cannot be bypassed.",
                            );
                            ui_dp.https_continue_btn.set_visible(false);
                        } else {
                            ui_dp.https_desc_label.set_label(
                                "This website is repeatedly redirecting to an insecure HTTP connection.",
                            );
                            ui_dp.https_continue_btn.set_visible(true);
                        }

                        tb_dp.set_is_loading(false);
                        page_dp.set_title("Secure Connection Not Available");
                        let mut tab = tab_c_dp.borrow_mut();
                        tab.is_loading = false;
                        tab.title = "Secure Connection Not Available".to_string();
                        drop(tab);

                        ui_dp.stack.set_visible_child_name("https_error");
                        return true;
                    }

                    if https_mgr_dp.borrow().should_upgrade(uri_str, mode)
                        && let Some(upgraded) =
                            crate::privacy::https_only::upgrade_uri_to_https(uri_str)
                    {
                        https_mgr_dp.borrow_mut().record_upgrade_attempt(
                            tab_id,
                            uri_str,
                            &upgraded,
                            &host,
                        );
                        info!("HTTPS-Only upgrading (tab {tab_id}) {uri_str} -> {upgraded}");

                        // Ignore HTTP navigation first
                        decision.ignore();

                        // Schedule the HTTPS navigation asynchronously on the GTK main loop
                        // to avoid re-entrant navigation while the policy decision is pending!
                        let wv_clone = wv.clone();
                        glib::idle_add_local_once(move || {
                            wv_clone.load_uri(&upgraded);
                        });
                        return true;
                    }
                }
            }
            false
        });

        // 6. connect_load_failed (with HTTPS-Only Interstitial fallback & subresource isolation)
        let ui_fail = ui_item.clone();
        let tb_fail = self.toolbar.clone();
        let page_fail = page.clone();
        let tab_c_fail = tab_c.clone();
        let https_mgr_lf = self.https_manager.clone();
        let cfg_fail = self.config.clone();
        let nav_fail = self.clone();

        webview.connect_load_failed(move |wv, load_event, failing_uri, error| {
            crate::utils::net_diagnostics::record_load_failed(failing_uri, load_event, error);

            tb_fail.set_is_loading(false);
            tb_fail.address_bar().set_progress(0.0);
            nav_fail.load_line.set_visible(false);
            nav_fail.load_line.set_fraction(0.0);

            // Differentiate cancelled loads resulting from policy interventions (decision.ignore)
            // or Stop button clicks
            if crate::browser::webview::is_cancellation_error(error) {
                tracing::debug!(
                    "Ignoring cancelled load for {failing_uri} (event: {load_event:?})"
                );
                let mut tab = tab_c_fail.borrow_mut();
                tab.is_loading = false;
                tab.progress = 0.0;
                return true;
            }

            // Keep failing URI in address bar so user can edit it
            tb_fail.address_bar().set_text_if_unfocused(failing_uri);
            tb_fail.address_bar().set_security_status(
                "channel-insecure-symbolic",
                "Connection failed",
            );

            // Check if bare domain HTTPS upgrade failed
            let error_domain = error.domain().as_str();
            let error_code = error.code();
            let error_msg = error.message();
            let failure_kind = crate::browser::navigation::classify_network_failure_by_reason(
                error_domain,
                error_code,
                error_msg,
            );

            if let Some(fallback_http) = nav_fail
                .pending_bare_domain_fallbacks
                .borrow_mut()
                .remove(&tab_id)
            {
                // CRITICAL REQUIREMENT: Fall back only for connection-level failures (refused, timeout, no listener on 443).
                // NEVER fall back to HTTP after a TLS or certificate error!
                if crate::browser::navigation::is_eligible_for_http_fallback(failure_kind, false) {
                    let host = extract_host_from_uri(failing_uri).unwrap_or_else(|| failing_uri.to_string());
                    let is_private = crate::browser::navigation::is_localhost_or_private_ip(&host);

                    if is_private {
                        // Automatic http:// for localhost and private IP addresses
                        tb_fail.address_bar().set_text(&fallback_http);
                        tb_fail.address_bar().set_security_status(
                            "channel-insecure-symbolic",
                            "Not secure: Insecure connection (HTTP)",
                        );
                        wv.load_uri(&fallback_http);
                        return true;
                    } else {
                        // For public domains, do NOT fall back silently.
                        // Show inline bar under address bar: "This site does not support HTTPS. Open it over an insecure connection?"
                        // with Continue and Cancel buttons. Cancel keeps the failed https URL in the bar.
                        nav_fail.fallback_target.borrow_mut().replace((tab_id, fallback_http));
                        nav_fail.fallback_bar.set_visible(true);

                        tb_fail.set_is_loading(false);
                        page_fail.set_title("Connection Not Available");
                        let mut tab = tab_c_fail.borrow_mut();
                        tab.is_loading = false;
                        tab.title = "Connection Not Available".to_string();
                        drop(tab);

                        ui_fail
                            .error_label
                            .set_label(&format!("Could not connect via HTTPS to:\n{failing_uri}\n\n{error}"));
                        ui_fail.stack.set_visible_child_name("error");
                        return true;
                    }
                } else {
                    nav_fail.fallback_bar.set_visible(false);
                    nav_fail.fallback_target.borrow_mut().take();
                }
            }

            let tab_url = tab_c_fail.borrow().url.clone();
            let wv_uri = wv.uri().map(|u| u.to_string()).unwrap_or_default();

            // Check if this failure is for an active HTTPS upgrade attempt on this tab
            let is_upgrade_target = https_mgr_lf
                .borrow()
                .get_pending_upgrade(tab_id)
                .map(|p| p.upgraded_https_uri == failing_uri || p.original_http_uri == failing_uri)
                .unwrap_or(false);

            if is_upgrade_target
                && let Some(pending) = https_mgr_lf.borrow_mut().clear_upgrade_attempt(tab_id)
            {
                tb_fail.set_is_loading(false);
                page_fail.set_title("Secure Connection Not Available");
                let mut tab = tab_c_fail.borrow_mut();
                tab.is_loading = false;
                tab.title = "Secure Connection Not Available".to_string();
                drop(tab);

                let mode = cfg_fail.borrow().privacy.resolved_https_mode();
                ui_fail
                    .https_fallback_url
                    .borrow_mut()
                    .replace(pending.original_http_uri.clone());
                ui_fail.https_host_label.set_label(&pending.host);

                if mode == HttpsMode::Strict {
                    ui_fail.https_desc_label.set_label(
                        "HTTPS-Only Mode is set to Strict. Insecure HTTP connections are blocked and cannot be bypassed.",
                    );
                    ui_fail.https_continue_btn.set_visible(false);
                } else {
                    ui_fail.https_desc_label.set_label(
                        "HTTPS-Only Mode is enabled. The site you are attempting to visit does not support HTTPS.",
                    );
                    ui_fail.https_continue_btn.set_visible(true);
                }

                ui_fail.stack.set_visible_child_name("https_error");
                return true;
            }

            // Subresource isolation: If the failure is for an auxiliary resource or subframe
            // while the main page has already committed or navigated to a different URL, do NOT
            // tear down the main document and display a full-page error interstitial.
            let is_main_document_load = failing_uri == tab_url
                || failing_uri == wv_uri
                || wv_uri.is_empty()
                || tab_url.is_empty();

            if !is_main_document_load {
                tracing::warn!(
                    "Subresource load failed for {failing_uri} ({error}) at stage {load_event:?}; main document {tab_url} preserved"
                );
                return false;
            }

            tb_fail.set_is_loading(false);
            page_fail.set_title("Connection Error");
            let mut tab = tab_c_fail.borrow_mut();
            tab.is_loading = false;
            tab.title = "Connection Error".to_string();
            drop(tab);

            ui_fail
                .error_label
                .set_label(&format!("Could not connect to:\n{failing_uri}\n\n{error}"));
            ui_fail.stack.set_visible_child_name("error");
            true
        });

        // 7. connect_load_failed_with_tls_errors (Safe warning page, NO silent bypass!)
        let ui_tls = ui_item.clone();
        let page_tls = page.clone();
        let tab_c_tls = tab_c.clone();
        let tb_tls = self.toolbar.clone();
        let nav_tls = self.clone();

        webview.connect_load_failed_with_tls_errors(move |wv, failing_uri, cert, errors| {
            crate::utils::net_diagnostics::record_tls_failure(failing_uri, errors);

            // CRITICAL REQUIREMENT: NEVER fall back to HTTP after a TLS or certificate error
            // (expired, unknown CA, wrong host, revoked, and so on). Show certificate warning page only!
            nav_tls.pending_bare_domain_fallbacks.borrow_mut().remove(&tab_id);
            nav_tls.fallback_bar.set_visible(false);
            nav_tls.fallback_target.borrow_mut().take();

            tb_tls.set_is_loading(false);
            tb_tls.address_bar().set_text_if_unfocused(failing_uri);
            tb_tls.address_bar().set_security_status(
                "channel-insecure-symbolic",
                "Certificate error: Connection is not private",
            );

            let tab_url = tab_c_tls.borrow().url.clone();
            let wv_uri = wv.uri().map(|u| u.to_string()).unwrap_or_default();

            // If a subresource has TLS errors, let WebKit drop the subresource without
            // taking down the entire main document unless the main document itself failed.
            let is_main_document = failing_uri == tab_url
                || failing_uri == wv_uri
                || wv_uri.is_empty()
                || tab_url.is_empty();

            if !is_main_document {
                tracing::warn!("Subresource TLS failure for {failing_uri} ({errors:?}); dropped");
                return false;
            }

            tracing::warn!("Main document TLS certificate failure for: {failing_uri}");
            page_tls.set_title("Privacy Warning");
            let mut tab = tab_c_tls.borrow_mut();
            tab.is_loading = false;
            tab.title = "Privacy Warning".to_string();
            drop(tab);

            let host = extract_host_from_uri(failing_uri).unwrap_or_else(|| failing_uri.to_string());
            ui_tls.tls_host_label.set_label(&format!("Host: {host}"));

            let mut error_details = Vec::new();
            if errors.contains(gio::TlsCertificateFlags::UNKNOWN_CA) {
                error_details.push("Issuer authority is unknown or untrusted");
            }
            if errors.contains(gio::TlsCertificateFlags::BAD_IDENTITY) {
                error_details.push("Certificate hostname does not match destination");
            }
            if errors.contains(gio::TlsCertificateFlags::EXPIRED) {
                error_details.push("Certificate has expired");
            }
            if errors.contains(gio::TlsCertificateFlags::NOT_ACTIVATED) {
                error_details.push("Certificate is not yet active");
            }
            if errors.contains(gio::TlsCertificateFlags::REVOKED) {
                error_details.push("Certificate has been revoked");
            }
            if errors.contains(gio::TlsCertificateFlags::INSECURE) {
                error_details.push("Certificate uses an insecure signature algorithm");
            }
            if error_details.is_empty() {
                error_details.push("Certificate validation failure");
            }

            ui_tls.tls_detail_label.set_label(
                "Attackers might be trying to steal your information from this site.\n\
                 Shadhin Browser blocked this connection to protect your privacy.",
            );
            ui_tls.tls_error_flags_label.set_label(&format!(
                "Security Diagnostics: {}\nFlags: {:?}",
                error_details.join(", "),
                errors
            ));
            ui_tls.tls_advanced_revealer.set_reveal_child(false);
            ui_tls.active_tls_cert.borrow_mut().replace((
                cert.clone(),
                host,
                failing_uri.to_string(),
            ));

            ui_tls.stack.set_visible_child_name("tls");
            true
        });

        // 8. connect_permission_request (Camera, Mic, Geo, Notification, Clipboard)
        let ui_perm = ui_item.clone();
        let storage_perm = self.storage.clone();
        let wv_perm = webview.clone();
        let nav_perm = self.clone();
        webview.connect_permission_request(move |_, req| {
            let (ptype, _) = identify_permission_request(req);
            let domain = wv_perm
                .uri()
                .and_then(|u| extract_host_from_uri(u.as_str()))
                .unwrap_or_else(|| "This site".to_string());

            let ui_perm_async = ui_perm.clone();
            let req_async = req.clone();
            let domain_async = domain.clone();
            let storage_async = storage_perm.clone();
            let nav_async = nav_perm.clone();

            let domain_query = domain_async.clone();
            storage_perm.query_async(
                move |db| {
                    let repo = PermissionRepository::new(db.conn());
                    repo.get_permission(&domain_query, ptype).unwrap_or(None)
                },
                move |stored_state| match stored_state {
                    Some(PermissionState::Allow) => {
                        info!("Auto-allowing {ptype:?} for {domain} (saved preference)");
                        req_async.allow();
                    }
                    Some(PermissionState::Block) => {
                        info!("Auto-blocking {ptype:?} for {domain} (saved preference)");
                        req_async.deny();
                    }
                    None | Some(PermissionState::Ask) => {
                        let (icon_name, p_name) = match ptype {
                            crate::browser::permissions::PermissionType::Camera => {
                                ("camera-web-symbolic", "camera")
                            }
                            crate::browser::permissions::PermissionType::Microphone => {
                                ("audio-input-microphone-symbolic", "microphone")
                            }
                            crate::browser::permissions::PermissionType::CameraAndMicrophone => {
                                ("camera-web-symbolic", "camera and microphone")
                            }
                            crate::browser::permissions::PermissionType::Geolocation => {
                                ("find-location-symbolic", "location")
                            }
                            crate::browser::permissions::PermissionType::Notifications => {
                                ("preferences-system-notifications-symbolic", "notifications")
                            }
                            crate::browser::permissions::PermissionType::Clipboard => {
                                ("edit-paste-symbolic", "clipboard")
                            }
                            crate::browser::permissions::PermissionType::DeviceInfo => {
                                ("computer-symbolic", "device information")
                            }
                            crate::browser::permissions::PermissionType::Other => {
                                ("dialog-question-symbolic", "device features")
                            }
                        };

                        ui_perm_async.permission_icon.set_icon_name(Some(icon_name));
                        ui_perm_async
                            .permission_label
                            .set_label(&format!("{domain} wants to access your {p_name}"));
                        *ui_perm_async.active_permission_request.borrow_mut() =
                            Some(req_async.clone());

                        let heading = format!("Allow \"{domain}\" to access your {p_name}?");
                        let body = format!(
                            "\"{}\" is requesting permission to access your {}. The default action is Deny.",
                            domain, p_name
                        );

                        let dialog = adw::AlertDialog::builder()
                            .heading(&heading)
                            .body(&body)
                            .build();

                        let remember_check = gtk4::CheckButton::builder()
                            .label("Remember this decision for this site")
                            .active(true)
                            .halign(gtk4::Align::Center)
                            .margin_top(8)
                            .build();
                        dialog.set_extra_child(Some(&remember_check));

                        dialog.add_response("deny", "Deny");
                        dialog.add_response("allow", "Allow");

                        // The default action must be "Deny".
                        dialog.set_default_response(Some("deny"));
                        // Close response (e.g. Escape or dialog dismissal) must be "Deny".
                        dialog.set_close_response("deny");

                        dialog
                            .set_response_appearance("deny", adw::ResponseAppearance::Destructive);
                        dialog
                            .set_response_appearance("allow", adw::ResponseAppearance::Suggested);

                        let domain_resp = domain_async.clone();
                        let storage_resp = storage_async.clone();
                        let persistence_allowed = nav_async.persistence_allowed;
                        let req_resp = req_async;
                        let ui_resp = ui_perm_async.clone();

                        dialog.connect_response(None, move |_, response_id| {
                            *ui_resp.active_permission_request.borrow_mut() = None;
                            ui_resp.permission_revealer.set_reveal_child(false);
                            let remember = remember_check.is_active();
                            if response_id == "allow" {
                                info!(
                                    "Permission prompt: user ALLOWED {ptype:?} for {domain_resp} (remember={remember})"
                                );
                                if remember && persistence_allowed {
                                    let host_to_save = domain_resp.clone();
                                    storage_resp.dispatch(move |db| {
                                        let repo = PermissionRepository::new(db.conn());
                                        let _ = repo.set_permission(
                                            &host_to_save,
                                            ptype,
                                            PermissionState::Allow,
                                            persistence_allowed,
                                        );
                                    });
                                }
                                req_resp.allow();
                            } else {
                                info!(
                                    "Permission prompt: user DENIED {ptype:?} for {domain_resp} (remember={remember})"
                                );
                                if remember && persistence_allowed {
                                    let host_to_save = domain_resp.clone();
                                    storage_resp.dispatch(move |db| {
                                        let repo = PermissionRepository::new(db.conn());
                                        let _ = repo.set_permission(
                                            &host_to_save,
                                            ptype,
                                            PermissionState::Block,
                                            persistence_allowed,
                                        );
                                    });
                                }
                                req_resp.deny();
                            }
                        });

                        dialog.present(Some(&nav_async.window));
                    }
                },
            );
            true
        });

        // 9. connect_web_process_terminated
        let ui_crash = ui_item.clone();
        webview.connect_web_process_terminated(move |_, reason| {
            tracing::error!("Web process terminated: {reason:?}");
            ui_crash
                .error_label
                .set_label("This web page crashed unexpectedly.\nClick Try Again to reload.");
            ui_crash.stack.set_visible_child_name("error");
        });

        // 10. connect_create (Window creation & Popup blocking policy)
        let nav_create = self.clone();
        webview.connect_create(move |_, action| {
            if let Some(req) = action.request()
                && let Some(uri) = req.uri()
            {
                let uri_str = uri.as_str();
                info!("Page requested new window for {uri_str}; redirecting to new tab");
                nav_create.create_tab(Some(uri_str), Some(tab_idx));
            }
            None
        });

        // 11. connect_context_menu (Page, Link, Image contextual menus via hit_test)
        let nav_cm = self.clone();
        webview.connect_context_menu(move |view, menu, hit_test| {
            menu.remove_all();

            let is_link = hit_test.context_is_link();
            let is_image = hit_test.context_is_image();
            let link_uri = hit_test.link_uri();
            let image_uri = hit_test.image_uri();

            // Link Context Menu: Open Link in New Tab, Open Link in New Window, Copy Link Address
            if is_link && let Some(link) = link_uri {
                let link_str = link.to_string();

                let nav_tab = nav_cm.clone();
                let url_tab = link_str.clone();
                let act_tab = gtk4::gio::SimpleAction::new("ctx-open-link-tab", None);
                act_tab.connect_activate(move |_, _| {
                    nav_tab.create_tab(Some(&url_tab), None);
                });
                let item_tab = webkit6::ContextMenuItem::from_gaction(
                    &act_tab,
                    "Open Link in New Tab",
                    None,
                );
                menu.append(&item_tab);

                let nav_win = nav_cm.clone();
                let url_win = link_str.clone();
                let act_win = gtk4::gio::SimpleAction::new("ctx-open-link-win", None);
                act_win.connect_activate(move |_, _| {
                    if let Some(app) = nav_win
                        .window
                        .application()
                        .and_downcast::<adw::Application>()
                    {
                        let win = ShadhinWindow::new_with_urls(&app, std::slice::from_ref(&url_win));
                        win.present();
                    }
                });
                let item_win = webkit6::ContextMenuItem::from_gaction(
                    &act_win,
                    "Open Link in New Window",
                    None,
                );
                menu.append(&item_win);

                let url_copy = link_str;
                let act_copy = gtk4::gio::SimpleAction::new("ctx-copy-link", None);
                act_copy.connect_activate(move |_, _| {
                    if let Some(display) = gtk4::gdk::Display::default() {
                        display.clipboard().set_text(&url_copy);
                    }
                });
                let item_copy = webkit6::ContextMenuItem::from_gaction(
                    &act_copy,
                    "Copy Link Address",
                    None,
                );
                menu.append(&item_copy);
            }

            // Image Context Menu: Open Image in New Tab, Save Image As, Copy Image, Copy Image Address
            if is_image && let Some(img) = image_uri {
                if is_link {
                    menu.append(&webkit6::ContextMenuItem::new_separator());
                }

                let img_str = img.to_string();

                let nav_img = nav_cm.clone();
                let url_img = img_str.clone();
                let act_img_tab = gtk4::gio::SimpleAction::new("ctx-open-img-tab", None);
                act_img_tab.connect_activate(move |_, _| {
                    nav_img.create_tab(Some(&url_img), None);
                });
                let item_img_tab = webkit6::ContextMenuItem::from_gaction(
                    &act_img_tab,
                    "Open Image in New Tab",
                    None,
                );
                menu.append(&item_img_tab);

                let item_save_img = webkit6::ContextMenuItem::from_stock_action_with_label(
                    webkit6::ContextMenuAction::DownloadImageToDisk,
                    "Save Image As",
                );
                menu.append(&item_save_img);

                let item_copy_img = webkit6::ContextMenuItem::from_stock_action_with_label(
                    webkit6::ContextMenuAction::CopyImageToClipboard,
                    "Copy Image",
                );
                menu.append(&item_copy_img);

                let url_img_copy = img_str;
                let act_copy_img_url = gtk4::gio::SimpleAction::new("ctx-copy-img-addr", None);
                act_copy_img_url.connect_activate(move |_, _| {
                    if let Some(display) = gtk4::gdk::Display::default() {
                        display.clipboard().set_text(&url_img_copy);
                    }
                });
                let item_copy_addr = webkit6::ContextMenuItem::from_gaction(
                    &act_copy_img_url,
                    "Copy Image Address",
                    None,
                );
                menu.append(&item_copy_addr);
            }

            // Page Context Menu: Back, Forward, Reload, View Page Source
            // Only shown if neither a link nor an image is clicked!
            if !is_link && !is_image {
                let wv_b = view.clone();
                let act_back = gtk4::gio::SimpleAction::new("ctx-page-back", None);
                act_back.set_enabled(view.can_go_back());
                act_back.connect_activate(move |_, _| {
                    wv_b.go_back();
                });
                let item_back = webkit6::ContextMenuItem::from_gaction(&act_back, "Back", None);
                menu.append(&item_back);

                let wv_f = view.clone();
                let act_fwd = gtk4::gio::SimpleAction::new("ctx-page-fwd", None);
                act_fwd.set_enabled(view.can_go_forward());
                act_fwd.connect_activate(move |_, _| {
                    wv_f.go_forward();
                });
                let item_fwd = webkit6::ContextMenuItem::from_gaction(&act_fwd, "Forward", None);
                menu.append(&item_fwd);

                let wv_r = view.clone();
                let act_reload = gtk4::gio::SimpleAction::new("ctx-page-reload", None);
                act_reload.connect_activate(move |_, _| {
                    wv_r.reload();
                });
                let item_reload =
                    webkit6::ContextMenuItem::from_gaction(&act_reload, "Reload", None);
                menu.append(&item_reload);

                if let Some(uri) = view.uri() {
                    let nav_src = nav_cm.clone();
                    let uri_src = uri.to_string();
                    let act_source = gtk4::gio::SimpleAction::new("ctx-page-source", None);
                    act_source.connect_activate(move |_, _| {
                        nav_src.create_tab(Some(&format!("view-source:{uri_src}")), None);
                    });
                    let item_source = webkit6::ContextMenuItem::from_gaction(
                        &act_source,
                        "View Page Source",
                        None,
                    );
                    menu.append(&item_source);
                }
            }

            false
        });

        // 12. Fullscreen
        webview.connect_enter_fullscreen(|_| false);
        webview.connect_leave_fullscreen(|_| false);

        webview
    }

    pub fn execute_navigation(
        &self,
        tab_idx: usize,
        target: &NavigationTarget,
        page: &adw::TabPage,
    ) {
        match target {
            NavigationTarget::Url(raw_url) | NavigationTarget::Search(raw_url) => {
                let clean_url = crate::privacy::tracker::strip_tracking_parameters(raw_url);
                let tm = self.tab_manager.borrow();
                if let Some(tab_cell) = tm.get_tab(tab_idx) {
                    let tab_id = tab_cell.borrow().id;
                    self.https_manager
                        .borrow_mut()
                        .clear_upgrade_attempt(tab_id);

                    if let Some(bare_part) = clean_url.strip_prefix("https://") {
                        self.pending_bare_domain_fallbacks
                            .borrow_mut()
                            .insert(tab_id, format!("http://{bare_part}"));
                    }

                    let mut tab = tab_cell.borrow_mut();
                    tab.url = clean_url.clone();
                    tab.title = "Loading...".to_string();
                    tab.is_loading = true;
                    tab.has_active_webview = true;
                    page.set_title("Loading...");
                    self.toolbar.address_bar().set_text(&clean_url);
                    self.toolbar.set_is_loading(true);
                    self.toolbar
                        .address_bar()
                        .set_security_status("security-low-symbolic", "Connecting...");
                }
                drop(tm);

                let items = self.tab_items.borrow();
                if let Some(item) = items.get(tab_idx) {
                    let wv_opt = item.webview.borrow_mut();
                    let wv = if let Some(ref existing) = *wv_opt {
                        existing.clone()
                    } else {
                        drop(wv_opt);
                        self.setup_webview_for_tab(tab_idx, page)
                    };

                    if clean_url.starts_with("view-source:")
                        && let Some(settings) = webkit6::prelude::WebViewExt::settings(&wv)
                    {
                        settings.set_enable_developer_extras(true);
                    }

                    item.stack.set_visible_child_name("web");
                    info!("Loading URI on active tab {tab_idx}: {clean_url}");
                    wv.set_visible(true);
                    wv.load_uri(&clean_url);
                }

                self.sync_tabs();
            }
            NavigationTarget::Internal(page_id) => {
                let tm = self.tab_manager.borrow();
                if let Some(tab_cell) = tm.get_tab(tab_idx) {
                    let tab_id = tab_cell.borrow().id;
                    self.https_manager
                        .borrow_mut()
                        .clear_upgrade_attempt(tab_id);
                }
                drop(tm);

                let items = self.tab_items.borrow();
                let ui_item = items.get(tab_idx).cloned();
                drop(items);

                if page_id == "shadhin:bookmarks" || page_id == "about:bookmarks" {
                    let tm = self.tab_manager.borrow();
                    if let Some(tab_cell) = tm.get_tab(tab_idx) {
                        let mut tab = tab_cell.borrow_mut();
                        tab.url = "about:bookmarks".to_string();
                        tab.title = "Bookmarks".to_string();
                        tab.favicon = None;
                        tab.is_loading = false;
                        page.set_title("Bookmarks");
                        self.toolbar.address_bar().set_text("about:bookmarks");
                        self.toolbar
                            .address_bar()
                            .set_security_status("security-low-symbolic", "Internal application page");
                    }
                    drop(tm);
                    if let Some(item) = ui_item {
                        item.bookmarks_panel.refresh("");
                        item.stack.set_visible_child_name("bookmarks");
                    }
                    self.sync_tabs();
                } else if page_id == "shadhin:history" || page_id == "about:history" {
                    let tm = self.tab_manager.borrow();
                    if let Some(tab_cell) = tm.get_tab(tab_idx) {
                        let mut tab = tab_cell.borrow_mut();
                        tab.url = "about:history".to_string();
                        tab.title = "History".to_string();
                        tab.favicon = None;
                        tab.is_loading = false;
                        page.set_title("History");
                        self.toolbar.address_bar().set_text("about:history");
                        self.toolbar
                            .address_bar()
                            .set_security_status("security-low-symbolic", "Internal application page");
                    }
                    drop(tm);
                    if let Some(item) = ui_item {
                        item.history_panel.refresh("");
                        item.stack.set_visible_child_name("history");
                    }
                    self.sync_tabs();
                } else if page_id == "shadhin:downloads" || page_id == "about:downloads" {
                    self.toolbar.toggle_downloads_popover();
                    self.sync_tabs();
                } else if page_id == "shadhin:newtab"
                    || page_id == "about:newtab"
                    || page_id == "about:blank"
                {
                    let tm = self.tab_manager.borrow();
                    if let Some(tab_cell) = tm.get_tab(tab_idx) {
                        let mut tab = tab_cell.borrow_mut();
                        tab.url = "about:newtab".to_string();
                        tab.title = "New Tab".to_string();
                        tab.favicon = None;
                        tab.is_loading = false;
                        page.set_title("New Tab");
                        self.toolbar.address_bar().set_text("");
                        self.toolbar
                            .address_bar()
                            .set_security_status("security-low-symbolic", "New Tab");
                    }
                    drop(tm);

                    if let Some(item) = ui_item {
                        item.stack.set_visible_child_name("newtab");
                    }
                    self.sync_tabs();
                }
            }
            NavigationTarget::Blocked { scheme, reason } => {
                info!("Navigation blocked for scheme {scheme}: {reason}");
                let items = self.tab_items.borrow();
                if let Some(item) = items.get(tab_idx) {
                    item.error_label.set_label(&format!(
                        "Navigation Blocked by Security Policy\n\n\
                         Scheme '{scheme}:' is not permitted.\n\n\
                         {reason}"
                    ));
                    item.stack.set_visible_child_name("error");
                    page.set_title("Navigation Blocked");
                }
            }
        }
    }

    pub fn sync_tabs(&self) {
        let tm = self.tab_manager.borrow();
        let tabs_info: Vec<TabDisplayInfo> = tm
            .tabs()
            .iter()
            .map(|t| {
                let b = t.borrow();
                TabDisplayInfo {
                    title: b.title.clone(),
                    favicon: b.favicon.clone(),
                    is_newtab: b.is_newtab(),
                    is_discarded: b.is_discarded,
                    is_pinned: b.pinned,
                }
            })
            .collect();

        self.tab_strip.refresh(&tabs_info);

        let sel_idx = tm.selected_index();
        let tv_clone = self.tab_view.clone();
        self.sidebar
            .update_open_tabs(&tabs_info, sel_idx as i32, move |idx| {
                if idx >= 0 && idx < tv_clone.n_pages() {
                    let page = tv_clone.nth_page(idx);
                    tv_clone.set_selected_page(&page);
                }
            });

        if let Some(active_tab_cell) = tm.active_tab() {
            let active = active_tab_cell.borrow();
            if active.is_newtab() {
                self.toolbar.address_bar().set_text_if_unfocused("");
                self.toolbar
                    .address_bar()
                    .set_security_status("security-low-symbolic", "New Tab");
                self.toolbar.set_can_go_back(false);
                self.toolbar.set_can_go_forward(false);
                self.toolbar.set_is_loading(false);
                self.toolbar.address_bar().set_progress(0.0);
            } else {
                self.toolbar
                    .address_bar()
                    .set_text_if_unfocused(&active.url);
                self.toolbar
                    .address_bar()
                    .update_current_page(&active.url, &active.title);
                let items = self.tab_items.borrow();
                if let Some(ui_item) = items.get(sel_idx)
                    && let Some(ref wv) = *ui_item.webview.borrow()
                {
                    update_security_indicator_from_webview(&self.toolbar, wv, &active.url);
                } else if active.url.starts_with("http://") {
                    self.toolbar
                        .address_bar()
                        .set_security_status("channel-insecure-symbolic", "Not secure: Insecure connection (HTTP)");
                } else if active.url.starts_with("about:") || active.url.starts_with("shadhin:") {
                    self.toolbar
                        .address_bar()
                        .set_security_status("security-low-symbolic", "Internal application page");
                }
                self.toolbar.set_can_go_back(active.can_go_back);
                self.toolbar.set_can_go_forward(active.can_go_forward);
                self.toolbar.set_is_loading(active.is_loading);
                self.toolbar.address_bar().set_progress(active.progress);
                if let Some(host) = extract_host_from_uri(&active.url) {
                    let shield_enabled = self.config.borrow().privacy.shield_enabled;
                    self.toolbar
                        .privacy_popover()
                        .update_for_site(Some(&host), shield_enabled);
                } else {
                    self.toolbar.privacy_popover().update_for_site(None, true);
                }
            }

            let items = self.tab_items.borrow();
            if let Some(ui_item) = items.get(sel_idx)
                && let Some(ref wv) = *ui_item.webview.borrow()
            {
                self.find_bar.attach_webview(wv);
            }
        }
    }
}

impl ShadhinWindow {
    /// Synchronizes tab strip and sidebar with current tab manager state
    pub fn sync_tabs(&self) {
        self.nav_ctx.sync_tabs();
    }

    /// Sets up cross-component UI signals
    fn setup_signals(&self) {
        // 1. Sidebar Toggle Button in Toolbar
        let win_toggle = self.clone();
        self.toolbar.connect_sidebar_toggle(move || {
            let current_is_vert = win_toggle.config.borrow().tab_layout == "vertical";
            let next_layout = if current_is_vert { "top" } else { "vertical" };
            win_toggle.set_tab_layout(next_layout);
        });

        // Bookmarks Bar item click navigation
        let nav_bb = self.nav_ctx.clone();
        let tv_bb = self.tab_view.clone();
        let cfg_bb = self.config.clone();

        self.bookmarks_bar.connect_navigate(move |url| {
            if let Some(page) = tv_bb.selected_page() {
                let idx = tv_bb.page_position(&page) as usize;
                let target = classify_input(url, &cfg_bb.borrow().search_engine_url);
                nav_bb.execute_navigation(idx, &target, &page);
            }
        });

        // 2. New Tab Button in Titlebar Tab Strip
        let nav_new = self.nav_ctx.clone();
        self.tab_strip.connect_new_tab(move |opt_url| {
            info!("New Tab requested (URL: {opt_url:?})");
            nav_new.create_tab(opt_url.as_deref(), None);
        });

        // 3. Tab Context Menu Actions (Reload, Duplicate, Pin/Unpin, Close)
        let nav_reload = self.clone();
        let handle_reload = move |idx: i32| {
            if idx >= 0 {
                let items = nav_reload.tab_items.borrow();
                if let Some(item) = items.get(idx as usize)
                    && let Some(ref wv) = *item.webview.borrow()
                {
                    wv.reload();
                }
            }
        };

        let nav_dup = self.clone();
        let handle_duplicate = move |idx: i32| {
            if idx >= 0 {
                info!("Duplicating tab {idx}");
                let tm = nav_dup.tab_manager.borrow();
                if let Some(tab_cell) = tm.get_tab(idx as usize) {
                    let (url, parent_id) = {
                        let b = tab_cell.borrow();
                        (b.url.clone(), Some(b.id))
                    };
                    drop(tm);
                    nav_dup.create_tab(Some(&url), parent_id);
                }
            }
        };

        let nav_pin = self.clone();
        let handle_pin = move |idx: i32| {
            if idx >= 0 && idx < nav_pin.tab_view.n_pages() {
                let page = nav_pin.tab_view.nth_page(idx);
                let new_pinned = !page.is_pinned();
                nav_pin.tab_view.set_page_pinned(&page, new_pinned);
                let tm = nav_pin.tab_manager.borrow();
                if let Some(tab_cell) = tm.get_tab(idx as usize) {
                    tab_cell.borrow_mut().pinned = new_pinned;
                }
                drop(tm);
                nav_pin.sync_tabs();
            }
        };

        let nav_close = self.clone();
        let handle_close = move |idx: i32| {
            if idx >= 0 && idx < nav_close.tab_view.n_pages() {
                let page = nav_close.tab_view.nth_page(idx);
                nav_close.tab_view.close_page(&page);
            }
        };

        self.tab_strip.connect_reload_tab(handle_reload.clone());
        self.tab_strip.connect_duplicate_tab(handle_duplicate.clone());
        self.tab_strip.connect_pin_tab(handle_pin.clone());

        self.sidebar.connect_tab_reload(handle_reload);
        self.sidebar.connect_tab_duplicate(handle_duplicate);
        self.sidebar.connect_tab_pin(handle_pin);
        self.sidebar.connect_tab_close(handle_close);

        // Tab View native menu model (for horizontal tab bar right click)
        let tab_menu_model = gio::Menu::new();
        tab_menu_model.append(Some("Reload Tab"), Some("win.tab-reload"));
        tab_menu_model.append(Some("Duplicate Tab"), Some("win.tab-duplicate"));
        tab_menu_model.append(Some("Pin / Unpin Tab"), Some("win.tab-pin"));
        tab_menu_model.append(Some("Close Tab"), Some("win.tab-close"));
        self.tab_view.set_menu_model(Some(&tab_menu_model));

        let selected_tab_ctx = Rc::new(RefCell::new(None::<adw::TabPage>));
        let sel_tab_setup = selected_tab_ctx.clone();
        self.tab_view.connect_setup_menu(move |_, page_opt| {
            *sel_tab_setup.borrow_mut() = page_opt.cloned();
        });

        let nav_tr = self.clone();
        let sel_tr = selected_tab_ctx.clone();
        let act_tr = gio::SimpleAction::new("tab-reload", None);
        act_tr.connect_activate(move |_, _| {
            let page_opt = sel_tr.borrow().clone();
            if let Some(ref page) = page_opt {
                let idx = nav_tr.tab_view.page_position(page);
                if idx >= 0 {
                    let items = nav_tr.tab_items.borrow();
                    if let Some(item) = items.get(idx as usize)
                        && let Some(ref wv) = *item.webview.borrow()
                    {
                        wv.reload();
                    }
                }
            }
        });
        self.window.add_action(&act_tr);

        let nav_td = self.clone();
        let sel_td = selected_tab_ctx.clone();
        let act_td = gio::SimpleAction::new("tab-duplicate", None);
        act_td.connect_activate(move |_, _| {
            let page_opt = sel_td.borrow().clone();
            if let Some(ref page) = page_opt {
                let idx = nav_td.tab_view.page_position(page);
                if idx >= 0 {
                    let tm = nav_td.tab_manager.borrow();
                    if let Some(tab_cell) = tm.get_tab(idx as usize) {
                        let (url, parent_id) = {
                            let b = tab_cell.borrow();
                            (b.url.clone(), Some(b.id))
                        };
                        drop(tm);
                        nav_td.create_tab(Some(&url), parent_id);
                    }
                }
            }
        });
        self.window.add_action(&act_td);

        let nav_tp = self.clone();
        let sel_tp = selected_tab_ctx.clone();
        let act_tp = gio::SimpleAction::new("tab-pin", None);
        act_tp.connect_activate(move |_, _| {
            let page_opt = sel_tp.borrow().clone();
            if let Some(ref page) = page_opt {
                let new_pinned = !page.is_pinned();
                nav_tp.tab_view.set_page_pinned(page, new_pinned);
                let idx = nav_tp.tab_view.page_position(page);
                if idx >= 0 {
                    let tm = nav_tp.tab_manager.borrow();
                    if let Some(tab_cell) = tm.get_tab(idx as usize) {
                        tab_cell.borrow_mut().pinned = new_pinned;
                    }
                }
                nav_tp.sync_tabs();
            }
        });
        self.window.add_action(&act_tp);

        let nav_tc = self.clone();
        let sel_tc = selected_tab_ctx;
        let act_tc = gio::SimpleAction::new("tab-close", None);
        act_tc.connect_activate(move |_, _| {
            let page_opt = sel_tc.borrow().clone();
            if let Some(ref page) = page_opt {
                nav_tc.tab_view.close_page(page);
            }
        });
        self.window.add_action(&act_tc);

        // 4. Tab Context Menu Actions: Close Other Tabs
        let tm_co = self.tab_manager.clone();
        let tv_co = self.tab_view.clone();
        let ts_co = self.tab_strip.clone();
        let sb_co = self.sidebar.clone();
        let tb_co = self.toolbar.clone();

        self.tab_strip.connect_close_other_tabs(move |idx| {
            if idx >= 0 {
                info!("Closing all tabs except tab {idx}");
                let mut tm = tm_co.borrow_mut();
                tm.close_other_tabs(idx as usize);
                drop(tm);
                while tv_co.n_pages() > 1 {
                    let page = tv_co.nth_page(0);
                    tv_co.close_page_finish(&page, true);
                }
                Self::sync_tabs_external(&tm_co, &ts_co, &sb_co, &tv_co, &tb_co);
            }
        });

        // 5. Tab View Page Changed notification
        let tm_change = self.tab_manager.clone();
        let tb_change = self.toolbar.clone();
        let ts_change = self.tab_strip.clone();
        let sb_change = self.sidebar.clone();
        let tv_change = self.tab_view.clone();
        let fb_change = self.find_bar.clone();
        let items_change = self.tab_items.clone();
        let nav_change = self.nav_ctx.clone();
        let fb_bar_change = self.fallback_bar.clone();
        let fb_target_change = self.fallback_target.clone();

        self.tab_view.connect_selected_page_notify(move |tv| {
            if let Some(page) = tv.selected_page() {
                let idx = tv.page_position(&page) as usize;
                let mut tm = tm_change.borrow_mut();
                tm.switch_tab(idx);

                // Low-RAM tab discarding check: discard idle background tabs if > threshold
                let discarded =
                    tm.discard_idle_tabs(TAB_DISCARD_THRESHOLD, Duration::from_secs(300));
                for d_idx in discarded {
                    let items = items_change.borrow();
                    if let Some(item) = items.get(d_idx) {
                        let old_wv = item.webview.borrow_mut().take();
                        if let Some(ref wv) = old_wv {
                            item.stack.remove(wv);
                            info!("Removed active WebView for discarded tab {d_idx} to free WebProcess RAM");
                        }
                    }
                }
                drop(tm);

                // Check if activated tab was discarded and needs WebView reconstruction
                let (needs_restore, target_url) = {
                    let items = items_change.borrow();
                    let needs = items
                        .get(idx)
                        .map(|item| item.webview.borrow().is_none())
                        .unwrap_or(false);
                    let tm = tm_change.borrow();
                    let url = tm
                        .get_tab(idx)
                        .map(|t| t.borrow().url.clone())
                        .unwrap_or_default();
                    (needs, url)
                };

                if needs_restore
                    && (target_url.starts_with("http://") || target_url.starts_with("https://"))
                {
                    info!("Restoring discarded tab {idx} and reloading URL: {target_url}");
                    let webview = nav_change.setup_webview_for_tab(idx, &page);
                    webview.load_uri(&target_url);
                }

                Self::sync_tabs_external(
                    &tm_change, &ts_change, &sb_change, &tv_change, &tb_change,
                );

                // Sync HTTP fallback bar visibility for this tab
                if let Some(target) = fb_target_change.borrow().as_ref() {
                    let is_active_tab = target.0 == idx;
                    fb_bar_change.set_visible(is_active_tab);
                } else {
                    fb_bar_change.set_visible(false);
                }

                // Attach FindBar to newly selected tab webview
                let items = items_change.borrow();
                if let Some(item) = items.get(idx)
                    && let Some(ref wv) = *item.webview.borrow()
                {
                    fb_change.attach_webview(wv);
                    if let Some(tab_c) = tm_change.borrow().get_tab(idx) {
                        let b = tab_c.borrow();
                        update_security_indicator_from_webview(&tb_change, wv, &b.url);
                        tb_change.address_bar().update_current_page(&b.url, &b.title);
                    }
                }
            }
        });

        // 6. Tab Close Page handler
        let tm_close = self.tab_manager.clone();
        let items_close = self.tab_items.clone();
        let ts_close = self.tab_strip.clone();
        let sb_close = self.sidebar.clone();
        let tb_close = self.toolbar.clone();
        let tv_close = self.tab_view.clone();
        let https_mgr_close = self.https_manager.clone();

        self.tab_view.connect_close_page(move |tv, page| {
            let idx = tv.page_position(page) as usize;
            info!("Closing tab at index {idx}");
            let tab_id = tm_close.borrow().get_tab(idx).map(|t| t.borrow().id);
            if let Some(id) = tab_id {
                https_mgr_close.borrow_mut().clear_upgrade_attempt(id);
            }

            let mut tm = tm_close.borrow_mut();
            let _ = tm.close_tab(idx);
            drop(tm);

            let mut items = items_close.borrow_mut();
            if idx < items.len() {
                items.remove(idx);
            }
            drop(items);

            tv.close_page_finish(page, true);
            Self::sync_tabs_external(&tm_close, &ts_close, &sb_close, &tv_close, &tb_close);
            glib::Propagation::Stop
        });

        // 7. Address Bar Submission
        let nav_addr = self.nav_ctx.clone();
        let tv_addr = self.tab_view.clone();
        let cfg_addr = self.config.clone();
        let last_sub = Rc::new(RefCell::new((
            String::new(),
            std::time::Instant::now() - std::time::Duration::from_secs(10),
        )));

        self.toolbar.address_bar().connect_activate(move |input, new_tab| {
            let now = std::time::Instant::now();
            let mut last = last_sub.borrow_mut();
            if last.0 == input && now.duration_since(last.1) < std::time::Duration::from_millis(300) {
                tracing::info!("Ignoring duplicate address bar activation within 300ms for '{input}'");
                return;
            }
            *last = (input.to_string(), now);
            drop(last);

            info!("Address bar submission received: input='{input}', new_tab={new_tab}");
            if new_tab || tv_addr.n_pages() == 0 {
                nav_addr.create_tab(Some(input), None);
            } else {
                let page = tv_addr.selected_page().unwrap_or_else(|| tv_addr.nth_page(0));
                let idx = tv_addr.page_position(&page) as usize;
                let is_dev = std::env::var("SHADHIN_DEV").is_ok()
                    || std::env::var("SHADHIN_DEV_CSS").is_ok();
                let settings = crate::browser::navigation::ClassificationSettings {
                    search_template: cfg_addr.borrow().search_engine_url.clone(),
                    allow_file: true,
                    developer_extras: is_dev,
                };
                let target = classify_input(input, &settings);
                info!("Classified input '{input}' as target: {target:?}, navigating tab {idx}");
                nav_addr.execute_navigation(idx, &target, &page);
            }
        });

        // 8. Toolbar Navigation: Back, Forward, Reload / Stop
        let items_nav = self.tab_items.clone();
        let tv_nav = self.tab_view.clone();

        let items_b = items_nav.clone();
        let tv_b = tv_nav.clone();
        self.toolbar.connect_back(move || {
            if let Some(page) = tv_b.selected_page() {
                let idx = tv_b.page_position(&page) as usize;
                let items = items_b.borrow();
                if let Some(item) = items.get(idx)
                    && let Some(ref wv) = *item.webview.borrow()
                {
                    wv.go_back();
                }
            }
        });

        let items_f = items_nav.clone();
        let tv_f = tv_nav.clone();
        self.toolbar.connect_forward(move || {
            if let Some(page) = tv_f.selected_page() {
                let idx = tv_f.page_position(&page) as usize;
                let items = items_f.borrow();
                if let Some(item) = items.get(idx)
                    && let Some(ref wv) = *item.webview.borrow()
                {
                    wv.go_forward();
                }
            }
        });

        let items_r = items_nav.clone();
        let tv_r = tv_nav.clone();
        let tb_r = self.toolbar.clone();
        let ll_r = self.load_line.clone();
        self.toolbar.connect_reload(move |is_loading| {
            if let Some(page) = tv_r.selected_page() {
                let idx = tv_r.page_position(&page) as usize;
                let items = items_r.borrow();
                if let Some(item) = items.get(idx)
                    && let Some(ref wv) = *item.webview.borrow()
                {
                    if is_loading {
                        wv.stop_loading();
                        tb_r.set_is_loading(false);
                        ll_r.set_visible(false);
                        ll_r.set_fraction(0.0);
                    } else {
                        wv.reload();
                    }
                }
            }
        });

        // 9. Sidebar Navigation Items
        let tm_sb = self.tab_manager.clone();
        let items_sb = self.tab_items.clone();
        let tv_sb = self.tab_view.clone();
        let tb_sb = self.toolbar.clone();
        let win_sb = self.window.clone();
        let cfg_sb = self.config.clone();
        let storage_sb = self.storage.clone();
        let session_sb = self.nav_ctx.session.clone();
        let win_clone_sb = self.clone();

        self.sidebar.connect_page_selected(move |page_id| {
            info!("Sidebar item chosen: {page_id}");
            match page_id {
                "home" => {
                    if let Some(page) = tv_sb.selected_page() {
                        let idx = tv_sb.page_position(&page) as usize;
                        let tm = tm_sb.borrow();
                        if let Some(tab_cell) = tm.get_tab(idx) {
                            let mut tab = tab_cell.borrow_mut();
                            tab.url = "about:newtab".to_string();
                            tab.title = "New Tab".to_string();
                            tab.favicon = None;
                        }
                        page.set_title("New Tab");
                        tb_sb.address_bar().set_text("");
                        let items = items_sb.borrow();
                        if let Some(item) = items.get(idx) {
                            item.stack.set_visible_child_name("newtab");
                        }
                    }
                }
                "bookmarks" => {
                    if let Some(page) = tv_sb.selected_page() {
                        let idx = tv_sb.page_position(&page) as usize;
                        let tm = tm_sb.borrow();
                        if let Some(tab_cell) = tm.get_tab(idx) {
                            let mut tab = tab_cell.borrow_mut();
                            tab.url = "about:bookmarks".to_string();
                            tab.title = "Bookmarks".to_string();
                            tab.favicon = None;
                        }
                        page.set_title("Bookmarks");
                        tb_sb.address_bar().set_text("about:bookmarks");
                        let items = items_sb.borrow();
                        if let Some(item) = items.get(idx) {
                            item.bookmarks_panel.refresh("");
                            item.stack.set_visible_child_name("bookmarks");
                        }
                    }
                }
                "history" => {
                    if let Some(page) = tv_sb.selected_page() {
                        let idx = tv_sb.page_position(&page) as usize;
                        let tm = tm_sb.borrow();
                        if let Some(tab_cell) = tm.get_tab(idx) {
                            let mut tab = tab_cell.borrow_mut();
                            tab.url = "about:history".to_string();
                            tab.title = "History".to_string();
                            tab.favicon = None;
                        }
                        page.set_title("History");
                        tb_sb.address_bar().set_text("about:history");
                        let items = items_sb.borrow();
                        if let Some(item) = items.get(idx) {
                            item.history_panel.refresh("");
                            item.stack.set_visible_child_name("history");
                        }
                    }
                }
                "downloads" => {
                    tb_sb.toggle_downloads_popover();
                }
                "settings" => {
                    let win_c = win_clone_sb.clone();
                    SettingsWindow::present(
                        Some(&win_sb),
                        cfg_sb.clone(),
                        storage_sb.clone(),
                        Some(session_sb.borrow().clone()),
                        Some(move |layout: &str| {
                            win_c.set_tab_layout(layout);
                        }),
                    );
                }
                _ => {}
            }
        });

        // 10. Workspace Switcher
        let tm_ws = self.tab_manager.clone();
        let cur_ws = self.current_workspace.clone();
        let session_ws = self.session_mgr.clone();
        let active_sess_ws = self.active_network_session.clone();
        let dl_mgr_ws = self.download_mgr.clone();
        let persistence_allowed_ws = self.persistence_allowed;

        self.sidebar.set_on_workspace_selected(move |target_ws| {
            let prev_ws = cur_ws.borrow().clone();
            if prev_ws == target_ws {
                return;
            }

            // Save open tabs of current workspace
            let tabs: Vec<(String, String, bool)> = tm_ws
                .borrow()
                .tabs()
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let b = t.borrow();
                    (
                        b.url.clone(),
                        b.title.clone(),
                        i == tm_ws.borrow().selected_index(),
                    )
                })
                .collect();

            session_ws
                .borrow()
                .save_workspace_tabs(&prev_ws, tabs, persistence_allowed_ws);
            *cur_ws.borrow_mut() = target_ws.to_string();

            if persistence_allowed_ws {
                let new_session = session_ws
                    .borrow_mut()
                    .get_or_create_workspace_session(target_ws);
                *active_sess_ws.borrow_mut() = new_session.clone();
                dl_mgr_ws.attach_session(&new_session, true);
                apply_cookie_policy(
                    &new_session,
                    crate::privacy::cookies::CookiePolicy::BlockThirdParty,
                );
                apply_itp(&new_session, true);
            }

            info!("Switched to workspace '{target_ws}'");
        });

        // 11. Create New Workspace handler
        let sb_new_ws = self.sidebar.clone();
        let storage_new_ws = self.storage.clone();
        let sm_new_ws = self.session_mgr.clone();
        let win_new_ws = self.window.clone();

        self.sidebar.set_on_create_workspace(move || {
            let entry = gtk4::Entry::builder()
                .placeholder_text("Workspace name (e.g. Research)")
                .build();

            let dialog = adw::AlertDialog::builder()
                .heading("New Workspace")
                .body("Enter a name for the new isolated workspace:")
                .build();

            dialog.set_extra_child(Some(&entry));
            dialog.add_response("cancel", "Cancel");
            dialog.add_response("create", "Create");
            dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);

            let st = storage_new_ws.clone();
            let sb = sb_new_ws.clone();
            let sm_click = sm_new_ws.clone();

            dialog.choose(Some(&win_new_ws), None::<&gio::Cancellable>, move |resp| {
                if resp == "create" {
                    let name = entry.text().trim().to_string();
                    if !name.is_empty() {
                        let ws_id = name.to_lowercase().replace(' ', "-");
                        let n = name.clone();
                        let id_clone = ws_id.clone();
                        st.dispatch(move |db| {
                            let repo = WorkspaceRepository::new(db.conn());
                            let _ = repo.create(&id_clone, &n, crate::ui::theme::DEFAULT_ACCENT);
                        });
                        sm_click
                            .borrow_mut()
                            .get_or_create_workspace_session(&ws_id);
                        sb.add_workspace_item(&ws_id, &name);
                        info!("Created new workspace: '{name}' (id: '{ws_id}')");
                    }
                }
            });
        });

        // 12. Clean Shutdown on Close Request
        let cur_ws_close = self.current_workspace.clone();
        let tm_close = self.tab_manager.clone();
        let sm_close = self.session_mgr.clone();
        let storage_close = self.storage.clone();

        self.window.connect_close_request(move |_| {
            let ws = cur_ws_close.borrow().clone();
            let tabs: Vec<(String, String, bool)> = tm_close
                .borrow()
                .tabs()
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let b = t.borrow();
                    (
                        b.url.clone(),
                        b.title.clone(),
                        i == tm_close.borrow().selected_index(),
                    )
                })
                .collect();
            sm_close.borrow().save_workspace_tabs(&ws, tabs, true);
            sm_close.borrow().on_clean_shutdown();
            storage_close.optimize();
            gtk4::glib::Propagation::Proceed
        });

        // 13. Privacy Shield Popover Callbacks
        let app_pv = self.window.application().and_downcast::<adw::Application>();
        self.toolbar
            .privacy_popover()
            .set_on_private_window(move || {
                if let Some(ref app) = app_pv {
                    ShadhinWindow::new_private(app).present();
                }
            });

        let win_cd = self.window.clone();
        let storage_cd = self.storage.clone();
        let session_cd = self.nav_ctx.session.clone();
        let perm_cd = self.persistence_allowed;
        self.toolbar.privacy_popover().set_on_clear_data(move || {
            crate::ui::clear_data::show_clear_browsing_data_dialog(
                &win_cd,
                storage_cd.clone(),
                session_cd.borrow().clone(),
                perm_cd,
            );
        });

        let win_st = self.window.clone();
        let cfg_st = self.config.clone();
        let storage_st = self.storage.clone();
        let session_st = self.nav_ctx.session.clone();
        let win_clone_st = self.clone();
        self.toolbar
            .privacy_popover()
            .set_on_open_settings(move || {
                let win_c = win_clone_st.clone();
                SettingsWindow::present(
                    Some(&win_st),
                    cfg_st.clone(),
                    storage_st.clone(),
                    Some(session_st.borrow().clone()),
                    Some(move |layout: &str| {
                        win_c.set_tab_layout(layout);
                    }),
                );
            });

        let nav_ts = self.nav_ctx.clone();
        self.toolbar
            .privacy_popover()
            .set_on_toggle_site(move |domain, enabled| {
                let d = domain.to_string();
                let perm = nav_ts.persistence_allowed;
                nav_ts.storage.dispatch(move |db| {
                    let repo = SiteExceptionRepository::new(db.conn());
                    let _ = repo.set_exception(&d, enabled, enabled, enabled, perm);
                });
            });

        let nav_tm = self.nav_ctx.clone();
        self.toolbar
            .privacy_popover()
            .set_on_toggle_master(move |active| {
                let tv = &nav_tm.tab_view;
                if let Some(page) = tv.selected_page() {
                    let idx = tv.page_position(&page) as usize;
                    let items = nav_tm.tab_items.borrow();
                    if let Some(item) = items.get(idx)
                        && let Some(ref wv) = *item.webview.borrow()
                        && let Some(ucm) = wv.user_content_manager()
                    {
                        let cfg = nav_tm.config.borrow();
                        nav_tm.filter_manager.apply_filters(
                            &ucm,
                            active,
                            cfg.privacy.block_ads,
                            cfg.privacy.block_trackers,
                        );
                    }
                }
            });
    }

    /// Sets up complete keyboard shortcuts using GTK4 event controllers
    fn setup_shortcuts(&self) {
        let key_controller = gtk4::EventControllerKey::new();

        let tb_key = self.toolbar.clone();
        let tv_key = self.tab_view.clone();
        let ts_key = self.tab_strip.clone();
        let tm_key = self.tab_manager.clone();
        let items_key = self.tab_items.clone();
        let fb_key = self.find_bar.clone();
        let win_self = self.window.clone();
        let win_self_clone = self.clone();
        let cfg_key = self.config.clone();
        let storage_key = self.storage.clone();
        let bb_key = self.bookmarks_bar.clone();
        let nav_key = self.nav_ctx.clone();
        let session_key = self.nav_ctx.session.clone();
        let perm_key = self.persistence_allowed;

        key_controller.connect_key_pressed(move |_, key, _, modifier| {
            let ctrl = modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            let alt = modifier.contains(gtk4::gdk::ModifierType::ALT_MASK);
            let shift = modifier.contains(gtk4::gdk::ModifierType::SHIFT_MASK);

            // Esc: Close Find in Page
            if key == Key::Escape && fb_key.is_visible() {
                fb_key.close();
                return glib::Propagation::Stop;
            }

            // Ctrl+F: Find in Page
            if ctrl && key == Key::f {
                fb_key.show_and_focus();
                return glib::Propagation::Stop;
            }

            // Ctrl+T: New Tab
            if ctrl && !shift && key == Key::t {
                info!("Shortcut: Ctrl+T New Tab");
                ts_key.trigger_new_tab(None);
                return glib::Propagation::Stop;
            }

            // Ctrl+W: Close active tab
            if ctrl && !shift && key == Key::w {
                if let Some(page) = tv_key.selected_page() {
                    tv_key.close_page(&page);
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+Shift+T: Reopen closed tab
            if ctrl && shift && key == Key::T {
                info!("Shortcut: Ctrl+Shift+T Reopen Closed Tab");
                let mut tm = tm_key.borrow_mut();
                if let Some(info) = tm.pop_closed_tab() {
                    drop(tm);
                    info!("Reopening closed tab: {}", info.url);
                    ts_key.trigger_new_tab(Some(info.url));
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+L, Alt+D, or F6: Focus address bar and select all
            if (ctrl && key == Key::l)
                || (alt && (key == Key::d || key == Key::D))
                || key == Key::F6
            {
                tb_key.address_bar().focus();
                return glib::Propagation::Stop;
            }

            // Ctrl+Shift+R: Hard reload (bypass cache)
            if ctrl && shift && key == Key::R {
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    let items = items_key.borrow();
                    if let Some(item) = items.get(idx)
                        && let Some(ref wv) = *item.webview.borrow()
                    {
                        wv.reload_bypass_cache();
                        info!("Hard reload initiated for active tab");
                    }
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+R or F5: Reload active tab
            if (ctrl && key == Key::r) || key == Key::F5 {
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    let items = items_key.borrow();
                    if let Some(item) = items.get(idx)
                        && let Some(ref wv) = *item.webview.borrow()
                    {
                        wv.reload();
                    }
                }
                return glib::Propagation::Stop;
            }

            // Alt+Left: Back
            if alt && key == Key::Left {
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    let items = items_key.borrow();
                    if let Some(item) = items.get(idx)
                        && let Some(ref wv) = *item.webview.borrow()
                    {
                        wv.go_back();
                    }
                }
                return glib::Propagation::Stop;
            }

            // Alt+Right: Forward
            if alt && key == Key::Right {
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    let items = items_key.borrow();
                    if let Some(item) = items.get(idx)
                        && let Some(ref wv) = *item.webview.borrow()
                    {
                        wv.go_forward();
                    }
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+Tab: Next tab
            if ctrl && !shift && key == Key::Tab {
                let n = tv_key.n_pages();
                if n > 1
                    && let Some(page) = tv_key.selected_page()
                {
                    let cur = tv_key.page_position(&page);
                    let next = (cur + 1) % n;
                    let next_page = tv_key.nth_page(next);
                    tv_key.set_selected_page(&next_page);
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+Shift+Tab: Previous tab
            if ctrl && shift && key == Key::ISO_Left_Tab {
                let n = tv_key.n_pages();
                if n > 1
                    && let Some(page) = tv_key.selected_page()
                {
                    let cur = tv_key.page_position(&page);
                    let prev = if cur == 0 { n - 1 } else { cur - 1 };
                    let prev_page = tv_key.nth_page(prev);
                    tv_key.set_selected_page(&prev_page);
                }
                return glib::Propagation::Stop;
            }

            // Alt+1..9: Jump to Tab 1 to 9
            if alt {
                let digit = match key {
                    Key::_1 => Some(0),
                    Key::_2 => Some(1),
                    Key::_3 => Some(2),
                    Key::_4 => Some(3),
                    Key::_5 => Some(4),
                    Key::_6 => Some(5),
                    Key::_7 => Some(6),
                    Key::_8 => Some(7),
                    Key::_9 => Some(8),
                    _ => None,
                };
                if let Some(target_idx) = digit
                    && target_idx < tv_key.n_pages()
                {
                    let page = tv_key.nth_page(target_idx);
                    tv_key.set_selected_page(&page);
                    return glib::Propagation::Stop;
                }
            }

            // Ctrl+Plus / Ctrl+=: Zoom in
            if ctrl && (key == Key::plus || key == Key::equal) {
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    let items = items_key.borrow();
                    if let Some(item) = items.get(idx)
                        && let Some(ref wv) = *item.webview.borrow()
                    {
                        let new_zoom = (wv.zoom_level() + 0.1).min(5.0);
                        wv.set_zoom_level(new_zoom);
                        info!("Zoom level increased to: {new_zoom:.1}");
                    }
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+Minus: Zoom out
            if ctrl && key == Key::minus {
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    let items = items_key.borrow();
                    if let Some(item) = items.get(idx)
                        && let Some(ref wv) = *item.webview.borrow()
                    {
                        let new_zoom = (wv.zoom_level() - 0.1).max(0.25);
                        wv.set_zoom_level(new_zoom);
                        info!("Zoom level decreased to: {new_zoom:.1}");
                    }
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+0: Reset zoom
            if ctrl && key == Key::_0 {
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    let items = items_key.borrow();
                    if let Some(item) = items.get(idx)
                        && let Some(ref wv) = *item.webview.borrow()
                    {
                        wv.set_zoom_level(1.0);
                        info!("Zoom level reset to 1.0");
                    }
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+Shift+B: Toggle Bookmarks Bar
            if ctrl && shift && key == Key::B {
                bb_key.toggle_visibility();
                return glib::Propagation::Stop;
            }

            // F9 or Ctrl+B: Toggle Sidebar / Tab Layout
            if key == Key::F9 || (ctrl && !shift && key == Key::b) {
                let current_is_vert = win_self_clone.config.borrow().tab_layout == "vertical";
                let next_layout = if current_is_vert { "top" } else { "vertical" };
                win_self_clone.set_tab_layout(next_layout);
                return glib::Propagation::Stop;
            }

            // Ctrl+,: Preferences / Settings Window
            if ctrl && key == Key::comma {
                let win_c = win_self_clone.clone();
                SettingsWindow::present(
                    Some(&win_self),
                    cfg_key.clone(),
                    storage_key.clone(),
                    Some(session_key.borrow().clone()),
                    Some(move |layout: &str| {
                        win_c.set_tab_layout(layout);
                    }),
                );
                return glib::Propagation::Stop;
            }

            // Ctrl+Shift+N: New Private Window
            if ctrl && shift && key == Key::N {
                if let Some(app) = win_self.application().and_downcast::<adw::Application>() {
                    let priv_win = ShadhinWindow::new_private(&app);
                    priv_win.present();
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+Shift+Delete / KP_Delete: Clear Browsing Data Dialog
            if ctrl && shift && (key == Key::Delete || key == Key::KP_Delete) {
                crate::ui::clear_data::show_clear_browsing_data_dialog(
                    &win_self,
                    storage_key.clone(),
                    session_key.borrow().clone(),
                    perm_key,
                );
                return glib::Propagation::Stop;
            }

            // Ctrl+D: Bookmark active tab
            if ctrl && !shift && key == Key::d {
                tb_key.address_bar().trigger_bookmark();
                return glib::Propagation::Stop;
            }

            // Alt+Home: Navigate to homepage
            if alt && key == Key::Home {
                let home_url = cfg_key.borrow().homepage.clone();
                let target = classify_input(&home_url, &cfg_key.borrow().search_engine_url);
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    nav_key.execute_navigation(idx, &target, &page);
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+Shift+O: Open Bookmarks panel
            if ctrl && shift && key == Key::O {
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    let items = items_key.borrow();
                    if let Some(item) = items.get(idx) {
                        item.bookmarks_panel.refresh("");
                        item.stack.set_visible_child_name("bookmarks");
                        tb_key.address_bar().set_text("about:bookmarks");
                        tb_key.address_bar().set_security_state(true);
                    }
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+H: Open History panel
            if ctrl && !shift && key == Key::h {
                if let Some(page) = tv_key.selected_page() {
                    let idx = tv_key.page_position(&page) as usize;
                    let items = items_key.borrow();
                    if let Some(item) = items.get(idx) {
                        item.history_panel.refresh("");
                        item.stack.set_visible_child_name("history");
                        tb_key.address_bar().set_text("about:history");
                        tb_key.address_bar().set_security_state(true);
                    }
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+J: Toggle Downloads popover
            if ctrl && !shift && key == Key::j {
                tb_key.toggle_downloads_popover();
                return glib::Propagation::Stop;
            }

            // F11: Fullscreen toggle
            if key == Key::F11 {
                if win_self.is_fullscreen() {
                    win_self.unfullscreen();
                } else {
                    win_self.fullscreen();
                }
                return glib::Propagation::Stop;
            }

            // Ctrl+Shift+N: Private Window stub
            if ctrl && shift && key == Key::N {
                let toast = adw::Toast::new(
                    "Private browsing with isolated ephemeral storage arrives in Phase 5",
                );
                toast.set_timeout(3);
                info!("Ctrl+Shift+N pressed (DEV-ONLY: Phase 5)");
                return glib::Propagation::Stop;
            }

            glib::Propagation::Proceed
        });

        self.window.add_controller(key_controller);
    }

    /// Sets up window-level GActions and populates the Application Menu
    fn setup_actions(&self) {
        // 1. New Tab (win.new-tab)
        let ts_action = self.tab_strip.clone();
        let act_new_tab = gio::SimpleAction::new("new-tab", None);
        act_new_tab.connect_activate(move |_, _| {
            ts_action.trigger_new_tab(None);
        });
        self.window.add_action(&act_new_tab);

        // 2. Reopen Closed Tab (win.reopen-tab)
        let tm_action = self.tab_manager.clone();
        let ts_action = self.tab_strip.clone();
        let act_reopen = gio::SimpleAction::new("reopen-tab", None);
        act_reopen.connect_activate(move |_, _| {
            let mut tm = tm_action.borrow_mut();
            if let Some(info) = tm.pop_closed_tab() {
                drop(tm);
                ts_action.trigger_new_tab(Some(info.url));
            }
        });
        self.window.add_action(&act_reopen);

        // 3. Bookmarks Manager (win.bookmarks)
        let tv_action = self.tab_view.clone();
        let items_action = self.tab_items.clone();
        let tb_action = self.toolbar.clone();
        let act_bookmarks = gio::SimpleAction::new("bookmarks", None);
        act_bookmarks.connect_activate(move |_, _| {
            if let Some(page) = tv_action.selected_page() {
                let idx = tv_action.page_position(&page) as usize;
                let items = items_action.borrow();
                if let Some(item) = items.get(idx) {
                    item.bookmarks_panel.refresh("");
                    item.stack.set_visible_child_name("bookmarks");
                    tb_action.address_bar().set_text("about:bookmarks");
                    tb_action.address_bar().set_security_state(true);
                }
            }
        });
        self.window.add_action(&act_bookmarks);

        // 4. Browsing History (win.history)
        let tv_action = self.tab_view.clone();
        let items_action = self.tab_items.clone();
        let tb_action = self.toolbar.clone();
        let act_history = gio::SimpleAction::new("history", None);
        act_history.connect_activate(move |_, _| {
            if let Some(page) = tv_action.selected_page() {
                let idx = tv_action.page_position(&page) as usize;
                let items = items_action.borrow();
                if let Some(item) = items.get(idx) {
                    item.history_panel.refresh("");
                    item.stack.set_visible_child_name("history");
                    tb_action.address_bar().set_text("about:history");
                    tb_action.address_bar().set_security_state(true);
                }
            }
        });
        self.window.add_action(&act_history);

        // 5. Downloads (win.downloads)
        let tb_action = self.toolbar.clone();
        let act_downloads = gio::SimpleAction::new("downloads", None);
        act_downloads.connect_activate(move |_, _| {
            tb_action.toggle_downloads_popover();
        });
        self.window.add_action(&act_downloads);

        // 6. Zoom In (win.zoom-in)
        let tv_action = self.tab_view.clone();
        let items_action = self.tab_items.clone();
        let act_zoom_in = gio::SimpleAction::new("zoom-in", None);
        act_zoom_in.connect_activate(move |_, _| {
            if let Some(page) = tv_action.selected_page() {
                let idx = tv_action.page_position(&page) as usize;
                let items = items_action.borrow();
                if let Some(item) = items.get(idx)
                    && let Some(ref wv) = *item.webview.borrow()
                {
                    let new_zoom = (wv.zoom_level() + 0.1).min(5.0);
                    wv.set_zoom_level(new_zoom);
                }
            }
        });
        self.window.add_action(&act_zoom_in);

        // 7. Zoom Out (win.zoom-out)
        let tv_action = self.tab_view.clone();
        let items_action = self.tab_items.clone();
        let act_zoom_out = gio::SimpleAction::new("zoom-out", None);
        act_zoom_out.connect_activate(move |_, _| {
            if let Some(page) = tv_action.selected_page() {
                let idx = tv_action.page_position(&page) as usize;
                let items = items_action.borrow();
                if let Some(item) = items.get(idx)
                    && let Some(ref wv) = *item.webview.borrow()
                {
                    let new_zoom = (wv.zoom_level() - 0.1).max(0.25);
                    wv.set_zoom_level(new_zoom);
                }
            }
        });
        self.window.add_action(&act_zoom_out);

        // 8. Normal Zoom (win.zoom-reset)
        let tv_action = self.tab_view.clone();
        let items_action = self.tab_items.clone();
        let act_zoom_reset = gio::SimpleAction::new("zoom-reset", None);
        act_zoom_reset.connect_activate(move |_, _| {
            if let Some(page) = tv_action.selected_page() {
                let idx = tv_action.page_position(&page) as usize;
                let items = items_action.borrow();
                if let Some(item) = items.get(idx)
                    && let Some(ref wv) = *item.webview.borrow()
                {
                    wv.set_zoom_level(1.0);
                }
            }
        });
        self.window.add_action(&act_zoom_reset);

        // 9. Find in Page (win.find-in-page)
        let fb_action = self.find_bar.clone();
        let act_find = gio::SimpleAction::new("find-in-page", None);
        act_find.connect_activate(move |_, _| {
            fb_action.show_and_focus();
        });
        self.window.add_action(&act_find);

        // 10. Clear Browsing Data (win.clear-browsing-data)
        let win_action = self.window.clone();
        let storage_action = self.storage.clone();
        let session_action = self.active_network_session.clone();
        let perm_action = self.persistence_allowed;
        let act_clear = gio::SimpleAction::new("clear-browsing-data", None);
        act_clear.connect_activate(move |_, _| {
            crate::ui::clear_data::show_clear_browsing_data_dialog(
                &win_action,
                storage_action.clone(),
                session_action.borrow().clone(),
                perm_action,
            );
        });
        self.window.add_action(&act_clear);

        // 11. Preferences / Settings (win.settings)
        let win_action = self.window.clone();
        let cfg_action = self.config.clone();
        let storage_action = self.storage.clone();
        let session_action = self.active_network_session.clone();
        let win_clone_act = self.clone();
        let act_settings = gio::SimpleAction::new("settings", None);
        act_settings.connect_activate(move |_, _| {
            let win_c = win_clone_act.clone();
            SettingsWindow::present(
                Some(&win_action),
                cfg_action.clone(),
                storage_action.clone(),
                Some(session_action.borrow().clone()),
                Some(move |layout: &str| {
                    win_c.set_tab_layout(layout);
                }),
            );
        });
        self.window.add_action(&act_settings);

        // 12. About Shadhin Browser (win.about)
        let win_action = self.window.clone();
        let act_about = gio::SimpleAction::new("about", None);
        act_about.connect_activate(move |_, _| {
            Self::show_about_dialog(&win_action);
        });
        self.window.add_action(&act_about);

        // 13. Privacy Shield (win.privacy)
        let tb_priv_action = self.toolbar.clone();
        let act_priv = gio::SimpleAction::new("privacy", None);
        act_priv.connect_activate(move |_, _| {
            tb_priv_action.privacy_popover().toggle();
        });
        self.window.add_action(&act_priv);

        // Build and set PopoverMenu model on the main menu button
        let menu = Self::create_app_menu();
        self.toolbar.set_menu_model(&menu);
    }

    /// Constructs the structured Gio Menu model for the Main Menu popover
    fn create_app_menu() -> gio::Menu {
        let menu = gio::Menu::new();

        // Section 1: Windows & Tabs
        let section_tabs = gio::Menu::new();
        section_tabs.append(Some("New Tab"), Some("win.new-tab"));
        section_tabs.append(Some("New Window"), Some("app.new-window"));
        section_tabs.append(Some("New Private Window"), Some("app.new-private-window"));
        section_tabs.append(Some("Reopen Closed Tab"), Some("win.reopen-tab"));
        menu.append_section(None, &section_tabs);

        // Section 2: Browser Views
        let section_views = gio::Menu::new();
        section_views.append(Some("Bookmarks"), Some("win.bookmarks"));
        section_views.append(Some("History"), Some("win.history"));
        section_views.append(Some("Downloads"), Some("win.downloads"));
        menu.append_section(None, &section_views);

        // Section 3: Page & Zoom
        let section_page = gio::Menu::new();
        let zoom_menu = gio::Menu::new();
        zoom_menu.append(Some("Zoom In"), Some("win.zoom-in"));
        zoom_menu.append(Some("Zoom Out"), Some("win.zoom-out"));
        zoom_menu.append(Some("Reset Zoom"), Some("win.zoom-reset"));
        section_page.append_submenu(Some("Zoom"), &zoom_menu);
        section_page.append(Some("Find in Page…"), Some("win.find-in-page"));
        menu.append_section(None, &section_page);

        // Section 4: Tools & Settings
        let section_tools = gio::Menu::new();
        section_tools.append(Some("Privacy Shield"), Some("win.privacy"));
        section_tools.append(
            Some("Clear Browsing Data…"),
            Some("win.clear-browsing-data"),
        );
        section_tools.append(Some("Preferences"), Some("win.settings"));
        section_tools.append(Some("About Shadhin Browser"), Some("win.about"));
        menu.append_section(None, &section_tools);

        menu
    }

    /// Displays the Libadwaita About dialog
    fn show_about_dialog(parent: &adw::ApplicationWindow) {
        let about = adw::AboutDialog::builder()
            .application_name(crate::app::APP_NAME)
            .version(crate::app::APP_VERSION)
            .comments("Fast · Private · Open — Native Linux web browser")
            .website("https://github.com/pollol258/shadhin-browser")
            .issue_url("https://github.com/pollol258/shadhin-browser/issues")
            .developer_name("Shadhin Browser Contributors")
            .application_icon(crate::app::APP_ID)
            .license_type(gtk4::License::Gpl30)
            .build();
        about.present(Some(parent));
    }

    /// Presents the window on the display
    pub fn present(&self) {
        self.window.present();
    }

    /// Accessor to the underlying Libadwaita ApplicationWindow
    pub fn window(&self) -> &adw::ApplicationWindow {
        &self.window
    }

    /// Accessor to the toolbar
    pub fn toolbar(&self) -> &ShadhinToolbar {
        &self.toolbar
    }

    /// Accessor to the tab strip
    pub fn tab_strip(&self) -> &ShadhinTabStrip {
        &self.tab_strip
    }

    /// Accessor to the top tab bar
    pub fn tab_bar(&self) -> &adw::TabBar {
        &self.tab_bar
    }

    /// Accessor to the load progress line
    pub fn load_line(&self) -> &gtk4::ProgressBar {
        &self.load_line
    }

    /// Accessor to the inline HTTP fallback bar
    pub fn fallback_bar(&self) -> &gtk4::Box {
        &self.fallback_bar
    }

    /// Accessor to the sandbox status banner
    pub fn sandbox_banner(&self) -> &adw::Banner {
        &self.sandbox_banner
    }

    /// Applies tab layout: "top" (top tab strip) vs "vertical" (collapsible sidebar)
    pub fn set_tab_layout(&self, layout: &str) {
        let is_vert = layout == "vertical";
        self.tab_bar.set_visible(!is_vert);
        self.split_view.set_show_sidebar(is_vert);
        self.toolbar.update_sidebar_active(is_vert);
        self.config.borrow_mut().tab_layout = layout.to_string();
        let _ = self.config.borrow().save();
    }

    /// Accessor to the find bar
    pub fn find_bar(&self) -> &FindBar {
        &self.find_bar
    }

    /// Accessor to the underlying AdwTabView
    pub fn tab_view(&self) -> &adw::TabView {
        &self.tab_view
    }
}
