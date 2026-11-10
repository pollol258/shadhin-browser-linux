# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-10

### Added
- **Native User Interface & Design**: Built with GTK4 4.18 and Libadwaita 1.7 featuring an adaptive layout with responsive breakpoints (`AdwBreakpoint`), dark theme styling, compact header bar with crisp pixel-aligned 16px/20px integer icons, and non-blocking toast notifications.
- **Tab & Session Management**:
  - Multi-tab management powered by `AdwTabView` with top TabBar and vertical sidebar tab support.
  - Efficient background tab suspension and memory pressure monitoring.
  - Crash recovery lock file (`running.lock`) with single-instance detection.
  - Per-workspace isolation (Default, Work, Personal) with dedicated storage directories (`sessions/ws_<id>/`).
- **Minimalist Address Bar (Address Pill)**:
  - Clean pill design with centered URL text when unfocused and standard left-aligned editing when active.
  - Far-left site info / TLS lock status indicator.
  - Integrated reload/stop button embedded inside the address pill on the far right.
  - Real-time Omnibox autocomplete matching search queries, bookmarks, and browsing history.
- **New Tab Page (`about:newtab`)**:
  - Clean new tab page with customizable shortcut tiles.
  - Shortcut management with static tiles, "Move left" / "Move right" context menu, and add/edit shortcut dialog.
- **Settings Dialog (`AdwPreferencesDialog`)**:
  - Organized preference pages: General, Tabs & Appearance, Privacy & Security, Bookmarks & History, and About.
  - Dedicated "Clear Cached Web Content" action row for instant disk and memory cache clearing without signing out of websites.
  - Dedicated "Offer to save passwords" toggle and desktop keyring status display.
  - Saved site permissions management (Camera, Microphone, Geolocation, Notifications).
- **History & Bookmarks Management**:
  - Asynchronous SQLite repository (`shadhin.db`) with Write-Ahead Logging (WAL).
  - Netscape HTML and JSON import/export for browser bookmarks.
  - Multi-range data clearing dialog (Last Hour, 24 Hours, 7 Days, 4 Weeks, All Time).
- **Downloads Subsystem**:
  - Dedicated downloads popover with fixed non-collapsing layout and clean button spacing.
  - Real-time download progress, transfer speed, and ETA calculation.
  - Filename sanitization, directory traversal protection, and automatic collision evasion (`file (1).ext`).
  - Caution indicators for dangerous executable extensions (`.sh`, `.exe`, `.deb`, `.rpm`, etc.).
  - Automatic cleanup of orphaned partial files (`.crdownload`, `.part`, `.partial`) on startup and upon user cancellation.
- **WebKitGTK 6.0 Integration & Child Process Sandboxing**:
  - Multi-process architecture with unprivileged user namespaces and bubblewrap (`bwrap`) child process sandboxing verification.
  - Hardened security settings: disabled filesystem access from `file:` URLs, media playback gesture requirements, popup prevention, and disabled page caching for reduced WebProcess memory footprint.
  - Ephemeral private browsing windows (`NetworkSession::new_ephemeral()`) with no disk traces.
- **Privacy & Security Enforcement**:
  - Native content filter engine with EasyList and EasyPrivacy rule compilation.
  - Third-party tracking cookie blocking and ITP (Intelligent Tracking Prevention).
  - Persistent SQLite cookie jar configuration (`cookies.sqlite` per workspace) ensuring logins survive restarts.
  - HTTPS-Only Mode with automatic upgrade and safe certificate warning interstitial (strict against TLS bypass).
  - Abstract password manager foundation with Linux Desktop Secret Service (`org.freedesktop.secrets` / `oo7` / `keyring-rs`) integration.
- **Packaging & Release Optimization**:
  - Release profile optimizations (`codegen-units = 1`, `lto = "fat"`, `opt-level = "s"`, `strip = true`) yielding a compact ~2.9 MB release binary.
  - Baseline `x86-64` target tuning ensuring maximum performance and compatibility across older hardware (e.g., Intel Ivy Bridge) without AVX2/BMI2 requirements.
  - Desktop integration files: `.desktop` launcher, AppStream metadata, and multi-resolution icons.

### Security
- Zero `unsafe` code blocks across the browser application codebase (excluding GTK/WebKit C-FFI trampolines).
- File permission enforcement: strict `0700` (`rwx------`) on data, config, and session directories; `0600` (`rw-------`) on database and lock files.
- Refusal to synthesize unmeasurable ad/tracker metrics; transparent display of verifiable metrics.
- Disallowance of untrusted internal scheme execution (`javascript:`, `data:`, `file:`, `shadhin:`) from web content contexts.
