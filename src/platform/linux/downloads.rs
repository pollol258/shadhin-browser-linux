//! Linux WebKitGTK 6.0 download event hooks and management

use crate::platform::traits::DownloadHookPlatform;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use webkit6::Download;

#[derive(Clone, Default)]
pub struct LinuxDownloadHookManager {
    downloads: Rc<RefCell<HashMap<i64, Download>>>,
}

impl LinuxDownloadHookManager {
    pub fn new() -> Self {
        Self {
            downloads: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn register_active_download(&self, id: i64, download: Download) {
        self.downloads.borrow_mut().insert(id, download);
    }

    pub fn remove_active_download(&self, id: i64) -> Option<Download> {
        self.downloads.borrow_mut().remove(&id)
    }
}

impl DownloadHookPlatform for LinuxDownloadHookManager {
    fn cancel_download(&self, download_id: i64) {
        if let Some(download) = self.downloads.borrow().get(&download_id) {
            download.cancel();
        }
    }
}
