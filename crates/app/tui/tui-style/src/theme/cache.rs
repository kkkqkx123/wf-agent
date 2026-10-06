//! Theme cache and user configuration file management.

use std::path::PathBuf;

use super::data::{Theme, ThemeOverrides, ThemeSource};

/// Cache file path: `$XDG_CACHE_HOME/wf-cli/theme.json` (fallback
/// `$HOME/.cache/wf-cli/theme.json`). `None` when no home is discoverable.
pub fn theme_cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("wf-cli").join("theme.json"))
}

/// Persist a successfully probed theme (best effort; the `source` field is
/// serde-skipped and always loads back as [`ThemeSource::Cached`]).
pub fn save_theme_cache(theme: &Theme) {
    let Some(path) = theme_cache_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let Ok(payload) = serde_json::to_vec_pretty(theme) else {
        return;
    };
    let _ = std::fs::write(path, payload);
}

/// Load the last-known-good theme, marked [`ThemeSource::Cached`].
pub fn load_theme_cache() -> Option<Theme> {
    let path = theme_cache_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let mut theme: Theme = serde_json::from_str(&raw).ok()?;
    theme.source = ThemeSource::Cached;
    Some(theme)
}

/// User theme file path: `$XDG_CONFIG_HOME/wf-cli/theme.json` (fallback
/// `$HOME/.config/wf-cli/theme.json`). `None` when no home is discoverable.
pub fn theme_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("wf-cli").join("theme.json"))
}

/// Load the explicit user theme file, marked [`ThemeSource::File`]. Takes
/// priority over probing: a user edits this file and sends SIGUSR2 to switch
/// themes even when the terminal never answers OSC color queries. A missing
/// or malformed file is ignored and the probe / cache chain still applies.
pub fn load_theme_file() -> Option<Theme> {
    let path = theme_config_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let mut theme: Theme = serde_json::from_str(&raw).ok()?;
    theme.source = ThemeSource::File;
    Some(theme)
}

/// Theme overrides file path: `$XDG_CONFIG_HOME/wf-cli/theme-overrides.json` (fallback
/// `$HOME/.config/wf-cli/theme-overrides.json`). `None` when no home is discoverable.
pub fn theme_overrides_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("wf-cli").join("theme-overrides.json"))
}

/// Load theme overrides from the user's config directory.
pub fn load_theme_overrides() -> Option<ThemeOverrides> {
    let path = theme_overrides_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}
