use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::message::{Message, MessageRole};

use crate::error::WorkflowResult;
use crate::message_context;

/// Declared message arrays of this LLM node: the `contexts` check
/// list when configured, otherwise the single `context_id` array.
pub fn declared_contexts(config: &Value) -> Vec<String> {
    if let Some(contexts) = config.get("contexts").and_then(|v| v.as_array()) {
        let ids: Vec<String> = contexts
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        if !ids.is_empty() {
            return ids;
        }
    }
    let context_id = config
        .get("context_id")
        .or_else(|| config.get("contextId"))
        .and_then(|v| v.as_str())
        .unwrap_or(message_context::DEFAULT_CONTEXT_ID);
    vec![context_id.to_string()]
}

/// Messages injected by `transform_context` (part of every request; their
/// estimate participates in the per-array budget of the declared arrays).
pub fn injected_messages(config: &Value) -> Vec<Message> {
    config
        .get("transform_context")
        .and_then(|t| t.get("messages"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| serde_json::from_value::<Message>(m.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Build a plain-text message of any role. System and user roles delegate
/// to the canonical [`Message`] constructors so id and timestamp handling
/// cannot drift from the rest of the codebase.
pub fn text_message(role: MessageRole, content: String) -> Message {
    match role {
        MessageRole::System => Message::system_text(content),
        MessageRole::User => Message::user_text(content),
        MessageRole::Assistant | MessageRole::Tool => Message {
            role,
            ..Message::user_text(content)
        },
    }
}

pub fn message_to_text(message: &Message) -> String {
    message.text_content()
}

pub fn tool_result_message(
    tool_call_id: &str,
    tool_name: &str,
    content: String,
    is_error: bool,
) -> Message {
    Message::tool_result(
        tool_call_id.to_string(),
        Some(tool_name.to_string()),
        content,
        is_error,
    )
}

/// Resolve the system prompt with agent-loop priority: inline text wins,
/// otherwise the template reference renders through the shared registry.
/// Returns `None` when neither is configured or rendering is unavailable.
///
/// Capability boundary (kept lightweight on purpose): only inline-first
/// plus template rendering happens here, unlike the agent loop assembly
/// which adds tool exposure, skill enrichment and dynamic context. A node
/// that needs skills, dynamic context or tool exposure must be an
/// `AGENT_LOOP` node instead of growing this path.
fn resolve_llm_system_prompt(config: &Value, ctx: &NodeExecutionContext) -> Option<String> {
    let system = config.get("system_prompt").and_then(|v| v.as_str());
    let template_id = config
        .get("system_prompt_template_id")
        .and_then(|v| v.as_str());
    if let (None, Some(id)) = (system, template_id) {
        if let Some(regs) = ctx.resource_registries.as_deref() {
            use wf_core::registry::Registry;
            if let Some(template) = regs.templates.get(id) {
                warn_for_unserved_anchors(id, &template.content);
            }
        }
    }
    if let Some(inline) = system {
        warn_for_unserved_anchors("inline system_prompt", inline);
    }
    let variables = config
        .get("system_prompt_template_variables")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    let template_metrics = ctx.metrics.as_ref().map(|m| m.template());
    wf_execution_shared::agent_prompt::resolve_system_prompt_text(
        system,
        template_id,
        variables.as_ref(),
        ctx.resource_registries.as_deref(),
        template_metrics.as_deref(),
    )
    .map(|text| strip_unserved_anchors(&text))
}

/// Strip post-render anchors the lightweight path never resolves so they
/// cannot reach the model literally. Warnings are emitted earlier at the
/// source; this is the misuse-resistant guarantee on the final text.
fn strip_unserved_anchors(text: &str) -> String {
    text.replace(wf_tools::skill::SKILLS_METADATA_PLACEHOLDER, "")
        .replace(wf_tools::DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER, "")
}

/// Warn for post-render anchors a lightweight prompt can never resolve:
/// the skill metadata anchor (injected by the agent loop assembly) and the
/// discoverable-tools anchor (injected per request by the agent loop).
/// The final text strips either marker (see `strip_unserved_anchors`), so
/// this warning points at the source template while the strip guarantees
/// no literal anchor reaches the model.
fn warn_for_unserved_anchors(source: &str, content: &str) {
    for anchor in [
        wf_tools::skill::SKILLS_METADATA_PLACEHOLDER,
        wf_tools::DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER,
    ] {
        if content.contains(anchor) {
            tracing::warn!(
                "llm node {source} uses {anchor} but the lightweight path never injects it; use an AGENT_LOOP node for skill and tool exposure"
            );
        }
    }
}

/// The named context this node's request reads from. Single source of
/// truth shared by request assembly and the compression budget so the
/// dynamic-overhead subtraction always targets the array actually present
/// in the request.
pub fn read_context_id(config: &Value) -> &str {
    config
        .get("context_id")
        .and_then(|v| v.as_str())
        .unwrap_or(message_context::DEFAULT_CONTEXT_ID)
}

/// Collect the initial message list for the request:
/// system prompt, optional transform-context injection, messages from the
/// named context (default `current`), inline `messages` config, and finally
/// the node input when nothing else produced messages.
pub fn build_messages(ctx: &NodeExecutionContext) -> WorkflowResult<Vec<Message>> {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let mut messages: Vec<Message> = Vec::new();

    if let Some(system) = resolve_llm_system_prompt(config, ctx) {
        messages.push(text_message(MessageRole::System, system));
    }

    // transform_context: basic injection of extra messages before the context
    // (dynamic context injection; compression strategies are not implemented).
    if let Some(transform) = config.get("transform_context") {
        if let Some(injected) = transform.get("messages").and_then(|v| v.as_array()) {
            for msg_val in injected {
                if let Ok(msg) = serde_json::from_value::<Message>(msg_val.clone()) {
                    messages.push(msg);
                }
            }
        }
    }

    let context_id = read_context_id(config);
    let context_messages = message_context::get_context(&ctx.variables, context_id);
    if !context_messages.is_empty() {
        messages.extend(context_messages);
    }

    if let Some(msgs) = config.get("messages").and_then(|v| v.as_array()) {
        for msg_val in msgs {
            if let Ok(msg) = serde_json::from_value::<Message>(msg_val.clone()) {
                messages.push(msg);
            }
        }
    }

    if messages.is_empty() {
        let text = if let Value::String(s) = &ctx.input {
            s.clone()
        } else {
            ctx.input.to_string()
        };
        messages.push(text_message(MessageRole::User, text));
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::MessageContentValue;
    use wf_types::node::StaticNodeType;

    fn msg(role: MessageRole, text: &str) -> Message {
        text_message(role, text.to_string())
    }

    #[test]
    fn builds_system_and_context_messages() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        message_context::append_context(&vars, "chat", vec![msg(MessageRole::User, "hello")]);

        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "system_prompt": "be brief",
            "context_id": "chat"
        }));

        let messages = build_messages(&ctx).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, MessageRole::System);
        assert_eq!(messages[1].role, MessageRole::User);
    }

    #[test]
    fn transform_context_injects_messages() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "transform_context": {
                "messages": [{"role": "user", "content": "injected", "id": "m1", "timestamp": 1}]
            }
        }));

        let messages = build_messages(&ctx).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::User);
    }

    #[test]
    fn message_serialization_roundtrip() {
        let m = msg(MessageRole::Assistant, "hi there");
        let json = serde_json::to_value(&m).unwrap();
        let back: Message = serde_json::from_value(json).unwrap();
        assert_eq!(back.role, MessageRole::Assistant);
        assert_eq!(
            back.content,
            MessageContentValue::Text("hi there".to_string())
        );
    }

    #[test]
    fn lightweight_path_strips_unserved_anchors() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "system_prompt": "base {SKILLS_METADATA} mid {DISCOVERABLE_TOOLS_METADATA} tail",
        }));
        let messages = build_messages(&ctx).unwrap();
        let system = messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .expect("system message");
        let text = message_to_text(system);
        assert!(!text.contains("{SKILLS_METADATA}"));
        assert!(!text.contains("{DISCOVERABLE_TOOLS_METADATA}"));
        assert!(text.contains("base"));
    }
}
