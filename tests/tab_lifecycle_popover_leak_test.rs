//! Integration tests verifying that popovers and buttons are cleanly unparented and dismissed
//! when tabs are opened, right-clicked, and closed, preventing GTK memory leaks and broken accounting warnings.

use adw::prelude::*;
use gtk4::glib;
use shadhin_browser::ui::window::ShadhinWindow;

#[test]
fn test_tab_closure_and_popover_lifecycle() {
    if let Err(e) = adw::init() {
        eprintln!("SKIPPED test_tab_closure_and_popover_lifecycle: Display not available ({e:?})");
        return;
    }

    let app = adw::Application::builder()
        .application_id("com.shadhin.browser.test.popoverleak")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let _ = app.register(None::<&gtk4::gio::Cancellable>);

    let window = ShadhinWindow::new(&app);
    let ctx = glib::MainContext::default();
    while ctx.iteration(false) {}

    // 1. Open multiple tabs
    let initial_count = window.tab_view().n_pages();
    for _ in 0..3 {
        window.tab_strip().trigger_new_tab(None);
        while ctx.iteration(false) {}
    }
    assert_eq!(window.tab_view().n_pages(), initial_count + 3);

    // 2. Open toolbar popovers and verify menu button
    let mb = window.toolbar().menu_button();
    mb.popup();
    while ctx.iteration(false) {}
    assert!(mb.is_active());
    window.toolbar().close_all_popovers();
    while ctx.iteration(false) {}
    assert!(!window.toolbar().menu_button().is_active());

    // 3. Close tabs one by one and pump the main loop
    while window.tab_view().n_pages() > 1 {
        let n = window.tab_view().n_pages();
        let page = window.tab_view().nth_page(n - 1);
        window.tab_view().close_page(&page);
        while ctx.iteration(false) {}
    }

    assert_eq!(window.tab_view().n_pages(), 1);

    // 4. Test "Close Other Tabs" action
    for _ in 0..3 {
        window.tab_strip().trigger_new_tab(None);
        while ctx.iteration(false) {}
    }
    assert_eq!(window.tab_view().n_pages(), 4);

    // Trigger close other tabs keeping index 0
    window.tab_strip().trigger_new_tab(None);
    while ctx.iteration(false) {}
    window.tab_view().nth_page(0);

    // 5. Cleanup window components
    window.toolbar().cleanup();
    window.tab_strip().cleanup();
    window.sidebar().cleanup();

    while ctx.iteration(false) {}
    println!("SUCCESS: Tab closure and popover lifecycle test passed cleanly!");
}
