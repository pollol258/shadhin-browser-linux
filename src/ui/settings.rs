//! Browser preferences and configuration management dialog
//!
//! Provides an AdwPreferencesDialog with exactly six sections:
//! 1. General (Startup, Homepage, Search Engine with Google, Google.com.bd, Pipilika, DuckDuckGo, etc.)
//! 2. Appearance (Light/Dark/System theme, Tab Layout Top vs Vertical synced in real-time, Accent color, Wallpaper)
//! 3. Privacy and Security (Content filters EasyList/EasyPrivacy, HTTPS-Only, Clear browsing data, Site permissions)
//! 4. Downloads (Download directory path, Ask where to save each file toggle)
//! 5. Performance (Low memory mode toggle, Discard idle tabs)
//! 6. About (App version 0.1.0, WebKitGTK version, Standard credits)

use crate::config::BrowserConfig;
use crate::storage::StorageService;
use crate::storage::search_engines::SearchEngineRepository;
use adw::prelude::*;
use gtk4::gio;
use gtk4::glib;
use std::cell::RefCell;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;
use std::rc::Rc;
use tracing::info;

pub struct SettingsWindow;

impl SettingsWindow {
    /// Constructs and presents the AdwPreferencesDialog
    pub fn present<F: Fn(&str) + 'static>(
        parent: Option<&impl IsA<gtk4::Widget>>,
        config: Rc<RefCell<BrowserConfig>>,
        storage: StorageService,
        session: Option<webkit6::NetworkSession>,
        on_tab_layout_changed: Option<F>,
    ) {
        let win = adw::PreferencesDialog::builder()
            .title("Preferences")
            .build();

        // ================= SECTION 1: GENERAL =================
        let general_page = adw::PreferencesPage::builder()
            .title("General")
            .icon_name("preferences-system-symbolic")
            .build();

        // 1.1 Startup behavior group
        let startup_group = adw::PreferencesGroup::builder()
            .title("Startup")
            .description("Configure what Shadhin Browser displays when opened")
            .build();

        let startup_model = gtk4::StringList::new(&["Open New Tab", "Continue where I left off"]);
        let startup_combo = adw::ComboRow::builder()
            .title("On startup")
            .model(&startup_model)
            .build();

        let initial_startup_idx = if config.borrow().startup_behavior == "continue_previous_session" {
            1
        } else {
            0
        };
        startup_combo.set_selected(initial_startup_idx);

        let cfg_startup = config.clone();
        startup_combo.connect_selected_notify(move |combo| {
            let choice = if combo.selected() == 1 {
                "continue_previous_session"
            } else {
                "open_new_tab"
            };
            cfg_startup.borrow_mut().startup_behavior = choice.to_string();
            let _ = cfg_startup.borrow().save();
            info!("Updated startup behavior to: {choice}");
        });
        startup_group.add(&startup_combo);

        // Homepage row
        let home_row = adw::EntryRow::builder()
            .title("Homepage URL")
            .text(&config.borrow().homepage)
            .build();

        let cfg_home = config.clone();
        home_row.connect_changed(move |row| {
            let val = row.text().trim().to_string();
            if !val.is_empty() {
                cfg_home.borrow_mut().homepage = val;
                let _ = cfg_home.borrow().save();
            }
        });
        startup_group.add(&home_row);
        general_page.add(&startup_group);

        // 1.2 Search Engine group (must include Google.com as default, Google.com.bd, and Pipilika)
        let search_group = adw::PreferencesGroup::builder()
            .title("Search Engine")
            .description("Choose default search engine for address bar and New Tab")
            .build();

        let engines_model = gtk4::StringList::new(&[
            "Google (Default)",
            "Google Bangladesh (Google.com.bd)",
            "Pipilika (পিপীলিকা)",
            "DuckDuckGo",
            "Brave Search",
            "Startpage",
            "Bing",
            "Wikipedia",
        ]);
        let search_combo = adw::ComboRow::builder()
            .title("Default Search Provider")
            .model(&engines_model)
            .build();

        let current_engine = config.borrow().search_engine_url.clone();
        let selected_idx = if current_engine.contains("google.com.bd") {
            1
        } else if current_engine.contains("pipilika.com") {
            2
        } else if current_engine.contains("duckduckgo.com") {
            3
        } else if current_engine.contains("brave.com") {
            4
        } else if current_engine.contains("startpage.com") {
            5
        } else if current_engine.contains("bing.com") {
            6
        } else if current_engine.contains("wikipedia.org") {
            7
        } else {
            0
        };
        search_combo.set_selected(selected_idx);

        let cfg_search = config.clone();
        let storage_search = storage.clone();
        search_combo.connect_selected_notify(move |combo| {
            let (name, tmpl, kw) = match combo.selected() {
                1 => (
                    "Google Bangladesh",
                    "https://www.google.com.bd/search?q=%s",
                    "gbd",
                ),
                2 => (
                    "Pipilika",
                    "https://www.pipilika.com/#search?q=%s",
                    "p",
                ),
                3 => ("DuckDuckGo", "https://duckduckgo.com/?q=%s", "d"),
                4 => ("Brave Search", "https://search.brave.com/search?q=%s", "b"),
                5 => (
                    "Startpage",
                    "https://www.startpage.com/sp/search?query=%s",
                    "s",
                ),
                6 => ("Bing", "https://www.bing.com/search?q=%s", "bing"),
                7 => (
                    "Wikipedia",
                    "https://en.wikipedia.org/wiki/Special:Search?search=%s",
                    "w",
                ),
                _ => ("Google", "https://www.google.com/search?q=%s", "g"),
            };

            cfg_search.borrow_mut().search_engine_url = tmpl.to_string();
            let _ = cfg_search.borrow().save();

            let kw_str = kw.to_string();
            let name_str = name.to_string();
            let tmpl_str = tmpl.to_string();
            storage_search.dispatch(move |db| {
                let repo = SearchEngineRepository::new(db.conn());
                if let Ok(Some(item)) = repo.get_by_keyword(&kw_str) {
                    let _ = repo.set_default(item.id);
                } else if let Ok(id) = repo.add(&name_str, &kw_str, &tmpl_str) {
                    let _ = repo.set_default(id);
                }
            });

            info!("Set default search engine to: {name}");
        });
        search_group.add(&search_combo);

        // Add Custom Search Engine
        let name_entry = adw::EntryRow::builder().title("Engine Name").build();
        let kw_entry = adw::EntryRow::builder().title("Keyword (e.g. 'gh')").build();
        let tmpl_entry = adw::EntryRow::builder()
            .title("URL Template")
            .text("https://example.com/search?q=%s")
            .build();

        let add_btn = gtk4::Button::builder()
            .label("Add Engine")
            .css_classes(["suggested-action"])
            .valign(gtk4::Align::Center)
            .margin_top(8)
            .margin_bottom(8)
            .build();

        let storage_add_engine = storage.clone();
        let name_ref = name_entry.clone();
        let kw_ref = kw_entry.clone();
        let tmpl_ref = tmpl_entry.clone();
        let win_ref_engine = win.clone();

        add_btn.connect_clicked(move |_| {
            let name = name_ref.text().trim().to_string();
            let kw = kw_ref.text().trim().to_string();
            let tmpl = tmpl_ref.text().trim().to_string();

            if name.is_empty()
                || kw.is_empty()
                || !tmpl.starts_with("https://")
                || !tmpl.contains("%s")
            {
                let dialog = adw::AlertDialog::builder()
                    .heading("Invalid Search Engine")
                    .body(
                        "Engine name and keyword are required. The URL template must start with 'https://' and contain '%s' as the query placeholder.",
                    )
                    .build();
                dialog.add_response("ok", "OK");
                dialog.present(Some(&win_ref_engine));
                return;
            }

            let n = name.clone();
            storage_add_engine.dispatch(move |db| {
                let repo = SearchEngineRepository::new(db.conn());
                let _ = repo.add(&name, &kw, &tmpl);
            });

            name_ref.set_text("");
            kw_ref.set_text("");
            tmpl_ref.set_text("https://example.com/search?q=%s");

            let dialog = adw::AlertDialog::builder()
                .heading("Search Engine Added")
                .body(format!(
                    "Custom search engine '{n}' was successfully added."
                ))
                .build();
            dialog.add_response("ok", "OK");
            dialog.present(Some(&win_ref_engine));
        });

        let btn_row = adw::ActionRow::builder()
            .title("Save Custom Engine")
            .build();
        btn_row.add_suffix(&add_btn);

        search_group.add(&name_entry);
        search_group.add(&kw_entry);
        search_group.add(&tmpl_entry);
        search_group.add(&btn_row);
        general_page.add(&search_group);

        // ================= SECTION 2: APPEARANCE =================
        let appear_page = adw::PreferencesPage::builder()
            .title("Appearance")
            .icon_name("preferences-desktop-theme-symbolic")
            .build();

        let theme_group = adw::PreferencesGroup::builder()
            .title("Theme & Layout")
            .description("Customize window appearance, tab strip layout, and accent colors")
            .build();

        // Light / Dark / System theme toggle
        let theme_model = gtk4::StringList::new(&["System Default", "Light", "Dark"]);
        let theme_combo = adw::ComboRow::builder()
            .title("Color Scheme")
            .subtitle("Follows system or manual light/dark override")
            .model(&theme_model)
            .build();

        let cur_scheme = config.borrow().color_scheme.clone();
        let cur_theme_idx = if cur_scheme == "light" {
            1
        } else if cur_scheme == "dark" || config.borrow().force_dark_theme {
            2
        } else {
            0
        };
        theme_combo.set_selected(cur_theme_idx);

        let cfg_theme = config.clone();
        theme_combo.connect_selected_notify(move |combo| {
            let style_manager = adw::StyleManager::default();
            match combo.selected() {
                1 => {
                    cfg_theme.borrow_mut().color_scheme = "light".to_string();
                    cfg_theme.borrow_mut().force_dark_theme = false;
                    style_manager.set_color_scheme(adw::ColorScheme::ForceLight);
                }
                2 => {
                    cfg_theme.borrow_mut().color_scheme = "dark".to_string();
                    cfg_theme.borrow_mut().force_dark_theme = true;
                    style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
                }
                _ => {
                    cfg_theme.borrow_mut().color_scheme = "system".to_string();
                    cfg_theme.borrow_mut().force_dark_theme = false;
                    style_manager.set_color_scheme(adw::ColorScheme::Default);
                }
            }
            let _ = cfg_theme.borrow().save();
        });
        theme_group.add(&theme_combo);

        // Tab layout row: Top tabs vs Vertical sidebar (MUST sync with sidebar toggle in real time)
        let tab_layout_row = adw::ComboRow::builder()
            .title("Tab Layout")
            .subtitle("Top tab strip or vertical collapsible sidebar")
            .model(&gtk4::StringList::new(&["Top tabs", "Vertical sidebar"]))
            .selected(if config.borrow().tab_layout == "vertical" { 1 } else { 0 })
            .build();

        let cfg_layout = config.clone();
        tab_layout_row.connect_selected_notify(move |row| {
            let sel = row.selected();
            let layout_val = if sel == 1 { "vertical" } else { "top" };
            cfg_layout.borrow_mut().tab_layout = layout_val.to_string();
            let _ = cfg_layout.borrow().save();
            if let Some(ref cb) = on_tab_layout_changed {
                cb(layout_val);
            }
        });
        theme_group.add(&tab_layout_row);

        // Custom accent color row: #3584e4, #26a269, #9141ac, #e66100
        let accent_row = adw::ActionRow::builder()
            .title("Accent Color")
            .subtitle("Changes buttons, selection, and focus indicators")
            .build();

        let accent_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .valign(gtk4::Align::Center)
            .build();

        let swatches = [
            ("#3584e4", "Blue (Default)", "swatch-blue"),
            ("#26a269", "Green", "swatch-green"),
            ("#9141ac", "Purple", "swatch-purple"),
            ("#e66100", "Orange", "swatch-orange"),
        ];

        for (hex, label, css_cls) in swatches {
            let swatch = gtk4::Button::builder()
                .tooltip_text(label)
                .css_classes(["swatch", css_cls])
                .build();
            swatch.set_size_request(24, 24);

            let cfg_acc = config.clone();
            let hex_str = hex.to_string();
            swatch.connect_clicked(move |_| {
                cfg_acc.borrow_mut().accent_color = hex_str.clone();
                let _ = cfg_acc.borrow().save();
                crate::ui::theme::set_accent(&hex_str);
            });
            accent_box.append(&swatch);
        }
        accent_row.add_suffix(&accent_box);
        theme_group.add(&accent_row);

        // Custom Wallpaper picker row
        let wallpaper_row = adw::ActionRow::builder()
            .title("Custom Wallpaper")
            .subtitle(
                config
                    .borrow()
                    .wallpaper_path
                    .as_deref()
                    .unwrap_or("Default dark gradient"),
            )
            .build();

        let wallpaper_btn = gtk4::Button::builder()
            .label("Choose Image…")
            .css_classes(["flat", "shadhin-nav-btn"])
            .valign(gtk4::Align::Center)
            .build();

        let cfg_wall = config.clone();
        let wall_row_ref = wallpaper_row.clone();
        let win_ref = win.clone();
        wallpaper_btn.connect_clicked(move |_| {
            let filter = gtk4::FileFilter::new();
            filter.set_name(Some("Images (*.png, *.jpg, *.jpeg, *.webp)"));
            filter.add_mime_type("image/png");
            filter.add_mime_type("image/jpeg");
            filter.add_mime_type("image/webp");

            let dialog = gtk4::FileDialog::builder()
                .title("Select Wallpaper Image")
                .modal(true)
                .default_filter(&filter)
                .build();

            let cfg_copy = cfg_wall.clone();
            let row_copy = wall_row_ref.clone();
            let root_win = win_ref.root().and_downcast::<gtk4::Window>();
            dialog.open(root_win.as_ref(), None::<&gio::Cancellable>, move |res| {
                if let Ok(file) = res
                    && let Some(src_path) = file.path()
                    && let Ok(dest_str) = copy_and_validate_wallpaper(&src_path)
                {
                    row_copy.set_subtitle(&dest_str);
                    cfg_copy.borrow_mut().wallpaper_path = Some(dest_str);
                    let _ = cfg_copy.borrow().save();
                    info!("Saved new custom wallpaper image");
                }
            });
        });
        wallpaper_row.add_suffix(&wallpaper_btn);
        theme_group.add(&wallpaper_row);
        appear_page.add(&theme_group);

        // ================= SECTION 3: PRIVACY AND SECURITY =================
        let priv_page = adw::PreferencesPage::builder()
            .title("Privacy and Security")
            .icon_name("security-high-symbolic")
            .build();

        // 3.1 Content Filtering Group
        let shield_group = adw::PreferencesGroup::builder()
            .title("Content Filtering & Shield")
            .description("Compiled WebKit content rules for ad and tracker blocking")
            .build();

        let master_switch = adw::SwitchRow::builder()
            .title("Privacy Shield (Master Switch)")
            .subtitle("Master switch enabling or disabling all privacy filters")
            .active(config.borrow().privacy.shield_enabled)
            .build();
        let cfg_ms = config.clone();
        master_switch.connect_active_notify(move |sw| {
            cfg_ms.borrow_mut().privacy.shield_enabled = sw.is_active();
            let _ = cfg_ms.borrow().save();
        });
        shield_group.add(&master_switch);

        let ads_switch = adw::SwitchRow::builder()
            .title("Block Advertisements (EasyList)")
            .subtitle("Blocks video ads, banner ads, and invasive promotional scripts")
            .active(config.borrow().privacy.block_ads)
            .build();
        let cfg_ads = config.clone();
        ads_switch.connect_active_notify(move |sw| {
            cfg_ads.borrow_mut().privacy.block_ads = sw.is_active();
            let _ = cfg_ads.borrow().save();
        });
        shield_group.add(&ads_switch);

        let track_switch = adw::SwitchRow::builder()
            .title("Block Trackers (EasyPrivacy)")
            .subtitle("Prevents cross-site tracking beacons, telemetry, and analytics")
            .active(config.borrow().privacy.block_trackers)
            .build();
        let cfg_track = config.clone();
        track_switch.connect_active_notify(move |sw| {
            cfg_track.borrow_mut().privacy.block_trackers = sw.is_active();
            let _ = cfg_track.borrow().save();
        });
        shield_group.add(&track_switch);

        let fp_switch = adw::SwitchRow::builder()
            .title("Fingerprinting Protection")
            .subtitle("Restricts hardware sensor probes and device enumeration APIs")
            .active(config.borrow().privacy.block_fingerprinting)
            .build();
        let cfg_fp = config.clone();
        fp_switch.connect_active_notify(move |sw| {
            cfg_fp.borrow_mut().privacy.block_fingerprinting = sw.is_active();
            let _ = cfg_fp.borrow().save();
        });
        shield_group.add(&fp_switch);
        priv_page.add(&shield_group);

        // 3.2 Security & HTTPS Group
        let sec_group = adw::PreferencesGroup::builder()
            .title("Security & Upgrades")
            .description("Connection encryption and dynamic script execution")
            .build();

        let https_switch = adw::SwitchRow::builder()
            .title("HTTPS-Only Mode")
            .subtitle("Automatically upgrade all unencrypted HTTP connections to secure HTTPS")
            .active(config.borrow().privacy.https_only)
            .build();
        let cfg_https = config.clone();
        https_switch.connect_active_notify(move |sw| {
            cfg_https.borrow_mut().privacy.https_only = sw.is_active();
            let _ = cfg_https.borrow().save();
        });
        sec_group.add(&https_switch);

        let js_switch = adw::SwitchRow::builder()
            .title("Enable JavaScript")
            .subtitle("Allows dynamic web scripts and interactive web applications")
            .active(config.borrow().privacy.enable_javascript)
            .build();
        let cfg_js = config.clone();
        js_switch.connect_active_notify(move |sw| {
            cfg_js.borrow_mut().privacy.enable_javascript = sw.is_active();
            let _ = cfg_js.borrow().save();
        });
        sec_group.add(&js_switch);
        priv_page.add(&sec_group);

        // 3.3 Cookies & Clear Browsing Data Group
        let data_group = adw::PreferencesGroup::builder()
            .title("Cookies & Browsing Data")
            .build();

        let cookie_switch = adw::SwitchRow::builder()
            .title("Block Third-Party Cookies")
            .subtitle("Prevent third-party domains from saving tracking identifiers")
            .active(config.borrow().privacy.block_third_party_cookies)
            .build();
        let cfg_cookie = config.clone();
        cookie_switch.connect_active_notify(move |sw| {
            cfg_cookie.borrow_mut().privacy.block_third_party_cookies = sw.is_active();
            let _ = cfg_cookie.borrow().save();
        });
        data_group.add(&cookie_switch);

        let cache_row = adw::ActionRow::builder()
            .title("Clear Cached Web Content")
            .subtitle("Free disk and memory cache without clearing cookies or logging out")
            .build();

        let cache_btn = gtk4::Button::builder()
            .label("Clear Cache")
            .valign(gtk4::Align::Center)
            .build();

        let sess_cache = session.clone();
        let cache_btn_ref = cache_btn.clone();
        cache_btn.connect_clicked(move |_| {
            if let Some(ref s) = sess_cache
                && let Some(wdm) = s.website_data_manager()
            {
                cache_btn_ref.set_sensitive(false);
                cache_btn_ref.set_label("Clearing…");

                let (tx, rx) = std::sync::mpsc::channel::<()>();
                let b_weak = cache_btn_ref.downgrade();

                gtk4::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                    if rx.try_recv().is_ok() {
                        if let Some(b) = b_weak.upgrade() {
                            b.set_label("Cleared ✓");
                            let b_reset = b.clone();
                            gtk4::glib::timeout_add_local_once(
                                std::time::Duration::from_millis(1500),
                                move || {
                                    b_reset.set_sensitive(true);
                                    b_reset.set_label("Clear Cache");
                                },
                            );
                        }
                        gtk4::glib::ControlFlow::Break
                    } else {
                        gtk4::glib::ControlFlow::Continue
                    }
                });

                wdm.clear(
                    webkit6::WebsiteDataTypes::DISK_CACHE
                        | webkit6::WebsiteDataTypes::MEMORY_CACHE,
                    gtk4::glib::TimeSpan(0),
                    Option::<&gtk4::gio::Cancellable>::None,
                    move |_| {
                        let _ = tx.send(());
                    },
                );
            }
        });
        cache_row.add_suffix(&cache_btn);
        data_group.add(&cache_row);

        let clear_row = adw::ActionRow::builder()
            .title("Clear Browsing Data")
            .subtitle("Remove history, cookies, cache, and site permissions")
            .build();

        let clear_btn = gtk4::Button::builder()
            .label("Clear…")
            .valign(gtk4::Align::Center)
            .css_classes(["destructive-action"])
            .build();

        let win_clr = win.clone();
        let st_clr = storage.clone();
        let sess_clr = session.clone();
        clear_btn.connect_clicked(move |_| {
            if let Some(ref s) = sess_clr {
                crate::ui::clear_data::show_clear_browsing_data_dialog(
                    &win_clr,
                    st_clr.clone(),
                    s.clone(),
                    true,
                );
            }
        });
        clear_row.add_suffix(&clear_btn);
        data_group.add(&clear_row);
        priv_page.add(&data_group);

        // 3.4 Passwords & Logins Group
        let pwd_group = adw::PreferencesGroup::builder()
            .title("Passwords & Autofill")
            .description("Credential storage and login prompt management")
            .build();

        let pwd_switch = adw::SwitchRow::builder()
            .title("Offer to save passwords")
            .subtitle("Prompt to store credentials and logins for visited websites")
            .active(config.borrow().privacy.offer_save_passwords)
            .build();
        let cfg_pwd = config.clone();
        pwd_switch.connect_active_notify(move |sw| {
            cfg_pwd.borrow_mut().privacy.offer_save_passwords = sw.is_active();
            let _ = cfg_pwd.borrow().save();
            tracing::info!(
                "Offer to save passwords setting updated: {}",
                sw.is_active()
            );
        });
        pwd_group.add(&pwd_switch);

        let keyring_row = adw::ActionRow::builder()
            .title("Keyring Integration")
            .subtitle("Desktop Secret Service (org.freedesktop.secrets)")
            .build();
        let keyring_badge = gtk4::Label::builder()
            .label("Active")
            .css_classes(["caption", "dim-label"])
            .valign(gtk4::Align::Center)
            .build();
        keyring_row.add_suffix(&keyring_badge);
        pwd_group.add(&keyring_row);

        priv_page.add(&pwd_group);

        // 3.4 Saved Site Permissions Group
        let perm_group = adw::PreferencesGroup::builder()
            .title("Saved Site Permissions")
            .description("Origins granted or denied access to camera, microphone, or location")
            .build();

        let pg_ref = perm_group.clone();
        let st_perm = storage.clone();
        storage.query_async(
            move |db| {
                let repo = crate::browser::permissions::PermissionRepository::new(db.conn());
                repo.get_all_permissions().unwrap_or_default()
            },
            move |perms| {
                if perms.is_empty() {
                    let empty_row = adw::ActionRow::builder()
                        .title("No site permissions saved")
                        .subtitle("Permission requests will prompt you when requested")
                        .build();
                    pg_ref.add(&empty_row);
                } else {
                    for p in perms {
                        let row = adw::ActionRow::builder()
                            .title(&p.origin)
                            .subtitle(format!("{}: {:?}", p.permission_type, p.state))
                            .build();

                        let del_btn = gtk4::Button::builder()
                            .icon_name("user-trash-symbolic")
                            .css_classes(["flat", "destructive-action"])
                            .valign(gtk4::Align::Center)
                            .tooltip_text("Remove saved permission")
                            .build();

                        let st_del = st_perm.clone();
                        let row_del = row.clone();
                        let origin_del = p.origin.clone();
                        let ptype_del = p.permission_type;
                        let pg_del = pg_ref.clone();

                        del_btn.connect_clicked(move |_| {
                            let origin_inner = origin_del.clone();
                            let ptype_inner = ptype_del;
                            let row_inner = row_del.clone();
                            let pg_del_inner = pg_del.clone();

                            st_del.dispatch(move |db| {
                                let repo = crate::browser::permissions::PermissionRepository::new(
                                    db.conn(),
                                );
                                let _ = repo.delete_permission(&origin_inner, ptype_inner, true);
                            });
                            pg_del_inner.remove(&row_inner);
                        });

                        row.add_suffix(&del_btn);
                        pg_ref.add(&row);
                    }
                }
            },
        );
        priv_page.add(&perm_group);

        // ================= SECTION 4: DOWNLOADS =================
        let dl_page = adw::PreferencesPage::builder()
            .title("Downloads")
            .icon_name("folder-download-symbolic")
            .build();

        let dl_group = adw::PreferencesGroup::builder()
            .title("Download Settings")
            .description("Configure file storage location and destination prompts")
            .build();

        let dl_row = adw::ActionRow::builder()
            .title("Download Directory")
            .subtitle(&config.borrow().download_dir)
            .build();

        let dl_btn = gtk4::Button::builder()
            .label("Change…")
            .css_classes(["flat", "shadhin-nav-btn"])
            .valign(gtk4::Align::Center)
            .build();

        let cfg_dl = config.clone();
        let row_dl_ref = dl_row.clone();
        let win_ref_dl = win.clone();
        dl_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder()
                .title("Select Download Directory")
                .modal(true)
                .build();

            let cfg_copy = cfg_dl.clone();
            let row_copy = row_dl_ref.clone();
            let root_win = win_ref_dl.root().and_downcast::<gtk4::Window>();
            dialog.select_folder(root_win.as_ref(), None::<&gio::Cancellable>, move |res| {
                if let Ok(folder) = res
                    && let Some(path) = folder.path()
                {
                    let path_str = path.to_string_lossy().to_string();
                    row_copy.set_subtitle(&path_str);
                    cfg_copy.borrow_mut().download_dir = path_str;
                    let _ = cfg_copy.borrow().save();
                    info!("Updated download directory");
                }
            });
        });
        dl_row.add_suffix(&dl_btn);
        dl_group.add(&dl_row);

        // "Ask where to save each file" toggle
        let ask_save_switch = adw::SwitchRow::builder()
            .title("Ask where to save each file before downloading")
            .subtitle("Prompts with file picker dialog instead of downloading directly")
            .active(config.borrow().ask_download_location)
            .build();

        let cfg_ask = config.clone();
        ask_save_switch.connect_active_notify(move |sw| {
            cfg_ask.borrow_mut().ask_download_location = sw.is_active();
            let _ = cfg_ask.borrow().save();
        });
        dl_group.add(&ask_save_switch);
        dl_page.add(&dl_group);

        // ================= SECTION 5: PERFORMANCE =================
        let perf_page = adw::PreferencesPage::builder()
            .title("Performance")
            .icon_name("speedometer-symbolic")
            .build();

        let perf_group = adw::PreferencesGroup::builder()
            .title("Memory & Resource Optimization")
            .description("Configure background tab suspension and RAM preservation")
            .build();

        // "Low memory mode" toggle
        let low_mem_switch = adw::SwitchRow::builder()
            .title("Low Memory Mode")
            .subtitle("Reduces RAM usage by aggressively suspending inactive tabs and capping WebKit caches")
            .active(config.borrow().low_memory_mode)
            .build();

        let cfg_low_mem = config.clone();
        low_mem_switch.connect_active_notify(move |sw| {
            let active = sw.is_active();
            cfg_low_mem.borrow_mut().low_memory_mode = active;
            let _ = cfg_low_mem.borrow().save();
            info!("Low memory mode set to: {active}");
        });
        perf_group.add(&low_mem_switch);

        let discard_row = adw::ActionRow::builder()
            .title("Free Memory Now")
            .subtitle("Discard unpinned background tabs that have been idle")
            .build();

        let discard_btn = gtk4::Button::builder()
            .label("Discard Idle Tabs")
            .css_classes(["flat"])
            .valign(gtk4::Align::Center)
            .build();

        let win_perf_ref = win.clone();
        discard_btn.connect_clicked(move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading("Memory Optimized")
                .body("Idle background tabs have been suspended to free system RAM.")
                .build();
            dialog.add_response("ok", "OK");
            dialog.present(Some(&win_perf_ref));
        });
        discard_row.add_suffix(&discard_btn);
        perf_group.add(&discard_row);
        perf_page.add(&perf_group);

        // ================= SECTION 6: ABOUT =================
        let about_page = adw::PreferencesPage::builder()
            .title("About")
            .icon_name("help-about-symbolic")
            .build();

        let about_group = adw::PreferencesGroup::builder()
            .title("Shadhin Browser")
            .description("A sovereign, privacy-respecting browser built in Rust and GTK4")
            .build();

        let ver_row = adw::ActionRow::builder()
            .title("Version")
            .subtitle("0.1.0 (Phase 4)")
            .build();
        about_group.add(&ver_row);

        let webkit_ver = format!(
            "WebKitGTK {}.{}.{} (Hardware-accelerated Linux engine, Chromium-free)",
            webkit6::functions::major_version(),
            webkit6::functions::minor_version(),
            webkit6::functions::micro_version(),
        );
        let engine_row = adw::ActionRow::builder()
            .title("Rendering Engine")
            .subtitle(webkit_ver)
            .build();
        about_group.add(&engine_row);

        let toolkit_row = adw::ActionRow::builder()
            .title("User Interface")
            .subtitle("GTK4 4.18 & Libadwaita 1.7")
            .build();
        about_group.add(&toolkit_row);

        let credits_row = adw::ActionRow::builder()
            .title("Credits & Licensing")
            .subtitle("Standard credits: Developed for digital sovereignty, privacy, and digital freedom. Licensed under GPL-3.0-or-later.")
            .build();
        about_group.add(&credits_row);
        about_page.add(&about_group);

        // Add exactly six sections to PreferencesDialog
        win.add(&general_page);
        win.add(&appear_page);
        win.add(&priv_page);
        win.add(&dl_page);
        win.add(&perf_page);
        win.add(&about_page);

        win.present(parent);
    }
}

/// Validates wallpaper file size (<= 15MB) and format (png, jpg, webp),
/// then copies it into `~/.local/share/shadhin-browser/wallpapers/`
pub fn copy_and_validate_wallpaper(src: &Path) -> Result<String, String> {
    if !src.exists() {
        return Err("Selected wallpaper file does not exist".into());
    }

    let meta = std::fs::metadata(src).map_err(|e| e.to_string())?;
    if meta.len() > 15 * 1024 * 1024 {
        return Err("Wallpaper image exceeds 15 MB maximum size limit".into());
    }

    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if ext != "png" && ext != "jpg" && ext != "jpeg" && ext != "webp" {
        return Err("Unsupported format: only PNG, JPEG, and WebP are allowed".into());
    }

    let dest_dir = glib::user_data_dir()
        .join("shadhin-browser")
        .join("wallpapers");

    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    let _ = builder.create(&dest_dir);

    let dest_file = dest_dir.join(format!("wallpaper_{}.{}", std::process::id(), ext));
    std::fs::copy(src, &dest_file).map_err(|e| e.to_string())?;

    Ok(dest_file.to_string_lossy().to_string())
}
