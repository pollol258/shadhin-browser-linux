//! Configuration management with TOML persistence
//!
//! Stores user preferences in `~/.config/shadhin-browser/config.toml`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SidebarState {
    #[default]
    Expanded,
    Compact,
    Hidden,
}

impl SidebarState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Expanded => "expanded",
            Self::Compact => "compact",
            Self::Hidden => "hidden",
        }
    }
}

impl std::str::FromStr for SidebarState {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().as_str() {
            "compact" => Self::Compact,
            "hidden" => Self::Hidden,
            _ => Self::Expanded, // Corrupt/missing/unknown falls back cleanly to Expanded
        })
    }
}

impl std::fmt::Display for SidebarState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Pure function to calculate the effective sidebar state based on window width and user preference.
/// Responsive rules:
/// - If window width < 650px: automatically collapse to Hidden.
/// - If window width < 900px: collapse Expanded to Compact; keep Compact as Compact, Hidden as Hidden.
/// - If window width >= 900px: restores the saved user preference.
pub fn compute_effective_sidebar_state(window_width: i32, user_pref: SidebarState) -> SidebarState {
    if window_width < 650 {
        SidebarState::Hidden
    } else if window_width < 900 {
        match user_pref {
            SidebarState::Expanded => SidebarState::Compact,
            other => other,
        }
    } else {
        user_pref
    }
}

fn default_accent() -> String {
    "#3584e4".to_string()
}

fn default_color_scheme() -> String {
    "system".to_string()
}

fn default_tab_layout() -> String {
    "top".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub homepage: String,
    pub search_engine_url: String,
    pub force_dark_theme: bool,
    pub startup_behavior: String, // "open_new_tab" or "continue_previous_session"
    pub download_dir: String,
    pub wallpaper_path: Option<String>,
    #[serde(default)]
    pub sidebar_state: SidebarState,
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    #[serde(default = "default_tab_layout")]
    pub tab_layout: String,
    pub privacy: PrivacyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    pub shield_enabled: bool,
    pub block_trackers: bool,
    pub block_ads: bool,
    pub block_fingerprinting: bool,
    pub https_only: bool,
    #[serde(default = "default_https_mode")]
    pub https_mode: String,
    pub block_third_party_cookies: bool,
    pub enable_javascript: bool,
}

fn default_https_mode() -> String {
    "prefer".to_string()
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            shield_enabled: true,
            block_trackers: true,
            block_ads: true,
            block_fingerprinting: true,
            https_only: true,
            https_mode: default_https_mode(),
            block_third_party_cookies: true,
            enable_javascript: true,
        }
    }
}

impl PrivacyConfig {
    pub fn resolved_https_mode(&self) -> crate::privacy::https_only::HttpsMode {
        if !self.https_only {
            crate::privacy::https_only::HttpsMode::Off
        } else {
            crate::privacy::https_only::HttpsMode::from_str_opt(&self.https_mode)
        }
    }
}

impl Default for BrowserConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let default_download = PathBuf::from(home).join("Downloads");
        Self {
            homepage: "about:newtab".to_string(),
            search_engine_url: "https://duckduckgo.com/?q=%s".to_string(),
            force_dark_theme: true,
            startup_behavior: "open_new_tab".to_string(),
            download_dir: default_download.to_string_lossy().to_string(),
            wallpaper_path: None,
            sidebar_state: SidebarState::default(),
            color_scheme: default_color_scheme(),
            accent_color: default_accent(),
            tab_layout: default_tab_layout(),
            privacy: PrivacyConfig::default(),
        }
    }
}

impl BrowserConfig {
    /// Returns the standard XDG configuration file path
    pub fn config_path() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".config")
            });
        base.join("shadhin-browser").join("config.toml")
    }

    /// Loads configuration from a specific path, backing up corrupt files and falling back to defaults
    pub fn load_from_path(path: &std::path::Path) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(parent) = path.parent()
                && let Ok(mut perms) = std::fs::metadata(parent).map(|m| m.permissions())
                && perms.mode() & 0o777 != 0o700
            {
                perms.set_mode(0o700);
                let _ = std::fs::set_permissions(parent, perms);
            }
            if let Ok(mut perms) = std::fs::metadata(path).map(|m| m.permissions())
                && perms.mode() & 0o777 != 0o600
            {
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(path, perms);
            }
        }

        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(path) {
            Ok(content) => match toml::from_str::<Self>(&content) {
                Ok(cfg) => cfg,
                Err(err) => {
                    tracing::warn!(
                        "Corrupt configuration at {:?}: {}. Backing up to config.toml.bak and restoring defaults.",
                        path,
                        err
                    );
                    let bak_path = path.with_extension("toml.bak");
                    let _ = std::fs::copy(path, bak_path);
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!(
                    "Could not read config at {:?}: {}. Using defaults.",
                    path,
                    e
                );
                Self::default()
            }
        }
    }

    /// Loads the configuration from disk, falling back to defaults if not found
    pub fn load() -> Self {
        Self::load_from_path(&Self::config_path())
    }

    /// Saves the current configuration to a specific path
    pub fn save_to_path(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                use std::os::unix::fs::PermissionsExt;
                if !parent.exists() {
                    let mut builder = std::fs::DirBuilder::new();
                    builder.recursive(true);
                    builder.mode(0o700);
                    let _ = builder.create(parent);
                } else if let Ok(mut perms) = std::fs::metadata(parent).map(|m| m.permissions()) {
                    perms.set_mode(0o700);
                    let _ = std::fs::set_permissions(parent, perms);
                }
            }
            #[cfg(not(unix))]
            {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(path, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perms) = std::fs::metadata(path).map(|m| m.permissions()) {
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(path, perms);
            }
        }
        Ok(())
    }

    /// Saves the current configuration to disk at standard XDG path
    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_to_path(&Self::config_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = BrowserConfig::default();
        assert_eq!(cfg.homepage, "about:newtab");
        assert!(cfg.force_dark_theme);
        assert!(cfg.privacy.shield_enabled);
        assert!(cfg.privacy.https_only);
        assert_eq!(cfg.privacy.https_mode, "prefer");
        assert_eq!(
            cfg.privacy.resolved_https_mode(),
            crate::privacy::https_only::HttpsMode::Prefer
        );
    }

    #[test]
    fn test_https_mode_resolution() {
        let mut priv_cfg = PrivacyConfig::default();
        assert_eq!(
            priv_cfg.resolved_https_mode(),
            crate::privacy::https_only::HttpsMode::Prefer
        );

        priv_cfg.https_mode = "strict".to_string();
        assert_eq!(
            priv_cfg.resolved_https_mode(),
            crate::privacy::https_only::HttpsMode::Strict
        );

        priv_cfg.https_mode = "off".to_string();
        assert_eq!(
            priv_cfg.resolved_https_mode(),
            crate::privacy::https_only::HttpsMode::Off
        );

        // When https_only is false, resolved_https_mode is always Off regardless of https_mode string
        priv_cfg.https_only = false;
        priv_cfg.https_mode = "strict".to_string();
        assert_eq!(
            priv_cfg.resolved_https_mode(),
            crate::privacy::https_only::HttpsMode::Off
        );
    }

    #[test]
    fn test_config_serialization() {
        let cfg = BrowserConfig::default();
        let toml_str = toml::to_string(&cfg).expect("Serialization failed");
        assert!(toml_str.contains("homepage = \"about:newtab\""));

        let deserialized: BrowserConfig =
            toml::from_str(&toml_str).expect("Deserialization failed");
        assert_eq!(deserialized.homepage, cfg.homepage);
        assert_eq!(deserialized.force_dark_theme, cfg.force_dark_theme);
    }

    #[test]
    fn test_config_disk_persistence() {
        let temp_dir = std::env::temp_dir().join("shadhin_test_config");
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_path = temp_dir.join("test_config.toml");

        let cfg = BrowserConfig {
            homepage: "https://example.com".to_string(),
            ..Default::default()
        };
        cfg.save_to_path(&config_path).expect("Save failed");

        let loaded = BrowserConfig::load_from_path(&config_path);
        assert_eq!(loaded.homepage, "https://example.com");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_corrupt_config_fallback_and_backup() {
        let temp_dir = std::env::temp_dir().join("shadhin_test_corrupt_config");
        let _ = std::fs::create_dir_all(&temp_dir);
        let path = temp_dir.join("config.toml");

        // Write invalid syntax
        std::fs::write(&path, b"homepage = [invalid toml").unwrap();

        let cfg = BrowserConfig::load_from_path(&path);
        // Falls back to default cleanly
        assert_eq!(cfg.homepage, "about:newtab");

        // Backup file was created
        let bak = temp_dir.join("config.toml.bak");
        assert!(bak.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sidebar_state_parsing_and_fallback() {
        use std::str::FromStr;
        assert_eq!(
            SidebarState::from_str("expanded").unwrap(),
            SidebarState::Expanded
        );
        assert_eq!(
            SidebarState::from_str("compact").unwrap(),
            SidebarState::Compact
        );
        assert_eq!(
            SidebarState::from_str("hidden").unwrap(),
            SidebarState::Hidden
        );
        // Case-insensitive and whitespace-tolerant
        assert_eq!(
            SidebarState::from_str("  COMPACT  ").unwrap(),
            SidebarState::Compact
        );
        // Corrupt / unknown string falls back cleanly to Expanded
        assert_eq!(
            SidebarState::from_str("invalid_state").unwrap(),
            SidebarState::Expanded
        );
        assert_eq!(SidebarState::from_str("").unwrap(), SidebarState::Expanded);
    }

    #[test]
    fn test_sidebar_state_serialization() {
        let cfg = BrowserConfig {
            sidebar_state: SidebarState::Compact,
            ..Default::default()
        };

        let toml_str = toml::to_string(&cfg).expect("Serialize config");
        assert!(toml_str.contains("sidebar_state = \"compact\""));

        let loaded: BrowserConfig = toml::from_str(&toml_str).expect("Deserialize config");
        assert_eq!(loaded.sidebar_state, SidebarState::Compact);

        // Missing sidebar_state in TOML falls back to Expanded default
        let old_toml = r#"
            homepage = "about:newtab"
            search_engine_url = "https://duckduckgo.com/?q=%s"
            force_dark_theme = true
            startup_behavior = "open_new_tab"
            download_dir = "/tmp"
            [privacy]
            shield_enabled = true
            block_trackers = true
            block_ads = true
            block_fingerprinting = true
            https_only = true
            block_third_party_cookies = true
            enable_javascript = true
        "#;
        let loaded_old: BrowserConfig = toml::from_str(old_toml).expect("Deserialize old config");
        assert_eq!(loaded_old.sidebar_state, SidebarState::Expanded);
    }

    #[test]
    fn test_responsive_sidebar_state_calculation() {
        // Window width >= 900 preserves user preference
        assert_eq!(
            compute_effective_sidebar_state(1200, SidebarState::Expanded),
            SidebarState::Expanded
        );
        assert_eq!(
            compute_effective_sidebar_state(900, SidebarState::Expanded),
            SidebarState::Expanded
        );
        assert_eq!(
            compute_effective_sidebar_state(900, SidebarState::Compact),
            SidebarState::Compact
        );
        assert_eq!(
            compute_effective_sidebar_state(900, SidebarState::Hidden),
            SidebarState::Hidden
        );

        // Window width between 650 and 899:
        // Expanded collapses to Compact
        assert_eq!(
            compute_effective_sidebar_state(899, SidebarState::Expanded),
            SidebarState::Compact
        );
        assert_eq!(
            compute_effective_sidebar_state(750, SidebarState::Expanded),
            SidebarState::Compact
        );
        assert_eq!(
            compute_effective_sidebar_state(650, SidebarState::Expanded),
            SidebarState::Compact
        );
        // Already Compact remains Compact
        assert_eq!(
            compute_effective_sidebar_state(750, SidebarState::Compact),
            SidebarState::Compact
        );
        // Hidden remains Hidden
        assert_eq!(
            compute_effective_sidebar_state(750, SidebarState::Hidden),
            SidebarState::Hidden
        );

        // Window width < 650: collapses to Hidden regardless of preference
        assert_eq!(
            compute_effective_sidebar_state(649, SidebarState::Expanded),
            SidebarState::Hidden
        );
        assert_eq!(
            compute_effective_sidebar_state(649, SidebarState::Compact),
            SidebarState::Hidden
        );
        assert_eq!(
            compute_effective_sidebar_state(500, SidebarState::Hidden),
            SidebarState::Hidden
        );
        assert_eq!(
            compute_effective_sidebar_state(320, SidebarState::Expanded),
            SidebarState::Hidden
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_config_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir =
            std::env::temp_dir().join(format!("shadhin_cfg_test_{}", std::process::id()));
        let cfg_dir = temp_dir.join("shadhin-browser");
        let cfg_file = cfg_dir.join("config.toml");

        let cfg = BrowserConfig::default();
        cfg.save_to_path(&cfg_file).unwrap();

        let dir_perms = std::fs::metadata(&cfg_dir).unwrap().permissions();
        assert_eq!(dir_perms.mode() & 0o777, 0o700);

        let file_perms = std::fs::metadata(&cfg_file).unwrap().permissions();
        assert_eq!(file_perms.mode() & 0o777, 0o600);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
