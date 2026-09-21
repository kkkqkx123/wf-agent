use wf_types::message::Message;

pub fn extract_text_content(message: &Message) -> String {
    message.text_content()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{MessageContentValue, MessageRole};

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
    fn test_extract_text_content() {
        let msg = make_text_message(MessageRole::User, "Hello");
        assert_eq!(extract_text_content(&msg), "Hello");
    }
}
