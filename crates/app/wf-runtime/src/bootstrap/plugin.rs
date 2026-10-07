use tracing::info;

use crate::error::RuntimeResult;

#[cfg(feature = "plugins")]
use std::sync::Arc;

#[cfg(feature = "plugins")]
use wf_llm::LlmGateway;

#[cfg(feature = "plugins")]
use super::config::PluginConfig;

/// Direct fallback for builds without the plugin engine: assemble each
/// requested built-in bundle and land it through the shared
/// `install_bundle` helper. Item-level rejections are reported loudly but
/// do not fail bootstrap, matching the engine bridge semantics; hook
/// failures fail loudly.
fn activate_builtin_resource_assemblers_legacy(
    opts: &wf_resource::registry::RegisterOptions,
    registries: &wf_resource::registry::ResourceRegistries,
    tool_registry: &wf_tools::registry::ToolRegistry,
) -> RuntimeResult<()> {
    for assembler in wf_resource::predefined::resource_assembler::builtin_resource_assemblers() {
        let meta = assembler.metadata();
        let Some(requested) = opts
            .resource_assembler_activation
            .iter()
            .find(|sa| sa.id == meta.id)
        else {
            continue;
        };
        assembler
            .on_before_assemble(&requested.config)
            .map_err(|e| {
                crate::error::RuntimeError::Config(format!(
                    "failed to activate resource assembler '{}': {e}",
                    meta.id
                ))
            })?;
        let bundle = assembler.assemble(&requested.config).map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "failed to activate resource assembler '{}': {e}",
                meta.id
            ))
        })?;
        let summary = wf_resource::resource_assembler::install_bundle(
            registries,
            tool_registry,
            &bundle,
            opts.skip_if_exists,
        );
        for fail in &summary.failed {
            tracing::warn!(
                assembler_id = %meta.id,
                resource = %fail.id,
                "resource assembler item registration failed: {}",
                fail.error
            );
        }
        assembler.on_after_install(&bundle).map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "failed to activate resource assembler '{}': {e}",
                meta.id
            ))
        })?;
    }
    Ok(())
}

#[cfg(feature = "plugins")]
pub async fn init_plugins(
    config: &PluginConfig,
    registries: Arc<wf_resource::registry::ResourceRegistries>,
    tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    llm_gateway: Arc<LlmGateway>,
) -> RuntimeResult<Option<wf_plugin::PluginEngine>> {
    if !config.enabled {
        return Ok(None);
    }

    let plugin_config = wf_plugin::PluginSystemConfig {
        enabled: true,
        paths: config.paths.clone(),
        auto_activate: config.auto_activate,
        guard_timeout_ms: config.guard_timeout_ms,
        lua_enabled: config.lua_enabled,
        native_enabled: config.native_enabled,
        wasm_enabled: config.wasm_enabled,
        ..Default::default()
    };

    let registry = Arc::new(wf_plugin::PluginRegistry::new());
    let contribution_manager = Arc::new(wf_plugin::ContributionManager::new());
    let bridge: Option<Arc<dyn wf_plugin::ContributionBridge>> = Some(Arc::new(
        crate::plugin_bridge::WfPluginBridge::new(registries, tool_registry, llm_gateway),
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

/// Bootstrap order is intentional: resource bundle assemblers activate
/// first so their output wins under skip-existing semantics, then the
/// predefined/custom batch fills the remaining ids. Both sides land through
/// the shared `install_bundle` helper, keeping direct installation and the
/// plugin-engine bridge identical.
pub async fn init_plugins_and_resources(
    opts: &wf_resource::registry::RegisterOptions,
    registries: &wf_resource::registry::ResourceRegistries,
    tool_registry: &wf_tools::registry::ToolRegistry,
    #[cfg(feature = "plugins")] plugin_engine: &Option<wf_plugin::PluginEngine>,
) -> RuntimeResult<()> {
    #[cfg(feature = "plugins")]
    match plugin_engine {
        Some(engine) => {
            crate::resource_assembler_adapter::activate_builtin_resource_assemblers_via_engine(
                engine, opts,
            )
            .await?;
        }
        None => {
            activate_builtin_resource_assemblers_legacy(opts, registries, tool_registry)?;
        }
    };
    #[cfg(not(feature = "plugins"))]
    activate_builtin_resource_assemblers_legacy(opts, registries, tool_registry)?;

    let resource_result = wf_resource::register_all(registries, tool_registry, opts);
    info!(
        "Resource registration: {} succeeded, {} failed",
        resource_result.succeeded.len(),
        resource_result.failed.len(),
    );
    for fail in &resource_result.failed {
        tracing::warn!("Resource registration failed: {} - {}", fail.id, fail.error);
    }
    Ok(())
}
