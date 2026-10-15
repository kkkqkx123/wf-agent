//! Mini session memory for demos: what enters the transcript.
//!
//! Re-exports the real `wf-mini` transcript so demos exercise the actual
//! implementation instead of a mirrored copy. Only the demo summaries
//! below live here.

use wf_types::message::{Message, MessageContentValue, MessageRole};

pub use wf_mini::transcript::Transcript;

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

/// Human-readable restore summary used by the session demo.
pub fn restore_summary() -> String {
    let stored = vec![
        message(MessageRole::System, "system"),
        message(MessageRole::User, "first question"),
        message(MessageRole::Assistant, "first answer"),
        message(MessageRole::Tool, "tool result"),
        message(MessageRole::User, "   "),
    ];
    let total = stored.len();
    let kept = Transcript::new().restore(stored);
    format!("restore keeps {kept}/{total} messages (user/assistant text only, blanks dropped)\n")
}

/// Human-readable push summary used by the session demo.
pub fn push_summary() -> String {
    let mut transcript = Transcript::new();
    let full = transcript.push_pair("what changed?", "patched login()");
    let blank = transcript.push_pair("retry?", "   ");
    format!(
        "completed turn records {} messages; blank answer records {} message (question only)\n",
        full.len(),
        blank.len()
    )
}
