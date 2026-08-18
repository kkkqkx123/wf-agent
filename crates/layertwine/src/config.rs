//! Configuration module for Layertwine, loaded from TOML files (TOML 1.0 / 1.1 via `toml` v0.8).
//!
//! Config is resolved from hardcoded defaults, then layered with
//! TOML files from multiple locations (low → high priority):
//!
//! 1. Hardcoded defaults (lowest)
//! 2. `~/.config/layertwine.toml` — user-global override
//! 3. `<binary-dir>/layertwine.toml` — per-installation override
//! 4. `<db-dir>/layertwine.toml` — per-repository override (highest)
//!
//! Each layer only needs to specify the fields it wants to override;
//! missing fields fall through to the layer below.
//!
//! See `.layertwine/layertwine.example.toml` for all available options.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::LayertwineError;

/// Recursively merge `overlay` into `base` at the `toml::Value` level.
/// Table fields present in `overlay` overwrite those in `base`; sub-tables
/// are merged recursively so partial overrides work at any depth.
fn merge_values(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_t), toml::Value::Table(overlay_t)) => {
            for (key, val) in overlay_t {
                if base_t.contains_key(&key) {
                    merge_values(&mut base_t[&key], val);
                } else {
                    base_t.insert(key, val);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

/// Default maintenance config — safe for most workloads.
impl Default for MaintenanceConfig {
    fn default() -> Self {
        MaintenanceConfig {
            auto_vacuum_mode: AutoVacuumMode::Incremental,
            freelist_threshold: 0.10,
            max_vacuum_pages: 1000,
            vacuum_full: false,
            journal_size_limit: 67_108_864, // 64 MB
            wal_autocheckpoint: 1000,
        }
    }
}

/// Auto-vacuum mode selector.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoVacuumMode {
    #[default]
    #[serde(alias = "incremental")]
    Incremental,
    #[serde(alias = "full")]
    Full,
    #[serde(alias = "none")]
    None,
}

impl AutoVacuumMode {
    pub fn as_pragma(&self) -> &'static str {
        match self {
            AutoVacuumMode::Incremental => "INCREMENTAL",
            AutoVacuumMode::Full => "FULL",
            AutoVacuumMode::None => "NONE",
        }
    }
}

/// Layertwine top-level configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LayertwineConfig {
    /// Path to the SQLite database file.
    #[serde(default = "default_db_path")]
    pub db_path: String,

    /// Maintenance / compaction settings.
    #[serde(default)]
    pub maintenance: MaintenanceConfig,
}

fn default_db_path() -> String {
    ".layertwine/layertwine.db".to_string()
}

impl Default for LayertwineConfig {
    fn default() -> Self {
        LayertwineConfig {
            db_path: default_db_path(),
            maintenance: MaintenanceConfig::default(),
        }
    }
}

/// Maintenance / compaction configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MaintenanceConfig {
    /// Vacuum mode to apply at database init: "incremental", "full", or "none".
    #[serde(default)]
    pub auto_vacuum_mode: AutoVacuumMode,

    /// Freelist ratio threshold (0.0 – 1.0). When freelist/page_count exceeds this,
    /// incremental_vacuum is triggered.
    #[serde(default = "default_freelist_threshold")]
    pub freelist_threshold: f64,

    /// Maximum number of pages to reclaim per incremental_vacuum call.
    #[serde(default = "default_max_vacuum_pages")]
    pub max_vacuum_pages: i64,

    /// Opt-in: use full VACUUM (exclusive lock, rebuilds entire file).
    /// WARNING: blocks all other connections during execution.
    #[serde(default)]
    pub vacuum_full: bool,

    /// WAL journal size limit in bytes. 0 = unlimited.
    #[serde(default = "default_journal_size_limit")]
    pub journal_size_limit: i64,

    /// WAL autocheckpoint page count threshold. 0 = disable automatic checkpoint.
    #[serde(default = "default_wal_autocheckpoint")]
    pub wal_autocheckpoint: i32,
}

fn default_freelist_threshold() -> f64 {
    0.10
}
fn default_max_vacuum_pages() -> i64 {
    1000
}
fn default_journal_size_limit() -> i64 {
    67_108_864
}
fn default_wal_autocheckpoint() -> i32 {
    1000
}

impl LayertwineConfig {
    /// Load configuration from a single TOML file.
    ///
    /// Returns `Ok(None)` if the file does not exist.
    pub fn from_file(path: &Path) -> Result<Option<Self>, LayertwineError> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| LayertwineError::General(format!("failed to read config: {}", e)))?;
        let config: LayertwineConfig = toml::from_str(&content)
            .map_err(|e| LayertwineError::General(format!("config parse error: {}", e)))?;
        Ok(Some(config))
    }

    /// Resolve the search paths in order (low → high priority).
    ///
    /// - `~/.config/layertwine.toml`
    /// - `<binary-dir>/layertwine.toml`
    /// - `<db-dir>/layertwine.toml`
    pub fn config_paths(db_dir: &Path) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = Vec::new();

        // 1. User-global: ~/.config/layertwine.toml
        if let Some(home) = home_dir() {
            paths.push(home.join(".config").join("layertwine.toml"));
        }

        // 2. Next to the binary
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                paths.push(exe_dir.join("layertwine.toml"));
            }
        }

        // 3. Database directory (highest local priority)
        paths.push(db_dir.join("layertwine.toml"));

        paths
    }

    /// Load configuration by layering from hardcoded defaults, then overlaying
    /// TOML files from low to high priority (see [`config_paths`]).
    ///
    /// - Lower-priority files only set fields they define; everything else
    ///   falls through to the layer below.
    /// - Files that don't exist are silently skipped.
    pub fn load_with_priority(db_dir: &Path) -> Result<Self, LayertwineError> {
        let defaults_str = toml::to_string(&LayertwineConfig::default())
            .map_err(|e| LayertwineError::General(format!("serialize defaults: {}", e)))?;
        let mut base: toml::Value = toml::from_str(&defaults_str)
            .map_err(|e| LayertwineError::General(format!("parse defaults: {}", e)))?;

        for path in Self::config_paths(db_dir) {
            if !path.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .map_err(|e| LayertwineError::General(format!("read {}: {}", path.display(), e)))?;
            let overlay: toml::Value = toml::from_str(&content).map_err(|e| {
                LayertwineError::General(format!("parse {}: {}", path.display(), e))
            })?;
            merge_values(&mut base, overlay);
        }

        let out = toml::to_string_pretty(&base)
            .map_err(|e| LayertwineError::General(format!("serialize merged config: {}", e)))?;
        let config: LayertwineConfig = toml::from_str(&out)
            .map_err(|e| LayertwineError::General(format!("deserialize merged config: {}", e)))?;
        Ok(config)
    }
}

/// Returns the user's home directory.
fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// Runtime options for a single `compact()` call, derived from config or overrides.
#[derive(Debug, Clone)]
pub struct CompactOptions {
    /// Freelist ratio threshold.
    pub freelist_threshold: f64,
    /// Max pages per incremental_vacuum call.
    pub max_vacuum_pages: i64,
    /// Use full VACUUM instead of incremental.
    pub vacuum_full: bool,
}

impl From<&MaintenanceConfig> for CompactOptions {
    fn from(cfg: &MaintenanceConfig) -> Self {
        CompactOptions {
            freelist_threshold: cfg.freelist_threshold,
            max_vacuum_pages: cfg.max_vacuum_pages,
            vacuum_full: cfg.vacuum_full,
        }
    }
}

impl Default for CompactOptions {
    fn default() -> Self {
        CompactOptions {
            freelist_threshold: 0.10,
            max_vacuum_pages: 1000,
            vacuum_full: false,
        }
    }
}

/// Result of a compact operation.
#[derive(Debug, Clone)]
pub struct CompactReport {
    /// Whether WAL checkpoint was performed.
    pub wal_checkpointed: bool,
    /// Free pages before compaction.
    pub freelist_before: i64,
    /// Total pages before compaction.
    pub total_pages: i64,
    /// Free pages after compaction.
    pub freelist_after: i64,
    /// Whether vacuum was actually executed.
    pub vacuum_performed: bool,
    /// Summary message.
    pub message: String,
}
