use wf_types::message::Message;

pub fn user_text(text: impl Into<String>) -> Message {
    Message::user_text(text.into())
}

pub fn system_text(text: impl Into<String>) -> Message {
    Message::system_text(text.into())
}

pub fn tool_result_message(tool_call_id: impl Into<String>, content: impl Into<String>) -> Message {
    Message::tool_result(tool_call_id.into(), None, content.into(), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{MessageContentValue, MessageRole};

    #[test]
    fn user_text_builds_user_role_message() {
        let msg = user_text("hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, MessageContentValue::Text("hello".to_string()));
        assert!(msg.tool_call_id.is_none());
        assert!(msg.tool_calls.is_none());
        assert!(msg.timestamp > 0);
    }

    #[test]
    fn system_text_builds_system_role_message() {
        let msg = system_text("be helpful");
        assert_eq!(msg.role, MessageRole::System);
        assert_eq!(
            msg.content,
            MessageContentValue::Text("be helpful".to_string())
        );
    }

    #[test]
    fn tool_result_message_carries_call_id() {
        let msg = tool_result_message("call_42", "the result");
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_42"));
        assert_eq!(
            msg.content,
            MessageContentValue::Text("the result".to_string())
        );
    }
}
