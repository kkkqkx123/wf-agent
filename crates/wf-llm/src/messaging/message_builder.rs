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
