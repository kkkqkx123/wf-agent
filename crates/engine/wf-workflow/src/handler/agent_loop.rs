use async_trait::async_trait;
use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;

use wf_agent::checkpoint::AgentCheckpointStrategy;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_agent::VariableBackedVisibilityStore;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_llm::LlmGateway;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, HookConfig};
use wf_tools::registry::ToolRegistry;

use wf_types::message::Message;
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::message_context;

/// Derive the discoverable-metadata verbosity options from the effective
/// tool call format config (delegates to the shared wf-tools function).
fn parse_agent_hooks(agent_config: Option<&wf_types::agent::AgentConfig>) -> Vec<HookConfig> {
    agent_config
        .and_then(|c| c.hooks.as_ref())
        .map(|hooks| {
            hooks
                .iter()
                .map(|h| HookConfig {
                    hook_type: serde_json::to_string(&h.hook_type)
                        .map(|t| t.trim_matches('"').to_string())
                        .unwrap_or_else(|_| format!("{:?}", h.hook_type)),
                    condition: h.condition.clone(),
                    enabled: h.enabled.unwrap_or(true),
                    parallel: None,
                    continue_on_error: None,
                    receiver: h.receiver.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Collect the initial conversation: inline `conversation` messages plus all
/// messages from the named contexts listed in `message_inputs`, plus any
/// tool-visibility announcement messages appended to the default context
/// (tail system-message injection for formal tool activation).
fn collect_initial_conversation(ctx: &NodeExecutionContext) -> Vec<Message> {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let mut conversation: Vec<Message> = Vec::new();

    if let Some(inline) = config.get("conversation").and_then(|v| v.as_array()) {
        for msg in inline {
            if let Ok(m) = serde_json::from_value(msg.clone()) {
                conversation.push(m);
            }
        }
    }

    if let Some(inputs) = config
        .get("message_inputs")
        .or_else(|| config.get("messageInputs"))
        .and_then(|v| v.as_array())
    {
        for entry in inputs {
            let source = entry
                .get("source_context_id")
                .or_else(|| entry.get("sourceContextId"))
                .and_then(|v| v.as_str());
            if let Some(source) = source {
                conversation.extend(message_context::get_context(&ctx.variables, source));
            }
        }
    }

    // Import `tool_visibility` system announcements (TOOL_VISIBILITY node
    // tail messages) so formal activation is visible in the agent loop.
    let announcements: Vec<Message> =
        message_context::get_context(&ctx.variables, message_context::DEFAULT_CONTEXT_ID)
            .into_iter()
            .filter(|m| {
                m.role == wf_types::message::MessageRole::System
                    && m.metadata
                        .as_ref()
                        .and_then(|meta| meta.get("type"))
                        .map(|t| t == &Value::String("tool_visibility".to_string()))
                        .unwrap_or(false)
            })
            .collect();
    conversation.extend(announcements);

    conversation
}

/// Export the final conversation to the target contexts declared in
/// `message_outputs`.
fn export_conversation(ctx: &NodeExecutionContext, conversation: &[Message]) {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    if let Some(outputs) = config
        .get("message_outputs")
        .or_else(|| config.get("messageOutputs"))
        .and_then(|v| v.as_array())
    {
        for entry in outputs {
            let target = entry
                .get("target_context_id")
                .or_else(|| entry.get("targetContextId"))
                .and_then(|v| v.as_str());
            if let Some(target) = target {
                message_context::register_context(&ctx.variables, target, conversation.to_vec());
            }
        }
    }
}

/// Render the `general` tool description at loop assembly time.
///
/// The text comes from the `tool-visibility.general_description` resource
/// template (variables: `tool_call_format`, `invoke_example`), so it follows
/// custom resource overrides and the effective tool call format; the
/// per-turn schema assembly (wf-agent) writes the rendered text into the
/// routed tool copy. Executions without injected registries fall back to
/// `None` (the builtin static description, current behavior).
fn render_general_description(
    general_enabled: bool,
    regs: Option<&wf_resource::ResourceRegistries>,
    tool_call_format: Option<&wf_types::llm::ToolCallFormatConfig>,
) -> Option<String> {
    if !general_enabled {
        return None;
    }
    let regs = regs?;
    let format = tool_call_format.map(|f| &f.format);
    let mut variables = std::collections::HashMap::new();
    variables.insert(
        "tool_call_format".to_string(),
        format
            .map(|f| f.to_string())
            .unwrap_or_else(|| "xml".to_string()),
    );
    variables.insert("invoke_example".to_string(), general_invoke_example(format));
    wf_resource::render_template(
        regs,
        wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID,
        &wf_resource::TemplateRenderOptions {
            variables,
            ..Default::default()
        },
    )
}

/// Invoke example for the `general` tool description, adapted to the outer
/// tool call format: XML formats teach the `<tool_use>`-wrapped JSON body;
/// JSON formats teach the bare JSON body the model must emit.
fn general_invoke_example(format: Option<&wf_types::llm::ToolCallFormat>) -> String {
    match format {
        Some(wf_types::llm::ToolCallFormat::JsonWrapped)
        | Some(wf_types::llm::ToolCallFormat::JsonRaw)
        | Some(wf_types::llm::ToolCallFormat::Native) => {
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

/// Resolve the configured system prompt: an inline `system_prompt` wins;
/// otherwise `system_prompt_template_id` is rendered through the unified
/// template engine (fragments pseudo variable and `{{var}}` substitution
/// included) with `system_prompt_template_variables` as values. Executions
/// without injected registries or without any prompt config return `None`.
fn resolve_configured_system_prompt(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    regs: Option<&wf_resource::ResourceRegistries>,
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
    wf_resource::render_template(
        regs,
        template_id,
        &wf_resource::TemplateRenderOptions {
            variables,
            ..Default::default()
        },
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

pub struct AgentLoopHandler {
    gateway: Arc<LlmGateway>,
}

impl AgentLoopHandler {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl NodeHandler for AgentLoopHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::AgentLoop
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl AgentLoopHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        // Canonical AgentLoopNodeConfig: either an inline AgentDefinition or
        // an agent_loop_id reference. Agent loop id resolution is not
        // supported without an agent registry, so a referenced loop must
        // ship an inline definition.
        let definition: Option<wf_types::agent::AgentDefinition> =
            match config.get("inline_definition") {
                None => None,
                Some(v) => crate::config_parse::parse_node_config_or_warn(
                    &ctx.node_id,
                    "inner.inline_definition",
                    v,
                    None,
                ),
            };
        let agent_config = definition.as_ref().and_then(|d| d.config.as_ref());

        if definition.is_none() {
            return Err(WorkflowError::Internal(
                "AGENT_LOOP node requires an inline_definition (or a resolvable agent_loop_id)"
                    .to_string(),
            ));
        }

        let model = agent_config
            .and_then(|c| c.profile_id.clone())
            .ok_or_else(|| {
                WorkflowError::OperationError(
                    "AGENT_LOOP node requires a profile_id in inline_definition.config".to_string(),
                )
            })?;
        // Configured system prompt: inline `system_prompt` wins over
        // `system_prompt_template_id` (rendered through the unified
        // template engine); the dynamic context block (current time /
        // environment / enabled skills / custom sections) is prepended
        // when enabled.
        let system_prompt = {
            let base =
                resolve_configured_system_prompt(agent_config, ctx.resource_registries.as_deref());
            let dynamic = build_dynamic_system_context(agent_config, ctx.tool_registry.as_deref());
            match (base, dynamic) {
                (Some(sp), Some(dynamic_block)) => Some(format!("{}\n\n{}", dynamic_block, sp)),
                (Some(sp), None) => Some(sp),
                (None, dynamic_block) => dynamic_block,
            }
        };
        let max_iterations = agent_config
            .and_then(|c| c.max_iterations)
            .unwrap_or(wf_agent::constants::DEFAULT_MAX_ITERATIONS);
        let stream_enabled = agent_config.and_then(|c| c.stream).unwrap_or(false);
        let max_execution_time = config.get("execution_timeout").and_then(|v| v.as_u64());

        // Effective tool call format config: the profile-level full config
        // supplies the description options (`include_description` /
        // `description_style`, currently unconsumed); the agent-level
        // canonical string overrides the format name when present.
        let tool_call_format = {
            let profile_config = self
                .gateway
                .profile_registry()
                .get(&model)
                .and_then(|p| p.tool_call_format);
            let agent_format = agent_config
                .and_then(|c| c.tool_call_format.as_ref())
                .and_then(|format| wf_types::llm::ToolCallFormatConfig::from_format_str(format));
            match (profile_config, agent_format) {
                (Some(mut profile), Some(agent)) => {
                    profile.format = agent.format;
                    Some(profile)
                }
                (Some(profile), None) => Some(profile),
                (None, agent) => agent,
            }
        };
        // Metadata verbosity derived from the effective config: Brief keeps
        // the enhanced `name(type, required)` shape, Detailed attaches
        // parameter descriptions, `include_description=false` reverts to the
        // legacy names-only list.
        let metadata_options = wf_tools::discoverable_metadata_options(tool_call_format.as_ref());

        let tool_names: Vec<String> = agent_config
            .and_then(|c| c.available_tools.as_ref())
            .map(|tools| {
                tools
                    .available
                    .iter()
                    .chain(tools.initial.as_ref().into_iter().flatten())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let available_tools = agent_config.and_then(|c| c.available_tools.as_ref());

        // Tools visible in the initial schema: the explicit `initial` list,
        // or every available tool when none is configured.
        let initial_tool_names: Vec<String> = available_tools
            .and_then(|tools| {
                tools
                    .initial
                    .as_ref()
                    .filter(|initial| !initial.is_empty())
                    .cloned()
            })
            .unwrap_or_else(|| tool_names.clone());

        let discoverable_tool_names: Vec<String> = available_tools
            .and_then(|tools| tools.discoverable.clone())
            .unwrap_or_default();

        let enable_general_tool = available_tools.and_then(|tools| tools.enable_general_tool);

        let hidden_tool_names: Vec<String> = available_tools
            .and_then(|tools| tools.hidden.clone())
            .unwrap_or_default();

        // Tools formally activated by prior TOOL_VISIBILITY unblock nodes
        // (seeded into the run's ToolDiscoveryState).
        let activated_tool_names: Vec<String> =
            wf_agent::visibility::collect_activated_tools(&ctx.variables)
                .into_iter()
                .collect();

        let text = if let Value::String(s) = &ctx.input {
            s.clone()
        } else {
            ctx.input.to_string()
        };

        // Progressive disclosure: when the agent has the `skill` tool and
        // the runtime skill loader is available, inject enabled-skill
        // metadata into the system prompt so the model knows what skills
        // exist without loading any content.
        let system_prompt = system_prompt.as_ref().map(|sp| {
            let has_skill_tool = tool_names.iter().any(|name| name == "skill");
            let skill_injected = if has_skill_tool {
                match ctx
                    .tool_registry
                    .as_ref()
                    .and_then(|registry| registry.skill_loader())
                {
                    Some(loader) => {
                        let enabled = loader.get_enabled_skills();
                        if enabled.is_empty() {
                            sp.clone()
                        } else {
                            wf_tools::skill::inject_skill_metadata(sp, &enabled)
                        }
                    }
                    None => sp.clone(),
                }
            } else {
                sp.clone()
            };

            // MCP LLM visibility: when the agent can use MCP tools (the
            // generic `use_mcp` tool or registered `mcp_*` tools) and a
            // connection manager is available, append a compact server/tool
            // summary to the prompt.
            let has_mcp_tools = tool_names
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
                    let provider =
                        wf_tools::mcp::McpToolsDynamicContextProvider::new((*manager).clone());
                    let generated = provider
                        .generate_context(&wf_tools::mcp::McpToolsContextOptions::default());
                    if generated.has_servers && !generated.content.is_empty() {
                        format!("{}\n\n{}", skill_injected, generated.content)
                    } else {
                        skill_injected
                    }
                }
                None => skill_injected,
            }
        });

        // Single assembly-time exposure resolution shared by the discoverable
        // metadata injection and the `general` description override: config
        // lists + metadata exposure, filtered by the available pool and the
        // hidden blocklist (assembly-time overrides and activations are
        // empty), so metadata and schema always match.
        let exposure_resolution = ctx.tool_registry.as_ref().map(|registry| {
            wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
                registry: registry.as_ref(),
                available_names: &tool_names,
                initial_names: &initial_tool_names,
                discoverable_names: &discoverable_tool_names,
                hidden_names: &hidden_tool_names,
                enable_general_tool,
                activated_tools: &std::collections::HashSet::new(),
                exposure_overrides: &std::collections::HashMap::new(),
            })
        });

        // General tool description (templateable): rendered at assembly time
        // from the `tool-visibility.general_description` resource template
        // when the general tool is actually exposed; the per-turn schema
        // assembly (wf-agent) writes the rendered text into the routed tool
        // copy, so the description follows custom resource overrides and the
        // tool call format. Executions without injected registries fall back
        // to the builtin static description (current behavior).
        let general_description = render_general_description(
            exposure_resolution
                .as_ref()
                .map(|resolution| resolution.general_enabled)
                .unwrap_or(false),
            ctx.resource_registries.as_deref(),
            tool_call_format.as_ref(),
        );

        // Discoverable tool metadata block (templateable): rendered at
        // assembly time from the `tool-visibility.discoverable_metadata`
        // resource template (variable `{tool_list}`) when the `general` tool
        // is actually exposed; the per-turn request assembly (wf-agent)
        // injects the block into the system prompt, so the metadata always
        // matches the schema. Executions without injected registries fall
        // back to the built-in metadata text (`None`).
        let discoverable_metadata_block = exposure_resolution.as_ref().and_then(|resolution| {
            if !resolution.general_enabled || resolution.discoverable.is_empty() {
                return None;
            }
            let entries = wf_tools::generate_discoverable_tool_entries_with_options(
                &resolution.discoverable,
                &metadata_options,
            );
            let variables =
                std::collections::HashMap::from([("tool_list".to_string(), entries.join("\n"))]);
            let block = ctx
                .resource_registries
                .as_deref()
                .and_then(|regs| {
                    wf_resource::render_template(
                        regs,
                        wf_resource::DISCOVERABLE_METADATA_TEMPLATE_ID,
                        &wf_resource::TemplateRenderOptions {
                            variables: variables.clone(),
                            ..Default::default()
                        },
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

        let message = if let Some(ref sp) = system_prompt {
            format!("{}\n\n{}", sp, text)
        } else {
            text
        };

        let tool_registry = ctx
            .tool_registry
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(ToolRegistry::new()));
        let mut coordinator = AgentLoopCoordinator::new(self.gateway.clone(), tool_registry);
        if let Some(ref bus) = ctx.event_bus {
            coordinator = coordinator.with_event_bus(bus.clone());
        }
        // Nested agent loops inherit the parent's tool-level approval config
        // (external handler and/or policy options).
        if let Some(options) = ctx.tool_approval_options.clone() {
            coordinator = coordinator.with_approval_options(options);
        }
        if let Some(handler) = ctx.tool_approval_handler.clone() {
            coordinator = coordinator.with_approval_handler(handler);
        }
        // Runtime visibility gate: reads `__tool_blocked_*` workflow markers
        // (written by TOOL_VISIBILITY nodes). Blocks intercept at execution
        // time only; the visible schema is assembled independently.
        let visibility_store =
            std::sync::Arc::new(VariableBackedVisibilityStore::new(ctx.variables.clone()));
        coordinator = coordinator.with_visibility_store(visibility_store);
        if let Some(checkpoint) = agent_config.and_then(|c| c.checkpoint.as_ref()) {
            if checkpoint.enabled {
                let interval = checkpoint.interval_iterations.unwrap_or(1);
                coordinator = coordinator.with_checkpoint_strategy(
                    AgentCheckpointStrategy::every_n_iterations(interval),
                );
            }
        }

        let exec_config: wf_types::llm::LlmExecutionConfig =
            crate::config_parse::parse_node_config_or_warn(
                &ctx.node_id,
                "inner (LlmExecutionConfig)",
                config,
                wf_types::llm::LlmExecutionConfig::default(),
            );
        let loop_config = AgentLoopConfig {
            agent_id: ctx.node_id.clone(),
            model,
            available_tool_names: tool_names,
            initial_tool_names,
            discoverable_tool_names,
            enable_general_tool,
            activated_tool_names,
            hidden_tool_names,
            hooks: parse_agent_hooks(agent_config),
            max_iterations: Some(max_iterations),
            max_execution_time,
            tool_call_format: tool_call_format.clone(),
            token_limit: exec_config.token_limit.map(u64::from),
            token_warning_threshold: exec_config.token_warning_threshold,
            enable_token_tracking: exec_config.enable_token_tracking,
            general_description,
            discoverable_metadata_block,
        };

        let loop_input = AgentLoopInput {
            message,
            context: std::collections::HashMap::new(),
            conversation: collect_initial_conversation(ctx),
        };

        if stream_enabled {
            // Stream pass-through: forward deltas and lifecycle events to the
            // workflow event bus, aggregate the final result.
            let mut stream = coordinator.execute_stream(loop_config, loop_input).await;
            let mut final_result = Value::Null;
            let mut iterations = 0u32;
            let mut last_error: Option<String> = None;
            while let Some(event) = stream.next().await {
                if let Some(ref bus) = ctx.event_bus {
                    let event_type = match &event {
                        wf_agent::AgentStreamEvent::LlmDelta { .. } => {
                            wf_types::events::EventType::LlmStreamChunk
                        }
                        wf_agent::AgentStreamEvent::ToolStart { .. } => {
                            wf_types::events::EventType::AgentToolExecutionStarted
                        }
                        wf_agent::AgentStreamEvent::ToolEnd { .. } => {
                            wf_types::events::EventType::AgentToolExecutionCompleted
                        }
                        wf_agent::AgentStreamEvent::IterationStart { .. } => {
                            wf_types::events::EventType::AgentIterationStarted
                        }
                        wf_agent::AgentStreamEvent::IterationEnd { .. } => {
                            wf_types::events::EventType::AgentIterationCompleted
                        }
                        wf_agent::AgentStreamEvent::Completed { .. } => {
                            wf_types::events::EventType::AgentCompleted
                        }
                        wf_agent::AgentStreamEvent::Failed { .. } => {
                            wf_types::events::EventType::AgentFailed
                        }
                        wf_agent::AgentStreamEvent::Interrupted { .. } => {
                            wf_types::events::EventType::AgentCancelled
                        }
                    };
                    let bus_event = wf_types::events::BaseEvent {
                        id: wf_common::generate_id(),
                        r#type: event_type,
                        timestamp: wf_common::now(),
                        workflow_id: Some(ctx.execution_id.clone()),
                        execution_id: Some(ctx.execution_id.clone()),
                        agent_loop_id: Some(ctx.node_id.clone()),
                        event_name: None,
                        metadata: serde_json::to_value(&event).ok().and_then(|v| {
                            v.as_object()
                                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        }),
                    };
                    bus.publish_logged(
                        bus_event,
                        &format!("workflow={} agent-loop={}", ctx.execution_id, ctx.node_id),
                    )
                    .ok();
                }
                match event {
                    wf_agent::AgentStreamEvent::Completed {
                        result,
                        iterations: it,
                    } => {
                        final_result = result;
                        iterations = it;
                    }
                    wf_agent::AgentStreamEvent::Failed { error } => {
                        last_error = Some(error);
                    }
                    _ => {}
                }
            }

            if let Some(error) = last_error {
                return Err(WorkflowError::AgentError(
                    wf_agent::AgentError::ExecutionError(error),
                ));
            }

            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "iteration_count".to_string(),
                Value::Number(iterations.into()),
            );
            metadata.insert("node_id".to_string(), Value::String(ctx.node_id.clone()));
            return Ok(NodeExecutionResult {
                output: final_result,
                next_node_ids: Vec::new(),
                metadata,
            });
        }

        match coordinator.execute(loop_config, loop_input).await {
            Ok(output) => {
                export_conversation(ctx, &output.conversation);

                let mut metadata = std::collections::HashMap::new();
                metadata.insert(
                    "iteration_count".to_string(),
                    Value::Number(output.iterations.into()),
                );
                metadata.insert("node_id".to_string(), Value::String(ctx.node_id.clone()));
                metadata.insert(
                    "message_count".to_string(),
                    Value::Number(serde_json::Number::from(output.conversation.len() as u64)),
                );

                let final_content = output.result;

                Ok(NodeExecutionResult {
                    output: final_content,
                    next_node_ids: Vec::new(),
                    metadata,
                })
            }
            Err(e) => Err(WorkflowError::AgentError(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hooks_from_agent_config() {
        let agent_config = serde_json::from_value::<wf_types::agent::AgentConfig>(serde_json::json!({
            "profile_id": "mock",
            "hooks": [
                {"hook_type": "BEFORE_ITERATION", "event_name": "before_iteration", "enabled": true},
                {"hook_type": "AFTER_TOOL_CALL", "event_name": "after_tool_call", "enabled": false}
            ]
        }))
        .expect("canonical agent config should parse");
        let hooks = parse_agent_hooks(Some(&agent_config));
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].hook_type, "BEFORE_ITERATION");
        assert!(hooks[0].enabled);
        assert!(!hooks[1].enabled);
    }

    #[test]
    fn no_hooks_without_agent_config() {
        assert!(parse_agent_hooks(None).is_empty());
    }

    #[test]
    fn general_description_follows_template_resource() {
        // No registries: falls back to None (builtin static description).
        assert!(render_general_description(true, None, None).is_none());
        assert!(render_general_description(false, None, None).is_none());

        // A custom template override wins and receives the format variable.
        let regs = wf_resource::ResourceRegistries::new();
        use wf_core::registry::MutableRegistry;
        regs.templates
            .register(
                wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID.to_string(),
                std::sync::Arc::new(wf_types::Template {
                    id: wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID.into(),
                    name: "custom general".into(),
                    description: None,
                    category: "tool-visibility".into(),
                    content: "Call inner tools with format={tool_call_format}".into(),
                    variables: None,
                    fragments: None,
                }),
            )
            .unwrap();

        let format = Some(wf_types::llm::ToolCallFormatConfig {
            format: wf_types::llm::ToolCallFormat::Xml,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        });
        let rendered = render_general_description(true, Some(&regs), format.as_ref())
            .expect("configured template must render");
        assert!(rendered.contains("format=xml"), "got: {}", rendered);

        // Disabled general: nothing is rendered even with registries.
        assert!(render_general_description(false, Some(&regs), format.as_ref()).is_none());

        // Unregistered id fallback: the builtin default still renders.
        regs.templates
            .unregister(wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID);
        let builtin = render_general_description(true, Some(&regs), format.as_ref())
            .expect("builtin default must render");
        assert!(builtin.contains("<tool_use>"));
        assert!(builtin.contains("web_search"));

        // JsonWrapped: the builtin example switches to the bare JSON body.
        let json_format = Some(wf_types::llm::ToolCallFormatConfig {
            format: wf_types::llm::ToolCallFormat::JsonWrapped,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        });
        let json_builtin = render_general_description(true, Some(&regs), json_format.as_ref())
            .expect("builtin default must render");
        assert!(json_builtin.contains("\"tool\": \"general\""));
        assert!(
            !json_builtin.contains("<tool_use>"),
            "JSON formats must not teach the XML wrapper: {}",
            json_builtin
        );
    }

    #[test]
    fn collects_conversation_from_contexts() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        message_context::append_context(
            &vars,
            "chat",
            vec![Message {
                id: wf_types::Id::new(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text("hi".to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
        );
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "agent".to_string(),
            StaticNodeType::AgentLoop,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "message_inputs": [{"source_context_id": "chat"}]
        }));
        let conversation = collect_initial_conversation(&ctx);
        assert_eq!(conversation.len(), 1);
    }
}
