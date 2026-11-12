use std::collections::HashMap;

use serde_json::Value;

use super::AssembledPrompt;

/// Marker for volatile tail messages, sharing the `type` metadata convention
/// with `tool_visibility` tail announcements.
pub const DYNAMIC_CONTEXT_MESSAGE_TYPE: &str = "dynamic_context";
/// Marker for tool visibility tail announcements.
pub const TOOL_VISIBILITY_MESSAGE_TYPE: &str = "tool_visibility";
/// Marker for explicit tool change announcements describing declaration
/// array differences (added, removed, activated, blocked). Shares the tail
/// system-message convention with `tool_visibility`.
pub const TOOL_CHANGE_MESSAGE_TYPE: &str = "tool_change";

/// How the volatile tail travels: as a separate marked user message, so
/// dynamic state reads as the latest round user context while the user
/// task stays pure input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DynamicTailBearing {
    #[default]
    SeparateUserMessage,
}

/// True for tail volatile messages carrying the dynamic marker.
pub fn is_dynamic_context_message(msg: &wf_types::message::Message) -> bool {
    msg.metadata
        .as_ref()
        .and_then(|meta| meta.get("type"))
        .map(|t| t == &Value::String(DYNAMIC_CONTEXT_MESSAGE_TYPE.to_string()))
        .unwrap_or(false)
}

/// True for explicit tool change tail announcements.
pub fn is_tool_change_message(msg: &wf_types::message::Message) -> bool {
    msg.metadata
        .as_ref()
        .and_then(|meta| meta.get("type"))
        .map(|t| t == &Value::String(TOOL_CHANGE_MESSAGE_TYPE.to_string()))
        .unwrap_or(false)
}

/// True for tool visibility tail announcements.
pub fn is_tool_visibility_message(msg: &wf_types::message::Message) -> bool {
    msg.metadata
        .as_ref()
        .and_then(|meta| meta.get("type"))
        .map(|t| t == &Value::String(TOOL_VISIBILITY_MESSAGE_TYPE.to_string()))
        .unwrap_or(false)
}

/// Stable header exists when a non-announcement system message is present.
/// `tool_visibility`, `tool_change` and `dynamic_context` marked messages
/// never count.
///
/// Locating is owned here; wire concatenation of the located header plus
/// every announcement is owned by the gateway
/// (`wf_llm::tool::protocol::extract_system_message`).
pub fn has_stable_system_message(conversation: &[wf_types::message::Message]) -> bool {
    stable_system_index(conversation).is_some()
}

fn stable_system_index(conversation: &[wf_types::message::Message]) -> Option<usize> {
    conversation.iter().position(|m| {
        m.role == wf_types::message::MessageRole::System
            && !is_tool_visibility_message(m)
            && !is_tool_change_message(m)
            && !is_dynamic_context_message(m)
    })
}

/// Drop every stale volatile tail message from imported history so cross-round
/// imports never accumulate old tails. Fresh tails are added after this
/// filter, so unconditional removal is correct at import time.
pub fn strip_dynamic_context_messages(
    conversation: Vec<wf_types::message::Message>,
) -> Vec<wf_types::message::Message> {
    conversation
        .into_iter()
        .filter(|m| !is_dynamic_context_message(m))
        .collect()
}

/// Split inbound history into filtered history plus a preserved trailing
/// fresh tail. Stale tails in the middle are dropped; a suffix of marked
/// messages is treated as the freshly assembled tail and kept. Used where the
/// fresh tail already sits at the end of the inbound conversation.
pub fn split_trailing_dynamic_tail(
    conversation: Vec<wf_types::message::Message>,
) -> (
    Vec<wf_types::message::Message>,
    Vec<wf_types::message::Message>,
) {
    let mut trailing = Vec::new();
    let mut rest = conversation;
    while rest.last().map(is_dynamic_context_message).unwrap_or(false) {
        trailing.push(rest.pop().expect("non-empty"));
    }
    trailing.reverse();
    let filtered: Vec<wf_types::message::Message> = rest
        .into_iter()
        .filter(|m| !is_dynamic_context_message(m))
        .collect();
    (filtered, trailing)
}

/// Tail user message carrying an explicit tool change notice. User role
/// keeps the notice next to the latest input on the wire: system messages
/// are merged into the leading system block by the gateway, while user
/// messages stay at the tail, so the system prefix keeps its byte
/// stability. Marked so stable header checks skip it and transcripts can
/// filter it.
pub fn tool_change_message(content: String) -> wf_types::message::Message {
    let mut message = wf_types::message::Message::user_text(content);
    message.metadata = Some(HashMap::from([(
        "type".to_string(),
        Value::String(TOOL_CHANGE_MESSAGE_TYPE.to_string()),
    )]));
    message
}

/// Drop stale tool announcements (`tool_change` and `tool_visibility`)
/// from imported history so cross-instance imports never accumulate old
/// notices. Loop entry recomputes a fresh notice from the current
/// variables and target exposure after this filter.
pub fn strip_tool_announcements(
    conversation: Vec<wf_types::message::Message>,
) -> Vec<wf_types::message::Message> {
    conversation
        .into_iter()
        .filter(|m| !is_tool_change_message(m) && !is_tool_visibility_message(m))
        .collect()
}

/// Leading stable system message carrying the cacheable header.
pub fn stable_system_message(content: String) -> wf_types::message::Message {
    wf_types::message::Message::system_text(content)
}

/// Independent volatile tail message: user role so dynamic state reads as the
/// latest round user context, marked so stable checks and transcripts skip it.
pub fn dynamic_context_message(content: String) -> wf_types::message::Message {
    let mut message = wf_types::message::Message::user_text(content);
    message.metadata = Some(HashMap::from([(
        "type".to_string(),
        Value::String(DYNAMIC_CONTEXT_MESSAGE_TYPE.to_string()),
    )]));
    message
}

/// Apply assembled outputs to a round conversation: insert the stable header
/// as the leading system message once, then carry the tail as a separate
/// marked user message. A stale header is refreshed in place so
/// configuration changes take effect instead of lingering. Stored volatile
/// tails are always dropped first, so repeated assembly stays idempotent
/// without caller cleanup. The user task message stays pure input and is
/// never mutated here.
pub fn apply_assembled_prompt(
    conversation: &mut Vec<wf_types::message::Message>,
    assembled: &AssembledPrompt,
    bearing: DynamicTailBearing,
) {
    if let Some(ref header) = assembled.stable_header {
        match stable_system_index(conversation) {
            None => conversation.insert(0, stable_system_message(header.clone())),
            Some(index) => {
                if conversation[index].text_content() != *header {
                    conversation[index].content =
                        wf_types::message::MessageContentValue::Text(header.clone());
                }
            }
        }
    }
    if conversation.iter().any(is_dynamic_context_message) {
        conversation.retain(|m| !is_dynamic_context_message(m));
    }
    let Some(ref tail) = assembled.volatile_tail else {
        return;
    };
    match bearing {
        DynamicTailBearing::SeparateUserMessage => {
            conversation.push(dynamic_context_message(tail.clone()));
        }
    }
}
