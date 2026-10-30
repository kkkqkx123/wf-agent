use super::*;
use crate::infra::error::ApiError;
use wf_execution_shared::agent_prompt::PromptEnvironment;
use wf_resource::registry::{register_item_skip, ResourceRegistries};
use wf_tools::callback::AgentLoopConfig;
use wf_types::agent::AgentTemplate;
use wf_types::message::{Message, MessageContentValue, MessageRole};

fn env_for(regs: &ResourceRegistries) -> PromptEnvironment<'_> {
    PromptEnvironment::new(Some(regs), None, None)
}

fn main_template() -> AgentTemplate {
    wf_resource::predefined::agent_templates::main_agent_template()
}

/// Registries holding the built-in main agent template plus the built-in
/// prompt templates its `system_prompt_template_id` points at. Both halves
/// are required for the prompt assembly to resolve a system header.
fn registries() -> ResourceRegistries {
    let regs = ResourceRegistries::new();
    register_item_skip(
        &regs.agent_templates,
        MAIN_AGENT_TEMPLATE_ID.into(),
        main_template(),
    );
    wf_resource::predefined::agent_prompts::register(
        &regs,
        &wf_resource::registry::RegisterOptions::default(),
    );
    regs
}

fn empty_config(agent_id: &str) -> AgentLoopConfig {
    AgentLoopConfig {
        file_scope: None,
        agent_id: wf_types::Id::from(agent_id.to_string()),
        model: "mock".into(),
        max_iterations: None,
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: None,
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
        context_schema: None,
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
    let regs = registries();
    let t = resolve_template(&regs, MAIN_AGENT_TEMPLATE_ID).expect("builtin resolves by exact id");
    assert_eq!(t.id, MAIN_AGENT_TEMPLATE_ID);
}

#[test]
fn empty_id_is_rejected() {
    let regs = registries();
    assert!(resolve_template(&regs, "").is_none());
    let env = env_for(&regs);
    let err = resolve_and_apply(&env, empty_config(""), input_with("hi"))
        .expect_err("empty id is a caller bug");
    assert!(matches!(err, ApiError::NotFound { .. }));
}

#[test]
fn explicit_unknown_id_is_rejected() {
    let regs = registries();
    assert!(resolve_template(&regs, "unknown-agent").is_none());
    let env = env_for(&regs);
    let err = resolve_and_apply(&env, empty_config("unknown-agent"), input_with("hi"))
        .expect_err("unregistered id has no template semantics");
    assert!(matches!(err, ApiError::NotFound { .. }));
}

#[test]
fn disabled_template_is_rejected() {
    let regs = ResourceRegistries::new();
    let mut template = main_template();
    template.enabled = Some(false);
    register_item_skip(
        &regs.agent_templates,
        MAIN_AGENT_TEMPLATE_ID.into(),
        template,
    );
    let env = env_for(&regs);
    let err = resolve_and_apply(&env, empty_config(MAIN_AGENT_TEMPLATE_ID), input_with("hi"))
        .expect_err("disabled template must not run");
    assert!(matches!(err, ApiError::NotFound { .. }));
}

#[test]
fn private_template_is_rejected() {
    let regs = ResourceRegistries::new();
    let mut template = main_template();
    template.is_public = Some(false);
    register_item_skip(
        &regs.agent_templates,
        MAIN_AGENT_TEMPLATE_ID.into(),
        template,
    );
    let env = env_for(&regs);
    let err = resolve_and_apply(&env, empty_config(MAIN_AGENT_TEMPLATE_ID), input_with("hi"))
        .expect_err("private template must not run directly");
    assert!(matches!(err, ApiError::NotFound { .. }));
}

#[test]
fn system_template_is_rejected() {
    let regs = ResourceRegistries::new();
    let mut template = main_template();
    template.is_system = Some(true);
    register_item_skip(
        &regs.agent_templates,
        MAIN_AGENT_TEMPLATE_ID.into(),
        template,
    );
    let env = env_for(&regs);
    let err = resolve_and_apply(&env, empty_config(MAIN_AGENT_TEMPLATE_ID), input_with("hi"))
        .expect_err("system-flagged template must not run directly");
    assert!(matches!(err, ApiError::NotFound { .. }));
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
    let regs = registries();
    let mut config = empty_config(MAIN_AGENT_TEMPLATE_ID);
    config.max_iterations = Some(7);
    let env = env_for(&regs);
    let (config, _) = resolve_and_apply(&env, config, input_with("hi")).expect("apply");
    assert_eq!(config.max_iterations, Some(7));
    // Unset fields fall back to the template.
    assert!(config
        .available_tool_names
        .as_ref()
        .is_some_and(|names| names.contains(&"read_file".to_string())));
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
fn explicit_empty_tool_list_disables_all_tools() {
    let regs = registries();
    let mut config = empty_config(MAIN_AGENT_TEMPLATE_ID);
    config.available_tool_names = Some(Vec::new());
    let env = env_for(&regs);
    let (config, _) = resolve_and_apply(&env, config, input_with("hi")).expect("apply");
    // `Some(vec![])` is an explicit empty set: the template default must
    // not refill it.
    assert_eq!(config.available_tool_names, Some(Vec::new()));
}

#[test]
fn unset_tool_list_falls_back_to_template() {
    let regs = registries();
    let env = env_for(&regs);
    let (config, _) =
        resolve_and_apply(&env, empty_config(MAIN_AGENT_TEMPLATE_ID), input_with("hi"))
            .expect("apply");
    assert!(config
        .available_tool_names
        .as_ref()
        .is_some_and(|names| !names.is_empty()));
}

#[test]
fn system_prompt_seeded_when_absent() {
    let regs = registries();
    let env = env_for(&regs);
    let (_, input) =
        resolve_and_apply(&env, empty_config(MAIN_AGENT_TEMPLATE_ID), input_with("hi"))
            .expect("apply template");
    assert!(input.conversation.iter().any(|m| m.role == MessageRole::System
            && matches!(&m.content, MessageContentValue::Text(t) if t.contains("software engineering assistant"))));
}

#[test]
fn stale_system_message_is_refreshed_in_place() {
    let regs = registries();
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
        thinking_signature: None,
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
    let err = resolve_and_apply(&env, empty_config(MAIN_AGENT_TEMPLATE_ID), input_with("hi"))
        .expect_err("builtin requested but absent");
    assert!(matches!(err, ApiError::NotFound { .. }));
}

fn template_with_config(id: &str, config: serde_json::Value) -> AgentTemplate {
    let mut template = main_template();
    template.id = id.into();
    template.definition.config = Some(serde_json::from_value(config).expect("agent config"));
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
    let (_, input) = resolve_and_apply(&env, empty_config("agent-tail"), input).expect("apply");
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
        thinking_signature: None,
        metadata: Some(std::collections::HashMap::from([(
            "type".to_string(),
            serde_json::Value::String("dynamic_context".to_string()),
        )])),
    });
    let (_, input) = resolve_and_apply(&env, empty_config("agent-stale"), input).expect("apply");
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

fn workflow_regs() -> ResourceRegistries {
    let regs = registries();
    // The predefined template is system-flagged and would be gated; the
    // registry entry under test is a user-visible copy without the flag.
    let mut template = wf_resource::predefined::workflow::create_prefetch_workflow();
    template.is_system = None;
    register_item_skip(&regs.workflows, "wf-context-test".into(), template);
    regs
}

#[test]
fn workflow_context_resolves_for_registered_template() {
    let regs = workflow_regs();
    let summary = resolve_workflow_context(&regs, "wf-context-test").expect("summary");
    assert!(summary.contains("workflow:"));
    assert!(summary.contains("nodes ("));
}

#[test]
fn unregistered_workflow_context_is_rejected() {
    let regs = workflow_regs();
    let err = resolve_workflow_context(&regs, "no-such-workflow").expect_err("unknown id rejected");
    assert!(matches!(err, ApiError::NotFound { .. }));
}

#[test]
fn disabled_workflow_context_is_rejected() {
    // A fresh registry: `register_item_skip` would keep the runnable
    // entry already registered under the same id.
    let regs = ResourceRegistries::new();
    let mut template = wf_resource::predefined::workflow::create_prefetch_workflow();
    template.enabled = Some(false);
    register_item_skip(&regs.workflows, "wf-context-test".into(), template);
    let err = resolve_workflow_context(&regs, "wf-context-test")
        .expect_err("disabled workflow must not inject");
    assert!(matches!(err, ApiError::NotFound { .. }));
}
