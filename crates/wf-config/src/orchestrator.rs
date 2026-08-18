//! Configuration assembly orchestrator.
//!
//! Provides a single entry point for loading infrastructure configs from
//! project/global directories, applying env var overrides, and producing
//! a fully assembled configuration.
//!
//! Infrastructure presets: when
//! `configs/infrastructure/index.json` exists and a preset name is given,
//! the preset's `files` mapping drives which file backs each config domain.
//! Without an index/preset, fixed default filenames are used (legacy mode).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::env::{apply_env_overrides, env_parse_bool, env_parse_int, EnvMappingBuilder, EnvValue};
use crate::error::{ConfigError, ConfigResult};
use crate::layered;
use crate::preset::{
    find_preset_by_name, load_single_file_preset, resolve_preset_index, INDEX_FILE_NAME,
};
use crate::processor::file_checkpoint::merge_file_checkpoint_with_defaults;
use crate::processor::infrastructure::{
    merge_metrics_with_defaults, merge_output_with_defaults, merge_storage_with_defaults,
    merge_timeout_with_defaults,
};
use crate::processor::presets::transform_presets_config;
use crate::processor::sandbox_global::validate_sandbox_global;
use crate::processor::tools::{
    transform_glob_config, transform_list_files_config, transform_read_file_config, GlobConfig,
    GlobConfigInput, ListFilesConfig, ListFilesConfigInput, ReadFileConfig, ReadFileConfigInput,
};

use wf_types::config::file_checkpoint::FileCheckpointConfig;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::presets::PresetsConfig;
use wf_types::config::storage::{StorageConfig, StorageType};
use wf_types::config::timeout::TimeoutConfig;
use wf_types::script::sandbox::SandboxGlobalConfig;

/// Default infrastructure preset used when no explicit preset is provided.
pub const DEFAULT_INFRA_PRESET: &str = "development";

/// File names backing each infrastructure config domain.
#[derive(Debug, Clone, Default)]
pub struct InfrastructurePresetFiles {
    pub storage: String,
    pub timeout: String,
    pub metrics: String,
    pub output: String,
    pub sandbox: String,
    pub file_checkpoint: String,
    pub presets: String,
    pub tools: String,
}

impl InfrastructurePresetFiles {
    /// Fixed default filenames (legacy mode, used when no preset is resolved).
    pub fn default_filenames() -> Self {
        Self {
            storage: "storage.toml".to_string(),
            timeout: "timeout.toml".to_string(),
            metrics: "metrics.toml".to_string(),
            output: "output.toml".to_string(),
            sandbox: "sandbox.toml".to_string(),
            file_checkpoint: "file-checkpoint.toml".to_string(),
            presets: "presets.toml".to_string(),
            tools: "tools.toml".to_string(),
        }
    }
}

/// Default infrastructure file mapping (legacy filenames).
pub fn default_infra_file_mapping() -> InfrastructurePresetFiles {
    InfrastructurePresetFiles::default_filenames()
}

/// Tool-specific configuration sections (read_file / glob / list_files and
/// raw pass-through sections).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolConfigs {
    pub glob: Option<GlobConfig>,
    pub list_files: Option<ListFilesConfig>,
    pub read_file: Option<ReadFileConfig>,
    /// Sections without a dedicated processor, passed through verbatim
    /// (e.g. `writeFile`, `editFile`, `runShell`, ...).
    pub passthrough: HashMap<String, serde_json::Value>,
}

/// Fully assembled configuration produced by the orchestrator.
#[derive(Debug, Clone)]
pub struct AssembledConfig {
    pub storage: StorageConfig,
    pub timeout: TimeoutConfig,
    pub metrics: MetricsConfig,
    pub output: OutputConfig,
    pub sandbox: Option<SandboxGlobalConfig>,
    pub file_checkpoint: FileCheckpointConfig,
    pub presets: PresetsConfig,
    pub tools: ToolConfigs,
}

impl Default for AssembledConfig {
    fn default() -> Self {
        Self {
            storage: StorageConfig::default(),
            timeout: TimeoutConfig::default(),
            metrics: MetricsConfig::default(),
            output: OutputConfig {
                dir: "./outputs".to_string(),
                log_file_pattern: "app-{date}.log".to_string(),
                enable_log_terminal: true,
                enable_sdk_logs: true,
                sdk_log_level: wf_types::config::output::SdkLogLevel::Warn,
            },
            sandbox: None,
            file_checkpoint: FileCheckpointConfig::default(),
            presets: PresetsConfig::default(),
            tools: ToolConfigs::default(),
        }
    }
}

/// Programmatic overrides applied after file-based config loading.
#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    pub storage: Option<StorageConfig>,
    pub timeout: Option<TimeoutConfig>,
    pub metrics: Option<MetricsConfig>,
    pub output: Option<OutputConfig>,
    pub sandbox: Option<SandboxGlobalConfig>,
    pub file_checkpoint: Option<FileCheckpointConfig>,
    pub presets: Option<PresetsConfig>,
    pub tools: Option<ToolConfigs>,
}

/// Declarative env var mapping for infrastructure config overrides.
fn build_infra_env_mapping() -> HashMap<String, crate::env::EnvMappingEntry> {
    EnvMappingBuilder::new()
        .custom(
            "storage_type",
            "WF_STORAGE_TYPE",
            Box::new(|v| {
                let lower = v.to_lowercase();
                Ok(EnvValue::String(lower))
            }),
            None,
        )
        .string("storage_sqlite_db_path", "WF_STORAGE_SQLITE_DB_PATH", None)
        .custom(
            "timeout_default",
            "WF_TIMEOUT_DEFAULT",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "metrics_enabled",
            "WF_METRICS_ENABLED",
            Box::new(env_parse_bool),
            None,
        )
        .string("output_dir", "WF_OUTPUT_DIR", None)
        .build()
}

/// Assemble configuration from project directory.
///
/// Loads `configs/infrastructure/*.toml` files, applies env var overrides,
/// then applies any programmatic overrides.
///
/// Use [`ConfigOrchestratorBuilder`] for non-default infrastructure directory.
pub struct ConfigOrchestrator;

impl ConfigOrchestrator {
    /// Assemble configuration using default paths.
    pub fn assemble(
        project_dir: &Path,
        overrides: Option<ConfigOverrides>,
    ) -> ConfigResult<AssembledConfig> {
        ConfigOrchestratorBuilder::new(project_dir)
            .build()
            .assemble(overrides)
    }

    /// Assemble configuration with an infrastructure preset (TS
    /// `loadInfrastructureConfigs`). When `preset_name` is given and
    /// `configs/infrastructure/index.json` exists, the preset's `files`
    /// mapping selects the config files per domain; otherwise `default_paths`
    /// (or the fixed default filenames) are used.
    pub fn assemble_with_preset(
        project_dir: &Path,
        preset_name: Option<&str>,
        default_paths: Option<InfrastructurePresetFiles>,
        overrides: Option<ConfigOverrides>,
    ) -> ConfigResult<AssembledConfig> {
        ConfigOrchestratorBuilder::new(project_dir)
            .preset_name(preset_name)
            .default_paths(default_paths)
            .build()
            .assemble(overrides)
    }
}

/// Builder for [`ConfigOrchestrator`] with non-default paths.
pub struct ConfigOrchestratorBuilder {
    infra_dir: PathBuf,
    preset_name: Option<String>,
    default_paths: Option<InfrastructurePresetFiles>,
}

impl ConfigOrchestratorBuilder {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            infra_dir: project_dir.join("configs").join("infrastructure"),
            preset_name: None,
            default_paths: None,
        }
    }

    /// Override the infrastructure config directory.
    pub fn infra_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.infra_dir = dir.into();
        self
    }

    /// Set the infrastructure preset name (e.g. "development").
    pub fn preset_name(mut self, name: Option<&str>) -> Self {
        self.preset_name = name.map(|s| s.to_string());
        self
    }

    /// Set the fallback file mapping used when no preset is resolved.
    pub fn default_paths(mut self, paths: Option<InfrastructurePresetFiles>) -> Self {
        self.default_paths = paths;
        self
    }

    pub fn build(self) -> ConfigOrchestratorLoaded {
        ConfigOrchestratorLoaded {
            infra_dir: self.infra_dir,
            preset_name: self.preset_name,
            default_paths: self.default_paths,
        }
    }
}

/// Orchestrator with resolved paths, ready to assemble.
pub struct ConfigOrchestratorLoaded {
    infra_dir: PathBuf,
    preset_name: Option<String>,
    default_paths: Option<InfrastructurePresetFiles>,
}

impl ConfigOrchestratorLoaded {
    pub fn assemble(self, overrides: Option<ConfigOverrides>) -> ConfigResult<AssembledConfig> {
        let files = resolve_file_mapping(
            &self.infra_dir,
            self.preset_name.as_deref(),
            self.default_paths,
        );
        let mut config = Self::load_infrastructure_configs(&self.infra_dir, &files)?;
        Self::apply_env_overrides(&mut config)?;
        if let Some(o) = overrides {
            Self::apply_overrides(&mut config, o)?;
        }
        Ok(config)
    }

    /// Load all infrastructure config domains using the resolved file mapping.
    fn load_infrastructure_configs(
        infra_dir: &Path,
        files: &InfrastructurePresetFiles,
    ) -> ConfigResult<AssembledConfig> {
        let storage_path = infra_dir.join(&files.storage);
        let timeout_path = infra_dir.join(&files.timeout);
        let metrics_path = infra_dir.join(&files.metrics);
        let output_path = infra_dir.join(&files.output);
        let sandbox_path = infra_dir.join(&files.sandbox);

        let storage: StorageConfig = load_domain_config(&storage_path);
        let timeout: TimeoutConfig = load_domain_config(&timeout_path);
        let metrics: MetricsConfig = load_domain_config(&metrics_path);
        let output: OutputConfig = load_domain_config(&output_path);

        // Sandbox config is fail-fast: a malformed sandbox.toml must reject
        // startup instead of silently running with the weaker defaults.
        let sandbox: Option<SandboxGlobalConfig> = if sandbox_path.exists() {
            let config: SandboxGlobalConfig =
                layered::load_layered_config_sync(&[sandbox_path.as_path()]).map_err(|e| {
                    ConfigError::Validation(format!(
                        "Invalid sandbox global config in {}: {e}",
                        sandbox_path.display()
                    ))
                })?;
            validate_sandbox_global(&config)?;
            Some(config)
        } else {
            None
        };

        // File-checkpoint / presets / tools load leniently: parse failures
        // fall back to defaults (presets additionally fail fast on
        // validation errors).
        let file_checkpoint =
            load_domain_config::<FileCheckpointConfig>(&infra_dir.join(&files.file_checkpoint));
        let presets = transform_presets_config(load_domain_config::<PresetsConfig>(
            &infra_dir.join(&files.presets),
        ))?;
        let tools = load_tool_configs(infra_dir, files);

        let storage = merge_storage_with_defaults(&storage);
        let timeout = merge_timeout_with_defaults(&timeout);
        let metrics = merge_metrics_with_defaults(&metrics);
        let output = merge_output_with_defaults(&output);
        let file_checkpoint = merge_file_checkpoint_with_defaults(&file_checkpoint);

        Ok(AssembledConfig {
            storage,
            timeout,
            metrics,
            output,
            sandbox,
            file_checkpoint,
            presets,
            tools,
        })
    }

    /// Apply environment variable overrides using declarative mapping.
    fn apply_env_overrides(config: &mut AssembledConfig) -> ConfigResult<()> {
        let mapping = build_infra_env_mapping();
        apply_env_overrides(
            |key, value| match key {
                "storage_type" => {
                    if let Some(s) = value.as_string() {
                        match s {
                            "sqlite" => {
                                config.storage.storage_type = StorageType::Sqlite;
                            }
                            "postgres" => {
                                config.storage.storage_type = StorageType::Postgres;
                            }
                            "memory" => {
                                config.storage.storage_type = StorageType::Memory;
                            }
                            _ => {}
                        }
                    }
                }
                "storage_sqlite_db_path" => {
                    if let Some(path) = value.as_string() {
                        if !path.is_empty() {
                            if let Some(ref mut sqlite) = config.storage.sqlite {
                                sqlite.db_path = path.to_string();
                            } else {
                                config.storage.sqlite =
                                    Some(wf_types::config::storage::SqliteStorageConfig {
                                        db_path: path.to_string(),
                                        ..Default::default()
                                    });
                            }
                        }
                    }
                }
                "timeout_default" => {
                    if let Some(ms) = value.as_int() {
                        config.timeout.default = Some(ms);
                    }
                }
                "metrics_enabled" => {
                    if let Some(enabled) = value.as_bool() {
                        config.metrics.enabled = Some(enabled);
                    }
                }
                "output_dir" => {
                    if let Some(dir) = value.as_string() {
                        if !dir.is_empty() {
                            config.output.dir = dir.to_string();
                        }
                    }
                }
                _ => {}
            },
            &mapping,
        )
    }

    fn apply_overrides(
        config: &mut AssembledConfig,
        overrides: ConfigOverrides,
    ) -> ConfigResult<()> {
        if let Some(storage) = overrides.storage {
            config.storage = merge_storage_with_defaults(&storage);
        }
        if let Some(timeout) = overrides.timeout {
            config.timeout = merge_timeout_with_defaults(&timeout);
        }
        if let Some(metrics) = overrides.metrics {
            config.metrics = merge_metrics_with_defaults(&metrics);
        }
        if let Some(output) = overrides.output {
            config.output = merge_output_with_defaults(&output);
        }
        if let Some(sandbox) = overrides.sandbox {
            config.sandbox = Some(sandbox);
        }
        if let Some(file_checkpoint) = overrides.file_checkpoint {
            config.file_checkpoint = merge_file_checkpoint_with_defaults(&file_checkpoint);
        }
        if let Some(presets) = overrides.presets {
            config.presets = transform_presets_config(presets)?;
        }
        if let Some(tools) = overrides.tools {
            config.tools = tools;
        }
        Ok(())
    }
}

/// Load a single-file preset definition (JSON) and extract its `files`
/// mapping, resolving relative paths against the preset file's directory.
pub fn load_infrastructure_preset(
    infra_dir: &Path,
    preset_name: &str,
) -> ConfigResult<InfrastructurePresetFiles> {
    let resolved = resolve_preset_index(infra_dir)?;
    let entry = find_preset_by_name(&resolved, preset_name).ok_or_else(|| {
        ConfigError::NotFound(format!(
            "Infrastructure preset '{preset_name}' not found in {}",
            infra_dir.join(INDEX_FILE_NAME).display()
        ))
    })?;
    let value = load_single_file_preset::<serde_json::Value>(entry)?;
    let files = value.get("files").ok_or_else(|| {
        ConfigError::Validation(format!(
            "Infrastructure preset '{preset_name}' has no 'files' mapping"
        ))
    })?;
    let files = files.as_object().ok_or_else(|| {
        ConfigError::Validation(format!(
            "Infrastructure preset '{preset_name}' 'files' must be an object"
        ))
    })?;

    let base_dir = entry.file_path.parent().unwrap_or(infra_dir);
    let mut mapping = InfrastructurePresetFiles::default();
    for (key, target) in files {
        let target = target.as_str().ok_or_else(|| {
            ConfigError::Validation(format!(
                "Infrastructure preset '{preset_name}' file path for '{key}' must be a string"
            ))
        })?;
        let path = base_dir.join(target);
        match key.as_str() {
            "storage" => mapping.storage = path.to_string_lossy().to_string(),
            "timeout" => mapping.timeout = path.to_string_lossy().to_string(),
            "metrics" => mapping.metrics = path.to_string_lossy().to_string(),
            "output" => mapping.output = path.to_string_lossy().to_string(),
            "sandbox" => mapping.sandbox = path.to_string_lossy().to_string(),
            "file_checkpoint" => mapping.file_checkpoint = path.to_string_lossy().to_string(),
            "presets" => mapping.presets = path.to_string_lossy().to_string(),
            "tools" => mapping.tools = path.to_string_lossy().to_string(),
            _ => {}
        }
    }
    Ok(mapping)
}

/// Resolve the file mapping for each domain:
/// 1. no `index.json` -> default paths (or fixed filenames when absent);
/// 2. preset name given and found -> the preset's `files` mapping;
/// 3. otherwise -> default paths (or fixed filenames when absent).
fn resolve_file_mapping(
    infra_dir: &Path,
    preset_name: Option<&str>,
    default_paths: Option<InfrastructurePresetFiles>,
) -> InfrastructurePresetFiles {
    // Legacy fallback (no explicit default paths): fixed default filenames.
    let fallback = || {
        default_paths
            .clone()
            .unwrap_or_else(InfrastructurePresetFiles::default_filenames)
    };
    let index_path = infra_dir.join(INDEX_FILE_NAME);
    if !index_path.exists() {
        return fallback();
    }
    match preset_name {
        Some(name) => match load_infrastructure_preset(infra_dir, name) {
            Ok(files) => files,
            Err(e) => {
                warn!(error = %e, "failed to resolve infrastructure preset '{name}'; falling back to default file mapping");
                fallback()
            }
        },
        None => fallback(),
    }
}

/// Load a single config domain file leniently: a missing or unparseable
/// file falls back to the domain defaults (with a warning).
fn load_domain_config<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    if !path.exists() {
        return T::default();
    }
    match layered::load_layered_config_sync::<T>(&[path]) {
        Ok(config) => config,
        Err(e) => {
            warn!(
                error = %e,
                "failed to parse config file {}; falling back to defaults",
                path.display()
            );
            T::default()
        }
    }
}

/// Convert camelCase keys to snake_case recursively (tools.toml may use
/// either casing, e.g. `maxResults` or `max_results`).
fn normalize_camel_case(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                let mut snake = String::new();
                for (i, c) in key.chars().enumerate() {
                    if c.is_uppercase() {
                        if i > 0 {
                            snake.push('_');
                        }
                        snake.push(c.to_ascii_lowercase());
                    } else {
                        snake.push(c);
                    }
                }
                out.insert(snake, normalize_camel_case(val));
            }
            serde_json::Value::Object(out)
        }
        other => other,
    }
}

/// Load tool-specific configuration sections.
///
/// `read_file` / `glob` / `list_files` sections are routed through their
/// dedicated processors; other sections (e.g. `writeFile`, `editFile`,
/// `runShell`, `sessionNote`, `backendShell`) pass through verbatim.
/// Both snake_case and camelCase section/key names are accepted.
pub fn load_tool_configs(infra_dir: &Path, files: &InfrastructurePresetFiles) -> ToolConfigs {
    let path = infra_dir.join(&files.tools);
    let mut tools = ToolConfigs::default();
    if !path.exists() {
        return tools;
    }
    let value: toml::Value = match layered::load_layered_config_sync(&[path.as_path()]) {
        Ok(value) => value,
        Err(e) => {
            warn!(
                error = %e,
                "failed to parse tool configs {}; falling back to defaults",
                path.display()
            );
            return tools;
        }
    };
    let Some(table) = value.as_table() else {
        return tools;
    };
    for (section, section_value) in table {
        let json = serde_json::to_value(section_value).unwrap_or(serde_json::Value::Null);
        match section.as_str() {
            "read_file" | "readFile" => {
                let input = normalize_camel_case(json);
                match serde_json::from_value::<ReadFileConfigInput>(input)
                    .map_err(ConfigError::from)
                    .and_then(transform_read_file_config)
                {
                    Ok(config) => tools.read_file = Some(config),
                    Err(e) => {
                        warn!(error = %e, "invalid [read_file] section in {}", path.display())
                    }
                }
            }
            "glob" => {
                let input = normalize_camel_case(json);
                match serde_json::from_value::<GlobConfigInput>(input)
                    .map_err(ConfigError::from)
                    .and_then(transform_glob_config)
                {
                    Ok(config) => tools.glob = Some(config),
                    Err(e) => warn!(error = %e, "invalid [glob] section in {}", path.display()),
                }
            }
            "list_files" | "listFiles" => {
                let input = normalize_camel_case(json);
                match serde_json::from_value::<ListFilesConfigInput>(input)
                    .map_err(ConfigError::from)
                    .and_then(transform_list_files_config)
                {
                    Ok(config) => tools.list_files = Some(config),
                    Err(e) => {
                        warn!(error = %e, "invalid [list_files] section in {}", path.display())
                    }
                }
            }
            _ => {
                tools.passthrough.insert(section.clone(), json);
            }
        }
    }
    tools
}

/// Load infrastructure configs using the default preset
/// (`development`), with the fixed default filenames as fallback.
pub fn load_default_infrastructure_configs(project_dir: &Path) -> ConfigResult<AssembledConfig> {
    ConfigOrchestrator::assemble_with_preset(
        project_dir,
        Some(DEFAULT_INFRA_PRESET),
        Some(default_infra_file_mapping()),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn setup_test_project(dir: &Path) {
        let infra = dir.join("configs").join("infrastructure");
        std::fs::create_dir_all(&infra).unwrap();

        let mut f = std::fs::File::create(infra.join("storage.toml")).unwrap();
        writeln!(f, "type = \"sqlite\"\n[sqlite]\ndb_path = \"./test.db\"").unwrap();

        let mut f = std::fs::File::create(infra.join("timeout.toml")).unwrap();
        writeln!(f, "default = 60000").unwrap();

        let mut f = std::fs::File::create(infra.join("metrics.toml")).unwrap();
        writeln!(f, "enabled = true\nreporting_interval = 5000").unwrap();

        let mut f = std::fs::File::create(infra.join("output.toml")).unwrap();
        writeln!(f, "dir = \"./test-outputs\"\nlog_file_pattern = \"test.log\"\nenable_log_terminal = false\nenable_sdk_logs = false\nsdk_log_level = \"info\"").unwrap();
    }

    /// Project with a preset index + development preset whose `files` mapping
    /// points at custom filenames.
    fn setup_preset_project(dir: &Path) {
        let infra = dir.join("configs").join("infrastructure");
        std::fs::create_dir_all(&infra).unwrap();

        write_json(
            &infra.join("index.json"),
            r#"{"version": "1.0", "type": "infrastructure_presets", "paths": ["./*.json"]}"#,
        );
        write_json(
            &infra.join("development.json"),
            r#"{"id": "development", "name": "Development", "files": {"storage": "./custom-storage.toml", "timeout": "./custom-timeout.toml", "metrics": "./custom-metrics.toml", "output": "./custom-output.toml", "file_checkpoint": "./custom-checkpoint.toml", "presets": "./custom-presets.toml", "tools": "./custom-tools.toml", "sandbox": "./custom-sandbox.toml"}}"#,
        );

        let mut f = std::fs::File::create(infra.join("custom-storage.toml")).unwrap();
        writeln!(f, "type = \"postgres\"\n[postgres]\nhost = \"localhost\"\nport = 5432\nusername = \"u\"\npassword = \"p\"\ndatabase = \"d\"").unwrap();
        let mut f = std::fs::File::create(infra.join("custom-timeout.toml")).unwrap();
        writeln!(f, "default = 42000").unwrap();
        let mut f = std::fs::File::create(infra.join("custom-metrics.toml")).unwrap();
        writeln!(f, "enabled = false").unwrap();
        let mut f = std::fs::File::create(infra.join("custom-output.toml")).unwrap();
        writeln!(f, "dir = \"./preset-outputs\"\nlog_file_pattern = \"p.log\"\nenable_log_terminal = false\nenable_sdk_logs = false\nsdk_log_level = \"info\"").unwrap();
    }

    fn write_json(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn clear_wf_env_vars() {
        for var in [
            "WF_STORAGE_TYPE",
            "WF_STORAGE_SQLITE_DB_PATH",
            "WF_TIMEOUT_DEFAULT",
            "WF_METRICS_ENABLED",
            "WF_OUTPUT_DIR",
        ] {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn test_assemble_from_project_dir() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-{}-assemble", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_test_project(&dir);

        let config = ConfigOrchestrator::assemble(&dir, None).unwrap();

        assert_eq!(config.storage.storage_type, StorageType::Sqlite);
        let sqlite = config.storage.sqlite.as_ref().unwrap();
        assert_eq!(sqlite.db_path, "./test.db");
        assert_eq!(config.timeout.default, Some(60000));
        assert_eq!(config.metrics.reporting_interval, Some(5000));
        assert_eq!(config.output.dir, "./test-outputs");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn repo_metrics_toml_matches_schema() {
        // Stage 6 acceptance: the checked-in `configs/infrastructure/metrics.toml`
        // parses against the current `MetricsConfig` schema (no removed
        // `template_metrics` section, new fields present). Skipped when the
        // workspace `configs/` dir is absent (vendored/standalone builds).
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        if !repo_root.join("configs").join("infrastructure").exists() {
            return;
        }
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();
        let config = ConfigOrchestrator::assemble(&repo_root, None).unwrap();
        let metrics = config.metrics;
        assert_eq!(metrics.enabled, Some(true));
        assert_eq!(metrics.retention_ms, Some(3_600_000));
        let thresholds = metrics.anomaly_thresholds.unwrap();
        assert_eq!(thresholds.max_error_count, Some(100));
        assert!((thresholds.min_success_rate.unwrap() - 0.8).abs() < 1e-9);
        assert!(metrics.subgraph_metrics.is_some());
    }

    #[test]
    fn test_assemble_defaults_when_no_files() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let config = ConfigOrchestrator::assemble(&dir, None).unwrap();

        assert_eq!(config.storage.storage_type, StorageType::Memory);
        assert_eq!(config.timeout.default, Some(30000));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_with_overrides() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-override-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_test_project(&dir);

        let overrides = ConfigOverrides {
            timeout: Some(TimeoutConfig {
                default: Some(99999),
                ..Default::default()
            }),
            ..Default::default()
        };

        let config = ConfigOrchestrator::assemble(&dir, Some(overrides)).unwrap();
        assert_eq!(config.timeout.default, Some(99999));
        assert_eq!(config.storage.storage_type, StorageType::Sqlite);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_env_override() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_test_project(&dir);

        std::env::set_var("WF_STORAGE_TYPE", "memory");
        let config = ConfigOrchestrator::assemble(&dir, None).unwrap();
        assert_eq!(config.storage.storage_type, StorageType::Memory);
        clear_wf_env_vars();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_builder_custom_infra_dir() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-builder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let custom_infra = dir.join("custom").join("infra");
        std::fs::create_dir_all(&custom_infra).unwrap();

        let mut f = std::fs::File::create(custom_infra.join("storage.toml")).unwrap();
        writeln!(f, "type = \"postgres\"\n[postgres]\nhost = \"localhost\"\nport = 5432\nusername = \"user\"\npassword = \"pass\"\ndatabase = \"test\"").unwrap();

        let config = ConfigOrchestratorBuilder::new(&dir)
            .infra_dir(&custom_infra)
            .build()
            .assemble(None)
            .unwrap();

        assert_eq!(config.storage.storage_type, StorageType::Postgres);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_with_preset_hits_preset() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-preset-hit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_preset_project(&dir);

        let config = ConfigOrchestrator::assemble_with_preset(
            &dir,
            Some(DEFAULT_INFRA_PRESET),
            Some(default_infra_file_mapping()),
            None,
        )
        .unwrap();

        // The development preset maps storage to custom-storage.toml.
        assert_eq!(config.storage.storage_type, StorageType::Postgres);
        assert_eq!(config.timeout.default, Some(42000));
        assert_eq!(config.metrics.enabled, Some(false));
        assert_eq!(config.output.dir, "./preset-outputs");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_with_preset_falls_back_on_missing_preset() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-preset-miss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_preset_project(&dir);

        // Preset name does not exist in the index: fall back to default paths.
        let config = ConfigOrchestrator::assemble_with_preset(
            &dir,
            Some("nonexistent"),
            Some(default_infra_file_mapping()),
            None,
        )
        .unwrap();
        assert_eq!(config.storage.storage_type, StorageType::Memory);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_default_infrastructure_configs_resolves_default_preset() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir =
            std::env::temp_dir().join(format!("wf-orch-default-preset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_preset_project(&dir);

        let config = load_default_infrastructure_configs(&dir).unwrap();
        assert_eq!(config.storage.storage_type, StorageType::Postgres);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assemble_new_domain_overrides() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let dir = std::env::temp_dir().join(format!("wf-orch-new-domain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        setup_test_project(&dir);

        let overrides = ConfigOverrides {
            presets: Some(PresetsConfig {
                context_compression: Some(
                    wf_types::config::presets::ContextCompressionPresetConfig {
                        enabled: Some(true),
                        threshold: Some(0.9),
                        max_tokens: Some(2048),
                        strategy: Some("sliding_window".to_string()),
                    },
                ),
                predefined_tools: None,
                predefined_prompts: None,
            }),
            tools: Some(ToolConfigs {
                read_file: Some(ReadFileConfig {
                    workspace_dir: None,
                    max_file_size: 1000,
                    max_chars: 2000,
                    max_lines: 50,
                    enable_ignore: true,
                    enable_protect: true,
                    model_id: None,
                }),
                ..Default::default()
            }),
            file_checkpoint: Some(FileCheckpointConfig {
                enabled: true,
                workspace_root: Some("/data".to_string()),
                max_delta_chain_length: 40,
                custom_ignore_patterns: None,
                storage: None,
                failure_behavior: wf_types::config::file_checkpoint::FailureBehavior::Error,
                ..Default::default()
            }),
            ..Default::default()
        };

        let config = ConfigOrchestrator::assemble(&dir, Some(overrides)).unwrap();
        let presets = config.presets.context_compression.unwrap();
        assert_eq!(presets.enabled, Some(true));
        assert_eq!(presets.max_tokens, Some(2048));
        assert!(config.tools.read_file.is_some());
        assert_eq!(config.tools.read_file.as_ref().unwrap().max_file_size, 1000);
        assert!(config.file_checkpoint.enabled);
        assert_eq!(config.file_checkpoint.max_delta_chain_length, 40);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_full_bundle_assembly_from_repo_configs() {
        // Stage 6 acceptance: the checked-in `configs/infrastructure/` preset
        // bundle (development.json + all domain files) assembles fully.
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        if !repo_root.join("configs").join("infrastructure").exists() {
            return;
        }
        let _lock = ENV_LOCK.lock().unwrap();
        clear_wf_env_vars();

        let config = load_default_infrastructure_configs(&repo_root).unwrap();

        // storage/metrics come from the preset mapping (same files).
        assert_eq!(config.metrics.enabled, Some(true));
        assert_eq!(config.timeout.default, Some(30000));
        assert!(config.sandbox.is_some(), "repo sandbox.toml must load");
        assert_eq!(config.output.dir, "./outputs");
        assert!(
            config.presets.context_compression.is_some(),
            "repo presets.toml must load"
        );
        assert!(
            config.tools.read_file.is_some(),
            "repo tools.toml [read_file] must load"
        );
        assert_eq!(
            config.tools.read_file.as_ref().unwrap().max_file_size,
            500_000,
            "read_file defaults applied"
        );
        assert_eq!(
            config.file_checkpoint.max_delta_chain_length, 20,
            "repo file-checkpoint.toml must load"
        );
    }
}
