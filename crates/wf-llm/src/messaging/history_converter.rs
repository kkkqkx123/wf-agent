use wf_types::llm::{ToolCallFormat, ToolCallMarkers};
use wf_types::message::{LlmToolCall, Message, MessageContent, MessageContentValue, MessageRole};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryFormat {
    Full,
    Condensed,
    ToolOnly,
    NoTool,
}

/// Default XML tags used for text-mode tool call/result rendering
/// (matches the deprecated TS `DEFAULT_XML_TAGS`).
const XML_TOOL_CALL: &str = "tool_use";
const XML_TOOL_NAME: &str = "tool_name";
const XML_TOOL_ARGS: &str = "parameters";
const XML_TOOL_RESULT: &str = "tool_result";
const XML_TOOL_CALL_ID: &str = "tool_call_id";
const XML_TOOL_OUTPUT: &str = "tool_output";

const DEFAULT_JSON_START: &str = "<<<TOOL_CALL>>>";
const DEFAULT_JSON_END: &str = "<<<END_TOOL_CALL>>>";

/// Convert an entire message history from native function-calling format to a
/// text-based tool format (XML/JSON), mirroring the deprecated TS
/// `HistoryConverter.convertToTextMode`:
/// - assistant messages with `tool_calls` get the calls rendered into their
///   content and the `tool_calls` field dropped
/// - tool-result messages become user messages with the result rendered as text
///
/// Returns messages unchanged for `ToolCallFormat::Native`.
pub fn convert_to_text_mode(
    messages: &[Message],
    format: &ToolCallFormat,
    markers: Option<&ToolCallMarkers>,
) -> Vec<Message> {
    if *format == ToolCallFormat::Native {
        return messages.to_vec();
    }

    messages
        .iter()
        .map(|msg| {
            if msg.role == MessageRole::Assistant
                && msg.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty())
            {
                convert_assistant_message(msg, format, markers)
            } else if msg.role == MessageRole::Tool && msg.tool_call_id.is_some() {
                convert_tool_result_message(msg, format, markers)
            } else {
                msg.clone()
            }
        })
        .collect()
}

/// Convert an assistant message with tool calls into a text-only message.
pub fn convert_assistant_message(
    message: &Message,
    format: &ToolCallFormat,
    markers: Option<&ToolCallMarkers>,
) -> Message {
    let mut converted = message.clone();
    let calls = message.tool_calls.as_deref().unwrap_or_default();

    let tool_call_text = render_tool_calls(calls, format, markers);
    let existing = extract_text(message);
    converted.content = MessageContentValue::Text(if existing.is_empty() {
        tool_call_text
    } else {
        format!("{existing}\n\n{tool_call_text}")
    });
    converted.tool_calls = None;
    converted
}

/// Convert a tool-result message into a text-mode user message.
pub fn convert_tool_result_message(
    message: &Message,
    format: &ToolCallFormat,
    markers: Option<&ToolCallMarkers>,
) -> Message {
    let mut converted = message.clone();
    let id = message.tool_call_id.as_deref().unwrap_or("");
    converted.role = MessageRole::User;
    converted.content = MessageContentValue::Text(render_tool_result(
        id,
        &extract_text(message),
        format,
        markers,
    ));
    converted.tool_call_id = None;
    converted
}

fn extract_text(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                MessageContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Render tool calls into the text representation of the given format.
pub fn render_tool_calls(
    calls: &[LlmToolCall],
    format: &ToolCallFormat,
    markers: Option<&ToolCallMarkers>,
) -> String {
    match format {
        ToolCallFormat::Xml => calls
            .iter()
            .map(|tc| {
                format!(
                    "<{XML_TOOL_CALL}>\n<{XML_TOOL_NAME}>{}</{XML_TOOL_NAME}>\n<{XML_TOOL_ARGS}>{}</{XML_TOOL_ARGS}>\n</{XML_TOOL_CALL}>",
                    tc.function.name, tc.function.arguments
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        ToolCallFormat::JsonWrapped | ToolCallFormat::JsonRaw => {
            let start = markers
                .and_then(|m| m.start.as_deref())
                .unwrap_or(DEFAULT_JSON_START);
            let end = markers
                .and_then(|m| m.end.as_deref())
                .unwrap_or(DEFAULT_JSON_END);
            calls
                .iter()
                .map(|tc| {
                    let parsed = serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
                        .unwrap_or_else(|_| serde_json::json!(tc.function.arguments));
                    format!(
                        "{start}\n{}\n{end}",
                        serde_json::json!({
                            "tool": tc.function.name,
                            "parameters": parsed,
                        })
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        ToolCallFormat::Native => String::new(),
    }
}

/// Render a single tool result into the text representation of the given format.
pub fn render_tool_result(
    tool_call_id: &str,
    output: &str,
    format: &ToolCallFormat,
    markers: Option<&ToolCallMarkers>,
) -> String {
    match format {
        ToolCallFormat::Xml => format!(
            "<{XML_TOOL_RESULT}>\n<{XML_TOOL_CALL_ID}>{tool_call_id}</{XML_TOOL_CALL_ID}>\n<{XML_TOOL_OUTPUT}>{output}</{XML_TOOL_OUTPUT}>\n</{XML_TOOL_RESULT}>"
        ),
        ToolCallFormat::JsonWrapped | ToolCallFormat::JsonRaw => {
            let start = markers
                .and_then(|m| m.start.as_deref())
                .unwrap_or(DEFAULT_JSON_START);
            let end = markers
                .and_then(|m| m.end.as_deref())
                .unwrap_or(DEFAULT_JSON_END);
            format!(
                "{start}\n{}\n{end}",
                serde_json::json!({
                    "tool_call_id": tool_call_id,
                    "output": output,
                })
            )
        }
        ToolCallFormat::Native => output.to_string(),
    }
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

    fn make_tool_call(id: &str, name: &str, args: &str) -> LlmToolCall {
        LlmToolCall {
            id: id.to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: name.to_string(),
                arguments: args.to_string(),
            },
        }
    }

    fn make_assistant_with_calls(calls: Vec<LlmToolCall>) -> Message {
        let mut msg = make_msg(MessageRole::Assistant, "checking");
        msg.tool_calls = Some(calls);
        msg
    }

    fn make_tool_result(id: &str, output: &str) -> Message {
        let mut msg = make_msg(MessageRole::Tool, output);
        msg.tool_call_id = Some(id.to_string());
        msg
    }

    #[test]
    fn native_format_returns_messages_unchanged() {
        let messages = vec![
            make_assistant_with_calls(vec![make_tool_call("c1", "get_weather", "{}")]),
            make_tool_result("c1", "sunny"),
        ];
        let converted = convert_to_text_mode(&messages, &ToolCallFormat::Native, None);
        assert_eq!(converted.len(), 2);
        assert!(converted[0].tool_calls.is_some(), "native keeps tool_calls");
    }

    #[test]
    fn xml_conversion_embeds_tool_calls_and_results() {
        let messages = vec![
            make_assistant_with_calls(vec![make_tool_call(
                "c1",
                "get_weather",
                r#"{"city":"Beijing"}"#,
            )]),
            make_tool_result("c1", "sunny"),
        ];
        let converted = convert_to_text_mode(&messages, &ToolCallFormat::Xml, None);

        let assistant = &converted[0];
        assert!(assistant.tool_calls.is_none(), "tool_calls dropped");
        let MessageContentValue::Text(text) = &assistant.content else {
            panic!("expected text");
        };
        assert!(text.contains("<tool_use>"), "{text}");
        assert!(text.contains("<tool_name>get_weather</tool_name>"), "{text}");
        assert!(text.contains(r#"{"city":"Beijing"}"#), "{text}");

        let result = &converted[1];
        assert_eq!(result.role, MessageRole::User, "tool result -> user");
        let MessageContentValue::Text(result_text) = &result.content else {
            panic!("expected text");
        };
        assert!(result_text.contains("<tool_result>"), "{result_text}");
        assert!(result_text.contains("<tool_call_id>c1</tool_call_id>"), "{result_text}");
        assert!(result_text.contains("sunny"), "{result_text}");
    }

    #[test]
    fn json_conversion_uses_markers() {
        let messages = vec![
            make_assistant_with_calls(vec![make_tool_call("c1", "get_weather", "{}")]),
            make_tool_result("c1", "sunny"),
        ];
        let converted = convert_to_text_mode(&messages, &ToolCallFormat::JsonWrapped, None);

        let MessageContentValue::Text(text) = &converted[0].content else {
            panic!("expected text");
        };
        assert!(text.contains("<<<TOOL_CALL>>>"), "{text}");
        let parsed: serde_json::Value = serde_json::from_str(
            text.split("<<<TOOL_CALL>>>")
                .nth(1)
                .unwrap()
                .split("<<<END_TOOL_CALL>>>")
                .next()
                .unwrap()
                .trim(),
        )
        .expect("marker payload must be JSON");
        assert_eq!(parsed["tool"], serde_json::json!("get_weather"));
        assert_eq!(parsed["parameters"], serde_json::json!({}));

        let MessageContentValue::Text(result_text) = &converted[1].content else {
            panic!("expected text");
        };
        assert!(result_text.contains("\"tool_call_id\":\"c1\""), "{result_text}");
        assert!(result_text.contains("\"output\":\"sunny\""), "{result_text}");
    }

    #[test]
    fn assistant_without_calls_is_untouched() {
        let messages = vec![make_msg(MessageRole::Assistant, "plain answer")];
        let converted = convert_to_text_mode(&messages, &ToolCallFormat::Xml, None);
        assert_eq!(converted[0].content, messages[0].content);
        assert!(converted[0].tool_calls.is_none());
    }
}
