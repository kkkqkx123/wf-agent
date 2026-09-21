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

use wf_execution_shared::agent_prompt::PromptEnvironment;
use wf_resource::registry::ResourceRegistries;
use wf_tools::callback::AgentLoopConfig;
use wf_types::agent::{AgentConfig, AgentTemplate};
use wf_types::message::Message;
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
    seed_initial_messages(&mut input.conversation, cfg);
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
fn seed_initial_messages(conversation: &mut Vec<Message>, cfg: &AgentConfig) {
    let Some(initial) = cfg.initial_messages.as_ref() else {
        return;
    };
    if !conversation.is_empty() || initial.is_empty() {
        return;
    }
    conversation.extend(initial.iter().cloned());
}

/// Render tool exposure blocks from the resolved buckets. Runs for both
/// templated and untemplated configs so the direct path never falls back to
/// the builtin generation while the workflow path uses custom resources.
fn attach_exposure_artifacts(
    env: &PromptEnvironment,
    config: &mut AgentLoopConfig,
) {
    let artifacts = wf_execution_shared::agent_prompt::build_exposure_artifacts(
        env,
        config.tool_call_protocol.as_ref(),
        &config.available_tool_names,
        &config.initial_tool_names,
        &config.discoverable_tool_names,
        &config.hidden_tool_names,
        config.enable_general_tool,
    );
    if let Some(description) = artifacts.general_description {
        config.general_description = Some(description);
    }
    if let Some(block) = artifacts.discoverable_metadata_block {
        config.discoverable_metadata_block = Some(block);
    }
}

/// Assemble the stable header and volatile tail for the resolved template and
/// land them in the round conversation. Stale tails are dropped first; the
/// header seeds the leading system message once and the tail travels as a
/// separate marked user message. The user task message stays pure input.
fn attach_prompt_assembly(
    env: &PromptEnvironment,
    agent_config: Option<&AgentConfig>,
    config: &AgentLoopConfig,
    input: &mut wf_tools::callback::AgentLoopInput,
) {
    use wf_execution_shared::agent_prompt::{
        apply_assembled_prompt, assemble_agent_prompt, strip_dynamic_context_messages,
        DynamicTailBearing,
    };
    input.conversation = strip_dynamic_context_messages(std::mem::take(&mut input.conversation));
    let assembled = assemble_agent_prompt(
        agent_config,
        &input.context,
        env,
        &config.available_tool_names,
    );
    let mut message = std::mem::take(&mut input.message);
    apply_assembled_prompt(
        &mut input.conversation,
        &assembled,
        DynamicTailBearing::SeparateUserMessage,
        &mut message,
    );
    input.message = message;
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
        if requests_builtin_main(agent_id) {
            return Err(ApiError::not_found(
                "agent_template",
                MAIN_AGENT_TEMPLATE_ID,
            ));
        }
        let mut config = apply_final_defaults(config);
        attach_exposure_artifacts(env, &mut config);
        attach_prompt_assembly(env, None, &config, &mut input);
        return Ok((config, input));
    };
    let mut applied = apply_template_defaults(config, &template, &mut input);
    let agent_config = template.definition.config.as_ref();
    attach_exposure_artifacts(env, &mut applied);
    attach_prompt_assembly(env, agent_config, &applied, &mut input);
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
/// never inside the execution APIs. The environment carries resource
/// registries plus the tool registry and metrics the shared assembly needs;
/// callers holding the full application context build it from there.
pub fn resolve_run_params(
    env: &PromptEnvironment,
    mut params: RunAgentLoopParams,
) -> crate::infra::error::ApiResult<RunAgentLoopParams> {
    let (config, input) = resolve_and_apply(env, params.config, params.input)?;
    params.config = config;
    params.input = input;
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_execution_shared::agent_prompt::PromptEnvironment;
    use wf_resource::registry::{register_item_skip, ResourceRegistries};
    use wf_types::message::{MessageContentValue, MessageRole};

    fn env_for(regs: &ResourceRegistries) -> PromptEnvironment<'_> {
        PromptEnvironment::new(Some(regs), None, None)
    }

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
        let env = env_for(&regs);
        let err = resolve_and_apply(&env, empty_config(""), input_with("hi"))
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
        let env = env_for(&regs);
        let (config, _) = resolve_and_apply(&env, empty_config("unknown-agent"), input_with("hi"))
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
        let env = env_for(&regs);
        let (config, _) = resolve_and_apply(&env, config, input_with("hi")).expect("apply");
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
        let env = env_for(&regs);
        let (_, input) = resolve_and_apply(
            &env,
            empty_config(MAIN_AGENT_TEMPLATE_ID),
            input_with("hi"),
        )
        .expect("apply template");
        assert!(input.conversation.iter().any(|m| m.role == MessageRole::System
            && matches!(&m.content, MessageContentValue::Text(t) if t.contains("software engineering assistant"))));
    }

    #[test]
    fn stale_system_message_is_refreshed_in_place() {
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
        let env = env_for(&regs);
        let (_, input) = resolve_and_apply(&env, empty_config(MAIN_AGENT_TEMPLATE_ID), input)
            .expect("apply template");
        let systems: Vec<_> = input
            .conversation
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .collect();
        assert_eq!(systems.len(), 1);
        assert!(matches!(
            &systems[0].content,
            MessageContentValue::Text(t) if t.contains("software engineering assistant")
        ));
    }

    #[test]
    fn missing_builtin_main_agent_errors() {
        let regs = ResourceRegistries::new();
        let env = env_for(&regs);
        let err = resolve_and_apply(
            &env,
            empty_config(MAIN_AGENT_TEMPLATE_ID),
            input_with("hi"),
        )
        .expect_err("builtin requested but absent");
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    fn template_with_config(id: &str, config: serde_json::Value) -> AgentTemplate {
        let mut template = main_template();
        template.id = id.into();
        template.definition.config =
            Some(serde_json::from_value(config).expect("agent config"));
        template
    }

    #[test]
    fn direct_path_renders_system_prompt_template_id() {
        use wf_core::registry::MutableRegistry;
        let regs = ResourceRegistries::new();
        regs.templates
            .register(
                "prompt-tpl".to_string(),
                std::sync::Arc::new(wf_types::Template {
                    id: "prompt-tpl".into(),
                    name: "prompt".into(),
                    description: None,
                    category: "test".into(),
                    content: "greetings {{who}}".into(),
                    variables: None,
                    fragments: None,
                }),
            )
            .unwrap();
        register_item_skip(
            &regs.agent_templates,
            "agent-tpl".into(),
            template_with_config(
                "agent-tpl",
                serde_json::json!({
                    "system_prompt_template_id": "prompt-tpl",
                    "system_prompt_template_variables": {"who": "direct"},
                }),
            ),
        );
        let env = env_for(&regs);
        let (_, input) =
            resolve_and_apply(&env, empty_config("agent-tpl"), input_with("hi")).expect("apply");
        assert_eq!(input.message, "hi");
        let header = input
            .conversation
            .iter()
            .find(|m| m.role == MessageRole::System)
            .expect("stable header");
        assert!(matches!(
            &header.content,
            MessageContentValue::Text(t) if t.contains("greetings direct")
        ));
    }

    #[test]
    fn direct_path_tail_is_marked_user_message_and_task_stays_pure() {
        let regs = ResourceRegistries::new();
        register_item_skip(
            &regs.agent_templates,
            "agent-tail".into(),
            template_with_config(
                "agent-tail",
                serde_json::json!({
                    "system_prompt": "stable base",
                    "dynamic_context": {"include_todo_list": true},
                }),
            ),
        );
        let env = env_for(&regs);
        let mut input = input_with("do work");
        input.context.insert(
            "todo_list".to_string(),
            serde_json::json!([{"content": "write code", "status": "pending"}]),
        );
        let (_, input) =
            resolve_and_apply(&env, empty_config("agent-tail"), input).expect("apply");
        assert_eq!(input.message, "do work");
        assert_eq!(input.conversation.len(), 2);
        assert_eq!(input.conversation[0].role, MessageRole::System);
        assert!(matches!(
            &input.conversation[0].content,
            MessageContentValue::Text(t) if t.contains("stable base") && !t.contains("TODO")
        ));
        assert_eq!(input.conversation[1].role, MessageRole::User);
        let marker = input.conversation[1]
            .metadata
            .as_ref()
            .and_then(|m| m.get("type"))
            .expect("tail marker");
        assert_eq!(
            marker,
            &serde_json::Value::String("dynamic_context".to_string())
        );
        assert!(matches!(
            &input.conversation[1].content,
            MessageContentValue::Text(t) if t.contains("TODO list:") && t.contains("write code")
        ));
    }

    #[test]
    fn direct_path_drops_stale_tail_on_import() {
        let regs = ResourceRegistries::new();
        register_item_skip(
            &regs.agent_templates,
            "agent-stale".into(),
            template_with_config(
                "agent-stale",
                serde_json::json!({
                    "system_prompt": "stable",
                    "dynamic_context": {"include_todo_list": true},
                }),
            ),
        );
        let env = env_for(&regs);
        let mut input = input_with("hi");
        input.conversation.push(Message {
            id: "old-tail".into(),
            role: MessageRole::User,
            content: MessageContentValue::Text("old tail".into()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: Some(std::collections::HashMap::from([(
                "type".to_string(),
                serde_json::Value::String("dynamic_context".to_string()),
            )])),
        });
        let (_, input) =
            resolve_and_apply(&env, empty_config("agent-stale"), input).expect("apply");
        let tails: Vec<_> = input
            .conversation
            .iter()
            .filter(|m| {
                m.metadata
                    .as_ref()
                    .and_then(|meta| meta.get("type"))
                    .map(|t| t == &serde_json::Value::String("dynamic_context".to_string()))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            tails.is_empty(),
            "missing todo variable must warn and skip without fabricating a tail"
        );
        assert!(!tails.iter().any(|m| m.id.as_str() == "old-tail"));
        assert_eq!(input.message, "hi");
    }
}
