use std::sync::Arc;

use tracing::info;

use wf_resource::registrar::{Options as ResourceOptions, Registries};
use wf_resource::starter::BundleRegistry;

use crate::error::RuntimeResult;
use crate::lifecycle::{shutdown_channel, ShutdownHandle, ShutdownWaiter};
use crate::logger::{init_tracing, LogConfig};
use crate::mode::{detect_all, ModeInfo};
use crate::storage_manager::{StorageConfig, StorageManager};

#[derive(Debug, Clone, Default)]
pub struct ResourceConfig {
    pub options: ResourceOptions,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeConfig {
    pub storage: StorageConfig,
    pub log_config: LogConfig,
    pub mode_override: Option<super::mode::ExecutionMode>,
    pub resource: ResourceConfig,
    #[cfg(feature = "plugins")]
    pub plugins: PluginConfig,
}

#[cfg(feature = "plugins")]
#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub enabled: bool,
    pub paths: Vec<std::path::PathBuf>,
    pub auto_activate: bool,
    pub guard_timeout_ms: u64,
}

#[cfg(feature = "plugins")]
impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            paths: vec![std::path::PathBuf::from("./plugins")],
            auto_activate: true,
            guard_timeout_ms: 10000,
        }
    }
}

pub struct Runtime {
    pub storage_manager: StorageManager,
    pub mode_info: ModeInfo,
    pub shutdown_handle: ShutdownHandle,
    pub _shutdown_waiter: ShutdownWaiter,
    pub registries: Arc<Registries>,
    pub bundles: Arc<BundleRegistry>,
    #[cfg(feature = "plugins")]
    pub plugin_engine: Option<wf_plugin::PluginEngine>,
}

impl Runtime {
    pub async fn bootstrap(config: RuntimeConfig) -> RuntimeResult<Self> {
        let mode_info = detect_all(config.mode_override);
        let effective_log_config = adjust_log_config(config.log_config, &mode_info);

        let _guard = init_tracing(&effective_log_config)?;

        info!("Bootstrapping runtime in {:?} mode", mode_info.mode);

        let mut storage_manager = StorageManager::new(config.storage);
        storage_manager.initialize().await?;

        let registries = Arc::new(Registries::new());
        let bundles = Arc::new(BundleRegistry::new());

        let resource_result =
            wf_resource::register_all(&registries, &bundles, &config.resource.options);
        info!(
            "Resource registration: {} succeeded, {} failed",
            resource_result.succeeded.len(),
            resource_result.failed.len(),
        );
        for fail in &resource_result.failed {
            tracing::warn!("Resource registration failed: {} - {}", fail.id, fail.error);
        }

        let (shutdown_handle, _shutdown_waiter) = shutdown_channel();

        #[cfg(feature = "plugins")]
        let plugin_engine = init_plugins(&config.plugins).await?;

        info!("Runtime bootstrap complete");

        Ok(Self {
            storage_manager,
            mode_info,
            shutdown_handle,
            _shutdown_waiter,
            registries,
            bundles,
            #[cfg(feature = "plugins")]
            plugin_engine,
        })
    }

    pub fn registries(&self) -> &Registries {
        &self.registries
    }

    pub fn bundles(&self) -> &BundleRegistry {
        &self.bundles
    }

    pub async fn shutdown(mut self) -> RuntimeResult<()> {
        #[cfg(feature = "plugins")]
        if let Some(engine) = self.plugin_engine.take() {
            engine.shutdown().await;
        }

        self.storage_manager.close().await?;
        info!("Runtime shutdown complete");
        Ok(())
    }

    pub fn storage(&self) -> &StorageManager {
        &self.storage_manager
    }

    pub fn mode(&self) -> &ModeInfo {
        &self.mode_info
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_handle.is_shutting_down()
    }

    pub fn trigger_shutdown(&self) {
        self.shutdown_handle.trigger();
    }

    #[cfg(feature = "plugins")]
    pub fn plugin_engine(&self) -> Option<&wf_plugin::PluginEngine> {
        self.plugin_engine.as_ref()
    }
}

#[cfg(feature = "plugins")]
async fn init_plugins(config: &PluginConfig) -> RuntimeResult<Option<wf_plugin::PluginEngine>> {
    if !config.enabled {
        return Ok(None);
    }

    let plugin_config = wf_plugin::PluginSystemConfig {
        enabled: true,
        paths: config.paths.clone(),
        auto_activate: config.auto_activate,
        guard_timeout_ms: config.guard_timeout_ms,
        ..Default::default()
    };

    let registry = Arc::new(wf_plugin::PluginRegistry::new());
    let contribution_manager = Arc::new(wf_plugin::ContributionManager::new());
    let bridge: Option<Arc<dyn wf_plugin::ContributionBridge>> =
        Some(Arc::new(crate::plugin_bridge::WfPluginBridge));

    let event_bus = wf_core::EventBus::new(256);

    let mut engine = wf_plugin::PluginEngine::new(
        registry,
        contribution_manager,
        bridge,
        plugin_config,
        env!("CARGO_PKG_VERSION"),
    )
    .with_event_bus(event_bus);

    engine.initialize().await.map_err(|e| {
        tracing::error!("Plugin engine initialization failed: {}", e);
        crate::error::RuntimeError::Config(format!("Plugin init failed: {}", e))
    })?;

    Ok(Some(engine))
}

fn adjust_log_config(mut config: LogConfig, mode_info: &ModeInfo) -> LogConfig {
    if mode_info.is_json_mode() && matches!(config.format, crate::logger::LogFormat::Full) {
        config.format = crate::logger::LogFormat::Json;
    }

    if mode_info.is_silent_mode() {
        config.level = "off".to_string();
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::ExecutionMode;
    use crate::storage_manager::StorageBackendType;
    use wf_core::registry::Registry;

    fn clear_env_vars() {
        std::env::remove_var("CLI_MODE");
        std::env::remove_var("HEADLESS");
        std::env::remove_var("TEST_MODE");
        std::env::remove_var("CLI_OUTPUT_FORMAT");
        std::env::remove_var("NO_COLOR");
    }

    #[tokio::test]
    async fn test_runtime_bootstrap_memory() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                backend_type: StorageBackendType::Memory,
                ..Default::default()
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        assert!(runtime.storage().is_initialized());
        assert!(runtime.mode().is_test());
        assert!(!runtime.registries().tools.is_empty());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_registries_populated() {
        clear_env_vars();

        let config = RuntimeConfig {
            log_config: LogConfig::default().with_level("off"),
            resource: ResourceConfig::default(),
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        // Verify registries are populated from register_all()
        assert!(!runtime.registries().tools.is_empty());
        assert!(!runtime.registries().fragments.is_empty());
        assert!(!runtime.registries().prompt_templates.is_empty());
        assert!(!runtime.registries().tool_descriptions.is_empty());
        assert!(!runtime.registries().agent_templates.is_empty());
        assert!(!runtime.registries().trigger_templates.is_empty());
        assert!(!runtime.registries().workflows.is_empty());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_trigger_shutdown() {
        clear_env_vars();

        let config = RuntimeConfig {
            storage: StorageConfig {
                backend_type: StorageBackendType::Memory,
                ..Default::default()
            },
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        assert!(!runtime.is_shutting_down());
        runtime.trigger_shutdown();
        assert!(runtime.is_shutting_down());

        runtime.shutdown().await.unwrap();

        clear_env_vars();
    }

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert!(matches!(
            config.storage.backend_type,
            StorageBackendType::Memory
        ));
        assert!(config.mode_override.is_none());
    }

    #[test]
    fn test_adjust_log_config_json_mode() {
        use crate::logger::LogFormat;

        let mode = ModeInfo {
            mode: ExecutionMode::Headless,
            output_format: crate::mode::OutputFormat::Json,
            color_enabled: false,
        };

        let config = LogConfig::default().with_format(LogFormat::Full);
        let adjusted = adjust_log_config(config, &mode);

        assert_eq!(adjusted.format, LogFormat::Json);
    }

    #[test]
    fn test_adjust_log_config_silent_mode() {
        let mode = ModeInfo {
            mode: ExecutionMode::Interactive,
            output_format: crate::mode::OutputFormat::Silent,
            color_enabled: false,
        };

        let config = LogConfig::default().with_level("info");
        let adjusted = adjust_log_config(config, &mode);

        assert_eq!(adjusted.level, "off");
    }
}
