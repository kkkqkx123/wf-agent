use std::collections::HashMap;

use serde_json::Value;
use wf_core::registry::Registry;
use wf_resource::registry::ResourceRegistries;

use super::variables::PromptEnvironment;

/// Build the stable system header. Returns `None` when no base prompt and no
/// stable dynamic section is configured.
pub fn build_stable_header(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    env: &PromptEnvironment,
    available: wf_tools::tool_exposure::AvailablePool<'_>,
    working_directory: Option<&str>,
) -> Option<String> {
    let template_metrics = env.template_metrics();
    let base = resolve_configured_system_prompt(
        agent_config,
        env.resource_registries,
        template_metrics.as_deref(),
    );
    let dynamic = build_dynamic_system_context(agent_config, env, available, working_directory);
    let prompt = match (base, dynamic) {
        (Some(sp), Some(block)) => Some(format!("{}\n\n{}", block, sp)),
        (Some(sp), None) => Some(sp),
        (None, block) => block,
    }?;
    Some(enrich_system_prompt(env, &prompt, available))
}

fn resolve_configured_system_prompt(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    regs: Option<&ResourceRegistries>,
    template_metrics: Option<&wf_metrics::TemplateMetricsCollector>,
) -> Option<String> {
    let config = agent_config?;
    resolve_system_prompt_text(
        config.system_prompt.as_deref(),
        config.system_prompt_template_id.as_deref(),
        config.system_prompt_template_variables.as_ref(),
        regs,
        template_metrics,
    )
}

/// Shared system prompt resolution with agent-loop priority: inline text
/// wins without rendering, otherwise the template reference renders through
/// the shared registry. Model-bound entry with shape checks and residual
/// denial, so partial prompts never reach the model. Inline text carrying
/// placeholder shapes stays literal by design; interpolation needs a
/// template reference. Single home for both the agent assembly and the
/// lightweight model node so the two entries cannot drift.
pub fn resolve_system_prompt_text(
    system_prompt: Option<&str>,
    template_id: Option<&str>,
    variables: Option<&HashMap<String, Value>>,
    regs: Option<&ResourceRegistries>,
    template_metrics: Option<&wf_metrics::TemplateMetricsCollector>,
) -> Option<String> {
    if let Some(sp) = system_prompt {
        if template_id.is_some() {
            tracing::warn!(
                "both inline system_prompt and system_prompt_template_id are set; inline text wins and the template reference is ignored"
            );
        }
        if sp.contains("{{") {
            tracing::warn!(
                "inline system_prompt contains placeholder shapes but inline text is never rendered; use system_prompt_template_id for interpolation"
            );
        }
        return Some(sp.to_string());
    }
    let template_id = template_id?;
    let regs = regs?;
    let empty = HashMap::new();
    let variables = variables.unwrap_or(&empty);
    wf_resource::render_template_for_model(regs, template_id, variables, template_metrics)
}

fn pool_has_tool(
    available: wf_tools::tool_exposure::AvailablePool<'_>,
    registry: Option<&wf_tools::registry::ToolRegistry>,
    name: &str,
) -> bool {
    match available {
        wf_tools::tool_exposure::AvailablePool::Explicit(names) => {
            names.iter().any(|candidate| candidate == name)
        }
        wf_tools::tool_exposure::AvailablePool::Unset => registry
            .map(|registry| registry.list_tools().iter().any(|tool| tool.name == name))
            .unwrap_or(false),
    }
}

fn pool_has_mcp_tools(
    available: wf_tools::tool_exposure::AvailablePool<'_>,
    registry: Option<&wf_tools::registry::ToolRegistry>,
) -> bool {
    match available {
        wf_tools::tool_exposure::AvailablePool::Explicit(names) => names
            .iter()
            .any(|name| name == "use_mcp" || name.starts_with("mcp_")),
        wf_tools::tool_exposure::AvailablePool::Unset => registry
            .map(|registry| {
                registry
                    .list_tools()
                    .iter()
                    .any(|tool| tool.name == "use_mcp" || tool.name.starts_with("mcp_"))
            })
            .unwrap_or(false),
    }
}

fn build_dynamic_system_context(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    env: &PromptEnvironment,
    available: wf_tools::tool_exposure::AvailablePool<'_>,
    working_directory: Option<&str>,
) -> Option<String> {
    let dyn_cfg = agent_config?.dynamic_context.as_ref()?;
    let has_any = dyn_cfg.include_environment_info.unwrap_or(false)
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
        include_env: dyn_cfg.include_environment_info.unwrap_or(false),
        working_directory: working_directory.map(str::to_string),
        ..Default::default()
    };
    if dyn_cfg.include_skills.unwrap_or(false) {
        // Progressive disclosure wins: when the `skill` tool is available,
        // skill presentation moves to the injected skill metadata block
        // (see `enrich_system_prompt`) and the static list is skipped so
        // skills are never announced twice. Both paths read the same
        // loader, so skipping loses nothing when nothing is enabled.
        let progressive_disclosure = pool_has_tool(available, env.tool_registry, "skill");
        if !progressive_disclosure {
            if let Some(loader) = env
                .tool_registry
                .and_then(|registry| registry.skill_loader())
            {
                system_cfg.include_skills = true;
                system_cfg.skills = loader
                    .get_enabled_skills()
                    .into_iter()
                    .map(|s| format!("{}: {}", s.name, s.description))
                    .collect();
            }
        }
    }
    if dyn_cfg.include_workflows.unwrap_or(false) {
        system_cfg.include_workflows = true;
        if let Some(regs) = env.resource_registries {
            system_cfg.workflows = regs
                .workflows
                .list()
                .iter()
                .filter_map(|key| regs.workflows.get(key))
                .map(|template| {
                    if template.description.is_empty() {
                        template.name.clone()
                    } else {
                        format!("{}: {}", template.name, template.description)
                    }
                })
                .collect();
        }
    }
    if let Some(ref sections) = dyn_cfg.custom_sections {
        system_cfg.custom_sections = sections
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
    }

    let rendered = wf_resource::build_system_context(&system_cfg);
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

fn enrich_system_prompt(
    env: &PromptEnvironment,
    system_prompt: &str,
    available: wf_tools::tool_exposure::AvailablePool<'_>,
) -> String {
    let has_skill_tool = pool_has_tool(available, env.tool_registry, "skill");
    let skill_injected = if has_skill_tool {
        match env
            .tool_registry
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

    let has_mcp_tools = pool_has_mcp_tools(available, env.tool_registry);
    if !has_mcp_tools {
        return skill_injected;
    }
    match env
        .tool_registry
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
