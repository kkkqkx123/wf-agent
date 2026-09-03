//! Domain adapter: owns the runtime and exposes the application-facing API
//! to the CLI forms.
//!
//! Every CLI form (headless run / mini / TUI) drives a single
//! [`DomainAdapter`] built through [`Runtime::bootstrap`]; the adapter hides
//! runtime assembly details and keeps one teardown path (`shutdown`).

use std::sync::Arc;

use wf_api::infra::context::ApiContext;
use wf_core::event::EventBus;
use wf_runtime::bootstrap::{Runtime, RuntimeConfig};
use wf_runtime::mode::{ExecutionMode, ModeInfo};
use wf_runtime::storage_manager::StorageManager;

use crate::args::Cli;
use crate::error::{CliError, CliResult};
use crate::mode::CliMode;

/// Application-facing runtime handle for a CLI session.
pub struct DomainAdapter {
    runtime: Runtime,
}

impl DomainAdapter {
    /// Bootstrap the runtime from a fully assembled config.
    pub async fn bootstrap(config: RuntimeConfig) -> CliResult<Self> {
        let runtime = Runtime::bootstrap(config)
            .await
            .map_err(|err| CliError::Configuration(format!("runtime bootstrap failed: {err}")))?;
        Ok(Self { runtime })
    }

    /// Bootstrap with the given CLI arguments: maps the resolved form to the
    /// runtime execution mode and applies CLI-level config overrides.
    pub async fn bootstrap_for_cli(cli: &Cli, cli_mode: CliMode) -> CliResult<Self> {
        let config = runtime_config_for_cli(cli, cli_mode);
        Self::bootstrap(config).await
    }

    /// Shared application API context (query domain, agent/workflow engines).
    pub fn api_context(&self) -> &ApiContext {
        self.runtime.api_context()
    }

    /// Shared lifecycle event bus.
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.runtime.event_bus
    }

    /// Runtime mode information (execution mode / output format / color).
    pub fn mode(&self) -> &ModeInfo {
        self.runtime.mode()
    }

    /// Storage manager (backend config + initialized context).
    pub fn storage(&self) -> &StorageManager {
        self.runtime.storage()
    }

    /// LLM gateway (profile resolution + model invocation). Headless runs
    /// surface gateway diagnostics through it; profile listing / warm-up
    /// also use the same handle.
    pub fn llm_gateway(&self) -> &std::sync::Arc<wf_api::LlmGateway> {
        &self.api_context().llm_gateway
    }

    /// Whether the runtime is in the middle of a shutdown.
    pub fn is_shutting_down(&self) -> bool {
        self.runtime.is_shutting_down()
    }

    /// Tear the runtime down, releasing storage and background tasks.
    pub async fn shutdown(self) -> CliResult<()> {
        self.runtime
            .shutdown()
            .await
            .map_err(|err| CliError::Configuration(format!("runtime shutdown failed: {err}")))
    }
}

/// Build the runtime config for a CLI invocation: mode override + CLI-level
/// knobs over the defaults (memory storage, warn logging).
pub fn runtime_config_for_cli(cli: &Cli, cli_mode: CliMode) -> RuntimeConfig {
    let mut config = RuntimeConfig {
        mode_override: Some(match cli_mode {
            CliMode::Run => ExecutionMode::Headless,
            CliMode::Mini | CliMode::Tui => ExecutionMode::Interactive,
        }),
        ..Default::default()
    };

    if let Some(storage) = parse_storage_config(cli.storage.as_deref()) {
        config.storage = storage;
    }

    if let Some(level) = cli.log_level.as_deref() {
        let lower = level.to_ascii_lowercase();
        let normalized = match lower.as_str() {
            "warning" => "warn",
            other => other,
        };
        config.log_config = config.log_config.with_level(normalized.to_string());
    }

    if let Some(timeout_ms) = cli.timeout {
        config.timeout = wf_types::config::timeout::TimeoutConfig {
            default: Some(timeout_ms as i64),
            ..Default::default()
        };
    }

    if let Some(approval) = cli.approval.as_deref() {
        if let Some(tool_approval) = parse_tool_approval(approval) {
            config.tool_approval = tool_approval;
        }
    }

    if let Some(path) = cli.config.clone() {
        config.infra = Some(wf_runtime::bootstrap::InfraSourceConfig {
            project_root: Some(path),
            ..Default::default()
        });
    }

    config
}

fn parse_storage_config(spec: Option<&str>) -> Option<wf_types::config::storage::StorageConfig> {
    let spec = spec?;
    if spec == "memory" {
        return Some(wf_types::config::storage::StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Memory,
            sqlite: None,
            postgres: None,
            app_name: None,
        });
    }
    if spec == "sqlite" {
        return Some(wf_types::config::storage::StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: String::new(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        });
    }
    if let Some(path) = spec.strip_prefix("sqlite:") {
        return Some(wf_types::config::storage::StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: path.to_string(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        });
    }
    None
}

fn parse_tool_approval(mode: &str) -> Option<wf_types::config::tool_approval::ToolApprovalConfig> {
    let lower = mode.to_ascii_lowercase();
    match lower.as_str() {
        "auto" => Some(wf_types::config::tool_approval::ToolApprovalConfig {
            enabled: false,
            options: None,
        }),
        "manual" => Some(wf_types::config::tool_approval::ToolApprovalConfig {
            enabled: true,
            options: None,
        }),
        "llm" => Some(wf_types::config::tool_approval::ToolApprovalConfig {
            enabled: true,
            options: Some(wf_types::tool::approval::ToolApprovalOptions {
                auto_approval_enabled: Some(false),
                ..wf_types::tool::approval::ToolApprovalOptions::empty()
            }),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[tokio::test]
    async fn bootstrap_with_defaults_is_healthy_and_shutdown_is_clean() {
        let adapter = DomainAdapter::bootstrap(RuntimeConfig::default())
            .await
            .unwrap();

        // Application API context is reachable and composed.
        let api = adapter.api_context();
        let _ = &api.storage;
        let _ = &api.registries;
        let _ = &api.tool_registry;
        let _ = &api.llm_gateway;

        // Lifecycle event bus is accessible.
        let _ = adapter.event_bus();

        // Runtime mode reflects the default (interactive programmatic default
        // in runtime terms; the CLI form mapping is exercised below).
        let mode = adapter.mode();
        assert!(!mode.is_headless());

        adapter.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn cli_mode_maps_to_runtime_execution_mode() {
        let config = runtime_config_for_cli(
            &Cli::try_parse_from(["wf", "run", "x"]).unwrap(),
            CliMode::Run,
        );
        assert_eq!(config.mode_override, Some(ExecutionMode::Headless));

        let config = runtime_config_for_cli(
            &Cli::try_parse_from(["wf", "--mini"]).unwrap(),
            CliMode::Mini,
        );
        assert_eq!(config.mode_override, Some(ExecutionMode::Interactive));
    }

    #[test]
    fn runtime_config_storage_mapping() {
        let cli = Cli::try_parse_from(["wf", "--storage", "memory"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert_eq!(
            cfg.storage.storage_type,
            wf_types::config::storage::StorageType::Memory
        );

        let cli = Cli::try_parse_from(["wf", "--storage", "sqlite:/tmp/wf.db"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert_eq!(
            cfg.storage.storage_type,
            wf_types::config::storage::StorageType::Sqlite
        );
        assert_eq!(cfg.storage.sqlite.as_ref().unwrap().db_path, "/tmp/wf.db");

        let cli = Cli::try_parse_from(["wf"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert_eq!(
            cfg.storage.storage_type,
            wf_types::config::storage::StorageType::Memory
        );
    }

    #[test]
    fn runtime_config_log_level_mapping() {
        let cli = Cli::try_parse_from(["wf", "--log-level", "debug"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert_eq!(cfg.log_config.level, "debug");

        let cli = Cli::try_parse_from(["wf", "--log-level", "INFO"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert_eq!(cfg.log_config.level, "info");

        let cli = Cli::try_parse_from(["wf"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert_eq!(cfg.log_config.level, "warn");
    }

    #[test]
    fn runtime_config_timeout_and_approval_mapping() {
        let cli = Cli::try_parse_from(["wf", "--timeout", "5000"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert_eq!(cfg.timeout.default, Some(5000));

        let cli = Cli::try_parse_from(["wf", "--approval", "auto"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert!(!cfg.tool_approval.enabled);

        let cli = Cli::try_parse_from(["wf", "--approval", "manual"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert!(cfg.tool_approval.enabled);
        assert!(cfg.tool_approval.options.is_none());

        let cli = Cli::try_parse_from(["wf", "--approval", "llm"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert!(cfg.tool_approval.enabled);
        assert!(cfg.tool_approval.options.is_some());
    }

    #[test]
    fn runtime_config_infra_mapping() {
        let cli = Cli::try_parse_from(["wf", "--config", "/tmp/proj"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert!(cfg.infra.is_some());
        assert_eq!(
            cfg.infra.unwrap().project_root.unwrap().to_string_lossy(),
            "/tmp/proj"
        );

        let cli = Cli::try_parse_from(["wf"]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert!(cfg.infra.is_none());
    }

    #[tokio::test]
    async fn bootstrap_with_sqlite_storage_creates_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let spec = format!("sqlite:{}", db_path.display());
        let cli = Cli::try_parse_from(["wf", "--storage", &spec]).unwrap();
        let cfg = runtime_config_for_cli(&cli, CliMode::Run);
        assert_eq!(
            cfg.storage.storage_type,
            wf_types::config::storage::StorageType::Sqlite
        );
        let adapter = DomainAdapter::bootstrap(cfg).await.unwrap();
        assert!(db_path.exists() || dir.path().join("test.db").exists());
        adapter.shutdown().await.unwrap();
    }
}
