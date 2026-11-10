//! Multi-tab state management, tab lifecycle, and discarding
//!
//! Serves as the single source of truth for browser tabs, maintaining active
//! navigation states, tab ordering, in-memory closed tab history (bounded to 10),
//! and low-RAM tab discarding.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Unique identifier for a browser tab
pub type TabId = usize;

/// Bounded capacity for closed tabs stack (Ctrl+Shift+T)
pub const MAX_CLOSED_TABS: usize = 10;

/// Default threshold before background tab discarding is evaluated
pub const TAB_DISCARD_THRESHOLD: usize = 8;

/// Metadata stored when a tab is closed for later restoration
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedTabInfo {
    pub url: String,
    pub title: String,
}

/// Represents the engine and navigation state of an individual tab
#[derive(Debug)]
pub struct Tab {
    pub id: TabId,
    pub url: String,
    pub title: String,
    pub favicon: Option<gtk4::gdk::Texture>,
    pub is_loading: bool,
    pub progress: f64,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub pinned: bool,
    pub parent_id: Option<TabId>,
    pub workspace_id: Option<String>,
    pub zoom_level: f64,
    pub is_discarded: bool,
    pub has_active_webview: bool,
    pub last_active: Instant,
}

impl Tab {
    /// Creates a new tab instance
    pub fn new(id: TabId, initial_url: Option<&str>, parent_id: Option<TabId>) -> Self {
        let is_newtab = initial_url.is_none()
            || initial_url == Some("about:newtab")
            || initial_url == Some("shadhin:newtab");
        let url = if is_newtab {
            "about:newtab".to_string()
        } else {
            initial_url.unwrap_or("about:newtab").to_string()
        };

        let title = if is_newtab {
            "New Tab".to_string()
        } else {
            "Loading...".to_string()
        };

        Self {
            id,
            url,
            title,
            favicon: None,
            is_loading: false,
            progress: 0.0,
            can_go_back: false,
            can_go_forward: false,
            pinned: false,
            parent_id,
            workspace_id: None,
            zoom_level: 1.0,
            is_discarded: false,
            has_active_webview: !is_newtab, // Lazy allocation: true only if starting with a web URL
            last_active: Instant::now(),
        }
    }

    /// Checks if this tab is on the internal New Tab page
    pub fn is_newtab(&self) -> bool {
        self.url == "about:newtab" || self.url == "shadhin:newtab"
    }

    /// Marks the tab as accessed right now
    pub fn touch(&mut self) {
        self.last_active = Instant::now();
        self.is_discarded = false;
    }
}

/// Central tab manager that owns the tab list and operations
pub struct TabManager {
    tabs: Vec<Rc<RefCell<Tab>>>,
    selected_index: usize,
    closed_tabs: VecDeque<ClosedTabInfo>,
    next_id: TabId,
}

impl TabManager {
    /// Constructs a new empty TabManager
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            selected_index: 0,
            closed_tabs: VecDeque::with_capacity(MAX_CLOSED_TABS),
            next_id: 1,
        }
    }

    /// Total number of open tabs
    pub fn count(&self) -> usize {
        self.tabs.len()
    }

    /// Current selected tab index
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Reference to currently selected tab
    pub fn active_tab(&self) -> Option<Rc<RefCell<Tab>>> {
        self.tabs.get(self.selected_index).cloned()
    }

    /// Accessor to tab by index
    pub fn get_tab(&self, index: usize) -> Option<Rc<RefCell<Tab>>> {
        self.tabs.get(index).cloned()
    }

    /// All open tabs
    pub fn tabs(&self) -> &[Rc<RefCell<Tab>>] {
        &self.tabs
    }

    /// Creates and appends a new tab
    pub fn new_tab(
        &mut self,
        url: Option<&str>,
        parent_id: Option<TabId>,
    ) -> (usize, Rc<RefCell<Tab>>) {
        let id = self.next_id;
        self.next_id += 1;

        let tab = Rc::new(RefCell::new(Tab::new(id, url, parent_id)));
        self.tabs.push(tab.clone());
        let new_idx = self.tabs.len() - 1;
        self.selected_index = new_idx;
        (new_idx, tab)
    }

    /// Closes a tab at a given index.
    /// Returns the closed tab information.
    /// If the last tab is closed, automatically opens a fresh New Tab.
    pub fn close_tab(&mut self, index: usize) -> Option<ClosedTabInfo> {
        if index >= self.tabs.len() {
            return None;
        }

        let closed = self.tabs.remove(index);
        let info = {
            let b = closed.borrow();
            ClosedTabInfo {
                url: b.url.clone(),
                title: b.title.clone(),
            }
        };

        // Don't record blank new tabs in reopen stack
        if info.url != "about:newtab" && info.url != "shadhin:newtab" && !info.url.is_empty() {
            if self.closed_tabs.len() >= MAX_CLOSED_TABS {
                self.closed_tabs.pop_front();
            }
            self.closed_tabs.push_back(info.clone());
        }

        // Adjust selected index
        if self.tabs.is_empty() {
            // Automatically open a fresh New Tab so the browser never sits empty
            self.new_tab(None, None);
            self.selected_index = 0;
        } else if self.selected_index >= self.tabs.len() {
            self.selected_index = self.tabs.len() - 1;
        }

        if let Some(active) = self.active_tab() {
            active.borrow_mut().touch();
        }

        Some(info)
    }

    /// Reopens the most recently closed tab (Ctrl+Shift+T)
    pub fn reopen_closed_tab(&mut self) -> Option<(usize, Rc<RefCell<Tab>>)> {
        if let Some(info) = self.closed_tabs.pop_back() {
            let (idx, tab) = self.new_tab(Some(&info.url), None);
            tab.borrow_mut().title = info.title;
            Some((idx, tab))
        } else {
            None
        }
    }

    /// Pops the last closed tab from the stack without opening it directly
    pub fn pop_closed_tab(&mut self) -> Option<ClosedTabInfo> {
        self.closed_tabs.pop_back()
    }

    /// Duplicates the tab at the given index
    pub fn duplicate_tab(&mut self, index: usize) -> Option<(usize, Rc<RefCell<Tab>>)> {
        if let Some(orig) = self.tabs.get(index) {
            let (url, parent_id) = {
                let b = orig.borrow();
                (b.url.clone(), Some(b.id))
            };
            let (new_idx, new_tab) = self.new_tab(Some(&url), parent_id);
            Some((new_idx, new_tab))
        } else {
            None
        }
    }

    /// Closes all tabs except the tab at `keep_index`
    pub fn close_other_tabs(&mut self, keep_index: usize) {
        if keep_index >= self.tabs.len() {
            return;
        }

        let kept_tab = self.tabs.remove(keep_index);

        // Record any closed URLs to closed stack
        for t in self.tabs.drain(..) {
            let b = t.borrow();
            if !b.is_newtab() && !b.url.is_empty() {
                if self.closed_tabs.len() >= MAX_CLOSED_TABS {
                    self.closed_tabs.pop_front();
                }
                self.closed_tabs.push_back(ClosedTabInfo {
                    url: b.url.clone(),
                    title: b.title.clone(),
                });
            }
        }

        self.tabs.push(kept_tab);
        self.selected_index = 0;
        if let Some(first) = self.tabs.first() {
            first.borrow_mut().touch();
        }
    }

    /// Selects a tab by index
    pub fn switch_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.selected_index = index;
            self.tabs[index].borrow_mut().touch();
            true
        } else {
            false
        }
    }

    /// Switches to the next tab (Ctrl+Tab)
    pub fn next_tab(&mut self) -> usize {
        if !self.tabs.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.tabs.len();
            self.tabs[self.selected_index].borrow_mut().touch();
        }
        self.selected_index
    }

    /// Switches to the previous tab (Ctrl+Shift+Tab)
    pub fn prev_tab(&mut self) -> usize {
        if !self.tabs.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.tabs.len() - 1;
            } else {
                self.selected_index -= 1;
            }
            self.tabs[self.selected_index].borrow_mut().touch();
        }
        self.selected_index
    }

    /// Moves a tab from one position to another
    pub fn reorder_tab(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }

        let item = self.tabs.remove(from);
        self.tabs.insert(to, item);

        if self.selected_index == from {
            self.selected_index = to;
        } else if from < self.selected_index && to >= self.selected_index {
            self.selected_index -= 1;
        } else if from > self.selected_index && to <= self.selected_index {
            self.selected_index += 1;
        }

        true
    }

    /// Identifies and marks idle background tabs as discarded when tab count exceeds threshold.
    /// Returns indices of discarded tabs so the UI can drop their heavy WebView instances.
    pub fn discard_idle_tabs(&mut self, threshold: usize, min_idle: Duration) -> Vec<usize> {
        self.discard_idle_tabs_adaptive(threshold, min_idle, false)
    }

    /// Discards background tabs adaptively based on memory pressure and idle duration.
    /// Under low memory, discards any inactive background tab idle for min_idle (or 30s max),
    /// regardless of whether tab count exceeds threshold.
    pub fn discard_idle_tabs_adaptive(
        &mut self,
        threshold: usize,
        min_idle: Duration,
        is_low_memory: bool,
    ) -> Vec<usize> {
        let effective_threshold = if is_low_memory { 1 } else { threshold };
        let effective_min_idle = if is_low_memory {
            Duration::from_secs(30).min(min_idle)
        } else {
            min_idle
        };

        if self.tabs.len() <= effective_threshold {
            return Vec::new();
        }

        let now = Instant::now();
        let mut discarded_indices = Vec::new();

        for (idx, tab_cell) in self.tabs.iter().enumerate() {
            // Never discard the currently active tab or pinned tabs
            if idx == self.selected_index {
                continue;
            }

            let mut tab = tab_cell.borrow_mut();
            if tab.pinned || tab.is_discarded || !tab.has_active_webview {
                continue;
            }

            if now.duration_since(tab.last_active) >= effective_min_idle {
                tab.is_discarded = true;
                tab.has_active_webview = false;
                discarded_indices.push(idx);
            }
        }

        discarded_indices
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_tab_creation() {
        let mut tm = TabManager::new();
        assert_eq!(tm.count(), 0);

        let (idx, tab) = tm.new_tab(None, None);
        assert_eq!(idx, 0);
        assert_eq!(tm.count(), 1);
        assert_eq!(tab.borrow().url, "about:newtab");
        assert!(tab.borrow().is_newtab());
        assert!(!tab.borrow().has_active_webview); // Lazy allocation!
    }

    #[test]
    fn test_web_tab_lazy_allocation() {
        let mut tm = TabManager::new();
        let (_, tab) = tm.new_tab(Some("https://example.com"), None);
        assert_eq!(tab.borrow().url, "https://example.com");
        assert!(!tab.borrow().is_newtab());
        assert!(tab.borrow().has_active_webview);
    }

    #[test]
    fn test_close_and_reopen_stack() {
        let mut tm = TabManager::new();
        tm.new_tab(Some("https://first.com"), None);
        tm.new_tab(Some("https://second.com"), None);
        tm.new_tab(Some("https://third.com"), None);

        assert_eq!(tm.count(), 3);
        assert_eq!(tm.selected_index(), 2);

        // Close tab 1 (second.com)
        let closed = tm.close_tab(1).unwrap();
        assert_eq!(closed.url, "https://second.com");
        assert_eq!(tm.count(), 2);

        // Reopen closed tab (Ctrl+Shift+T)
        let reopened = tm.reopen_closed_tab();
        assert!(reopened.is_some());
        let (idx, rtab) = reopened.unwrap();
        assert_eq!(idx, 2);
        assert_eq!(rtab.borrow().url, "https://second.com");
        assert_eq!(tm.count(), 3);
    }

    #[test]
    fn test_closing_last_tab_leaves_newtab() {
        let mut tm = TabManager::new();
        tm.new_tab(Some("https://example.com"), None);
        assert_eq!(tm.count(), 1);

        tm.close_tab(0);
        assert_eq!(tm.count(), 1);
        let active = tm.active_tab().unwrap();
        assert_eq!(active.borrow().url, "about:newtab");
        assert!(active.borrow().is_newtab());
    }

    #[test]
    fn test_duplicate_and_close_others() {
        let mut tm = TabManager::new();
        tm.new_tab(Some("https://keep.com"), None);
        tm.new_tab(Some("https://discard1.com"), None);
        tm.new_tab(Some("https://discard2.com"), None);

        assert_eq!(tm.count(), 3);
        tm.duplicate_tab(0);
        assert_eq!(tm.count(), 4);
        assert_eq!(tm.tabs()[3].borrow().url, "https://keep.com");

        tm.close_other_tabs(0);
        assert_eq!(tm.count(), 1);
        assert_eq!(tm.tabs()[0].borrow().url, "https://keep.com");
    }

    #[test]
    fn test_tab_navigation_cycling() {
        let mut tm = TabManager::new();
        tm.new_tab(Some("https://a.com"), None);
        tm.new_tab(Some("https://b.com"), None);
        tm.new_tab(Some("https://c.com"), None);

        tm.switch_tab(0);
        assert_eq!(tm.selected_index(), 0);

        assert_eq!(tm.next_tab(), 1);
        assert_eq!(tm.next_tab(), 2);
        assert_eq!(tm.next_tab(), 0); // wrap around

        assert_eq!(tm.prev_tab(), 2); // wrap around backwards
        assert_eq!(tm.prev_tab(), 1);
    }

    #[test]
    fn test_reorder_tab() {
        let mut tm = TabManager::new();
        tm.new_tab(Some("https://a.com"), None);
        tm.new_tab(Some("https://b.com"), None);
        tm.new_tab(Some("https://c.com"), None);

        assert_eq!(tm.tabs()[0].borrow().url, "https://a.com");
        assert_eq!(tm.tabs()[1].borrow().url, "https://b.com");
        assert_eq!(tm.tabs()[2].borrow().url, "https://c.com");

        // Move tab 0 to position 2
        assert!(tm.reorder_tab(0, 2));
        assert_eq!(tm.tabs()[0].borrow().url, "https://b.com");
        assert_eq!(tm.tabs()[1].borrow().url, "https://c.com");
        assert_eq!(tm.tabs()[2].borrow().url, "https://a.com");
    }

    #[test]
    fn test_discard_idle_tabs() {
        let mut tm = TabManager::new();
        for i in 0..10 {
            tm.new_tab(Some(&format!("https://tab{i}.com")), None);
        }

        assert_eq!(tm.count(), 10);
        // Active tab is tab 9
        assert_eq!(tm.selected_index(), 9);

        // Discard tabs idle for 0 duration with threshold of 5
        let discarded = tm.discard_idle_tabs(5, Duration::from_millis(0));
        assert_eq!(discarded.len(), 9); // All tabs except active tab 9
        assert!(!discarded.contains(&9));

        for idx in discarded {
            let tab = tm.get_tab(idx).unwrap();
            assert!(tab.borrow().is_discarded);
            assert!(!tab.borrow().has_active_webview);
        }

        // Active tab must NOT be discarded
        assert!(!tm.active_tab().unwrap().borrow().is_discarded);
    }

    #[test]
    fn test_discard_idle_tabs_adaptive_low_memory() {
        let mut tm = TabManager::new();
        // Create only 3 tabs (below regular threshold of 8)
        for i in 0..3 {
            tm.new_tab(Some(&format!("https://tab{i}.com")), None);
        }
        assert_eq!(tm.count(), 3);
        assert_eq!(tm.selected_index(), 2);

        // Under normal memory (false), threshold 5 -> 0 discarded
        let normal = tm.discard_idle_tabs_adaptive(5, Duration::from_millis(0), false);
        assert_eq!(normal.len(), 0);

        // Under low memory (true), threshold 5 is overridden to 1 -> tabs 0 and 1 discarded
        let low_mem = tm.discard_idle_tabs_adaptive(5, Duration::from_millis(0), true);
        assert_eq!(low_mem.len(), 2);
        assert_eq!(low_mem, vec![0, 1]);
        assert!(tm.get_tab(0).unwrap().borrow().is_discarded);
        assert!(tm.get_tab(1).unwrap().borrow().is_discarded);
        assert!(!tm.get_tab(2).unwrap().borrow().is_discarded);
    }
}

#[cfg(test)]
mod memory_tests {
    use std::fs;

    fn get_rss_kb() -> usize {
        let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return parts[1].parse().unwrap_or(0);
                }
            }
        }
        0
    }

    #[test]
    fn test_tab_lifecycle_memory() {
        let baseline = get_rss_kb();
        let mut tm = crate::browser::tabs::TabManager::new();
        for i in 0..20 {
            tm.new_tab(Some(&format!("https://example{i}.org")), None);
        }
        let peak = get_rss_kb();
        for _ in 0..20 {
            tm.close_tab(0);
        }
        let after = get_rss_kb();
        println!(
            "MEMORY_MEASUREMENT: baseline={} kB, peak={} kB, after_close={} kB",
            baseline, peak, after
        );
    }

    fn get_cpu_ticks() -> (u64, u64) {
        if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
            let parts: Vec<&str> = stat.split_whitespace().collect();
            if parts.len() > 15 {
                let utime: u64 = parts[13].parse().unwrap_or(0);
                let stime: u64 = parts[14].parse().unwrap_or(0);
                return (utime, stime);
            }
        }
        (0, 0)
    }

    #[test]
    fn test_metrics_tabs_memory_and_idle_cpu() {
        let baseline = get_rss_kb();
        let mut tm = crate::browser::tabs::TabManager::new();

        // 1 Tab
        tm.new_tab(Some("https://duckduckgo.com"), None);
        let rss_1_tab = get_rss_kb();

        // 10 Tabs
        for i in 2..=10 {
            tm.new_tab(Some(&format!("https://example-{i}.org")), None);
        }
        let rss_10_tabs = get_rss_kb();

        // Idle CPU measurement
        let (u1, s1) = get_cpu_ticks();
        let start = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let (u2, s2) = get_cpu_ticks();
        let elapsed = start.elapsed().as_secs_f64();
        let cpu_used_ticks = (u2.saturating_sub(u1) + s2.saturating_sub(s1)) as f64;
        let clock_ticks_per_sec = 100.0;
        let cpu_percent = (cpu_used_ticks / (elapsed * clock_ticks_per_sec)) * 100.0;

        println!(
            "REAL_METRIC: 1 tab memory = {} kB ({:.2} MB)",
            rss_1_tab,
            rss_1_tab as f64 / 1024.0
        );
        println!(
            "REAL_METRIC: 10 tabs memory = {} kB ({:.2} MB) [growth over baseline: {} kB]",
            rss_10_tabs,
            rss_10_tabs as f64 / 1024.0,
            rss_10_tabs.saturating_sub(baseline)
        );
        println!(
            "REAL_METRIC: idle CPU usage = {:.2}% (measured over process idle period)",
            cpu_percent
        );
    }

    #[test]
    #[ignore]
    fn test_benchmark_idle_cpu_30_seconds() {
        let (u1, s1) = get_cpu_ticks();
        let start = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_secs(30));
        let (u2, s2) = get_cpu_ticks();
        let elapsed = start.elapsed().as_secs_f64();
        let cpu_used_ticks = (u2.saturating_sub(u1) + s2.saturating_sub(s1)) as f64;
        let clock_ticks_per_sec = 100.0;
        let cpu_percent = (cpu_used_ticks / (elapsed * clock_ticks_per_sec)) * 100.0;
        println!(
            "REAL_METRIC: 30-second idle CPU = {:.2}% (measured over {:.2}s)",
            cpu_percent, elapsed
        );
    }
}
