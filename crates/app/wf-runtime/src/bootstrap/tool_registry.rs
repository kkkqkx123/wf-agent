use std::sync::Arc;

use tracing::{info, warn};

use crate::error::RuntimeResult;

pub async fn init_tool_registry_with_mcp(
    shell_config: &mut wf_shell::config::ShellToolConfig,
    sandbox_runtime: &Arc<wf_sandbox::SandboxRuntime>,
    skill_loader: Arc<wf_tools::SkillLoader>,
    mcp_manager: &Option<Arc<wf_tools::mcp::connection::McpConnectionManager>>,
    event_bus: &Arc<wf_core::event::EventBus>,
    code_context: Option<wf_integration::CodeContextConfig>,
) -> RuntimeResult<Arc<wf_tools::registry::ToolRegistry>> {
    if shell_config.output_event_enabled && shell_config.event_sink.is_none() {
        shell_config.event_sink = Some(Arc::new(
            crate::shell_event_bridge::ShellEventBusBridge::new(event_bus.clone()),
        ));
    }
    shell_config.sandbox_policy = Some(sandbox_runtime.default_policy().clone());
    let tool_registry = Arc::new(wf_tools::registry::ToolRegistry::new());
    wf_tools::register_builtin_handlers(
        &tool_registry,
        wf_tools::BuiltinHandlersConfig {
            shell: shell_config.clone(),
            code_context: code_context.unwrap_or_default(),
            ..Default::default()
        },
    )
    .map_err(|e| {
        crate::error::RuntimeError::Config(format!("Failed to register builtin handlers: {}", e))
    })?;
    tool_registry.set_skill_loader(skill_loader);
    if let Some(manager) = mcp_manager {
        tool_registry.set_mcp_manager(manager.clone());
        let registry = tool_registry.clone();
        let manager_clone = manager.clone();
        manager.set_on_connected(Arc::new(move |_server| {
            wf_tools::mcp::registration::register_connected_tools(&registry, &manager_clone);
        }));
        wf_tools::mcp::registration::register_use_mcp(&tool_registry).map_err(|e| {
            crate::error::RuntimeError::Config(format!("Failed to register use_mcp: {}", e))
        })?;
        wf_tools::mcp::registration::register_connected_tools(&tool_registry, manager);
    }
    Ok(tool_registry)
}

pub async fn hydrate_tool_registry_from_storage(
    tool_registry: &wf_tools::registry::ToolRegistry,
    storage_manager: &crate::storage_manager::StorageManager,
) {
    let Some(ctx) = storage_manager.shared_context() else {
        return;
    };
    let bridge = crate::tool_storage::StorageToolBridge::new(ctx.tool_definition.clone());
    match tool_registry.initialize_from_storage(&bridge).await {
        Ok(()) => {
            if tool_registry.tool_count() > 0 {
                info!(
                    "Tool registry hydrated from storage: {} persisted tools",
                    tool_registry.tool_count()
                );
            }
        }
        Err(err) => {
            warn!(error = %err, "failed to restore persisted tools; registry continues with runtime-registered tools");
        }
    }
}
