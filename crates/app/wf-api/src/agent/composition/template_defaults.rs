
use wf_execution_shared::agent_prompt::PromptEnvironment;
use wf_resource::registry::ResourceRegistries;
use wf_tools::callback::AgentLoopConfig;
use wf_types::agent::{AgentConfig, AgentTemplate};
use wf_types::tool::AvailableTools;

use crate::agent::agent_config::{DEFAULT_MAX_ITERATIONS, DEFAULT_MODEL};
use crate::infra::error::ApiError;

use super::*;

fn requests_builtin_main(agent_id: &str) -> bool {
    agent_id == MAIN_AGENT_TEMPLATE_ID
}

/// Resolve the effective agent template for a loop config.
///
/// Exact registry lookup under `agent_id`; the built-in id resolves to
/// the built-in `@standard/main` entry. Returns `None` when nothing
/// applies.
pub fn resolve_template(regs: &ResourceRegistries, agent_id: &str) -> Option<AgentTemplate> {
    use wf_core::registry::Registry;
    if !requests_builtin_main(agent_id) {
        return regs
            .agent_templates
            .get(agent_id)
            .map(|t| t.as_ref().clone());
    }
    regs.agent_templates
        .get(MAIN_AGENT_TEMPLATE_ID)
        .map(|t| t.as_ref().clone())
}

/// Apply the template's agent config as defaults under the caller's config.
///
/// Only unset caller fields are filled; every explicitly set caller field
/// wins. Initial messages seed an empty conversation; the stable header and
/// volatile tail are assembled by the shared prompt module after this step,
/// so inline seeding no longer lives here.
pub fn apply_template_defaults(
    mut config: AgentLoopConfig,
    template: &AgentTemplate,
    input: &mut wf_tools::callback::AgentLoopInput,
) -> AgentLoopConfig {
    let Some(cfg) = template.definition.config.as_ref() else {
        return config;
    };
    apply_agent_config_defaults(&mut config, cfg);
    super::prompt::seed_initial_messages(&mut input.conversation, cfg);
    config
}

fn apply_agent_config_defaults(config: &mut AgentLoopConfig, cfg: &AgentConfig) {
    if config.model == DEFAULT_MODEL || config.model.is_empty() {
        if let Some(profile_id) = cfg.profile_id.as_ref() {
            config.model = profile_id.clone();
        }
    }
    if config.max_iterations.is_none() {
        config.max_iterations = cfg.max_iterations;
    }
    if config.max_execution_time.is_none() {
        config.max_execution_time = cfg.max_execution_time;
    }
    if config.token_limit.is_none() {
        config.token_limit = cfg.token_limit;
    }
    if config.token_warning_threshold.is_none() {
        config.token_warning_threshold = cfg.token_warning_threshold;
    }
    if config.enable_token_tracking.is_none() {
        config.enable_token_tracking = cfg.enable_token_tracking;
    }
    if config.tool_call_protocol.is_none() {
        config.tool_call_protocol = cfg
            .tool_call_protocol
            .as_deref()
            .and_then(wf_types::llm::tool_call_protocol::ToolCallProtocolConfig::from_protocol_str);
    }
    if config.checkpoint_message_interval.is_none() {
        if let Some(interval) = cfg.checkpoint.as_ref().and_then(|c| c.message_interval) {
            if interval > 0 {
                config.checkpoint_message_interval = Some(interval);
            }
        }
    }
    if let Some(tools) = cfg.available_tools.as_ref() {
        apply_tool_defaults(config, tools);
    }
    if config.hooks.is_empty() {
        if let Some(hooks) = cfg.hooks.as_ref() {
            config.hooks = super::hooks::agent_hooks_to_loop(hooks);
        }
    }
}

fn apply_tool_defaults(config: &mut AgentLoopConfig, tools: &AvailableTools) {
    // `None` = unset, falls back to the definition layer; `Some(vec![])` =
    // explicit empty set, disabling every tool.
    if config.available_tool_names.is_none() {
        config.available_tool_names = Some(tools.available.clone());
    }
    if config.initial_tool_names.is_empty() {
        config.initial_tool_names = tools.initial.clone().unwrap_or_default();
    }
    if config.discoverable_tool_names.is_empty() {
        config.discoverable_tool_names = tools.discoverable.clone().unwrap_or_default();
    }
    if config.hidden_tool_names.is_empty() {
        if let Some(hidden) = tools.hidden.clone() {
            config.hidden_tool_names = hidden;
        }
    }
    if config.enable_general_tool.is_none() {
        config.enable_general_tool = tools.enable_general_tool;
    }
}

/// Seed template initial messages when the imported conversation is empty.
fn ensure_runnable(template: &AgentTemplate) -> Result<(), ApiError> {
    let runnable = template.enabled != Some(false)
        && template.is_public != Some(false)
        && template.is_system != Some(true);
    if runnable {
        Ok(())
    } else {
        Err(ApiError::not_found("agent_template", template.id.as_str()))
    }
}

/// Resolve the effective template for a config/input pair and apply its
/// defaults. See the module docs for the resolution semantics. After template
/// defaults the shared prompt module produces the stable header, volatile
/// tail and tool exposure blocks so both entries render identically.
pub fn resolve_and_apply(
    env: &PromptEnvironment,
    config: AgentLoopConfig,
    mut input: wf_tools::callback::AgentLoopInput,
) -> crate::infra::error::ApiResult<(AgentLoopConfig, wf_tools::callback::AgentLoopInput)> {
    let agent_id = config.agent_id.as_str();
    if agent_id.is_empty() {
        return Err(ApiError::not_found(
            "agent_template",
            MAIN_AGENT_TEMPLATE_ID,
        ));
    }
    let Some(regs) = env.resource_registries else {
        return Ok((apply_final_defaults(config), input));
    };
    let Some(template) = resolve_template(regs, agent_id) else {
        // An unregistered explicit id has no template semantics: reject
        // instead of silently running on built-in defaults under a label
        // that suggests a restricted agent was selected.
        return Err(ApiError::not_found("agent_template", agent_id));
    };
    ensure_runnable(&template)?;
    let mut applied = apply_template_defaults(config, &template, &mut input);
    let agent_config = template.definition.config.as_ref();
    super::prompt::attach_exposure_artifacts(env, &mut applied);
    super::prompt::attach_prompt_assembly(env, agent_config, &applied, &mut input);
    Ok((apply_final_defaults(applied), input))
}

/// Final fallback after template defaults: caller-built configs leave
/// `max_iterations` unset so templates can win; when neither side sets it
/// the shared `DEFAULT_MAX_ITERATIONS` applies.
fn apply_final_defaults(mut config: AgentLoopConfig) -> AgentLoopConfig {
    if config.max_iterations.is_none() {
        config.max_iterations = Some(DEFAULT_MAX_ITERATIONS);
    }
    config
}

/// Resolve a workflow template's context summary for agent-loop injection.
/// Same governance gate as agent templates: disabled, private or system
/// workflow templates are rejected as not found so the API does not expose
/// template governance details.
pub fn resolve_workflow_context(
    regs: &ResourceRegistries,
    workflow_id: &str,
) -> Result<String, ApiError> {
    use wf_core::registry::Registry;
    let template = regs
        .workflows
        .get(workflow_id)
        .ok_or_else(|| ApiError::not_found("workflow_template", workflow_id))?;
    let template = template.as_ref();
    let runnable = template.enabled != Some(false)
        && template.is_public != Some(false)
        && template.is_system != Some(true);
    if !runnable {
        return Err(ApiError::not_found("workflow_template", workflow_id));
    }
    Ok(super::mentions::workflow_summary(template))
}

