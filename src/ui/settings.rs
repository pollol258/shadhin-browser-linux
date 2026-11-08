//! Browser preferences and configuration management window
//!
//! Provides a Libadwaita Preferences Window with sections for General, Search,
//! Appearance, Downloads, Privacy, Workspaces, and About.

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

pub struct SettingsWindow {
    dialog: adw::PreferencesDialog,
}

impl SettingsWindow {
    /// Constructs and presents the Preferences Window
    pub fn present(
        parent: Option<&impl IsA<gtk4::Widget>>,
        config: Rc<RefCell<BrowserConfig>>,
        storage: StorageService,
        session: Option<webkit6::NetworkSession>,
    ) {
        let win = adw::PreferencesDialog::builder()
            .title("Preferences")
            .build();

        // ================= PAGE 1: GENERAL =================
        let general_page = adw::PreferencesPage::builder()
            .title("General")
            .icon_name("preferences-system-symbolic")
            .build();

        // Startup behavior group
        let startup_group = adw::PreferencesGroup::builder()
            .title("Startup")
            .description("Configure what Shadhin Browser displays when opened")
            .build();

        let startup_model = gtk4::StringList::new(&["Open New Tab", "Continue where I left off"]);
        let startup_combo = adw::ComboRow::builder()
            .title("On startup")
            .model(&startup_model)
            .build();

        let initial_startup_idx = if config.borrow().startup_behavior == "continue_previous_session"
        {
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
            .title("Homepage")
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

        // ================= PAGE 2: SEARCH =================
        let search_page = adw::PreferencesPage::builder()
            .title("Search")
            .icon_name("system-search-symbolic")
            .build();

        let search_group = adw::PreferencesGroup::builder()
            .title("Search Engine")
            .description("Choose default search engine for address bar and New Tab")
            .build();

        let engines_model = gtk4::StringList::new(&[
            "DuckDuckGo",
            "Brave Search",
            "Startpage",
            "Google",
            "Bing",
            "Wikipedia",
        ]);
        let search_combo = adw::ComboRow::builder()
            .title("Default Search Provider")
            .model(&engines_model)
            .build();

        let current_engine = config.borrow().search_engine_url.clone();
        let selected_idx = if current_engine.contains("brave.com") {
            1
        } else if current_engine.contains("startpage.com") {
            2
        } else if current_engine.contains("google.com") {
            3
        } else if current_engine.contains("bing.com") {
            4
        } else if current_engine.contains("wikipedia.org") {
            5
        } else {
            0
        };
        search_combo.set_selected(selected_idx);

        let cfg_search = config.clone();
        let storage_search = storage.clone();
        search_combo.connect_selected_notify(move |combo| {
            let (name, tmpl, kw) = match combo.selected() {
                1 => ("Brave Search", "https://search.brave.com/search?q=%s", "b"),
                2 => (
                    "Startpage",
                    "https://www.startpage.com/sp/search?query=%s",
                    "s",
                ),
                3 => ("Google", "https://www.google.com/search?q=%s", "g"),
                4 => ("Bing", "https://www.bing.com/search?q=%s", "bing"),
                5 => (
                    "Wikipedia",
                    "https://en.wikipedia.org/wiki/Special:Search?search=%s",
                    "w",
                ),
                _ => ("DuckDuckGo", "https://duckduckgo.com/?q=%s", "d"),
            };

            cfg_search.borrow_mut().search_engine_url = tmpl.to_string();
            let _ = cfg_search.borrow().save();

            let kw_str = kw.to_string();
            storage_search.dispatch(move |db| {
                let repo = SearchEngineRepository::new(db.conn());
                if let Ok(Some(item)) = repo.get_by_keyword(&kw_str) {
                    let _ = repo.set_default(item.id);
                }
            });

            info!("Set default search engine to: {name}");
        });
        search_group.add(&search_combo);
        search_page.add(&search_group);

        // Add Custom Search Engine Group
        let custom_group = adw::PreferencesGroup::builder()
            .title("Add Custom Search Engine")
            .description("Configure a custom search provider (must use https:// and contain %s)")
            .build();

        let name_entry = adw::EntryRow::builder().title("Engine Name").build();
        let kw_entry = adw::EntryRow::builder()
            .title("Keyword (e.g. 'gh')")
            .build();
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

        custom_group.add(&name_entry);
        custom_group.add(&kw_entry);
        custom_group.add(&tmpl_entry);
        custom_group.add(&btn_row);
        search_page.add(&custom_group);

        // Manage Custom Engines Group
        let list_custom_group = adw::PreferencesGroup::builder()
            .title("Installed Custom Search Engines")
            .description("Review and remove configured custom search providers")
            .build();

        let lcg_ref = list_custom_group.clone();
        let st_list = storage.clone();
        storage.query_async(
            move |db| {
                let repo = SearchEngineRepository::new(db.conn());
                repo.get_all().unwrap_or_default()
            },
            move |engines| {
                let mut count = 0;
                for engine in engines {
                    if !engine.is_builtin {
                        count += 1;
                        let row = adw::ActionRow::builder()
                            .title(&engine.name)
                            .subtitle(format!(
                                "keyword: '{}' · {}",
                                engine.keyword, engine.url_template
                            ))
                            .build();

                        let del_btn = gtk4::Button::builder()
                            .icon_name("user-trash-symbolic")
                            .css_classes(["flat", "destructive-action"])
                            .valign(gtk4::Align::Center)
                            .tooltip_text("Remove search engine")
                            .build();

                        let eng_id = engine.id;
                        let st_del = st_list.clone();
                        let row_clone = row.clone();
                        let lcg_clone = lcg_ref.clone();
                        del_btn.connect_clicked(move |_| {
                            st_del.dispatch(move |db| {
                                let repo = SearchEngineRepository::new(db.conn());
                                let _ = repo.delete(eng_id);
                            });
                            lcg_clone.remove(&row_clone);
                        });

                        row.add_suffix(&del_btn);
                        lcg_ref.add(&row);
                    }
                }
                if count == 0 {
                    let empty_row = adw::ActionRow::builder()
                        .title("No custom search engines added yet")
                        .build();
                    lcg_ref.add(&empty_row);
                }
            },
        );
        search_page.add(&list_custom_group);

        // ================= PAGE 3: APPEARANCE =================
        let appear_page = adw::PreferencesPage::builder()
            .title("Appearance")
            .icon_name("preferences-desktop-theme-symbolic")
            .build();

        let theme_group = adw::PreferencesGroup::builder()
            .title("Theme & Wallpapers")
            .description("Customize window appearance and background graphics")
            .build();

        let dark_switch = adw::SwitchRow::builder()
            .title("Dark Theme")
            .subtitle("Follows system or manual override")
            .active(config.borrow().color_scheme == "dark" || config.borrow().force_dark_theme)
            .build();

        let cfg_dark = config.clone();
        dark_switch.connect_active_notify(move |sw| {
            let active = sw.is_active();
            cfg_dark.borrow_mut().force_dark_theme = active;
            cfg_dark.borrow_mut().color_scheme = if active { "dark".to_string() } else { "light".to_string() };
            let _ = cfg_dark.borrow().save();
            let style_manager = adw::StyleManager::default();
            if active {
                style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
            } else {
                style_manager.set_color_scheme(adw::ColorScheme::ForceLight);
            }
        });
        theme_group.add(&dark_switch);

        // Accent color swatches row matching prototype: #3584e4, #26a269, #9141ac, #e66100
        let accent_row = adw::ActionRow::builder()
            .title("Accent Color")
            .subtitle("Changes buttons, selection and focus rings")
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

        // Tab layout row: Top tabs vs Vertical sidebar
        let tab_layout_row = adw::ComboRow::builder()
            .title("Tab Layout")
            .subtitle("Top tab strip or vertical sidebar")
            .model(&gtk4::StringList::new(&["Top tabs", "Vertical sidebar"]))
            .selected(if config.borrow().tab_layout == "vertical" { 1 } else { 0 })
            .build();

        let cfg_layout = config.clone();
        tab_layout_row.connect_selected_notify(move |row| {
            let sel = row.selected();
            let layout_val = if sel == 1 { "vertical" } else { "top" };
            cfg_layout.borrow_mut().tab_layout = layout_val.to_string();
            let _ = cfg_layout.borrow().save();
        });
        theme_group.add(&tab_layout_row);

        // Wallpaper picker row
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
            .label("Choose Image...")
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

        // ================= PAGE 4: DOWNLOADS =================
        let dl_page = adw::PreferencesPage::builder()
            .title("Downloads")
            .icon_name("folder-download-symbolic")
            .build();

        let dl_group = adw::PreferencesGroup::builder()
            .title("Download Settings")
            .description("Configure location and security options for files")
            .build();

        let dl_row = adw::ActionRow::builder()
            .title("Download Location")
            .subtitle(&config.borrow().download_dir)
            .build();

        let dl_btn = gtk4::Button::builder()
            .label("Change...")
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
        dl_page.add(&dl_group);

        // ================= PAGE 5: PRIVACY =================
        let priv_page = adw::PreferencesPage::builder()
            .title("Privacy")
            .icon_name("security-high-symbolic")
            .build();

        // Group 1: Shield & Content Filtering
        let shield_group = adw::PreferencesGroup::builder()
            .title("Content Filtering & Shield")
            .description("Native WebKit content filter rules (EasyList & EasyPrivacy)")
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
            .title("Block Ads")
            .subtitle("Compiled WebKit content rules blocking advertising domains")
            .active(config.borrow().privacy.block_ads)
            .build();
        let cfg_ads = config.clone();
        ads_switch.connect_active_notify(move |sw| {
            cfg_ads.borrow_mut().privacy.block_ads = sw.is_active();
            let _ = cfg_ads.borrow().save();
        });
        shield_group.add(&ads_switch);

        let track_switch = adw::SwitchRow::builder()
            .title("Block Trackers")
            .subtitle("Blocks cross-site tracking scripts and telemetry beacons")
            .active(config.borrow().privacy.block_trackers)
            .build();
        let cfg_track = config.clone();
        track_switch.connect_active_notify(move |sw| {
            cfg_track.borrow_mut().privacy.block_trackers = sw.is_active();
            let _ = cfg_track.borrow().save();
        });
        shield_group.add(&track_switch);

        let fp_switch = adw::SwitchRow::builder()
            .title("Fingerprinting Protection (limited)")
            .subtitle("Restricts standard hardware probe APIs; WebKitGTK does not randomize canvas")
            .active(config.borrow().privacy.block_fingerprinting)
            .build();
        let cfg_fp = config.clone();
        fp_switch.connect_active_notify(move |sw| {
            cfg_fp.borrow_mut().privacy.block_fingerprinting = sw.is_active();
            let _ = cfg_fp.borrow().save();
        });
        shield_group.add(&fp_switch);
        priv_page.add(&shield_group);

        // Group 2: Connection & Execution Controls
        let conn_group = adw::PreferencesGroup::builder()
            .title("Connection & Execution")
            .build();

        let https_switch = adw::SwitchRow::builder()
            .title("HTTPS-Only Mode")
            .subtitle("Automatically upgrade unencrypted HTTP connections to secure HTTPS")
            .active(config.borrow().privacy.https_only)
            .build();
        let cfg_https = config.clone();
        https_switch.connect_active_notify(move |sw| {
            cfg_https.borrow_mut().privacy.https_only = sw.is_active();
            let _ = cfg_https.borrow().save();
        });
        conn_group.add(&https_switch);

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
        conn_group.add(&js_switch);
        priv_page.add(&conn_group);

        // Group 3: Cookies & Browsing Data
        let data_group = adw::PreferencesGroup::builder()
            .title("Cookies & Browsing Data")
            .build();

        let cookie_switch = adw::SwitchRow::builder()
            .title("Block Third-Party Cookies")
            .subtitle("Prevent cross-site trackers from storing tracking cookies")
            .active(config.borrow().privacy.block_third_party_cookies)
            .build();
        let cfg_cookie = config.clone();
        cookie_switch.connect_active_notify(move |sw| {
            cfg_cookie.borrow_mut().privacy.block_third_party_cookies = sw.is_active();
            let _ = cfg_cookie.borrow().save();
        });
        data_group.add(&cookie_switch);

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

        // Group 4: Site Permissions Manager
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

        // ================= PAGE 6: WORKSPACES =================
        let ws_page = adw::PreferencesPage::builder()
            .title("Workspaces")
            .icon_name("folder-symbolic")
            .build();

        let ws_group = adw::PreferencesGroup::builder()
            .title("Manage Workspaces")
            .description("Configure isolated workspace profiles, names, and color tags")
            .build();

        let wsg_ref = ws_group.clone();
        let st_ws = storage.clone();
        let win_ws_ref = win.clone();
        storage.query_async(
            move |db| {
                let repo = crate::storage::workspaces::WorkspaceRepository::new(db.conn());
                repo.get_all().unwrap_or_default()
            },
            move |workspaces| {
                for ws in workspaces {
                    let row = adw::ActionRow::builder()
                        .title(&ws.name)
                        .subtitle(format!("ID: '{}' · Color: {}", ws.id, ws.color))
                        .build();

                    let dot = gtk4::Box::builder()
                        .css_classes(["shadhin-workspace-dot"])
                        .valign(gtk4::Align::Center)
                        .build();
                    row.add_prefix(&dot);

                    let ws_id = ws.id.clone();
                    let ws_name = ws.name.clone();
                    let ws_color = ws.color.clone();

                    // Rename button
                    let rename_btn = gtk4::Button::builder()
                        .icon_name("document-edit-symbolic")
                        .css_classes(["flat"])
                        .valign(gtk4::Align::Center)
                        .tooltip_text("Rename workspace")
                        .build();

                    let win_parent = win_ws_ref.clone();
                    let st_ren = st_ws.clone();
                    let row_ren = row.clone();
                    let id_ren = ws_id.clone();
                    let col_ren = ws_color.clone();
                    rename_btn.connect_clicked(move |_| {
                        let dialog = adw::AlertDialog::builder()
                            .heading("Rename Workspace")
                            .body("Enter new name for this workspace:")
                            .build();
                        let entry = gtk4::Entry::builder().text(&ws_name).build();
                        dialog.set_extra_child(Some(&entry));
                        dialog.add_response("cancel", "Cancel");
                        dialog.add_response("save", "Save");

                        let st_sub = st_ren.clone();
                        let row_sub = row_ren.clone();
                        let id_sub = id_ren.clone();
                        let col_sub = col_ren.clone();
                        dialog.choose(Some(&win_parent), None::<&gio::Cancellable>, move |resp| {
                            if resp == "save" {
                                let new_name = entry.text().trim().to_string();
                                if !new_name.is_empty() {
                                    row_sub.set_title(&new_name);
                                    let id_db = id_sub.clone();
                                    let col_db = col_sub.clone();
                                    st_sub.dispatch(move |db| {
                                        let repo =
                                            crate::storage::workspaces::WorkspaceRepository::new(
                                                db.conn(),
                                            );
                                        let _ = repo.update(&id_db, &new_name, &col_db);
                                    });
                                }
                            }
                        });
                    });
                    row.add_suffix(&rename_btn);

                    // Delete button (disabled for "default" workspace)
                    if ws_id != "default" {
                        let del_btn = gtk4::Button::builder()
                            .icon_name("user-trash-symbolic")
                            .css_classes(["flat", "destructive-action"])
                            .valign(gtk4::Align::Center)
                            .tooltip_text("Delete workspace")
                            .build();

                        let st_del = st_ws.clone();
                        let row_del = row.clone();
                        let wsg_del = wsg_ref.clone();
                        let id_del = ws_id.clone();
                        del_btn.connect_clicked(move |_| {
                            let id_to_delete = id_del.clone();
                            st_del.dispatch(move |db| {
                                let repo =
                                    crate::storage::workspaces::WorkspaceRepository::new(db.conn());
                                let _ = repo.delete(&id_to_delete);
                            });
                            wsg_del.remove(&row_del);
                        });
                        row.add_suffix(&del_btn);
                    }

                    wsg_ref.add(&row);
                }
            },
        );
        ws_page.add(&ws_group);

        // ================= PAGE 7: ABOUT =================
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
            .subtitle("0.1.0-alpha (Phase 4)")
            .build();
        about_group.add(&ver_row);

        let engine_row = adw::ActionRow::builder()
            .title("Rendering Engine")
            .subtitle("WebKitGTK 6.0 (Hardware-accelerated Linux engine, Chromium-free)")
            .build();
        about_group.add(&engine_row);

        let toolkit_row = adw::ActionRow::builder()
            .title("User Interface")
            .subtitle("GTK4 & Libadwaita 1.6+")
            .build();
        about_group.add(&toolkit_row);
        about_page.add(&about_group);

        win.add(&general_page);
        win.add(&search_page);
        win.add(&appear_page);
        win.add(&dl_page);
        win.add(&priv_page);
        win.add(&ws_page);
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
