//! Linux WebKitGTK 6.0 content filter compilation and attachment

use crate::platform::traits::ContentBlockerPlatform;
use gtk4::gio;
use gtk4::glib;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::sync_channel;
use webkit6::{UserContentFilter, UserContentFilterStore, UserContentManager};

#[derive(Clone)]
pub struct LinuxContentBlocker {
    active_filters: Rc<RefCell<HashMap<String, UserContentFilter>>>,
}

impl Default for LinuxContentBlocker {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxContentBlocker {
    pub fn new() -> Self {
        Self {
            active_filters: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn attach_to_manager(&self, manager: &UserContentManager, filter_id: &str) -> bool {
        if let Some(filter) = self.active_filters.borrow().get(filter_id) {
            manager.add_filter(filter);
            true
        } else {
            false
        }
    }

    pub fn detach_from_manager(&self, manager: &UserContentManager, filter_id: &str) {
        if let Some(filter) = self.active_filters.borrow().get(filter_id) {
            manager.remove_filter(filter);
        }
    }
}

impl ContentBlockerPlatform for LinuxContentBlocker {
    fn compile_rules_from_json(
        &self,
        store_dir: &Path,
        filter_id: &str,
        json_rules: &str,
    ) -> Result<(), String> {
        let store_path = store_dir.to_string_lossy().to_string();
        let store = UserContentFilterStore::new(&store_path);

        let bytes = glib::Bytes::from(json_rules.as_bytes());
        let (sender, receiver) = sync_channel(1);

        store.save(
            filter_id,
            &bytes,
            gio::Cancellable::NONE,
            move |res| {
                let _ = sender.send(res);
            },
        );

        match receiver.recv() {
            Ok(Ok(filter)) => {
                self.active_filters
                    .borrow_mut()
                    .insert(filter_id.to_string(), filter);
                Ok(())
            }
            Ok(Err(err)) => Err(format!("WebKit content filter compilation failed: {err}")),
            Err(e) => Err(format!("Content filter channel error: {e}")),
        }
    }

    fn is_filter_active(&self, filter_id: &str) -> bool {
        self.active_filters.borrow().contains_key(filter_id)
    }
}
