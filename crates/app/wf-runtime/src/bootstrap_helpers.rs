use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};

use wf_config::orchestrator::{default_infra_file_mapping, ConfigOrchestrator};
use wf_config::processor::llm_profile::{transform_llm_profile, validate_llm_profile};
use wf_llm::LlmGateway;
use wf_types::config::file_checkpoint::FileCheckpointConfig;
use wf_types::config::storage::{StorageConfig, StorageType};

use crate::error::RuntimeResult;
use crate::logger::LogConfig;
use crate::mode::ModeInfo;

use super::{InfraSourceConfig, LlmConfig, McpRuntimeConfig, RuntimeConfig};

pub fn storage_db_path(config: &StorageConfig) -> PathBuf {
    let app_name = config.app_name.as_deref().unwrap_or("app");
    config
        .sqlite
        .as_ref()
        .map(|c| c.db_path.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("./storage/{}.db", app_name)))
}

#[cfg(feature = "checkpoint")]
pub fn init_file_checkpoint_manager(
    config: &FileCheckpointConfig,
    event_bus: Arc<wf_core::event::EventBus>,
) -> RuntimeResult<(
    Option<wf_checkpoint::file::FileCheckpointManager>,
    Option<tokio::task::JoinHandle<()>>,
)> {
    if !config.enabled {
        return Ok((None, None));
    }
    match wf_checkpoint::file::FileCheckpointManager::open_from_config(config) {
        Ok(manager) => {
            info!("File checkpoint manager initialized (layertwine Sqlite)");
            let bus = wf_checkpoint::event::CheckpointEventBus::new();
            let handle = crate::checkpoint_event_bridge::spawn(event_bus, bus.clone());
            let manager = manager.with_event_bus(bus);
            Ok((Some(manager), Some(handle)))
        }
        Err(err) => Err(crate::error::RuntimeError::Config(format!(
            "Failed to initialize file checkpoint storage: {err}"
        ))),
    }
}

#[cfg(feature = "checkpoint")]
pub fn init_manual_change_service(
    config: &FileCheckpointConfig,
    manager: Option<&wf_checkpoint::file::FileCheckpointManager>,
) -> RuntimeResult<Option<wf_checkpoint::watcher::ManualChangeService>> {
    let Some(manager) = manager else {
        return Ok(None);
    };
    let Some(root) = config.workspace_root.as_deref() else {
        return Ok(None);
    };
    if !config.enabled || !config.manual_watch {
        return Ok(None);
    }
    let scan_config = wf_checkpoint::scan::ScanConfig {
        custom_ignore_patterns: config.custom_ignore_patterns.clone().unwrap_or_default(),
        failure_behavior: config.failure_behavior,
    };
    match wf_checkpoint::watcher::ManualChangeService::start(
        manager.clone(),
        root,
        scan_config,
        100,
        200,
    ) {
        Ok(service) => {
            info!(root = %root, "Manual file watcher started");
            Ok(Some(service))
        }
        Err(err) => Err(crate::error::RuntimeError::Config(format!(
            "Failed to start the manual file watcher: {err}"
        ))),
    }
}

#[cfg(feature = "checkpoint")]
pub fn init_gc_timer(
    config: &FileCheckpointConfig,
    manager: Option<&wf_checkpoint::file::FileCheckpointManager>,
) -> Option<tokio::task::JoinHandle<()>> {
    let interval_secs = config.gc_interval_secs?;
    if interval_secs == 0 {
        return None;
    }
    let manager = manager?.clone();
    let retention = config
        .gc_retention
        .map(|r| layertwine::git_sync::GcRetention {
            keep_recent_heads: r.keep_recent_heads,
        })
        .unwrap_or_default();
    let interval = std::time::Duration::from_secs(interval_secs);
    info!(interval_secs, "Periodic GC timer started");
    Some(tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            match manager.run_gc(retention) {
                Ok(stats) => {
                    info!(
                        removed_checkpoints = stats.removed_checkpoints,
                        removed_snapshots = stats.removed_snapshots,
                        freed_bytes = stats.freed_bytes,
                        "Periodic GC completed"
                    );
                }
                Err(err) => {
                    tracing::warn!("Periodic GC failed: {err}");
                }
            }
        }
    }))
}

pub async fn init_checkpoint_store(
    config: &StorageConfig,
) -> Arc<wf_storage::backend::StorageBackend> {
    use wf_storage::backend::StorageBackend;
    use wf_storage::decorator::instrumented::InstrumentedStore;

    let backend = match config.storage_type {
        StorageType::Memory => StorageBackend::new_memory(),
        #[cfg(feature = "sqlite")]
        StorageType::Sqlite => {
            let path = storage_db_path(config);
            match StorageBackend::new_sqlite(&path.to_string_lossy(), "checkpoint").await {
                Ok(store) => store,
                Err(err) => {
                    warn!(error = %err, path = %path.display(), "failed to open checkpoint store backend; checkpoints stay in memory");
                    StorageBackend::new_memory()
                }
            }
        }
        #[cfg(not(feature = "sqlite"))]
        StorageType::Sqlite => {
            warn!("Sqlite checkpoint store unavailable: enable the 'sqlite' feature");
            StorageBackend::new_memory()
        }
        #[cfg(feature = "postgres")]
        StorageType::Postgres => {
            let conn = config
                .postgres
                .as_ref()
                .map(|c| c.host.as_str())
                .unwrap_or_default();
            match wf_storage::store::postgres::PostgresStorage::new(conn, "checkpoint").await {
                Ok(store) => StorageBackend::Postgres(InstrumentedStore::new(store)),
                Err(err) => {
                    warn!(error = %err, "failed to open checkpoint store backend; checkpoints stay in memory");
                    StorageBackend::new_memory()
                }
            }
        }
        #[cfg(not(feature = "postgres"))]
        StorageType::Postgres => {
            warn!("PostgreSQL checkpoint store unavailable: enable the 'postgres' feature");
            StorageBackend::new_memory()
        }
    };
    Arc::new(backend)
}

#[cfg(feature = "sqlite")]
pub async fn init_event_persistence(
    config: &StorageConfig,
) -> Option<Arc<dyn wf_api::PersistenceLayer>> {
    use wf_api::PersistenceLayer as ApiPersistenceLayer;

    if config.storage_type != StorageType::Sqlite {
        return None;
    }
    let db_path = storage_db_path(config);

    let layer = match wf_api::StorePersistenceLayer::sqlite(&db_path.to_string_lossy()).await {
        Ok(store) => Arc::new(wf_api::BufferedPersistenceLayer::new(Arc::new(store))),
        Err(err) => {
            warn!(error = %err, path = %db_path.display(), "failed to open event persistence backend; events stay in memory");
            return None;
        }
    };
    if let Err(err) = layer.initialize().await {
        warn!(error = %err, "failed to initialize event persistence backend; events stay in memory");
        return None;
    }
    info!("Event persistence enabled: sqlite at {:?}", db_path);
    Some(layer as Arc<dyn ApiPersistenceLayer>)
}

#[cfg(not(feature = "sqlite"))]
pub async fn init_event_persistence(
    _config: &StorageConfig,
) -> Option<Arc<dyn wf_api::PersistenceLayer>> {
    None
}

pub async fn resolve_infra_config(
    mut config: RuntimeConfig,
    infra: &InfraSourceConfig,
) -> RuntimeResult<RuntimeConfig> {
    let project_root = infra.project_root.clone().unwrap_or_default();
    let preset_name = infra
        .preset_name
        .clone()
        .unwrap_or_else(|| wf_config::orchestrator::DEFAULT_INFRA_PRESET.to_string());

    let assembled = ConfigOrchestrator::assemble_with_preset(
        &project_root,
        Some(&preset_name),
        Some(default_infra_file_mapping()),
        Some(infra.overrides.clone()),
    )
    .map_err(|e| {
        crate::error::RuntimeError::Config(format!(
            "Infrastructure config resolution failed (preset `{preset_name}`): {e}"
        ))
    })?;

    if config.storage == StorageConfig::default() {
        config.storage = assembled.storage;
    }
    if config.timeout == wf_types::config::timeout::TimeoutConfig::default() {
        config.timeout = assembled.timeout;
    }
    if config.output == wf_types::config::output::OutputConfig::default() {
        config.output = assembled.output;
    }
    if config.metrics.is_none() {
        config.metrics = Some(assembled.metrics);
    }
    if config.sandbox.is_none() {
        config.sandbox = assembled.sandbox;
    }
    if config.presets == wf_types::config::presets::PresetsConfig::default() {
        config.presets = assembled.presets;
    }
    if config.tools == wf_config::orchestrator::ToolConfigs::default() {
        config.tools = assembled.tools;
    }
    if config.file_checkpoint == FileCheckpointConfig::default() {
        config.file_checkpoint = assembled.file_checkpoint;
    }
    if config.tool_approval == wf_types::config::tool_approval::ToolApprovalConfig::default() {
        config.tool_approval = assembled.tool_approval;
    }
    if config.limits == wf_types::config::limits::LimitsConfig::default() {
        config.limits = assembled.limits;
    }

    // Skill settings chain (global -> project, or collection mode). Lenient:
    // a missing/invalid skill config falls back to the defaults.
    if config.skills == wf_types::skill::SkillConfig::default() {
        let settings_dir = infra
            .settings_dir
            .as_deref()
            .unwrap_or_else(|| Path::new(""));
        let skills = match &infra.skills_collection {
            Some(name) => wf_config::skill::load_and_merge_skill_config_with_collection(
                settings_dir,
                &project_root,
                Some(name),
            ),
            None => wf_config::skill::load_and_merge_skill_config(settings_dir, &project_root),
        };
        match skills {
            Ok(skills) => config.skills = skills,
            Err(e) => warn!(error = %e, "failed to load skill settings chain; keeping defaults"),
        }
    }

    // MCP settings chain sources are inherited when not set explicitly.
    if config.mcp.settings_dir.is_none() {
        config.mcp.settings_dir = infra.settings_dir.clone();
    }
    if config.mcp.project_root.is_none() {
        config.mcp.project_root = infra.project_root.clone();
    }

    Ok(config)
}

pub fn init_llm_gateway(
    config: &LlmConfig,
    metrics: Option<&wf_metrics::MetricsRegistry>,
) -> RuntimeResult<Arc<LlmGateway>> {
    let mut gateway = LlmGateway::new();

    for profile in &config.profiles {
        validate_llm_profile(profile).map_err(|e| {
            crate::error::RuntimeError::Config(format!("Invalid LLM profile: {}", e))
        })?;
        let transformed = transform_llm_profile(profile, &std::collections::HashMap::new())
            .map_err(|e| {
                crate::error::RuntimeError::Config(format!("Invalid LLM profile: {}", e))
            })?;
        gateway.register_profile(transformed).map_err(|e| {
            crate::error::RuntimeError::Config(format!("Failed to register LLM profile: {}", e))
        })?;
    }

    if let Some(registry) = metrics {
        gateway = gateway.with_token_metrics(registry.token().as_ref().clone());
    }

    Ok(Arc::new(gateway))
}

pub async fn init_mcp(
    config: &McpRuntimeConfig,
) -> Option<Arc<wf_tools::mcp::connection::McpConnectionManager>> {
    use wf_tools::mcp::connection::{McpConnectionManager, McpServerRegistry};

    let (Some(settings_dir), Some(project_root)) = (&config.settings_dir, &config.project_root)
    else {
        return None;
    };

    let settings =
        wf_config::mcp::load_and_merge_mcp_settings(settings_dir, project_root).unwrap_or_default();
    if settings.mcp_servers.is_empty() {
        return None;
    }

    let registry = Arc::new(McpServerRegistry::new());
    let manager = Arc::new(McpConnectionManager::new(registry));
    for (name, server_config) in &settings.mcp_servers {
        if let Err(e) = manager.connect_server(name, server_config.clone()).await {
            tracing::warn!("MCP server '{}' failed to connect: {}", name, e);
        }
    }
    Some(manager)
}

pub fn activate_builtin_resource_plugins_legacy(
    bundles: &wf_resource::resource_plugin::ResourcePluginRegistry,
    opts: &wf_resource::registry::RegisterOptions,
    registries: &wf_resource::registry::ResourceRegistries,
    tool_registry: &wf_tools::registry::ToolRegistry,
) -> RuntimeResult<()> {
    for plugin in wf_resource::predefined::resource_plugin::builtin_resource_plugins() {
        bundles.register(plugin).map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "failed to register built-in resource plugin: {e}"
            ))
        })?;
    }
    for sa in &opts.resource_plugin_activation {
        bundles
            .activate(
                &sa.id,
                &sa.config,
                registries,
                tool_registry,
                opts.skip_if_exists,
            )
            .map_err(|e| {
                crate::error::RuntimeError::Config(format!(
                    "failed to activate resource plugin '{}': {e}",
                    sa.id
                ))
            })?;
    }
    Ok(())
}

#[cfg(feature = "plugins")]
pub async fn init_plugins(
    config: &super::PluginConfig,
    registries: Arc<wf_resource::registry::ResourceRegistries>,
    tool_registry: Arc<wf_tools::registry::ToolRegistry>,
) -> RuntimeResult<Option<wf_plugin::PluginEngine>> {
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
    let bridge: Option<Arc<dyn wf_plugin::ContributionBridge>> = Some(Arc::new(
        crate::plugin_bridge::WfPluginBridge::new(registries, tool_registry),
    ));

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

pub fn adjust_log_config(mut config: LogConfig, mode_info: &ModeInfo) -> LogConfig {
    if mode_info.is_json_mode() && matches!(config.format, crate::logger::LogFormat::Full) {
        config.format = crate::logger::LogFormat::Json;
    }

    if mode_info.is_silent_mode() {
        config.level = "off".to_string();
    }

    config
}
