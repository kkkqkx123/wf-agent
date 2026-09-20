//! Agent template resolution at the composition boundary.
//!
//! Canonical home of the agent resolve logic. This module
//! is a factory for fully-resolved [`RunAgentLoopParams`]: it turns a
//! caller's intent ("run agent X with these overrides") into a complete loop
//! config by applying the registered agent template's [`AgentConfig`]
//! defaults under the caller's explicit fields. Callers are the composition
//! entry points (HTTP handlers, CLI frontends); the execution APIs
//! (`agent_execution::run` / `stream` / `resume_from_checkpoint`) stay pure
//! executors and never consult templates.
//!
//! Resolution semantics (exact match only, no fallback or alias):
//! - the built-in id `@standard/main`: the built-in main agent template
//!   applies; it must be registered (`register_all` does this in
//!   production) or the request is rejected.
//! - an id with a registered template (user definition, or a user override
//!   registered under the built-in id): that template's defaults apply.
//! - an empty id is rejected: callers supply `DEFAULT_AGENT` before this
//!   boundary, so an empty id reaching here is a caller bug.
//! - any other explicit id: left untouched (`agent_id` remains a pure
//!   label), matching the pre-template behavior.

use wf_resource::registry::ResourceRegistries;
use wf_tools::callback::AgentLoopConfig;
use wf_types::agent::{AgentConfig, AgentTemplate};
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::tool::AvailableTools;

use crate::agent::agent_config::{DEFAULT_MAX_ITERATIONS, DEFAULT_MODEL};
use crate::agent::agent_execution::RunAgentLoopParams;
use crate::infra::error::ApiError;

pub use wf_resource::predefined::agent_templates::MAIN_AGENT_TEMPLATE_ID;

/// True when the caller requests the built-in main agent: exact equality
/// with the built-in id only. Empty ids never match here; they are
/// rejected in `resolve_and_apply` so no silent default can regress.
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
/// wins. The template system prompt is seeded as the first system message
/// only when the imported conversation carries no system message.
pub fn apply_template_defaults(
    mut config: AgentLoopConfig,
    template: &AgentTemplate,
    input: &mut wf_tools::callback::AgentLoopInput,
) -> AgentLoopConfig {
    let Some(cfg) = template.definition.config.as_ref() else {
        return config;
    };
    apply_agent_config_defaults(&mut config, cfg);
    seed_initial_messages(&mut input.conversation, cfg);
    seed_system_prompt(&mut input.conversation, cfg);
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
            config.hooks = super::hook::agent_hooks_to_loop(hooks);
        }
    }
}

fn apply_tool_defaults(config: &mut AgentLoopConfig, tools: &AvailableTools) {
    if config.available_tool_names.is_empty() {
        config.available_tool_names = tools.available.clone();
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
/// Runs before system prompt seeding so a system message inside the initial
/// set suppresses the separate system prompt seed.
fn seed_initial_messages(conversation: &mut Vec<Message>, cfg: &AgentConfig) {
    let Some(initial) = cfg.initial_messages.as_ref() else {
        return;
    };
    if !conversation.is_empty() || initial.is_empty() {
        return;
    }
    conversation.extend(initial.iter().cloned());
}

/// Seed the template system prompt as the first system message when the
/// conversation does not already carry one.
fn seed_system_prompt(conversation: &mut Vec<Message>, cfg: &AgentConfig) {
    let Some(prompt) = cfg.system_prompt.as_ref() else {
        return;
    };
    if conversation.iter().any(|m| m.role == MessageRole::System) {
        return;
    }
    conversation.insert(
        0,
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::System,
            content: MessageContentValue::Text(prompt.clone()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        },
    );
}

/// Resolve the effective template for a config/input pair and apply its
/// defaults. See the module docs for the resolution semantics.
pub fn resolve_and_apply(
    regs: &ResourceRegistries,
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
    let Some(template) = resolve_template(regs, agent_id) else {
        if requests_builtin_main(agent_id) {
            return Err(ApiError::not_found(
                "agent_template",
                MAIN_AGENT_TEMPLATE_ID,
            ));
        }
        return Ok((apply_final_defaults(config), input));
    };
    let applied = apply_template_defaults(config, &template, &mut input);
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

/// Composition-boundary factory: resolve the params' agent template and
/// return fully-resolved run parameters. Call this where a request is
/// translated into [`RunAgentLoopParams`] (HTTP handler, CLI frontend) —
/// never inside the execution APIs.
pub fn resolve_run_params(
    regs: &ResourceRegistries,
    mut params: RunAgentLoopParams,
) -> crate::infra::error::ApiResult<RunAgentLoopParams> {
    let (config, input) = resolve_and_apply(regs, params.config, params.input)?;
    params.config = config;
    params.input = input;
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registry::{register_item_skip, ResourceRegistries};

    fn main_template() -> AgentTemplate {
        wf_resource::predefined::agent_templates::main_agent_template()
    }

    fn empty_config(agent_id: &str) -> AgentLoopConfig {
        AgentLoopConfig {
            agent_id: wf_types::Id::from(agent_id.to_string()),
            model: "mock".into(),
            max_iterations: None,
            max_execution_time: None,
            hooks: Vec::new(),
            available_tool_names: Vec::new(),
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            activated_tool_names: Vec::new(),
            hidden_tool_names: Vec::new(),
            tool_call_protocol: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            general_description: None,
            discoverable_metadata_block: None,
            history_normalization: false,
            checkpoint_message_interval: None,
        }
    }

    fn input_with(message: &str) -> wf_tools::callback::AgentLoopInput {
        wf_tools::callback::AgentLoopInput {
            message: message.into(),
            context: Default::default(),
            conversation: Vec::new(),
        }
    }

    #[test]
    fn builtin_template_resolves_for_builtin_id() {
        let regs = ResourceRegistries::new();
        register_item_skip(
            &regs.agent_templates,
            MAIN_AGENT_TEMPLATE_ID.into(),
            main_template(),
        );
        let t =
            resolve_template(&regs, MAIN_AGENT_TEMPLATE_ID).expect("builtin resolves by exact id");
        assert_eq!(t.id, MAIN_AGENT_TEMPLATE_ID);
    }

    #[test]
    fn empty_id_is_rejected() {
        let regs = ResourceRegistries::new();
        register_item_skip(
            &regs.agent_templates,
            MAIN_AGENT_TEMPLATE_ID.into(),
            main_template(),
        );
        assert!(resolve_template(&regs, "").is_none());
        let err = resolve_and_apply(&regs, empty_config(""), input_with("hi"))
            .expect_err("empty id is a caller bug");
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[test]
    fn explicit_unknown_id_does_not_fall_back() {
        let regs = ResourceRegistries::new();
        register_item_skip(
            &regs.agent_templates,
            MAIN_AGENT_TEMPLATE_ID.into(),
            main_template(),
        );
        assert!(resolve_template(&regs, "unknown-agent").is_none());
        let (config, _) = resolve_and_apply(&regs, empty_config("unknown-agent"), input_with("hi"))
            .expect("explicit id without template is untouched");
        assert!(config.available_tool_names.is_empty());
    }

    #[test]
    fn user_template_wins_over_builtin_for_override_id() {
        // Real registration order: user templates load first, then
        // `register_all` skips the built-in id (register_item_skip).
        let regs = ResourceRegistries::new();
        let mut custom = main_template();
        custom.definition.name = "User Override".into();
        register_item_skip(&regs.agent_templates, MAIN_AGENT_TEMPLATE_ID.into(), custom);
        register_item_skip(
            &regs.agent_templates,
            MAIN_AGENT_TEMPLATE_ID.into(),
            main_template(),
        );
        let t = resolve_template(&regs, MAIN_AGENT_TEMPLATE_ID).expect("override");
        assert_eq!(t.definition.name, "User Override");
    }

    #[test]
    fn caller_fields_win_over_template_defaults() {
        let regs = ResourceRegistries::new();
        register_item_skip(
            &regs.agent_templates,
            MAIN_AGENT_TEMPLATE_ID.into(),
            main_template(),
        );
        let mut config = empty_config(MAIN_AGENT_TEMPLATE_ID);
        config.max_iterations = Some(7);
        let (config, _) = resolve_and_apply(&regs, config, input_with("hi")).expect("apply");
        assert_eq!(config.max_iterations, Some(7));
        // Unset fields fall back to the template.
        assert!(config
            .available_tool_names
            .contains(&"read_file".to_string()));
        assert_eq!(
            config
                .tool_call_protocol
                .as_ref()
                .map(|p| p.format.to_string())
                .as_deref(),
            Some("native")
        );
    }

    #[test]
    fn system_prompt_seeded_when_absent() {
        let regs = ResourceRegistries::new();
        register_item_skip(
            &regs.agent_templates,
            MAIN_AGENT_TEMPLATE_ID.into(),
            main_template(),
        );
        let (_, input) = resolve_and_apply(
            &regs,
            empty_config(MAIN_AGENT_TEMPLATE_ID),
            input_with("hi"),
        )
        .expect("apply template");
        assert!(input.conversation.iter().any(|m| m.role == MessageRole::System
            && matches!(&m.content, MessageContentValue::Text(t) if t.contains("software engineering assistant"))));
    }

    #[test]
    fn existing_system_message_is_preserved() {
        let regs = ResourceRegistries::new();
        register_item_skip(
            &regs.agent_templates,
            MAIN_AGENT_TEMPLATE_ID.into(),
            main_template(),
        );
        let mut input = input_with("hi");
        input.conversation.push(Message {
            id: wf_types::Id::new(),
            role: MessageRole::System,
            content: MessageContentValue::Text("caller prompt".into()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        });
        let (_, input) = resolve_and_apply(&regs, empty_config(MAIN_AGENT_TEMPLATE_ID), input)
            .expect("apply template");
        let systems: Vec<_> = input
            .conversation
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .collect();
        assert_eq!(systems.len(), 1);
        assert!(matches!(
            &systems[0].content,
            MessageContentValue::Text(t) if t == "caller prompt"
        ));
    }

    #[test]
    fn missing_builtin_main_agent_errors() {
        let regs = ResourceRegistries::new();
        let err = resolve_and_apply(
            &regs,
            empty_config(MAIN_AGENT_TEMPLATE_ID),
            input_with("hi"),
        )
        .expect_err("builtin requested but absent");
        assert!(matches!(err, ApiError::NotFound { .. }));
    }
}
