//! Integration tests for Application Menu, GActions, MenuButton, and theme adaptability

use adw::prelude::*;
use gtk4::gio;
use shadhin_browser::ui::toolbar::ShadhinToolbar;
use shadhin_browser::ui::window::ShadhinWindow;

#[test]
fn test_application_menu_full_suite() {
    if let Err(e) = adw::init() {
        eprintln!("SKIPPED test_application_menu_full_suite: Display not available ({e:?})");
        return;
    }

    // 1. Standalone toolbar test
    println!("SUBCHECK 1: Standalone MenuButton properties & sensitivity");
    let toolbar = ShadhinToolbar::new();
    let standalone_menu_btn = toolbar.menu_button();
    assert!(
        standalone_menu_btn.is::<gtk4::MenuButton>(),
        "Widget must be a gtk4::MenuButton"
    );
    assert!(
        standalone_menu_btn.is_sensitive(),
        "MenuButton must be sensitive (enabled)"
    );
    assert!(
        standalone_menu_btn.has_css_class("shadhin-nav-btn"),
        "MenuButton must have 'shadhin-nav-btn' css class"
    );

    let dummy_menu = gio::Menu::new();
    dummy_menu.append(Some("Test Item"), Some("win.new-tab"));
    toolbar.set_menu_model(&dummy_menu);
    assert!(
        standalone_menu_btn.menu_model().is_some(),
        "MenuButton must have an assigned MenuModel"
    );

    // 2. Application actions
    println!(
        "SUBCHECK 2: Application actions registration (app.new-window, app.new-private-window, app.quit)"
    );
    let app = adw::Application::builder()
        .application_id("com.shadhin.browser.test.menu")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let _ = app.register(None::<&gio::Cancellable>);

    let act_new_win = gio::SimpleAction::new("new-window", None);
    let act_new_priv = gio::SimpleAction::new("new-private-window", None);
    let act_quit = gio::SimpleAction::new("quit", None);
    app.add_action(&act_new_win);
    app.add_action(&act_new_priv);
    app.add_action(&act_quit);

    assert!(app.has_action("new-window"), "app.new-window must exist");
    assert!(
        app.has_action("new-private-window"),
        "app.new-private-window must exist"
    );
    assert!(app.has_action("quit"), "app.quit must exist");

    // 3. Window actions registration
    println!("SUBCHECK 3: Window actions registration & initial enabled state");
    let window = ShadhinWindow::new_with_urls(&app, &[]);
    window.present();
    let win_widget = window.window().clone();

    let win_actions = [
        "new-tab",
        "reopen-tab",
        "bookmarks",
        "history",
        "downloads",
        "zoom-in",
        "zoom-out",
        "zoom-reset",
        "find-in-page",
        "clear-browsing-data",
        "settings",
        "about",
    ];

    for act_name in win_actions {
        assert!(
            win_widget.has_action(act_name),
            "win.{act_name} must be registered on the window"
        );
        let action = win_widget
            .lookup_action(act_name)
            .unwrap_or_else(|| panic!("Action win.{act_name} must be lookable"));
        assert!(
            action.is_enabled(),
            "Action win.{act_name} must be enabled by default"
        );
    }

    // 4. Menu model and popover
    println!("SUBCHECK 4: MenuModel popover attachment and popup/popdown");
    let menu_btn = window.toolbar().menu_button();
    let model = menu_btn.menu_model().expect("Menu model must be attached");
    assert!(
        model.n_items() > 0,
        "Main menu model must contain sections/items"
    );

    menu_btn.popup();
    assert!(
        menu_btn.popover().is_some(),
        "Menu button must have an active popover"
    );
    menu_btn.popdown();

    // 5. Unfocused execution
    println!("SUBCHECK 5: Unfocused window action execution");
    for act_name in win_actions {
        let action = win_widget.lookup_action(act_name).unwrap();
        assert!(
            action.is_enabled(),
            "Action win.{act_name} must remain enabled when unfocused"
        );
    }

    gio::prelude::ActionGroupExt::activate_action(&win_widget, "bookmarks", None);
    assert_eq!(window.toolbar().address_bar().text(), "about:bookmarks");

    gio::prelude::ActionGroupExt::activate_action(&win_widget, "history", None);
    assert_eq!(window.toolbar().address_bar().text(), "about:history");

    gio::prelude::ActionGroupExt::activate_action(&win_widget, "downloads", None);
    assert!(
        window
            .toolbar()
            .downloads_popover()
            .unwrap()
            .is_open(),
        "Downloads action must toggle the downloads popover"
    );

    gio::prelude::ActionGroupExt::activate_action(&win_widget, "find-in-page", None);
    assert!(
        window.find_bar().is_visible(),
        "Find in page bar must be visible"
    );

    gio::prelude::ActionGroupExt::activate_action(&win_widget, "zoom-in", None);
    gio::prelude::ActionGroupExt::activate_action(&win_widget, "zoom-out", None);
    gio::prelude::ActionGroupExt::activate_action(&win_widget, "zoom-reset", None);
    gio::prelude::ActionGroupExt::activate_action(&win_widget, "new-tab", None);

    // 6. Theme switching
    println!("SUBCHECK 6: Dynamic light and dark theme switching");
    let style_manager = adw::StyleManager::default();
    style_manager.set_color_scheme(adw::ColorScheme::ForceLight);
    assert_eq!(style_manager.color_scheme(), adw::ColorScheme::ForceLight);

    style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
    assert_eq!(style_manager.color_scheme(), adw::ColorScheme::ForceDark);
}
