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
    pub fn llm_gateway(&self) -> &std::sync::Arc<wf_llm::LlmGateway> {
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
    let config = RuntimeConfig {
        mode_override: Some(match cli_mode {
            CliMode::Run => ExecutionMode::Headless,
            CliMode::Mini | CliMode::Tui => ExecutionMode::Interactive,
        }),
        ..Default::default()
    };

    // Surface CLI flags as config where the runtime supports them; unknown
    // options are ignored for now (log level defaults to warn).
    let _ = cli;

    config
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
}
