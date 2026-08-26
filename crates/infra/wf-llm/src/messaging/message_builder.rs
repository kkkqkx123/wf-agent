use wf_common::time;
use wf_types::message::{Message, MessageContentValue, MessageRole};

pub fn user_text(text: impl Into<String>) -> Message {
    Message {
        id: wf_types::Id::new(),
        role: MessageRole::User,
        content: MessageContentValue::Text(text.into()),
        timestamp: time::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

pub fn system_text(text: impl Into<String>) -> Message {
    Message {
        id: wf_types::Id::new(),
        role: MessageRole::System,
        content: MessageContentValue::Text(text.into()),
        timestamp: time::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

pub fn tool_result_message(tool_call_id: impl Into<String>, content: impl Into<String>) -> Message {
    Message {
        id: wf_types::Id::new(),
        role: MessageRole::Tool,
        content: MessageContentValue::Text(content.into()),
        timestamp: time::now(),
        tool_call_id: Some(tool_call_id.into()),
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
