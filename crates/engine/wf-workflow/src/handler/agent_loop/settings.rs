//! Static configuration extraction for an `AGENT_LOOP` node: parse the node
//! config and the injected gateway into the owned settings the coordinator
//! consumes (model, budgets, effective tool-call protocol, tool buckets).

use serde_json::Value;

use wf_execution_shared::context::NodeExecutionContext;
use wf_llm::LlmGateway;
use wf_tools::callback::HookConfig;

use crate::error::{WorkflowError, WorkflowResult};

/// Everything an agent-loop run needs that is derivable from the node config
/// and the injected gateway before any exposure resolution or execution.
pub(crate) struct AgentLoopSettings {
    pub definition: wf_types::agent::AgentDefinition,
    pub model: String,
    pub max_iterations: u32,
    pub stream_enabled: bool,
    pub max_execution_time: Option<u64>,
    pub tool_call_protocol: Option<wf_types::llm::ToolCallProtocolConfig>,
    pub available_tool_names: Vec<String>,
    pub initial_tool_names: Vec<String>,
    pub discoverable_tool_names: Vec<String>,
    pub hidden_tool_names: Vec<String>,
    pub enable_general_tool: Option<bool>,
    pub activated_tool_names: Vec<String>,
    pub input_text: String,
    pub hooks: Vec<HookConfig>,
    pub token_limit: Option<u64>,
    pub token_warning_threshold: Option<u32>,
    pub enable_token_tracking: Option<bool>,
    pub checkpoint_message_interval: Option<u32>,
}

impl AgentLoopSettings {
    pub fn agent_config(&self) -> Option<&wf_types::agent::AgentConfig> {
        self.definition.config.as_ref()
    }
}

/// Derive the discoverable-metadata verbosity options from the effective
/// tool call format config (delegates to the shared wf-tools function).
fn parse_agent_hooks(agent_config: Option<&wf_types::agent::AgentConfig>) -> Vec<HookConfig> {
    agent_config
        .and_then(|c| c.hooks.as_ref())
        .map(|hooks| {
            hooks
                .iter()
                .map(|h| HookConfig {
                    hook_type: h.hook_type_name().to_string(),
                    condition: h.condition.clone(),
                    enabled: h.enabled.unwrap_or(true),
                    priority: h.priority.unwrap_or(0),
                    payload: h.event_payload.clone(),
                    handler: h.handler.clone(),
                    create_checkpoint: h.create_checkpoint,
                    checkpoint_description: h.checkpoint_description.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve the effective tool call format config: the profile-level full
/// config supplies the description options; the agent-level canonical string
/// overrides the format name when present.
fn resolve_tool_call_protocol(
    ctx: &NodeExecutionContext,
    gateway: &LlmGateway,
    model: &str,
    agent_config: Option<&wf_types::agent::AgentConfig>,
) -> Option<wf_types::llm::ToolCallProtocolConfig> {
    let profile_config = gateway
        .profile_registry()
        .get(model)
        .and_then(|p| p.tool_call_protocol);
    let agent_format = agent_config
        .and_then(|c| c.tool_call_protocol.as_ref())
        .and_then(|format| {
            match wf_types::llm::ToolCallProtocolConfig::from_protocol_str(format) {
                Some(config) => Some(config),
                None => {
                    // An explicit but unknown agent-level override must
                    // not silently dissolve into the profile default.
                    tracing::warn!(
                        node_id = %ctx.node_id,
                        field = "inner.inline_definition.config.tool_call_protocol",
                        value = %format,
                        "unknown tool call format, ignoring agent-level override"
                    );
                    None
                }
            }
        });
    match (profile_config, agent_format) {
        (Some(mut profile), Some(agent)) => {
            profile.format = agent.format;
            Some(profile)
        }
        (Some(profile), None) => Some(profile),
        (None, agent) => agent,
    }
}

/// Parse the node config into the owned `AgentLoopSettings`. The definition is
/// mandatory (a referenced loop must ship an inline definition) and the model
/// profile is required; both surface as errors rather than degrading silently.
pub(crate) fn load_settings(
    ctx: &NodeExecutionContext,
    gateway: &LlmGateway,
) -> WorkflowResult<AgentLoopSettings> {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

    // Canonical AgentLoopNodeConfig: either an inline AgentDefinition or
    // an agent_loop_id reference. Agent loop id resolution is not
    // supported without an agent registry, so a referenced loop must
    // ship an inline definition.
    let definition: Option<wf_types::agent::AgentDefinition> = match config.get("inline_definition")
    {
        None => None,
        Some(v) => crate::config_parse::parse_node_config_or_warn(
            &ctx.node_id,
            "inner.inline_definition",
            v,
            None,
        ),
    };
    let definition = definition.ok_or_else(|| {
        WorkflowError::Internal(
            "AGENT_LOOP node requires an inline_definition (or a resolvable agent_loop_id)"
                .to_string(),
        )
    })?;
    let agent_config = definition.config.as_ref();

    let model = agent_config
        .and_then(|c| c.profile_id.clone())
        .ok_or_else(|| {
            WorkflowError::OperationError(
                "AGENT_LOOP node requires a profile_id in inline_definition.config".to_string(),
            )
        })?;

    let max_iterations = agent_config
        .and_then(|c| c.max_iterations)
        .unwrap_or(wf_agent::constants::DEFAULT_MAX_ITERATIONS);
    let stream_enabled = agent_config.and_then(|c| c.stream).unwrap_or(false);
    // Wall-clock execution budget: the agent definition wins, falling
    // back to the node-level `execution_timeout` override.
    let max_execution_time = agent_config
        .and_then(|c| c.max_execution_time)
        .or_else(|| config.get("execution_timeout").and_then(|v| v.as_u64()));

    let tool_call_protocol = resolve_tool_call_protocol(ctx, gateway, &model, agent_config);

    let available_tools = agent_config.and_then(|c| c.available_tools.as_ref());

    let available_tool_names: Vec<String> = available_tools
        .map(|tools| {
            tools
                .available
                .iter()
                .chain(tools.initial.as_ref().into_iter().flatten())
                .cloned()
                .collect()
        })
        .unwrap_or_default();

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
        .unwrap_or_else(|| available_tool_names.clone());

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

    let input_text = if let Value::String(s) = &ctx.input {
        s.clone()
    } else {
        ctx.input.to_string()
    };

    let exec_config: wf_types::llm::LlmExecutionConfig =
        crate::config_parse::parse_node_config_or_warn(
            &ctx.node_id,
            "inner (LlmExecutionConfig)",
            config,
            wf_types::llm::LlmExecutionConfig::default(),
        );

    Ok(AgentLoopSettings {
        hooks: parse_agent_hooks(agent_config),
        token_limit: agent_config
            .and_then(|c| c.token_limit)
            .or_else(|| exec_config.token_limit.map(u64::from)),
        token_warning_threshold: agent_config
            .and_then(|c| c.token_warning_threshold)
            .or(exec_config.token_warning_threshold),
        enable_token_tracking: agent_config
            .and_then(|c| c.enable_token_tracking)
            .or(exec_config.enable_token_tracking),
        checkpoint_message_interval: agent_config
            .and_then(|c| c.checkpoint.as_ref())
            .and_then(|c| c.message_interval),
        definition,
        model,
        max_iterations,
        stream_enabled,
        max_execution_time,
        tool_call_protocol,
        available_tool_names,
        initial_tool_names,
        discoverable_tool_names,
        hidden_tool_names,
        enable_general_tool,
        activated_tool_names,
        input_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hooks_from_agent_config() {
        let agent_config = serde_json::from_value::<wf_types::agent::AgentConfig>(serde_json::json!({
            "profile_id": "mock",
            "hooks": [
                {"hook_type": "BEFORE_ITERATION", "enabled": true},
                {"hook_type": "AFTER_TOOL_CALL", "enabled": false}
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
}
