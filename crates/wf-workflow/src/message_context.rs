//! Named message contexts shared by LLM / AGENT_LOOP / TOOL_VISIBILITY nodes.
//!
//! Contexts are stored in the workflow variable map under a reserved prefix
//! (`__msg_ctx__`) so they are part of the execution state (checkpoints and
//! variable snapshots) and readable from any node in the workflow.

use dashmap::DashMap;
use serde_json::Value;
use wf_types::message::Message;

/// Variable-map prefix for named message contexts.
pub const CONTEXT_PREFIX: &str = "__msg_ctx__";
/// Default context used when a node does not name one.
pub const DEFAULT_CONTEXT_ID: &str = "current";

fn normalize_id(context_id: &str) -> String {
    if context_id.is_empty() {
        DEFAULT_CONTEXT_ID.to_string()
    } else {
        context_id.to_string()
    }
}

fn context_key(context_id: &str) -> String {
    format!("{}{}", CONTEXT_PREFIX, normalize_id(context_id))
}

/// Read the message list of a named context (empty when absent).
pub fn get_context(variables: &DashMap<String, Value>, context_id: &str) -> Vec<Message> {
    match variables.get(&context_key(context_id)) {
        Some(entry) => serde_json::from_value(entry.clone()).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Append messages to a named context, creating it when needed.
pub fn append_context(
    variables: &DashMap<String, Value>,
    context_id: &str,
    messages: Vec<Message>,
) {
    if messages.is_empty() {
        return;
    }
    let mut existing = get_context(variables, context_id);
    existing.extend(messages);
    if let Ok(value) = serde_json::to_value(&existing) {
        variables.insert(context_key(context_id), value);
    }
}

/// Replace the full content of a named context.
pub fn register_context(
    variables: &DashMap<String, Value>,
    context_id: &str,
    messages: Vec<Message>,
) {
    if let Ok(value) = serde_json::to_value(&messages) {
        variables.insert(context_key(context_id), value);
    }
}

/// Whether a named context has been registered (even when empty).
pub fn has_context(variables: &DashMap<String, Value>, context_id: &str) -> bool {
    variables.contains_key(&context_key(context_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_types::message::{MessageContentValue, MessageRole};

    fn msg(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn context_roundtrip() {
        let vars = Arc::new(DashMap::new());
        append_context(&vars, "chat", vec![msg(MessageRole::User, "hi")]);
        assert!(has_context(&vars, "chat"));
        let ctx = get_context(&vars, "chat");
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].role, MessageRole::User);

        register_context(&vars, "chat", vec![msg(MessageRole::Assistant, "yo")]);
        assert_eq!(get_context(&vars, "chat").len(), 1);
        assert_eq!(get_context(&vars, "chat")[0].role, MessageRole::Assistant);
    }

    #[test]
    fn default_context_fallback() {
        let vars = Arc::new(DashMap::new());
        append_context(&vars, "", vec![msg(MessageRole::User, "hi")]);
        assert!(has_context(&vars, DEFAULT_CONTEXT_ID));
        assert_eq!(get_context(&vars, "current").len(), 1);
        assert_eq!(get_context(&vars, "missing").len(), 0);
    }
}
