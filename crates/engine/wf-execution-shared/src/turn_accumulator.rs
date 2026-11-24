//! Pure accumulator for one provider stream turn.
//!
//! Shared by the chat execution chain and the conversation turn driver: both
//! consume the same `MessageStreamEvent`s and fold text, usage and the final
//! message into one shape. Sink side effects (SSE frames, event-bus deltas,
//! runtime snapshots) stay with the caller and are described by [`AccumAction`],
//! so the two paths keep one event-classification implementation without
//! dragging their different transports into the abstraction.

use wf_types::llm::{merge_usage_non_zero, MessageStreamEvent, TokenUsageStats};
use wf_types::message::Message;

/// What the caller must do to its sink after an event was accumulated.
pub enum AccumAction {
    /// Plain text delta; forward to the sink.
    Text(String),
    /// Reasoning delta; forward to the sink.
    Reasoning(String),
    /// A usage report; already folded into the accumulator.
    Usage(TokenUsageStats),
    /// Provider or stream failed; stop and report this reason.
    Failed(String),
    /// Provider or stream aborted; stop and report this reason.
    Interrupted(String),
    /// No sink action needed (the accumulator still updated its state).
    None,
}

/// Folded state of one streamed answer.
#[derive(Debug, Default)]
pub struct TurnAccumulator {
    pub answer: String,
    pub usage: Option<TokenUsageStats>,
    pub assistant: Option<Message>,
    pub refusal: Option<String>,
}

impl TurnAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consume one provider event, update the folded state and report the
    /// sink action it implies. New `MessageStreamEvent` variants are a compile
    /// error here on purpose: both callers stay in lockstep with the protocol.
    pub fn consume(&mut self, event: MessageStreamEvent) -> AccumAction {
        match event {
            MessageStreamEvent::Text(text) => {
                self.answer.push_str(&text.text);
                AccumAction::Text(text.text)
            }
            MessageStreamEvent::Stream(chunk) => {
                self.answer.push_str(&chunk.content);
                AccumAction::Text(chunk.content)
            }
            MessageStreamEvent::ReasoningText(reasoning) => {
                AccumAction::Reasoning(reasoning.reasoning)
            }
            MessageStreamEvent::Message(message) => {
                self.assistant = Some(message.message);
                AccumAction::None
            }
            MessageStreamEvent::FinalMessage(final_message) => {
                if let Some(reported) = final_message.usage {
                    merge_usage_non_zero(&mut self.usage, &reported);
                }
                self.assistant = Some(final_message.message);
                AccumAction::None
            }
            MessageStreamEvent::Usage(reported) => {
                merge_usage_non_zero(&mut self.usage, &reported.usage);
                AccumAction::Usage(reported.usage)
            }
            MessageStreamEvent::Error(error) => AccumAction::Failed(error.error),
            MessageStreamEvent::Abort(abort) => AccumAction::Interrupted(abort.reason),
            MessageStreamEvent::Refusal(refused) => {
                self.refusal = Some(refused.snapshot);
                AccumAction::None
            }
            MessageStreamEvent::RedactedThinking(_)
            | MessageStreamEvent::Annotation(_)
            | MessageStreamEvent::NativeBlock(_)
            | MessageStreamEvent::End(_)
            | MessageStreamEvent::Connect(_)
            | MessageStreamEvent::Start(_)
            | MessageStreamEvent::InputJson(_)
            | MessageStreamEvent::ToolCallDelta(_) => AccumAction::None,
        }
    }

    /// Reason to report when a stream ended without a final message, preferring
    /// a non-empty refusal snapshot over the caller's generic fallback.
    pub fn missing_message_reason(&self, fallback: impl Into<String>) -> String {
        self.refusal
            .clone()
            .filter(|reason| !reason.is_empty())
            .unwrap_or_else(|| fallback.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(prompt: u32) -> TokenUsageStats {
        TokenUsageStats {
            prompt_tokens: prompt,
            completion_tokens: 0,
            total_tokens: prompt,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            prompt_tokens_cost: None,
            completion_tokens_cost: None,
            total_cost: None,
            provider_total: None,
        }
    }

    #[test]
    fn folds_text_and_usage() {
        let mut acc = TurnAccumulator::new();
        match acc.consume(MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
            text: "hello".into(),
            snapshot: String::new(),
        })) {
            AccumAction::Text(text) => assert_eq!(text, "hello"),
            _ => panic!("text event must produce a text action"),
        }
        assert_eq!(acc.answer, "hello");
        assert!(matches!(
            acc.consume(MessageStreamEvent::Usage(wf_types::llm::MessageStreamUsage {
                usage: usage(3),
            })),
            AccumAction::Usage(_)
        ));
        assert_eq!(acc.usage.as_ref().unwrap().prompt_tokens, 3);
    }

    #[test]
    fn refusal_reason_wins_over_fallback() {
        let mut acc = TurnAccumulator::new();
        acc.consume(MessageStreamEvent::Refusal(wf_types::llm::MessageStreamRefusal {
            refusal: String::new(),
            snapshot: "declined".into(),
        }));
        assert_eq!(acc.missing_message_reason("fallback"), "declined");
    }
}
