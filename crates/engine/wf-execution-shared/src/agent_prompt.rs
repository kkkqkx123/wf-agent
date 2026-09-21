//! Single owner for agent prompt assembly: stable header plus volatile tail.
//!
//! Both the workflow `AGENT_LOOP` handler and the direct composition boundary
//! call this module so one agent definition renders identically on both
//! entries. The module holds no executor and knows nothing about session
//! origins; it only turns an [`AgentConfig`] plus a narrow variable view
//! into texts and into the messages that carry them.
//!
//! Stable header: inline `system_prompt` wins, `system_prompt_template_id`
//! renders next, stable dynamic sections (environment, skills, workflows,
//! custom sections) prepend, then skill metadata and MCP summary enrich.
//! Time, todo, pinned and workspace content never enter here.
//!
//! Volatile tail: current time plus todo, pinned, workspace tree and custom
//! data read through [`VariableSource`]. Missing or misshapen values warn
//! and skip; the engine never fabricates data.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use wf_core::registry::Registry;
use wf_metrics::MetricsRegistry;
use wf_resource::registry::ResourceRegistries;
use wf_tools::registry::ToolRegistry;

/// Marker for volatile tail messages, sharing the `type` metadata convention
/// with `tool_visibility` tail announcements.
pub const DYNAMIC_CONTEXT_MESSAGE_TYPE: &str = "dynamic_context";
/// Marker for tool visibility tail announcements.
pub const TOOL_VISIBILITY_MESSAGE_TYPE: &str = "tool_visibility";

/// Narrow variable access shared by the workflow variable table and the
/// direct path input context.
pub trait VariableSource {
    fn get_variable(&self, name: &str) -> Option<Value>;
}

impl VariableSource for HashMap<String, Value> {
    fn get_variable(&self, name: &str) -> Option<Value> {
        self.get(name).cloned()
    }
}

impl VariableSource for dashmap::DashMap<String, Value> {
    fn get_variable(&self, name: &str) -> Option<Value> {
        self.get(name).map(|v| v.clone())
    }
}

impl VariableSource for std::sync::Arc<dashmap::DashMap<String, Value>> {
    fn get_variable(&self, name: &str) -> Option<Value> {
        self.get(name).map(|v| v.clone())
    }
}

impl crate::context::NodeExecutionContext {
    fn variable_source_get(&self, name: &str) -> Option<Value> {
        self.variables.get(name).map(|v| v.clone())
    }
}

impl VariableSource for crate::context::NodeExecutionContext {
    fn get_variable(&self, name: &str) -> Option<Value> {
        self.variable_source_get(name)
    }
}

/// Environment the assembly reads through: resource templates, tool state
/// for skill and MCP enrichment, and metrics for template observability.
pub struct PromptEnvironment<'a> {
    pub resource_registries: Option<&'a ResourceRegistries>,
    pub tool_registry: Option<&'a ToolRegistry>,
    pub metrics: Option<&'a MetricsRegistry>,
}

impl<'a> PromptEnvironment<'a> {
    pub fn new(
        resource_registries: Option<&'a ResourceRegistries>,
        tool_registry: Option<&'a ToolRegistry>,
        metrics: Option<&'a MetricsRegistry>,
    ) -> Self {
        Self {
            resource_registries,
            tool_registry,
            metrics,
        }
    }

    pub fn empty() -> Self {
        Self {
            resource_registries: None,
            tool_registry: None,
            metrics: None,
        }
    }

    fn template_metrics(&self) -> Option<std::sync::Arc<wf_metrics::TemplateMetricsCollector>> {
        self.metrics.map(|m| m.template())
    }
}

/// Stable header plus volatile tail texts for one run.
pub struct AssembledPrompt {
    pub stable_header: Option<String>,
    pub volatile_tail: Option<String>,
}

/// How the volatile tail travels: separate marked user message by default,
/// merged into the user task only as an explicit compatibility branch for
/// model services that require strict role alternation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DynamicTailBearing {
    #[default]
    SeparateUserMessage,
    MergedIntoTask,
}

/// Assemble both outputs for one run.
pub fn assemble_agent_prompt(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    vars: &impl VariableSource,
    env: &PromptEnvironment,
    available_tool_names: &[String],
) -> AssembledPrompt {
    AssembledPrompt {
        stable_header: build_stable_header(agent_config, env, available_tool_names),
        volatile_tail: build_volatile_tail(agent_config, vars),
    }
}

/// Build the stable system header. Returns `None` when no base prompt and no
/// stable dynamic section is configured.
pub fn build_stable_header(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    env: &PromptEnvironment,
    available_tool_names: &[String],
) -> Option<String> {
    let template_metrics = env.template_metrics();
    let base = resolve_configured_system_prompt(
        agent_config,
        env.resource_registries,
        template_metrics.as_deref(),
    );
    let dynamic = build_dynamic_system_context(agent_config, env);
    let prompt = match (base, dynamic) {
        (Some(sp), Some(block)) => Some(format!("{}\n\n{}", block, sp)),
        (Some(sp), None) => Some(sp),
        (None, block) => block,
    }?;
    Some(enrich_system_prompt(
        env,
        &prompt,
        available_tool_names,
    ))
}

fn resolve_configured_system_prompt(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    regs: Option<&ResourceRegistries>,
    template_metrics: Option<&wf_metrics::TemplateMetricsCollector>,
) -> Option<String> {
    let config = agent_config?;
    if let Some(ref sp) = config.system_prompt {
        return Some(sp.clone());
    }
    let template_id = config.system_prompt_template_id.as_deref()?;
    let regs = regs?;
    let mut variables = HashMap::new();
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

fn build_dynamic_system_context(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    env: &PromptEnvironment,
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
        include_time: false,
        include_env: dyn_cfg.include_environment_info.unwrap_or(false),
        ..Default::default()
    };
    if dyn_cfg.include_skills.unwrap_or(false) {
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
    available_tool_names: &[String],
) -> String {
    let has_skill_tool = available_tool_names.iter().any(|name| name == "skill");
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

    let has_mcp_tools = available_tool_names
        .iter()
        .any(|name| name == "use_mcp" || name.starts_with("mcp_"));
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

/// Build the volatile tail text. Time comes from the local clock; todo,
/// pinned, workspace tree and custom data come from variables. Enabled but
/// missing or misshapen values warn and skip; the engine never fabricates
/// data. Returns `None` when nothing is enabled or nothing has data.
pub fn build_volatile_tail(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    vars: &impl VariableSource,
) -> Option<String> {
    let dyn_cfg = agent_config?.dynamic_context.as_ref()?;
    let wants_time = dyn_cfg.include_current_time.unwrap_or(false);
    let wants_todo = dyn_cfg.include_todo_list.unwrap_or(false);
    let wants_workspace = dyn_cfg.include_workspace_files.unwrap_or(false);
    let wants_pinned = dyn_cfg.include_pinned_files.unwrap_or(false);
    let wants_custom = vars.get_variable("custom_data").is_some();
    if !wants_time && !wants_todo && !wants_workspace && !wants_pinned && !wants_custom {
        return None;
    }

    let mut input = wf_resource::UserInput::default();
    if wants_time {
        input.current_time = Some(wf_resource::current_time_text());
    }
    if wants_todo {
        match vars.get_variable("todo_list") {
            Some(value) => match parse_todo_items(value) {
                Some(items) => input.todos = items,
                None => tracing::warn!("todo_list variable has an unsupported shape; skipped"),
            },
            None => tracing::warn!("todo_list requested but no todo_list variable is set; skipped"),
        }
    }
    if wants_pinned {
        match vars.get_variable("pinned_files") {
            Some(value) => {
                if let Ok(items) =
                    serde_json::from_value::<Vec<wf_types::PinnedFileItem>>(value.clone())
                {
                    input.pinned = items
                        .into_iter()
                        .map(|item| std::path::PathBuf::from(item.path))
                        .collect();
                } else if let Ok(paths) = serde_json::from_value::<Vec<String>>(value) {
                    input.pinned = paths.into_iter().map(std::path::PathBuf::from).collect();
                } else {
                    tracing::warn!("pinned_files variable has an unsupported shape; skipped");
                }
            }
            None => {
                tracing::warn!("pinned_files requested but no pinned_files variable is set; skipped")
            }
        }
    }
    if wants_workspace {
        match vars.get_variable("workspace_file_tree") {
            Some(Value::String(tree)) => input.tree = Some(tree),
            Some(_) => tracing::warn!("workspace_file_tree variable is not a string; skipped"),
            None => {
                tracing::warn!("workspace files requested but no workspace_file_tree variable is set; skipped")
            }
        }
    }
    if let Some(value) = vars.get_variable("custom_data") {
        match parse_custom_data(value) {
            Some(map) if !map.is_empty() => input.custom_data = Some(map),
            Some(_) => {}
            None => tracing::warn!("custom_data variable has an unsupported shape; skipped"),
        }
    }

    let rendered = wf_resource::build_user_context(&input);
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

/// Minimal todo display shape: full [`wf_types::TodoItem`] items win, but a
/// bare `{content, status}` object without `id` also renders. Status strings
/// follow the `update_todo_list` tool vocabulary. Returns `None` when the
/// value is not a todo array at all.
fn parse_todo_items(value: Value) -> Option<Vec<wf_types::TodoItem>> {
    if let Ok(items) = serde_json::from_value::<Vec<wf_types::TodoItem>>(value.clone()) {
        return Some(items);
    }
    let raw = value.as_array()?;
    if raw.is_empty() {
        return Some(Vec::new());
    }
    let mut items = Vec::with_capacity(raw.len());
    for entry in raw {
        let content = entry
            .get("content")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        let status_str = entry
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending");
        let status = match status_str.to_ascii_lowercase().as_str() {
            "pending" => wf_types::TodoStatus::Pending,
            "in_progress" | "inprogress" | "in-progress" | "doing" => {
                wf_types::TodoStatus::InProgress
            }
            "completed" | "complete" | "done" => wf_types::TodoStatus::Completed,
            "cancelled" | "canceled" => wf_types::TodoStatus::Cancelled,
            _ => return None,
        };
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(wf_common::generate_id);
        items.push(wf_types::TodoItem {
            id,
            content: content.to_string(),
            status,
            priority: None,
            created_at: None,
            updated_at: None,
            metadata: None,
        });
    }
    Some(items)
}

fn parse_custom_data(value: Value) -> Option<HashMap<String, String>> {
    let obj = value.as_object()?;
    let mut map = HashMap::new();
    for (k, v) in obj {
        let rendered = match v {
            Value::String(s) => s.clone(),
            Value::Null => continue,
            other => other.to_string(),
        };
        if !rendered.trim().is_empty() {
            map.insert(k.clone(), rendered);
        }
    }
    Some(map)
}

/// True for tail volatile messages carrying the dynamic marker.
pub fn is_dynamic_context_message(msg: &wf_types::message::Message) -> bool {
    msg.metadata
        .as_ref()
        .and_then(|meta| meta.get("type"))
        .map(|t| t == &Value::String(DYNAMIC_CONTEXT_MESSAGE_TYPE.to_string()))
        .unwrap_or(false)
}

/// True for tool visibility tail announcements.
pub fn is_tool_visibility_message(msg: &wf_types::message::Message) -> bool {
    msg.metadata
        .as_ref()
        .and_then(|meta| meta.get("type"))
        .map(|t| t == &Value::String(TOOL_VISIBILITY_MESSAGE_TYPE.to_string()))
        .unwrap_or(false)
}

/// Stable header exists when a non-announcement system message is present.
/// Both `tool_visibility` and `dynamic_context` marked messages never count.
pub fn has_stable_system_message(conversation: &[wf_types::message::Message]) -> bool {
    conversation.iter().any(|m| {
        m.role == wf_types::message::MessageRole::System
            && !is_tool_visibility_message(m)
            && !is_dynamic_context_message(m)
    })
}

/// Drop every stale volatile tail message from imported history so cross-round
/// imports never accumulate old tails. Fresh tails are added after this
/// filter, so unconditional removal is correct at import time.
pub fn strip_dynamic_context_messages(
    conversation: Vec<wf_types::message::Message>,
) -> Vec<wf_types::message::Message> {
    conversation
        .into_iter()
        .filter(|m| !is_dynamic_context_message(m))
        .collect()
}

/// Split inbound history into filtered history plus a preserved trailing
/// fresh tail. Stale tails in the middle are dropped; a suffix of marked
/// messages is treated as the freshly assembled tail and kept. Used where the
/// fresh tail already sits at the end of the inbound conversation.
pub fn split_trailing_dynamic_tail(
    conversation: Vec<wf_types::message::Message>,
) -> (
    Vec<wf_types::message::Message>,
    Vec<wf_types::message::Message>,
) {
    let mut trailing = Vec::new();
    let mut rest = conversation;
    while rest.last().map(is_dynamic_context_message).unwrap_or(false) {
        trailing.push(rest.pop().expect("non-empty"));
    }
    trailing.reverse();
    let filtered: Vec<wf_types::message::Message> = rest
        .into_iter()
        .filter(|m| !is_dynamic_context_message(m))
        .collect();
    (filtered, trailing)
}

/// Leading stable system message carrying the cacheable header.
pub fn stable_system_message(content: String) -> wf_types::message::Message {
    wf_types::message::Message {
        id: wf_common::generate_id(),
        role: wf_types::message::MessageRole::System,
        content: wf_types::message::MessageContentValue::Text(content),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

/// Independent volatile tail message: user role so dynamic state reads as the
/// latest round user context, marked so stable checks and transcripts skip it.
pub fn dynamic_context_message(content: String) -> wf_types::message::Message {
    wf_types::message::Message {
        id: wf_common::generate_id(),
        role: wf_types::message::MessageRole::User,
        content: wf_types::message::MessageContentValue::Text(content),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: Some(HashMap::from([(
            "type".to_string(),
            Value::String(DYNAMIC_CONTEXT_MESSAGE_TYPE.to_string()),
        )])),
    }
}

/// Apply assembled outputs to a round conversation: insert the stable header
/// as the leading system message once, then carry the tail per the bearing.
/// Separate message is the default; merged form concatenates with the user
/// task for strict alternation services.
pub fn apply_assembled_prompt(
    conversation: &mut Vec<wf_types::message::Message>,
    assembled: &AssembledPrompt,
    bearing: DynamicTailBearing,
    user_task: &mut String,
) {
    if let Some(ref header) = assembled.stable_header {
        if !has_stable_system_message(conversation) {
            conversation.insert(0, stable_system_message(header.clone()));
        }
    }
    let Some(ref tail) = assembled.volatile_tail else {
        return;
    };
    match bearing {
        DynamicTailBearing::SeparateUserMessage => {
            conversation.push(dynamic_context_message(tail.clone()));
        }
        DynamicTailBearing::MergedIntoTask => {
            if user_task.is_empty() {
                *user_task = tail.clone();
            } else {
                *user_task = format!("{}\n\n{}", tail, user_task);
            }
        }
    }
}

/// Pre-rendered tool exposure blocks handed to the loop config.
pub struct ExposureArtifacts {
    pub general_description: Option<String>,
    pub discoverable_metadata_block: Option<String>,
}

/// Resolve exposure once and render the `general` description plus the
/// discoverable metadata block from that single resolution. Assembly-time
/// overrides and activations stay empty; per-turn resolution owns those.
#[allow(clippy::too_many_arguments)]
pub fn build_exposure_artifacts(
    env: &PromptEnvironment,
    tool_call_protocol: Option<&wf_types::llm::ToolCallProtocolConfig>,
    available_tool_names: &[String],
    initial_tool_names: &[String],
    discoverable_tool_names: &[String],
    hidden_tool_names: &[String],
    enable_general_tool: Option<bool>,
) -> ExposureArtifacts {
    let metadata_options = wf_tools::discoverable_metadata_options(tool_call_protocol);
    let exposure_resolution = env.tool_registry.map(|registry| {
        wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry,
            available_names: available_tool_names,
            initial_names: initial_tool_names,
            discoverable_names: discoverable_tool_names,
            hidden_names: hidden_tool_names,
            enable_general_tool,
            activated_tools: &HashSet::new(),
            exposure_overrides: &HashMap::new(),
        })
    });

    let template_metrics = env.template_metrics();
    let general_description = render_general_description(
        exposure_resolution
            .as_ref()
            .map(|resolution| resolution.general_enabled)
            .unwrap_or(false),
        env.resource_registries,
        tool_call_protocol,
        template_metrics.as_deref(),
    );

    let discoverable_metadata_block = exposure_resolution.as_ref().and_then(|resolution| {
        if !resolution.general_enabled || resolution.discoverable.is_empty() {
            return None;
        }
        let entries = wf_tools::generate_discoverable_tool_entries_with_options(
            &resolution.discoverable,
            &metadata_options,
        );
        let variables = HashMap::from([("tool_list".to_string(), entries.join("\n"))]);
        let block = env
            .resource_registries
            .and_then(|regs| {
                wf_resource::render_template_with_metrics(
                    regs,
                    wf_resource::DISCOVERABLE_METADATA_TEMPLATE_ID,
                    &wf_resource::TemplateRenderOptions {
                        variables: variables.clone(),
                        ..Default::default()
                    },
                    template_metrics.as_deref(),
                )
            })
            .unwrap_or_else(|| {
                wf_tools::generate_discoverable_tools_metadata_with_options(
                    &resolution.discoverable,
                    &metadata_options,
                )
            });
        Some(block)
    });

    ExposureArtifacts {
        general_description,
        discoverable_metadata_block,
    }
}

fn render_general_description(
    general_enabled: bool,
    regs: Option<&ResourceRegistries>,
    tool_call_protocol: Option<&wf_types::llm::ToolCallProtocolConfig>,
    template_metrics: Option<&wf_metrics::TemplateMetricsCollector>,
) -> Option<String> {
    if !general_enabled {
        return None;
    }
    let regs = regs?;
    let format = tool_call_protocol.map(|f| &f.format);
    let mut variables = HashMap::new();
    variables.insert(
        "tool_call_protocol".to_string(),
        format
            .map(|f| f.to_string())
            .unwrap_or_else(|| "xml".to_string()),
    );
    variables.insert(
        "invoke_example".to_string(),
        general_invoke_example(format),
    );
    wf_resource::render_template_with_metrics(
        regs,
        wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID,
        &wf_resource::TemplateRenderOptions {
            variables,
            ..Default::default()
        },
        template_metrics,
    )
}

fn general_invoke_example(format: Option<&wf_types::llm::ToolCallProtocol>) -> String {
    match format {
        Some(wf_types::llm::ToolCallProtocol::JsonWrapped)
        | Some(wf_types::llm::ToolCallProtocol::JsonRaw)
        | Some(wf_types::llm::ToolCallProtocol::Native) => {
            "{\"tool\": \"general\", \"parameters\": {\"request\": \"{\\\"tool\\\": \\\"web_search\\\", \
             \\\"parameters\\\": {\\\"query\\\": \\\"rust\\\"}}\"}}"
                .to_string()
        }
        _ => "<tool_use>\n  <tool_name>general</tool_name>\n  <parameters>\n    \
              <request>{\"tool\": \"web_search\", \"parameters\": {\"query\": \"rust\"}}</request>\n  \
              </parameters>\n</tool_use>"
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_config_with(json: serde_json::Value) -> wf_types::agent::AgentConfig {
        serde_json::from_value(json).expect("agent config")
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
        let assembled = assemble_agent_prompt(Some(&config), &vars, &env, &[]);
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
        let assembled = assemble_agent_prompt(Some(&config), &vars, &env, &[]);
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
        let assembled = assemble_agent_prompt(Some(&config), &vars, &env, &[]);
        assert_eq!(assembled.stable_header.as_deref(), Some("base"));
        assert!(assembled.volatile_tail.is_some());
        assert!(assembled.volatile_tail.unwrap().contains("Current time:"));
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
        let mut task = "do work".to_string();
        apply_assembled_prompt(
            &mut conversation,
            &AssembledPrompt {
                stable_header: Some("header".into()),
                volatile_tail: Some("tail".into()),
            },
            DynamicTailBearing::SeparateUserMessage,
            &mut task,
        );
        assert_eq!(task, "do work");
        assert_eq!(conversation.len(), 2);
        assert_eq!(conversation[0].role, wf_types::message::MessageRole::System);
        assert_eq!(conversation[1].role, wf_types::message::MessageRole::User);
        assert!(is_dynamic_context_message(&conversation[1]));
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
        let stable_map = build_stable_header(Some(&config), &env, &[]);
        let stable_node = build_stable_header(Some(&config), &env, &[]);
        assert_eq!(stable_map, stable_node);
        assert!(!stable_map.unwrap().contains("TODO"));
    }
}
