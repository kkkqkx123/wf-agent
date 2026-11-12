use super::*;
use serde_json::Value;
use std::collections::HashMap;
use wf_resource::registry::ResourceRegistries;
use wf_tools::registry::ToolRegistry;

fn agent_config_with(json: serde_json::Value) -> wf_types::agent::AgentConfig {
    serde_json::from_value(json).expect("agent config")
}

/// A per-test skill directory (tag-suffixed, like the repo-wide temp dir
/// convention) so parallel tests never share or wipe each other's path.
fn registry_with_one_skill(tag: &str) -> (std::path::PathBuf, ToolRegistry) {
    let dir = std::env::temp_dir().join(format!("wf-stable-skills-{}-{}", tag, std::process::id()));
    let skill_dir = dir.join("demo-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: demo-skill\ndescription: Demo skill\n---\n\n# Body\n",
    )
    .unwrap();
    let loader = std::sync::Arc::new(wf_tools::skill::SkillLoader::new(
        wf_types::skill::SkillConfig {
            paths: vec![dir.to_string_lossy().to_string()],
            auto_scan: Some(true),
        },
    ));
    assert_eq!(loader.get_enabled_skills().len(), 1);
    let registry = ToolRegistry::new();
    registry.set_skill_loader(loader);
    (dir, registry)
}

fn skills_config() -> wf_types::agent::AgentConfig {
    agent_config_with(serde_json::json!({
        "system_prompt": "base",
        "dynamic_context": {"include_skills": true},
    }))
}

#[test]
fn stable_header_skips_static_skills_when_skill_tool_present() {
    let (dir, registry) = registry_with_one_skill("skill-tool");
    let env = PromptEnvironment::new(None, Some(&registry), None);
    let header = build_stable_header(
        Some(&skills_config()),
        &env,
        wf_tools::tool_exposure::AvailablePool::Explicit(&["skill".to_string()]),
        None,
    )
    .expect("header");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        header.contains("Available skills:"),
        "progressive disclosure owns skill presentation: {header}"
    );
    assert!(
        !header.contains("<skills>"),
        "static skill list must not duplicate the metadata block: {header}"
    );
}

#[test]
fn stable_header_keeps_static_skills_without_skill_tool() {
    let (dir, registry) = registry_with_one_skill("no-skill-tool");
    let env = PromptEnvironment::new(None, Some(&registry), None);
    let header = build_stable_header(
        Some(&skills_config()),
        &env,
        wf_tools::tool_exposure::AvailablePool::Explicit(&[]),
        None,
    )
    .expect("header");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        header.contains("<skills>"),
        "static skill list stays without progressive disclosure: {header}"
    );
    assert!(
        !header.contains("Available skills:"),
        "no metadata injection without the skill tool: {header}"
    );
}

#[test]
fn stable_header_prefers_inline_over_template() {
    let regs = ResourceRegistries::new();
    let env = PromptEnvironment::new(Some(&regs), None, None);
    let vars = HashMap::new();
    let config = agent_config_with(serde_json::json!({
        "system_prompt": "inline",
        "system_prompt_template_id": "missing",
    }));
    let assembled = assemble_agent_prompt(
        Some(&config),
        &vars,
        &env,
        wf_tools::tool_exposure::AvailablePool::Explicit(&[]),
        None,
    );
    assert_eq!(assembled.stable_header.as_deref(), Some("inline"));
    assert!(assembled.volatile_tail.is_none());
}

#[test]
fn stable_header_renders_template_id() {
    use wf_core::registry::MutableRegistry;
    let regs = ResourceRegistries::new();
    regs.templates
        .register(
            "tpl-1".to_string(),
            std::sync::Arc::new(wf_types::Template {
                id: "tpl-1".into(),
                name: "tpl".into(),
                description: None,
                category: "test".into(),
                content: "hello {{name}}".into(),
                variables: None,
                fragments: None,
            }),
        )
        .unwrap();
    let env = PromptEnvironment::new(Some(&regs), None, None);
    let vars = HashMap::new();
    let config = agent_config_with(serde_json::json!({
        "system_prompt_template_id": "tpl-1",
        "system_prompt_template_variables": {"name": "world"},
    }));
    let assembled = assemble_agent_prompt(
        Some(&config),
        &vars,
        &env,
        wf_tools::tool_exposure::AvailablePool::Explicit(&[]),
        None,
    );
    assert_eq!(assembled.stable_header.as_deref(), Some("hello world"));
}

#[test]
fn stable_header_excludes_volatile_time() {
    let env = PromptEnvironment::empty();
    let vars = HashMap::new();
    let config = agent_config_with(serde_json::json!({
        "system_prompt": "base",
        "dynamic_context": {"include_current_time": true},
    }));
    let assembled = assemble_agent_prompt(
        Some(&config),
        &vars,
        &env,
        wf_tools::tool_exposure::AvailablePool::Explicit(&[]),
        None,
    );
    assert_eq!(assembled.stable_header.as_deref(), Some("base"));
    assert!(assembled.volatile_tail.is_some());
    assert!(assembled.volatile_tail.unwrap().contains("Current time:"));
}

#[test]
fn stable_header_reports_execution_workdir() {
    let env = PromptEnvironment::empty();
    let vars = HashMap::new();
    let config = agent_config_with(serde_json::json!({
        "system_prompt": "base",
        "dynamic_context": {"include_environment_info": true},
    }));
    let assembled = assemble_agent_prompt(
        Some(&config),
        &vars,
        &env,
        wf_tools::tool_exposure::AvailablePool::Explicit(&[]),
        Some("/sessions/abc"),
    );
    let header = assembled.stable_header.expect("header");
    assert!(header.contains("Working directory: /sessions/abc"));
    assert!(assembled.volatile_tail.is_none());
}

#[test]
fn volatile_tail_accepts_minimal_todo_shape() {
    let mut vars = HashMap::new();
    vars.insert(
        "todo_list".to_string(),
        serde_json::json!([
            {"content": "write code", "status": "pending"},
            {"content": "ship it", "status": "in_progress"},
        ]),
    );
    let config = agent_config_with(serde_json::json!({
        "dynamic_context": {"include_todo_list": true},
    }));
    let tail = build_volatile_tail(Some(&config), &vars).expect("tail");
    assert!(tail.contains("TODO list:"));
    assert!(tail.contains("write code"));
    assert!(tail.contains("ship it"));
}

#[test]
fn volatile_tail_skips_missing_variable() {
    let vars = HashMap::new();
    let config = agent_config_with(serde_json::json!({
        "dynamic_context": {"include_todo_list": true},
    }));
    assert!(build_volatile_tail(Some(&config), &vars).is_none());
}

#[test]
fn stable_check_ignores_marked_messages() {
    let marked_system = wf_types::message::Message {
        id: "1".into(),
        role: wf_types::message::MessageRole::System,
        content: wf_types::message::MessageContentValue::Text("tail".into()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: Some(HashMap::from([(
            "type".to_string(),
            Value::String(DYNAMIC_CONTEXT_MESSAGE_TYPE.to_string()),
        )])),
    };
    assert!(!has_stable_system_message(&[marked_system]));
    let plain = stable_system_message("header".into());
    assert!(has_stable_system_message(&[plain]));
}

#[test]
fn import_filter_drops_stale_tails() {
    let tail = dynamic_context_message("old".into());
    let user = wf_types::message::Message {
        id: "u".into(),
        role: wf_types::message::MessageRole::User,
        content: wf_types::message::MessageContentValue::Text("hi".into()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let filtered = strip_dynamic_context_messages(vec![tail, user.clone()]);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "u");
}

#[test]
fn trailing_split_preserves_fresh_tail() {
    let user = wf_types::message::Message {
        id: "u".into(),
        role: wf_types::message::MessageRole::User,
        content: wf_types::message::MessageContentValue::Text("hi".into()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let stale = dynamic_context_message("old".into());
    let fresh = dynamic_context_message("new".into());
    let (filtered, trailing) =
        split_trailing_dynamic_tail(vec![stale, user.clone(), fresh.clone()]);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "u");
    assert_eq!(trailing.len(), 1);
    assert_eq!(trailing[0].id, fresh.id);
}

#[test]
fn apply_inserts_header_once_and_appends_tail() {
    let mut conversation = Vec::new();
    apply_assembled_prompt(
        &mut conversation,
        &AssembledPrompt {
            stable_header: Some("header".into()),
            volatile_tail: Some("tail".into()),
        },
        DynamicTailBearing::SeparateUserMessage,
    );
    assert_eq!(conversation.len(), 2);
    assert_eq!(conversation[0].role, wf_types::message::MessageRole::System);
    assert_eq!(conversation[1].role, wf_types::message::MessageRole::User);
    assert!(is_dynamic_context_message(&conversation[1]));
}

#[test]
fn apply_refreshes_stale_header_in_place() {
    let mut conversation = vec![stable_system_message("old".into())];
    apply_assembled_prompt(
        &mut conversation,
        &AssembledPrompt {
            stable_header: Some("new".into()),
            volatile_tail: None,
        },
        DynamicTailBearing::SeparateUserMessage,
    );
    assert_eq!(conversation.len(), 1);
    assert_eq!(conversation[0].text_content(), "new");
}

#[test]
fn workflow_and_direct_variable_sources_agree() {
    let config = agent_config_with(serde_json::json!({
        "system_prompt": "base",
        "dynamic_context": {"include_todo_list": true},
    }));
    let todo = serde_json::json!([
        {"content": "write code", "status": "pending"},
    ]);
    let mut map_vars = HashMap::new();
    map_vars.insert("todo_list".to_string(), todo.clone());
    let dash_vars = std::sync::Arc::new(dashmap::DashMap::new());
    dash_vars.insert("todo_list".to_string(), todo);
    let node_ctx = crate::context::NodeExecutionContext::new(
        "exec".into(),
        "node".into(),
        wf_types::node::StaticNodeType::AgentLoop,
        serde_json::Value::Null,
        dash_vars,
    );
    let from_map = build_volatile_tail(Some(&config), &map_vars).expect("map tail");
    let from_node = build_volatile_tail(Some(&config), &node_ctx).expect("node tail");
    assert_eq!(from_map, from_node);
    let env = PromptEnvironment::empty();
    let stable_map = build_stable_header(
        Some(&config),
        &env,
        wf_tools::tool_exposure::AvailablePool::Explicit(&[]),
        None,
    );
    let stable_node = build_stable_header(
        Some(&config),
        &env,
        wf_tools::tool_exposure::AvailablePool::Explicit(&[]),
        None,
    );
    assert_eq!(stable_map, stable_node);
    assert!(!stable_map.unwrap().contains("TODO"));
}
