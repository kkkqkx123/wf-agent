use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

pub fn truncate_message_content(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        content.to_string()
    } else {
        format!("{}...[truncated]", &content[..max_chars])
    }
}

pub fn truncate_message(message: &Message, max_chars: usize) -> Message {
    let mut cloned = message.clone();
    if let MessageContentValue::Text(text) = &cloned.content {
        if text.len() > max_chars {
            cloned.content = MessageContentValue::Text(truncate_message_content(text, max_chars));
        }
    }
    cloned
}

pub fn merge_consecutive_messages(messages: &[Message]) -> Vec<Message> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < messages.len() {
        let mut current = messages[i].clone();

        if current.role == MessageRole::Assistant {
            let mut j = i + 1;
            while j < messages.len() && messages[j].role == MessageRole::Assistant {
                if let MessageContentValue::Text(merged_text) = &current.content {
                    if let MessageContentValue::Text(next_text) = &messages[j].content {
                        let mut combined = merged_text.clone();
                        combined.push('\n');
                        combined.push_str(next_text);
                        current.content = MessageContentValue::Text(combined);
                    }
                }
                if let Some(ref tool_calls) = messages[j].tool_calls {
                    match &mut current.tool_calls {
                        Some(existing) => existing.extend_from_slice(tool_calls),
                        None => current.tool_calls = Some(tool_calls.clone()),
                    }
                }
                j += 1;
            }
            i = j;
        } else {
            i += 1;
        }

        result.push(current);
    }

    result
}

pub fn split_long_message(message: &Message, max_chars: usize) -> Vec<Message> {
    match &message.content {
        MessageContentValue::Text(text) if text.len() > max_chars => {
            let chunks: Vec<&str> = text.as_bytes()
                .chunks(max_chars)
                .filter_map(|chunk| std::str::from_utf8(chunk).ok())
                .collect();

            chunks.iter().enumerate().map(|(i, chunk)| {
                let mut msg = message.clone();
                msg.id = wf_types::Id::new();
                msg.content = MessageContentValue::Text(chunk.to_string());
                if i > 0 {
                    msg.tool_calls = None;
                }
                msg
            }).collect()
        }
        _ => vec![message.clone()],
    }
}

pub fn extract_text_content(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(blocks) => {
            blocks.iter().filter_map(|block| {
                match block {
                    MessageContent::Text { text } => Some(text.clone()),
                    _ => None,
                }
            }).collect::<Vec<_>>().join("\n")
        }
    }
}

pub fn count_total_chars(messages: &[Message]) -> usize {
    messages.iter().map(|m| {
        match &m.content {
            MessageContentValue::Text(text) => text.len(),
            MessageContentValue::Rich(blocks) => {
                blocks.iter().filter_map(|b| {
                    match b {
                        MessageContent::Text { text } => Some(text.len()),
                        _ => None,
                    }
                }).sum()
            }
        }
    }).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_text_message(role: MessageRole, text: &str) -> Message {
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
    fn test_truncate_message_content() {
        let result = truncate_message_content("hello world", 5);
        assert_eq!(result, "hello...[truncated]");
    }

    #[test]
    fn test_no_truncate_needed() {
        let result = truncate_message_content("hi", 10);
        assert_eq!(result, "hi");
    }

    #[test]
    fn test_merge_consecutive_assistant() {
        let msg1 = make_text_message(MessageRole::Assistant, "First part");
        let msg2 = make_text_message(MessageRole::Assistant, "Second part");
        let merged = merge_consecutive_messages(&[msg1, msg2]);
        assert_eq!(merged.len(), 1);
        match &merged[0].content {
            MessageContentValue::Text(text) => assert_eq!(text, "First part\nSecond part"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_extract_text_content() {
        let msg = make_text_message(MessageRole::User, "Hello");
        assert_eq!(extract_text_content(&msg), "Hello");
    }

    #[test]
    fn test_count_total_chars() {
        let messages = vec![
            make_text_message(MessageRole::User, "abc"),
            make_text_message(MessageRole::Assistant, "defgh"),
        ];
        assert_eq!(count_total_chars(&messages), 8);
    }
}
