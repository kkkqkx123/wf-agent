//! Mini session memory for demos: what enters the transcript.
//!
//! Only completed turns enter memory; interrupted and failed turns never do.
//! Restore keeps user/assistant text only and drops blanks.

use wf_types::message::{Message, MessageContentValue, MessageRole};

fn message(role: MessageRole, text: &str) -> Message {
    Message {
        id: format!("preview-{text}"),
        role,
        content: MessageContentValue::Text(text.to_string()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    }
}

/// One completed round: the question plus the non-blank answer.
pub fn push_pair(user: &str, assistant: &str) -> Vec<Message> {
    let mut created = Vec::with_capacity(2);
    created.push(message(MessageRole::User, user));
    if !assistant.trim().is_empty() {
        created.push(message(MessageRole::Assistant, assistant));
    }
    created
}

/// Filter stored messages the way session restore does.
pub fn restore_filter(stored: &[Message]) -> Vec<&Message> {
    stored
        .iter()
        .filter(|m| {
            matches!(m.role, MessageRole::User | MessageRole::Assistant)
                && matches!(&m.content, MessageContentValue::Text(t) if !t.trim().is_empty())
        })
        .collect()
}

/// Human-readable restore summary used by the session demo.
pub fn restore_summary() -> String {
    let stored = vec![
        message(MessageRole::System, "system"),
        message(MessageRole::User, "first question"),
        message(MessageRole::Assistant, "first answer"),
        message(MessageRole::Tool, "tool result"),
        message(MessageRole::User, "   "),
    ];
    let kept = restore_filter(&stored);
    format!(
        "restore keeps {}/{} messages (user/assistant text only, blanks dropped)\n",
        kept.len(),
        stored.len()
    )
}

/// Human-readable push summary used by the session demo.
pub fn push_summary() -> String {
    let full = push_pair("what changed?", "patched login()");
    let blank = push_pair("retry?", "   ");
    format!(
        "completed turn records {} messages; blank answer records {} message (question only)\n",
        full.len(),
        blank.len()
    )
}
