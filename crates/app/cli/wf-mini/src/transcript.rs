//! In-memory conversation transcript: the question/answer pairs that seed
//! every turn so the model remembers earlier rounds.
//!
//! Only completed turns enter the transcript; interrupted and failed turns
//! are dropped by the caller. The transcript is capped: past the limit the
//! oldest messages fall off and the caller prints one note per session.

use wf_types::message::{Message, MessageContentValue, MessageRole};

/// Maximum transcript messages sent as context (about twenty rounds).
///
/// Truncation is by message count, not token budget. Tool results never
/// enter the transcript, so the realistic exposure is an oversized
/// assistant reply; mini accepts that tradeoff for its fixed-size context
/// window rather than rationing by tokens.
pub const TRANSCRIPT_CAP: usize = 40;

/// Maximum messages requested when restoring a session from storage.
pub const RESTORE_LIMIT: usize = 40;

/// Ordered user/assistant messages of this session, oldest first.
#[derive(Debug, Default)]
pub struct Transcript {
    messages: Vec<Message>,
    truncation_noted: bool,
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
    /// messages for storage plus whether the caller should print the
    /// truncation note (true at most once per session).
    pub fn push_pair(&mut self, user: &str, assistant: &str) -> (Vec<Message>, bool) {
        let mut created = Vec::with_capacity(2);
        created.push(user_message(user));
        if !assistant.trim().is_empty() {
            created.push(assistant_message(assistant));
        }
        self.messages.extend(created.iter().cloned());
        let mut note = false;
        if self.messages.len() > TRANSCRIPT_CAP {
            let overflow = self.messages.len() - TRANSCRIPT_CAP;
            self.messages.drain(..overflow);
            if !self.truncation_noted {
                self.truncation_noted = true;
                note = true;
            }
        }
        (created, note)
    }

    /// Forget every round and allow the truncation note to print again.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.truncation_noted = false;
    }

    /// Rebuild from stored messages: keep user/assistant text only, drop
    /// blanks, retain the newest slice that fits the cap. Returns how many
    /// messages were restored.
    pub fn restore(&mut self, stored: Vec<Message>) -> usize {
        let mut kept: Vec<Message> = stored
            .into_iter()
            .filter(|message| {
                matches!(
                    message.role,
                    MessageRole::User | MessageRole::Assistant
                ) && matches!(&message.content, MessageContentValue::Text(text) if !text.trim().is_empty())
            })
            .collect();
        if kept.len() > TRANSCRIPT_CAP {
            let overflow = kept.len() - TRANSCRIPT_CAP;
            kept.drain(..overflow);
        }
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
        let (created, note) = transcript.push_pair("q", "a");
        assert_eq!(created.len(), 2);
        assert!(!note);
        assert_eq!(transcript.len(), 2);
        assert_eq!(transcript.messages()[0].role, MessageRole::User);
        assert_eq!(transcript.messages()[1].role, MessageRole::Assistant);
    }

    #[test]
    fn push_pair_skips_blank_answer_but_keeps_question() {
        let mut transcript = Transcript::new();
        let (created, _) = transcript.push_pair("q", "   ");
        assert_eq!(created.len(), 1);
        assert_eq!(transcript.len(), 1);
    }

    #[test]
    fn push_pair_truncates_oldest_and_notes_once() {
        let mut transcript = Transcript::new();
        let mut notes = 0;
        for index in 0..TRANSCRIPT_CAP + 4 {
            let (_, note) = transcript.push_pair(&format!("q{index}"), &format!("a{index}"));
            if note {
                notes += 1;
            }
        }
        assert_eq!(transcript.len(), TRANSCRIPT_CAP);
        assert_eq!(notes, 1);
    }

    #[test]
    fn clear_empties_and_rearms_note() {
        let mut transcript = Transcript::new();
        for index in 0..TRANSCRIPT_CAP + 2 {
            transcript.push_pair(&format!("q{index}"), &format!("a{index}"));
        }
        transcript.clear();
        assert!(transcript.is_empty());
        let (_, note) = transcript.push_pair("q", "a");
        assert!(!note);
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
    fn restore_caps_to_newest_slice() {
        let mut transcript = Transcript::new();
        let stored: Vec<Message> = (0..TRANSCRIPT_CAP + 10)
            .map(|index| stored(MessageRole::User, &format!("q{index:03}")))
            .collect();
        let restored = transcript.restore(stored);
        assert_eq!(restored, TRANSCRIPT_CAP);
        match &transcript.messages()[TRANSCRIPT_CAP - 1].content {
            MessageContentValue::Text(text) => assert!(text.ends_with("049")),
            other => panic!("expected text, got {other:?}"),
        }
    }
}