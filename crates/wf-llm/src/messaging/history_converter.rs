use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryFormat {
    Full,
    Condensed,
    ToolOnly,
    NoTool,
}

pub struct HistoryConverter;

impl HistoryConverter {
    pub fn convert(messages: &[Message], format: &HistoryFormat) -> Vec<Message> {
        match format {
            HistoryFormat::Full => messages.to_vec(),
            HistoryFormat::Condensed => Self::condense(messages),
            HistoryFormat::ToolOnly => Self::filter_tool_messages(messages),
            HistoryFormat::NoTool => Self::remove_tool_messages(messages),
        }
    }

    fn condense(messages: &[Message]) -> Vec<Message> {
        let mut result = Vec::new();

        for msg in messages {
            let mut condensed = msg.clone();
            if let MessageContentValue::Text(text) = &condensed.content {
                if text.len() > 200 {
                    condensed.content = MessageContentValue::Text(format!(
                        "{}...[{} chars]",
                        &text[..200],
                        text.len()
                    ));
                }
            }
            result.push(condensed);
        }

        result
    }

    fn filter_tool_messages(messages: &[Message]) -> Vec<Message> {
        messages
            .iter()
            .filter(|m| m.role == MessageRole::Tool || m.tool_calls.is_some())
            .cloned()
            .collect()
    }

    fn remove_tool_messages(messages: &[Message]) -> Vec<Message> {
        messages
            .iter()
            .filter(|m| m.role != MessageRole::Tool && m.tool_calls.is_none())
            .cloned()
            .collect()
    }

    pub fn to_plain_text(messages: &[Message]) -> String {
        messages
            .iter()
            .map(|msg| {
                let role_prefix = match msg.role {
                    MessageRole::System => "[System]",
                    MessageRole::User => "[User]",
                    MessageRole::Assistant => "[Assistant]",
                    MessageRole::Tool => "[Tool]",
                };

                let content = match &msg.content {
                    MessageContentValue::Text(text) => text.clone(),
                    MessageContentValue::Rich(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b {
                            MessageContent::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                };

                format!("{} {}", role_prefix, content)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn summarize(messages: &[Message]) -> String {
        let total = messages.len();
        let tool_calls: usize = messages
            .iter()
            .map(|m| m.tool_calls.as_ref().map(|tc| tc.len()).unwrap_or(0))
            .sum();

        format!("History: {} messages, {} tool calls", total, tool_calls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
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
    fn test_condense_long_messages() {
        let long_text = "a".repeat(500);
        let messages = vec![make_msg(MessageRole::User, &long_text)];
        let condensed = HistoryConverter::convert(&messages, &HistoryFormat::Condensed);
        match &condensed[0].content {
            MessageContentValue::Text(t) => assert!(t.contains("[500 chars]")),
            _ => panic!("Expected text"),
        }
    }

    #[test]
    fn test_remove_tool_messages() {
        let messages = vec![
            make_msg(MessageRole::User, "hello"),
            make_msg(MessageRole::Tool, "result"),
            make_msg(MessageRole::Assistant, "done"),
        ];
        let filtered = HistoryConverter::convert(&messages, &HistoryFormat::NoTool);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_to_plain_text() {
        let messages = vec![
            make_msg(MessageRole::User, "hello"),
            make_msg(MessageRole::Assistant, "hi"),
        ];
        let text = HistoryConverter::to_plain_text(&messages);
        assert!(text.contains("[User] hello"));
        assert!(text.contains("[Assistant] hi"));
    }
}
