use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use wf_core::Registry;
use wf_plugin::ContributionManager;
use wf_resource::registry::ResourceRegistries;
use wf_tools::error::ToolResult;
use wf_tools::executor::trait_def::ToolExecutionContext;
use wf_tools::registry::ToolRegistry;

pub(crate) fn log_behavioral_contributions(manager: &ContributionManager) {
    for (name, _) in manager.all_node_types() {
        tracing::debug!("  node-type: {}", name);
    }
    for (name, _) in manager.all_event_handlers() {
        tracing::debug!("  event-handler: {}", name);
    }
    for phase in manager.all_middleware_phases() {
        tracing::debug!("  middleware: {}", phase);
    }
}

pub(crate) fn sync_tool_handlers(
    tool_registry: &ToolRegistry,
    plugin_id: &str,
    manager: &ContributionManager,
) {
    let owned_tools: Vec<String> = manager
        .all_tools()
        .into_iter()
        .filter_map(|(id, owner)| (owner == plugin_id).then_some(id))
        .collect();
    let owned_types: Vec<String> = manager
        .all_tool_types()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    if !owned_types.is_empty() && owned_tools.is_empty() {
        tracing::warn!(
            plugin_id,
            tool_types = ?owned_types,
            "plugin registers tool-type executors but no declarative tools; handlers will not install"
        );
    }
    if !owned_tools.is_empty()
        && manager
            .all_tool_types()
            .into_iter()
            .all(|(_, owner)| owner != plugin_id)
    {
        tracing::warn!(
            plugin_id,
            tools = ?owned_tools,
            "plugin contributes tools without a paired tool-type executor; calls will resolve to NotFound until an executor is registered"
        );
    }
    if owned_types.len() > 1 && !owned_tools.is_empty() {
        tracing::warn!(
            plugin_id,
            tool_types = ?owned_types,
            tools = ?owned_tools,
            "multiple tool-type executors share one plugin tool set; last registration wins per tool id"
        );
    }
    for (type_name, _) in manager.all_tool_types() {
        if let Some(executor) = manager.get_tool_executor(&type_name) {
            let handler = make_plugin_tool_handler(executor.clone());
            for (id, owner) in manager.all_tools() {
                if owner == plugin_id {
                    tool_registry.register_stateless_async_handler(&id, handler.clone());
                    tracing::debug!("  tool-type: {} → handler for '{}'", type_name, id);
                }
            }
        }
    }
}

pub(crate) fn collect_bundle(
    plugin_id: &str,
    manager: &ContributionManager,
) -> wf_resource::resource_assembler::ResourceBundle {
    let mut bundle = wf_resource::resource_assembler::ResourceBundle::new();
    for (id, owner) in manager.all_workflows() {
        if owner == plugin_id {
            if let Some(wf) = manager.get_workflow(&id) {
                bundle.workflows.push((*wf).clone());
            }
        }
    }
    for (id, owner) in manager.all_prompts() {
        if owner == plugin_id {
            if let Some(t) = manager.get_prompt(&id) {
                bundle.prompts.push((*t).clone());
            }
        }
    }
    for (id, owner) in manager.all_fragments() {
        if owner == plugin_id {
            if let Some(f) = manager.get_fragment(&id) {
                bundle.fragments.push((*f).clone());
            }
        }
    }
    for (id, owner) in manager.all_agent_templates() {
        if owner == plugin_id {
            if let Some(a) = manager.get_agent_template(&id) {
                bundle.agent_templates.push((*a).clone());
            }
        }
    }
    for (id, owner) in manager.all_node_templates() {
        if owner == plugin_id {
            if let Some(n) = manager.get_node_template(&id) {
                bundle.node_templates.push((*n).clone());
            }
        }
    }
    for (id, owner) in manager.all_triggers() {
        if owner == plugin_id {
            if let Some(t) = manager.get_trigger(&id) {
                bundle.triggers.push((*t).clone());
            }
        }
    }
    for (id, owner) in manager.all_tool_descriptions() {
        if owner == plugin_id {
            if let Some(d) = manager.get_tool_description(&id) {
                bundle.tool_descriptions.push((*d).clone());
            }
        }
    }
    for (id, owner) in manager.all_tools() {
        if owner == plugin_id {
            if let Some(tool) = manager.get_tool(&id) {
                bundle.tools.push((*tool).clone());
            }
        }
    }
    bundle
}

pub(crate) fn install_resources(
    registries: &ResourceRegistries,
    tool_registry: &ToolRegistry,
    bundle: &wf_resource::resource_assembler::ResourceBundle,
    plugin_id: &str,
) {
    for tool in &bundle.tools {
        if tool_registry.has(&tool.id) {
            tracing::warn!(
                plugin_id,
                tool = %tool.id,
                "plugin tool id collides with a registered tool; install_bundle will skip it"
            );
        }
    }
    let summary = wf_resource::resource_assembler::install_bundle(
        registries,
        tool_registry,
        bundle,
        true,
    );
    for id in &summary.succeeded {
        tracing::debug!("  resource: {}", id);
    }
    for fail in &summary.failed {
        tracing::warn!(
            plugin_id,
            resource = %fail.id,
            "plugin resource registration failed: {}",
            fail.error
        );
    }
}

pub(crate) fn uninstall_resources(
    registries: &ResourceRegistries,
    tool_registry: &ToolRegistry,
    bundle: &wf_resource::resource_assembler::ResourceBundle,
) {
    wf_resource::resource_assembler::uninstall_bundle(registries, tool_registry, bundle);
    for tool in &bundle.tools {
        tool_registry.unregister_stateless_handler(&tool.id);
    }
}

pub(crate) fn make_plugin_tool_handler(
    executor: Arc<dyn wf_plugin::PluginToolExecutor>,
) -> wf_tools::executor::StatelessAsyncHandler {
    Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let executor = executor.clone();
        Box::pin(async move {
            executor
                .execute(wf_plugin::PluginToolContext { args })
                .await
                .map(|result| result.result)
                .map_err(|e| wf_tools::ToolError::ExecutionFailed {
                    tool_id: "plugin-tool".to_string(),
                    reason: e.to_string(),
                })
        }) as Pin<Box<dyn Future<Output = ToolResult<Value>> + Send>>
    })
}
