# Shadhin Browser (স্বাধীন ব্রাউজার)
**Fast · Private · Open**

A modern, production-oriented native Linux web browser built with Rust, GTK4, Libadwaita, and WebKitGTK 6.0.

Application ID: `com.shadhin.browser`

---

## 1. System Dependencies by Distribution

Shadhin Browser requires **WebKitGTK 6.0** (WebKit 2.40+), **GTK 4.10+**, and **Libadwaita 1.6+** (for AdwAlertDialog and AdwPreferencesDialog support).

### Ubuntu / Debian
> **Minimum Supported Releases:**
> - Ubuntu 23.04 (Lunar) / Ubuntu 24.04 LTS (Noble Numbat) or newer
> - Debian 12 (Bookworm) or Debian 13 (Trixie)
> *(Ubuntu 22.04 LTS shipped only WebKitGTK 4.0/4.1 and does NOT provide WebKitGTK 6.0 dev packages)*

```bash
sudo apt update
sudo apt install -y \
    build-essential \
    pkg-config \
    libgtk-4-dev \
    libadwaita-1-dev \
    libwebkitgtk-6.0-dev \
    libsqlite3-dev \
    libglib2.0-dev
```

### Fedora (Fedora 38+)
```bash
sudo dnf install -y \
    gcc \
    pkgconf-pkg-config \
    gtk4-devel \
    libadwaita-devel \
    webkitgtk6.0-devel \
    sqlite-devel \
    glib2-devel
```

### Arch Linux / Manjaro
```bash
sudo pacman -S --needed \
    base-devel \
    pkgconf \
    gtk4 \
    libadwaita \
    webkitgtk-6.0 \
    sqlite
```

### openSUSE (Tumbleweed / Leap 15.6+)
```bash
sudo zypper install -y \
    patterns-devel-base-devel_basis \
    pkg-config \
    gtk4-devel \
    libadwaita-devel \
    webkit2gtk-6_0-devel \
    sqlite3-devel
```

---

## 2. Running on Windows via WSL2 (with WSLg)

Windows does not natively provide GTK4 and WebKitGTK 6.0 C libraries. To build and run Shadhin Browser on a Windows workstation, use **WSL2** (Windows Subsystem for Linux 2) with **WSLg** (built-in graphical subsystem on Windows 10/11).

### Step 1: Install Ubuntu 24.04 LTS on WSL2
In PowerShell (as Administrator or standard terminal):
```powershell
wsl --install -d Ubuntu-24.04
```
*(If WSL is already installed, launch it with: `wsl -d Ubuntu-24.04`)*

### Step 2: Install All Required Development Packages in Ubuntu
Inside your Ubuntu terminal:
```bash
sudo apt update
sudo apt install -y \
    build-essential \
    pkg-config \
    libgtk-4-dev \
    libadwaita-1-dev \
    libwebkitgtk-6.0-dev \
    libsqlite3-dev \
    libglib2.0-dev \
    curl \
    git
```

### Step 3: Install the Rust Toolchain in WSL2
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

### Step 4: Access the Project and Build
Navigate to the project directory located on the Windows drive:
```bash
cd /mnt/c/dev/shadhin_browser
```
*(Tip: For significantly faster build and compilation times, you can clone or copy the repo directly into the WSL native filesystem, e.g., `~/shadhin_browser`)*

Compile and launch:
```bash
cargo build
cargo run
```
WSLg automatically maps the Wayland/X11 display (`WAYLAND_DISPLAY=wayland-0` / `DISPLAY=:0`), rendering the native GTK4/Libadwaita window seamlessly on your Windows desktop.

### Running on WSL2/WSLg
Depending on your Windows GPU driver and WSLg Mesa/Zink virtual driver configuration, WebKitGTK 6.0 may log Mesa DRI/Zink warnings (`MESA: error: ZINK: failed to choose pdev`) or struggle with DMABUF buffer sharing between the WebProcess and UI process.

To resolve blank windows or graphics glitches without compromising security:
```bash
# Recommended workaround for WSL2/WSLg: disables DMABUF renderer in WebKitGTK
WEBKIT_DISABLE_DMABUF_RENDERER=1 cargo run

# Secondary fallback if accelerated compositing encounters issues:
WEBKIT_DISABLE_COMPOSITING_MODE=1 cargo run
```
> **Security Confirmation**: `WEBKIT_DISABLE_DMABUF_RENDERER=1` only alters the graphical surface transfer protocol (switching from GPU DMABUF sharing to POSIX shared memory buffers). It does **NOT** disable WebKit's process sandbox (bubblewrap), content filters, network isolation, or any security feature. Never disable WebKit's sandbox (`WEBKIT_FORCE_SANDBOX=0` is strictly forbidden). All security features and process isolation boundaries remain fully intact.

---

## 3. Rust Toolchain Installation (Native Linux)

Rust 1.80+ (stable) is recommended:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

---

## 4. Building and Running

### Development Build
```bash
cargo build
```

### Run Shadhin Browser
```bash
cargo run

# On WSL2/WSLg with DMABUF fallback:
WEBKIT_DISABLE_DMABUF_RENDERER=1 cargo run

# With live CSS reload during theme development:
SHADHIN_DEV_CSS=1 cargo run
```

### Keyboard Shortcuts
- `Ctrl+T`: New tab
- `Ctrl+W`: Close tab
- `Ctrl+Shift+T`: Reopen closed tab
- `Ctrl+L` / `F6`: Focus address bar
- `Ctrl+R` / `F5`: Reload page
- `Ctrl+Shift+R`: Hard reload (bypass cache)
- `Ctrl+Plus` / `Ctrl+=`: Zoom in
- `Ctrl+Minus`: Zoom out
- `Ctrl+0`: Reset zoom (100%)
- `Ctrl+F`: Find in page (`Escape` to close)
- `Ctrl+D`: Bookmark active page
- `Ctrl+B` / `F9`: Toggle sidebar (Expanded/Compact <-> Hidden)
- `Ctrl+Shift+B`: Toggle bookmarks bar
- `Ctrl+Shift+O`: Open Bookmarks panel
- `Ctrl+H`: Open History panel
- `Ctrl+J`: Open Downloads panel
- `Ctrl+,`: Preferences / Settings dialog
- `Ctrl+Shift+N`: New Private Browsing window
- `Ctrl+Shift+Delete`: Clear Browsing Data dialog
- `Alt+Home`: Navigate to homepage

### Privacy & Security Highlights (Phase 5)
- **Native WebKit Content Filtering**: Bundled EasyList & EasyPrivacy rule sets compiled to WebKit bytecode via `WebKitUserContentFilterStore`.
- **Query Parameter Stripper**: Strips 21 telemetry parameters (`utm_*`, `fbclid`, `gclid`, etc.) during navigation.
- **HTTPS-Only Mode**: Automatically attempts HTTPS for all navigations; falls back to an informative warning interstitial if HTTPS is unavailable, with RFC1918/localhost exemptions.
- **Cookie & ITP Isolation**: Third-party cookies blocked by default; WebKit Intelligent Tracking Prevention (ITP) active.
- **Site Permissions**: Granular, non-modal permission prompt bars for Camera, Microphone, Location, and Notifications with persistent SQLite origin decisions.
- **Private Browsing Windows**: Ephemeral, in-memory `WebKitNetworkSession` instances with zero disk persistence of history, cookies, or cache.
- **Zero Telemetry**: No background tracking pings, no telemetry, no metrics fabrication.

### Command Line Options

```text
Usage: shadhin-browser [OPTIONS] [URL]

Options:
  -v, --version          Print version information and exit
  -h, --help             Print help message and exit
      --new-window       Open in a new standard window
  -p, --private-window   Open in a new private browsing window
      --profile-dir PATH Use custom profile directory (requires absolute path)

Positional Arguments:
  URL                    Web address or search terms to open
```

### Packaging & Distribution

Pre-built packages are generated into the `dist/` directory using our automated packaging pipeline:

```bash
# Build all available packages (.deb, Flatpak, .rpm) and compute SHA256 checksums
./scripts/package-all.sh

# Or build individual package formats:
./scripts/package-deb.sh      # Builds Debian/Ubuntu .deb archive
./scripts/package-flatpak.sh  # Builds Flatpak standalone bundle (.flatpak)
./scripts/package-rpm.sh      # Builds Fedora/RHEL .rpm package
```

Refer to:
- [Installation Guide](file:///home/ziaur/dev/shadhin_browser/docs/INSTALL.md)
- [Packaging Architecture](file:///home/ziaur/dev/shadhin_browser/docs/PACKAGING.md)
- [Flatpak Sandbox Guide](file:///home/ziaur/dev/shadhin_browser/docs/FLATPAK.md)
- [Security Tests & Verification Matrix](file:///home/ziaur/dev/shadhin_browser/docs/SECURITY_TESTS.md)
- [Manual Test Checklist](file:///home/ziaur/dev/shadhin_browser/docs/MANUAL_TEST_CHECKLIST.md)

---

## 5. Testing & Code Quality

### Format check
```bash
cargo fmt --all -- --check
```

### Linter (Clippy)
```bash
cargo clippy --all-targets -- -D warnings
```

### Unit and Integration Tests
```bash
cargo test
```

### Performance & Regression Benchmarks
```bash
# Run regression smoke test suite
cargo test --test bench_smoke_test

# Run database query plan & latency benchmarks (50k rows)
python3 scripts/bench/measure_db.py --optimize

# Run startup latency benchmarks
python3 scripts/bench/measure_startup.py --bin target/release/shadhin-browser --runs 3
```

---

## 6. Architecture Overview

```
shadhin-browser/
├── Cargo.toml          # Cargo manifest with synchronized gtk-rs ecosystem
├── SPEC.md             # Project specification
├── LICENSE             # GPL-3.0-or-later license
├── src/
│   ├── main.rs         # Entry point and tracing setup
│   ├── app.rs          # AdwApplication & main window activation
│   ├── config.rs       # TOML configuration & defaults
│   ├── browser/        # Webview, navigation, tabs, downloads, permissions
│   ├── privacy/        # Content filters, tracking protection, cookies, stats
│   ├── ui/             # Titlebar, toolbar, sidebar, tabs, New Tab page, privacy popover
│   ├── storage/        # SQLite database (history, bookmarks, shortcuts, workspaces)
│   └── utils/          # Logging and shared helpers
├── resources/
│   ├── reference/      # Visual target mockup
│   ├── icons/          # Symbolic SVG icons
│   └── styles/         # GTK4 CSS glassmorphic stylesheets
├── data/               # Desktop entry & AppStream metainfo
└── docs/               # Threat model, limitations, performance benchmarks
```
