use std::collections::HashMap;

use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

fn message_text(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(parts) => parts
            .iter()
            .filter_map(|part| match part {
                MessageContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn role_label(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "[System]",
        MessageRole::User => "[User]",
        MessageRole::Assistant => "[Assistant]",
        MessageRole::Tool => "[Tool]",
    }
}

/// Render a history as plain audit text, one line per message. Pure
/// presentation helper: never mutates or truncates stored history.
pub fn to_plain_text(messages: &[Message]) -> String {
    messages
        .iter()
        .map(|msg| format!("{} {}", role_label(&msg.role), message_text(msg)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One-line audit summary of a history: message and tool-call counts.
/// Pure presentation helper for logs and diagnostics.
pub fn summarize_counts(messages: &[Message]) -> String {
    let tool_calls: usize = messages
        .iter()
        .map(|m| m.tool_calls.as_ref().map(|calls| calls.len()).unwrap_or(0))
        .sum();
    format!(
        "History: {} messages, {} tool calls",
        messages.len(),
        tool_calls
    )
}

/// Substitute `{{key}}` placeholders in a text message. Returns a new
/// message; the input is never mutated. Intended for request assembly only.
pub fn inject_variables(message: &Message, variables: &HashMap<String, String>) -> Message {
    let mut injected = message.clone();
    if let MessageContentValue::Text(text) = &injected.content {
        let mut result = text.clone();
        for (key, value) in variables {
            result = result.replace(&format!("{{{{{key}}}}}"), value);
        }
        injected.content = MessageContentValue::Text(result);
    }
    injected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_message(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_common::generate_id(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn plain_text_renders_role_prefixes() {
        let history = vec![
            text_message(MessageRole::User, "hello"),
            text_message(MessageRole::Assistant, "hi"),
        ];
        let text = to_plain_text(&history);
        assert!(text.contains("[User] hello"));
        assert!(text.contains("[Assistant] hi"));
    }

    #[test]
    fn summary_counts_messages_and_tool_calls() {
        let mut assistant = text_message(MessageRole::Assistant, "checking");
        assistant.tool_calls = Some(vec![wf_types::message::LlmToolCall {
            id: "c1".to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: "search".to_string(),
                arguments: "{}".to_string(),
            },
        }]);
        let history = vec![text_message(MessageRole::User, "hi"), assistant];
        assert_eq!(
            summarize_counts(&history),
            "History: 2 messages, 1 tool calls"
        );
    }

    #[test]
    fn variable_injection_does_not_mutate_input() {
        let original = text_message(MessageRole::User, "Hello {{name}}");
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "World".to_string());
        let injected = inject_variables(&original, &variables);
        assert_eq!(
            injected.content,
            MessageContentValue::Text("Hello World".to_string())
        );
        assert_eq!(
            original.content,
            MessageContentValue::Text("Hello {{name}}".to_string())
        );
    }
}
