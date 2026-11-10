# Shadhin Browser — AI Handover & Architecture Guide

> **Target Audience**: Future AI assistants (Gemini, Claude, GPT, etc.) and human maintainers.  
> **Purpose**: This document provides the authoritative architectural blueprint, strict design rules, security constraints, and debugging guide for Shadhin Browser. Read this document before making code modifications or investigating bugs.

---

## 1. Project Overview & Philosophy

- **Project Name**: Shadhin Browser (`com.shadhin.browser`)
- **License**: GPL-3.0-or-later
- **Core Technology Stack**:
  - **Language**: Rust (2024 Edition)
  - **UI Toolkit**: GTK4 (`gtk4` crate v0.11, target GTK 4.18) and Libadwaita (`libadwaita` crate v0.9, target Libadwaita 1.7)
  - **Web Engine**: WebKitGTK 6.0 (`webkit6` crate v0.6, target WebKitGTK 2.40+)
  - **Storage**: SQLite 3 with WAL mode (`rusqlite` crate)
- **Target Hardware & Performance Objective**:
  - Optimized specifically for older Linux machines (e.g., Intel Ivy Bridge / Sandy Bridge Gen7 graphics, 8 GB RAM, dual-core CPUs).
  - Target idle resource consumption: **0.00% CPU** and lightweight memory footprint.
- **Strict CPU Architecture Rule**:
  > [!IMPORTANT]
  > **NEVER use `target-cpu=native` or `x86-64-v3` in Cargo profiles or compiler flags.**  
  > The release binary must remain strictly compatible with baseline `x86-64` (no AVX2, no FMA, no BMI2 instructions). Breaking this rule crashes the browser with `SIGILL` (Illegal Instruction) on older hardware.

---

## 2. Architecture & Subsystem Layout

```
shadhin-browser/
├── Cargo.toml                  # Package manifest, release profile, deb/rpm metadata
├── data/                       # Desktop integration (com.shadhin.browser.desktop, icons)
├── docs/                       # Specifications, threat models, limitation disclosures
├── resources/                  # GTK4 CSS stylesheets and symbolic SVGs
│   └── styles/
│       └── shadhin.css         # Glassmorphism styling & Adwaita overrides
├── src/
│   ├── main.rs                 # CLI entry point, logging initialization, single-instance routing
│   ├── app.rs                  # AdwApplication lifecycle & window factory
│   ├── config.rs               # TOML configuration parser, defaults & migration
│   ├── lib.rs                  # Crate root & module exports
│   ├── browser/                # Web engine integration & navigation
│   │   ├── downloads.rs        # Download manager, filename sanitization & temp file cleanup
│   │   ├── navigation.rs       # Omnibox URL classification, search formatting, IDN safety
│   │   ├── permissions.rs      # SQLite-backed site permission state (Camera, Mic, Geo, etc.)
│   │   ├── sandbox.rs          # Bubblewrap & Seccomp child WebProcess sandbox verification
│   │   ├── session.rs          # Isolated NetworkSession workspace coordinator & crash recovery
│   │   ├── tabs.rs             # Tab model, inactive tab discarding, lazy WebView loading
│   │   └── webview.rs          # Hardened WebView builder & adaptive memory pressure tuning
│   ├── privacy/                # Security enforcement & tracking mitigation
│   │   ├── adblock.rs          # EasyList/EasyPrivacy compiler via WebKitUserContentFilterStore
│   │   ├── cookies.rs          # Cookie acceptance policy, ITP, and data clearing
│   │   ├── https_only.rs       # HTTPS-Only upgrade interceptor & loop detector
│   │   ├── passwords.rs        # Desktop Keyring / Secret Service (org.freedesktop.secrets) foundation
│   │   ├── stats.rs            # Verifiable privacy statistics (no synthetic numbers)
│   │   └── tracker.rs          # Strip 21 telemetry query parameters (utm_*, fbclid, etc.)
│   ├── storage/                # SQLite async database subsystem
│   │   ├── mod.rs              # Dedicated background worker thread, WAL mode, PRAGMA optimize
│   │   ├── bookmarks.rs        # Netscape HTML & JSON import/export, bookmark tree CRUD
│   │   ├── downloads.rs        # Download history persistence & interrupted state tracking
│   │   ├── history.rs          # Frecency/visit history persistence & autocomplete queries
│   │   ├── shortcuts.rs        # New tab shortcuts repository
│   │   └── workspaces.rs       # Workspace compartmentalization & session tab persistence
│   ├── ui/                     # Native GTK4 / Libadwaita user interface
│   │   ├── window.rs           # Main application window, TabView controller, event signals
│   │   ├── toolbar.rs          # Headerbar, navigation buttons, downloads/menu popover triggers
│   │   ├── addressbar.rs       # Minimalist address pill, centered unfocused URL, autocomplete
│   │   ├── tabstrip.rs         # Client-side titlebar tab strip with lazy restoration
│   │   ├── sidebar.rs          # Tri-state responsive vertical sidebar (Expanded, Compact, Hidden)
│   │   ├── settings.rs         # AdwPreferencesDialog (General, Appearance, Privacy, About)
│   │   ├── downloads_popover.rs# Fixed-width downloads popover with progress and retry
│   │   ├── privacy_panel.rs    # Shield popover with site-specific toggle overrides
│   │   ├── clear_data.rs       # Multi-category browsing data clearing modal dialog
│   │   └── find_bar.rs         # In-page text search overlay
│   └── utils/                  # Cross-cutting utilities
│       ├── debounce.rs         # Thread-safe GLib timeout debouncer
│       └── net_diagnostics.rs  # TLS and network failure diagnostic records
└── tests/                      # Comprehensive integration, security & regression test suites
```

---

## 3. UI & CSS Rules (Critical GTK4 Constraints)

The GTK4 CSS engine is **not** a standard web browser engine. When modifying any UI component or `resources/styles/shadhin.css`, observe these rules:

1. **Unsupported CSS Features**:
   - **NO `@media` queries**: GTK4 CSS does not support `@media (max-width: ...)` or `@media (prefers-color-scheme)`. Responsive breakpoints must be handled in Rust using `adw::Breakpoint` or `connect_default_width_notify`.
   - **NO `display: flex` or `display: grid`**: Layouts must use native GTK widgets (`gtk4::Box`, `gtk4::Grid`, `adw::Clamp`).
   - **NO `clamp()`, `min()`, or `max()` with viewport units (`vw`, `vh`)**.
2. **Dynamic Sizing in Rust, Never CSS**:
   - Sizing calculations that respond to window resize (e.g., dynamic width of the Address Bar pill) **must be calculated in Rust** inside a window width change signal handler (`toolbar.update_address_pill_width(w)`), not in CSS.
3. **Address Bar (Address Pill) Design**:
   - **Centered Text Unfocused**: When the address bar is unfocused, text is centered (`halign: Center`).
   - **Left-Aligned Focused**: When clicked/focused for editing, text switches to standard left-alignment (`halign: Fill` / `xalign: 0.0`).
   - **Lock & Site Info**: Positioned on the far left inside the pill.
   - **Reload / Stop Button**: Positioned on the far right **inside** the address pill.
4. **Icon Rendering on Intel GPUs (Mesa crocus)**:
   - Always set an explicit integer `pixel_size` (e.g., `16` or `20`) on `gtk4::Image::from_icon_name(...)`.
   - Letting GTK auto-scale icons produces blurry, antialiased artifacts on older Intel Gen7 hardware.
5. **Zero Continuous Animations**:
   - Never use infinite CSS keyframe animations, pulsing glows, or spinning indicators.
   - Continuous animations force redraws every frame, spiking idle CPU and draining battery.
6. **Popover Parenting & Active State**:
   - Popovers must be attached to their parent widgets via `set_popover()` or explicit widget hierarchies.
   - Before a tab or widget is finalized or closed, ensure popovers are popped down or unparented to avoid GTK memory leaks and `"Broken accounting of active state"` warnings.

---

## 4. Security & Privacy Constraints

1. **Real Data Only (No Synthetic Counters)**:
   - Shadhin Browser strictly forbids fabricated or simulated metrics (e.g., fake "1,420 trackers blocked" counters).
   - If WebKit's native content filtering API does not expose the exact number of blocked subresources, display `—` with transparent documentation of WebKitGTK platform boundaries.
   - TLS security status (lock icon, certificate flags) and sandbox warnings must be directly driven by real WebKit engine signals.
2. **WebKit Child Process Sandboxing**:
   - Sandboxing is verified at runtime by examining WebKitWebProcess child PIDs in `src/browser/sandbox.rs`:
     - Checks `/proc/<pid>/status` for `Seccomp: 2`.
     - Checks `/proc/<pid>/mountinfo` for active Bubblewrap (`bwrap`) mounts.
   - **Never disable WebKit's sandbox** (e.g., never pass `--no-sandbox` or disable process isolation).
3. **Default-Deny Site Permissions**:
   - Camera, Microphone, Geolocation, and Notifications default to `Deny` or `Ask`.
   - Stored in `PermissionRepository` in SQLite. Decisions are scoped strictly to individual origins.
4. **Cookie & Session Persistence**:
   - In non-private windows, cookies must survive browser restarts.
   - Configured via `cookie_mgr.set_persistent_storage("cookies.sqlite", CookiePersistentStorage::Sqlite)` in `src/browser/session.rs`.
   - Standard session directories:
     - Data: `~/.local/share/shadhin-browser/sessions/ws_<workspace_id>/`
     - Cache: `~/.cache/shadhin-browser/sessions/ws_<workspace_id>/`
   - Private browsing windows must strictly use `NetworkSession::new_ephemeral()`, leaving zero disk traces.
5. **Zero Unsafe**:
   - Outside external C-FFI callbacks required by GTK/WebKit bindings, the application codebase enforces memory safety without `unsafe` blocks.

---

## 5. Known System Warnings (Harmless / Expected)

When running from the terminal, the following warning messages may appear in the logs. These are known upstream/hardware behaviors and do **not** represent bugs in Shadhin Browser:

| Warning Message | Source | Cause & Action |
| :--- | :--- | :--- |
| `MESA-INTEL: warning: support YUV colorspace...` | Mesa driver | Normal warning emitted by Mesa's `crocus` driver on older Intel Gen7 (Ivy Bridge) GPUs. Completely harmless; do not attempt to suppress. |
| `WebKitWebProcess: WARNING: Could not determine type of stream` | GStreamer | Emitted by GStreamer demuxer when probing video streams on certain CDNs (e.g., YouTube video advertisements). Harmless. |
| `Broken accounting of active state for widget 0x...` | GTK4 | Upstream GTK4 quirk when closing tabs with active toggle/menu states. Mitigated by explicit unparenting and `set_active(false)` handlers. |
| `GTask ... finalized without ever returning` | WebKitGTK / GLib | Occurs during rapid tab destruction if an internal async load operation was cancelled mid-flight. Safe to ignore. |

---

## 6. Build, Test & Packaging Commands

### Development
```bash
# Check code without full link
cargo check

# Format verification
cargo fmt --all -- --check

# Strict Clippy analysis (Must always pass with zero warnings)
cargo clippy --all-targets -- -D warnings
```

### Testing
```bash
# Run all unit and integration tests
cargo test

# Run a specific test suite
cargo test --test security_tests
cargo test --test live_omnibox_targets_test -- --nocapture
```

### Release Binary Build
```bash
# Builds the optimized, stripped ~2.9 MB release binary
cargo build --release

# Binary output location:
# target/release/shadhin-browser
```

### Packaging (.deb and .rpm)
```bash
# Generate Debian package (.deb)
cargo deb

# Output location:
# target/debian/shadhin-browser_<version>-1_amd64.deb

# Generate RPM package (.rpm)
cargo generate-rpm

# Output location:
# target/generate-rpm/shadhin-browser-<version>-1.x86_64.rpm
```

### Release Versioning
- **Current Stable Version**: `v0.1.0`
- **Annotated Tag**: `git tag -a v0.1.0 -m "Shadhin Browser v0.1.0 - First Public Beta"`

---

## 7. How to Debug & Diagnose Issues

1. **Real-Time Terminal Output**:
   Run the release or debug build directly from a terminal:
   ```bash
   RUST_LOG=info cargo run --release
   ```
   For deep diagnostics on network policies, HTTPS upgrades, or permission checks:
   ```bash
   RUST_LOG=debug cargo run
   ```
2. **Inspect Persistent SQLite Database**:
   ```bash
   sqlite3 ~/.local/share/shadhin-browser/shadhin.db
   # Useful tables: history, bookmarks, shortcuts, permissions, downloads, workspaces
   .tables
   SELECT * FROM permissions;
   SELECT * FROM downloads ORDER BY id DESC LIMIT 5;
   ```
3. **Verify Cookie Persistence**:
   Inspect the persistent SQLite cookie database for the default workspace:
   ```bash
   file ~/.local/share/shadhin-browser/sessions/ws_default/cookies.sqlite
   ```
4. **Verify WebProcess Sandbox**:
   Find the PID of a running WebKit child process and inspect its sandbox status:
   ```bash
   pgrep -f WebKitWebProcess
   # Replace <PID> with the actual process ID:
   grep -E "^(Seccomp|Uid|Gid)" /proc/<PID>/status
   grep "bwrap" /proc/<PID>/mountinfo
   ```
   `Seccomp: 2` confirms active seccomp filter confinement. Presence of `bwrap` in mountinfo confirms Bubblewrap unprivileged user namespace isolation.
