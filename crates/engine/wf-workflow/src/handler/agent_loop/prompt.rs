//! System-prompt assembly for an `AGENT_LOOP` run: the configured base prompt
//! (inline or template-rendered), the dynamic context block, and the skill /
//! MCP progressive-disclosure enrichment.

use serde_json::Value;

use wf_execution_shared::context::NodeExecutionContext;
use wf_tools::registry::ToolRegistry;

/// Build the effective system prompt: the configured base (inline or template)
/// with the dynamic context block prepended, then enriched with enabled-skill
/// metadata and the MCP server summary when the agent can reach those tools.
/// Returns `None` when no base/dynamic prompt is configured (the enrichment
/// only ever augments an existing prompt).
pub(crate) fn build_system_prompt(
    ctx: &NodeExecutionContext,
    agent_config: Option<&wf_types::agent::AgentConfig>,
    available_tool_names: &[String],
) -> Option<String> {
    let template_metrics = ctx.metrics.as_ref().map(|m| m.template());
    let base = resolve_configured_system_prompt(
        agent_config,
        ctx.resource_registries.as_deref(),
        template_metrics.as_deref(),
    );
    let dynamic = build_dynamic_system_context(agent_config, ctx.tool_registry.as_deref());
    let prompt = match (base, dynamic) {
        (Some(sp), Some(dynamic_block)) => Some(format!("{}\n\n{}", dynamic_block, sp)),
        (Some(sp), None) => Some(sp),
        (None, dynamic_block) => dynamic_block,
    }?;
    Some(enrich_system_prompt(ctx, &prompt, available_tool_names))
}

/// Progressive disclosure enrichment: inject enabled-skill metadata when the
/// agent has the `skill` tool, and append a compact MCP server/tool summary
/// when it can use MCP tools.
fn enrich_system_prompt(
    ctx: &NodeExecutionContext,
    system_prompt: &str,
    available_tool_names: &[String],
) -> String {
    let has_skill_tool = available_tool_names.iter().any(|name| name == "skill");
    let skill_injected = if has_skill_tool {
        match ctx
            .tool_registry
            .as_ref()
            .and_then(|registry| registry.skill_loader())
        {
            Some(loader) => {
                let enabled = loader.get_enabled_skills();
                if enabled.is_empty() {
                    system_prompt.to_string()
                } else {
                    wf_tools::skill::inject_skill_metadata(system_prompt, &enabled)
                }
            }
            None => system_prompt.to_string(),
        }
    } else {
        system_prompt.to_string()
    };

    // MCP LLM visibility: when the agent can use MCP tools (the generic
    // `use_mcp` tool or registered `mcp_*` tools) and a connection manager is
    // available, append a compact server/tool summary to the prompt.
    let has_mcp_tools = available_tool_names
        .iter()
        .any(|name| name == "use_mcp" || name.starts_with("mcp_"));
    if !has_mcp_tools {
        return skill_injected;
    }
    match ctx
        .tool_registry
        .as_ref()
        .and_then(|registry| registry.mcp_manager())
    {
        Some(manager) => {
            let provider = wf_tools::mcp::McpToolsDynamicContextProvider::new((*manager).clone());
            let generated =
                provider.generate_context(&wf_tools::mcp::McpToolsContextOptions::default());
            if generated.has_servers && !generated.content.is_empty() {
                format!("{}\n\n{}", skill_injected, generated.content)
            } else {
                skill_injected
            }
        }
        None => skill_injected,
    }
}

/// Resolve the configured system prompt: an inline `system_prompt` wins;
/// otherwise `system_prompt_template_id` is rendered through the unified
/// template engine (fragments pseudo variable and `{{var}}` substitution
/// included) with `system_prompt_template_variables` as values. Executions
/// without injected registries or without any prompt config return `None`.
fn resolve_configured_system_prompt(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    regs: Option<&wf_resource::ResourceRegistries>,
    template_metrics: Option<&wf_metrics::TemplateMetricsCollector>,
) -> Option<String> {
    let config = agent_config?;
    if let Some(ref sp) = config.system_prompt {
        return Some(sp.clone());
    }
    let template_id = config.system_prompt_template_id.as_deref()?;
    let regs = regs?;
    let mut variables = std::collections::HashMap::new();
    if let Some(ref meta) = config.system_prompt_template_variables {
        for (key, value) in meta {
            let rendered = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            variables.insert(key.clone(), rendered);
        }
    }
    wf_resource::render_template_with_metrics(
        regs,
        template_id,
        &wf_resource::TemplateRenderOptions {
            variables,
            ..Default::default()
        },
        template_metrics,
    )
}

/// Build the dynamic system-context block declared by the agent config
/// (current time / environment / enabled skills / custom sections) and
/// prepend it to the base prompt. Returns `None` when nothing is enabled.
fn build_dynamic_system_context(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    tool_registry: Option<&ToolRegistry>,
) -> Option<String> {
    let dyn_cfg = agent_config?.dynamic_context.as_ref()?;
    let has_any = dyn_cfg.include_current_time.unwrap_or(false)
        || dyn_cfg.include_environment_info.unwrap_or(false)
        || dyn_cfg.include_skills.unwrap_or(false)
        || dyn_cfg.include_workflows.unwrap_or(false)
        || dyn_cfg
            .custom_sections
            .as_ref()
            .map(|m| !m.is_empty())
            .unwrap_or(false);
    if !has_any {
        return None;
    }

    let mut system_cfg = wf_resource::SystemConfig {
        include_time: dyn_cfg.include_current_time.unwrap_or(false),
        include_env: dyn_cfg.include_environment_info.unwrap_or(false),
        ..Default::default()
    };
    if dyn_cfg.include_skills.unwrap_or(false) {
        if let Some(loader) = tool_registry.and_then(|registry| registry.skill_loader()) {
            system_cfg.skills = loader
                .get_enabled_skills()
                .into_iter()
                .map(|s| format!("{}: {}", s.name, s.description))
                .collect();
        }
    }
    if let Some(ref sections) = dyn_cfg.custom_sections {
        system_cfg.custom_sections = sections
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
    }

    let ctx = wf_resource::build_system_context(&system_cfg);
    if ctx.is_empty() {
        None
    } else {
        Some(ctx)
    }
}
