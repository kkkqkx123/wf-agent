use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::message::{Message, MessageContentValue, MessageRole};

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

pub fn text_message(role: MessageRole, content: String) -> Message {
    Message {
        id: wf_types::Id::new(),
        role,
        content: MessageContentValue::Text(content),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

pub fn message_to_text(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(parts) => {
            let mut out = String::new();
            for part in parts {
                if let wf_types::message::MessageContent::Text { text } = part {
                    out.push_str(text);
                }
            }
            out
        }
    }
}

pub fn tool_result_message(
    tool_call_id: &str,
    tool_name: &str,
    content: String,
    is_error: bool,
) -> Message {
    use std::collections::HashMap;
    Message {
        id: wf_types::Id::new(),
        role: MessageRole::Tool,
        content: MessageContentValue::Text(content),
        timestamp: wf_common::now(),
        tool_call_id: Some(tool_call_id.to_string()),
        tool_name: Some(tool_name.to_string()),
        tool_calls: None,
        thinking: None,
        metadata: Some(HashMap::from([(
            "is_error".to_string(),
            Value::Bool(is_error),
        )])),
    }
}

/// Collect the initial message list for the request:
/// system prompt, optional transform-context injection, messages from the
/// named context (default `current`), inline `messages` config, and finally
/// the node input when nothing else produced messages.
pub fn build_messages(ctx: &NodeExecutionContext) -> WorkflowResult<Vec<Message>> {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let mut messages: Vec<Message> = Vec::new();

    if let Some(system) = config.get("system_prompt").and_then(|v| v.as_str()) {
        messages.push(text_message(MessageRole::System, system.to_string()));
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

    let context_id = config
        .get("context_id")
        .and_then(|v| v.as_str())
        .unwrap_or(message_context::DEFAULT_CONTEXT_ID);
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
}
