//! In-memory conversation transcript: the question/answer pairs that seed
//! every turn so the model remembers earlier rounds.
//!
//! Only completed turns enter the transcript; interrupted and failed turns
//! are dropped by the caller. The transcript keeps the full history: the
//! engine owns all budget and compression decisions, so the CLI never
//! truncates or filters what it sends as context.

use wf_types::message::{Message, MessageContentValue, MessageRole};

/// Ordered user/assistant messages of this session, oldest first.
#[derive(Debug, Default)]
pub struct Transcript {
    messages: Vec<Message>,
}

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Append one completed round. An empty assistant reply still records
    /// the question (only the blank answer is skipped). Returns the created
    /// messages for storage.
    pub fn push_pair(&mut self, user: &str, assistant: &str) -> Vec<Message> {
        let mut created = Vec::with_capacity(2);
        created.push(user_message(user));
        if !assistant.trim().is_empty() {
            created.push(assistant_message(assistant));
        }
        self.messages.extend(created.iter().cloned());
        created
    }

    /// Forget every round.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Rebuild from stored messages: keep user/assistant text only, drop
    /// blanks, retain everything. Returns how many messages were restored.
    pub fn restore(&mut self, stored: Vec<Message>) -> usize {
        let kept: Vec<Message> = stored
            .into_iter()
            .filter(|message| {
                matches!(
                    message.role,
                    MessageRole::User | MessageRole::Assistant
                ) && matches!(&message.content, MessageContentValue::Text(text) if !text.trim().is_empty())
            })
            .collect();
        let restored = kept.len();
        self.messages = kept;
        restored
    }
}

fn user_message(text: &str) -> Message {
    Message {
        id: wf_common::generate_id(),
        role: MessageRole::User,
        content: MessageContentValue::Text(text.to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

fn assistant_message(text: &str) -> Message {
    Message {
        id: wf_common::generate_id(),
        role: MessageRole::Assistant,
        content: MessageContentValue::Text(text.to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(role: MessageRole, text: &str) -> Message {
        Message {
            id: format!("id-{text}"),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: 1,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn push_pair_records_question_and_answer() {
        let mut transcript = Transcript::new();
        let created = transcript.push_pair("q", "a");
        assert_eq!(created.len(), 2);
        assert_eq!(transcript.len(), 2);
        assert_eq!(transcript.messages()[0].role, MessageRole::User);
        assert_eq!(transcript.messages()[1].role, MessageRole::Assistant);
    }

    #[test]
    fn push_pair_skips_blank_answer_but_keeps_question() {
        let mut transcript = Transcript::new();
        let created = transcript.push_pair("q", "   ");
        assert_eq!(created.len(), 1);
        assert_eq!(transcript.len(), 1);
    }

    #[test]
    fn push_pair_keeps_full_history_without_cap() {
        let mut transcript = Transcript::new();
        for index in 0..100 {
            transcript.push_pair(&format!("q{index}"), &format!("a{index}"));
        }
        assert_eq!(transcript.len(), 200);
    }

    #[test]
    fn clear_empties() {
        let mut transcript = Transcript::new();
        for index in 0..10 {
            transcript.push_pair(&format!("q{index}"), &format!("a{index}"));
        }
        transcript.clear();
        assert!(transcript.is_empty());
    }

    #[test]
    fn restore_keeps_user_and_assistant_text_only() {
        let mut transcript = Transcript::new();
        let restored = transcript.restore(vec![
            stored(MessageRole::System, "system"),
            stored(MessageRole::User, "q"),
            stored(MessageRole::Assistant, "a"),
            stored(MessageRole::Tool, "tool-result"),
            stored(MessageRole::User, "   "),
        ]);
        assert_eq!(restored, 2);
        assert_eq!(transcript.len(), 2);
    }

    #[test]
    fn restore_keeps_everything_without_cap() {
        let mut transcript = Transcript::new();
        let stored: Vec<Message> = (0..100)
            .map(|index| stored(MessageRole::User, &format!("q{index:03}")))
            .collect();
        let restored = transcript.restore(stored);
        assert_eq!(restored, 100);
        match &transcript.messages()[99].content {
            MessageContentValue::Text(text) => assert!(text.ends_with("099")),
            other => panic!("expected text, got {other:?}"),
        }
    }
}
