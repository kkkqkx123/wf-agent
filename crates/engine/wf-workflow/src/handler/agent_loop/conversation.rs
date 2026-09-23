//! Loop-boundary conversation I/O: collect the inbound history from the node
//! config and message contexts, normalize it to this loop's target exposure,
//! and export the final conversation back to the declared output contexts.

use serde_json::Value;

use wf_execution_shared::context::NodeExecutionContext;
use wf_tools::registry::ToolRegistry;
use wf_types::message::Message;

use crate::message_context;

/// Collect the initial conversation: inline `conversation` messages plus all
/// messages from the named contexts listed in `message_inputs`, plus any
/// tool-visibility announcement messages appended to the default context
/// (tail system-message injection for formal tool activation). Volatile tail
/// lifecycle belongs downstream: exposure normalization and prompt assembly
/// drop stale tails before the fresh tail is assembled.
pub(crate) fn collect_initial_conversation(ctx: &NodeExecutionContext) -> Vec<Message> {
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

/// Target-exposure inputs for one loop-boundary normalization: which tools
/// this loop exposes and how history must be rewritten to match.
pub(crate) struct TargetExposure<'a> {
    pub registry: Option<&'a ToolRegistry>,
    pub available: &'a [String],
    pub initial: &'a [String],
    pub discoverable: &'a [String],
    pub hidden: &'a [String],
    pub enable_general_tool: Option<bool>,
    pub activated: &'a [String],
}

/// Normalize an inbound conversation to this loop's target exposure.
///
/// Upstream loops archived history in their own bucket shapes (a direct call
/// where this loop only discovers the tool, or a `general` wrap where this
/// loop exposes it directly). Rewriting once at the boundary keeps the new
/// schema and the replayed history consistent; stored archives stay verbatim
/// and the runtime gates remain authoritative over what may execute.
/// Without a tool registry there is nothing to resolve against, so the
/// conversation passes through unchanged.
///
/// Exposure overrides are intentionally empty here, matching the coordinator's
/// entity-build normalization and the per-turn resolution (no producer wires
/// overrides yet, so all three read the same empty source and cannot drift).
/// Thread a real overrides source through all three sites when one appears.
/// The coordinator re-normalizes idempotently at entity build, which is what
/// covers `call_agent` sub-agent inputs that never pass through this handler.
pub(crate) fn normalize_conversation_for_target(
    conversation: Vec<Message>,
    exposure: &TargetExposure<'_>,
) -> Vec<Message> {
    let TargetExposure {
        registry,
        available,
        initial,
        discoverable,
        hidden,
        enable_general_tool,
        activated,
    } = exposure;
    let Some(registry) = registry else {
        return wf_execution_shared::agent_prompt::strip_dynamic_context_messages(conversation);
    };
    let conversation =
        wf_execution_shared::agent_prompt::strip_dynamic_context_messages(conversation);
    let activated_tools: std::collections::HashSet<String> = activated.iter().cloned().collect();
    let resolution = wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
        registry,
        available_names: available,
        initial_names: initial,
        discoverable_names: discoverable,
        hidden_names: hidden,
        enable_general_tool: *enable_general_tool,
        activated_tools: &activated_tools,
        exposure_overrides: &std::collections::HashMap::new(),
    });
    wf_tools::general_history::normalize_history_for_exposure(&conversation, &resolution)
}

/// Export the final conversation to the target contexts declared in
/// `message_outputs`.
pub(crate) fn export_conversation(ctx: &NodeExecutionContext, conversation: &[Message]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::node::StaticNodeType;

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
