use wf_types::message::{Message, MessageRole};

#[derive(Debug, Clone)]
pub struct VisibleRange {
    pub start_index: usize,
    pub end_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityScope {
    All,
    NoSystem,
    UserAndAssistant,
    LastN(usize),
    FromIndex(usize),
}

pub struct VisibleRangeCalculator;

impl VisibleRangeCalculator {
    pub fn calculate(messages: &[Message], scope: &VisibilityScope) -> VisibleRange {
        if messages.is_empty() {
            return VisibleRange {
                start_index: 0,
                end_index: 0,
            };
        }

        let total = messages.len();

        match scope {
            VisibilityScope::All => VisibleRange {
                start_index: 0,
                end_index: total,
            },
            VisibilityScope::NoSystem => {
                let start = messages
                    .iter()
                    .position(|m| m.role != MessageRole::System)
                    .unwrap_or(total);
                VisibleRange {
                    start_index: start,
                    end_index: total,
                }
            }
            VisibilityScope::UserAndAssistant => {
                let start = messages
                    .iter()
                    .position(|m| m.role == MessageRole::User || m.role == MessageRole::Assistant)
                    .unwrap_or(total);
                VisibleRange {
                    start_index: start,
                    end_index: total,
                }
            }
            VisibilityScope::LastN(n) => {
                let start = if *n >= total { 0 } else { total - n };
                VisibleRange {
                    start_index: start,
                    end_index: total,
                }
            }
            VisibilityScope::FromIndex(idx) => {
                let start = *idx.min(&total);
                VisibleRange {
                    start_index: start,
                    end_index: total,
                }
            }
        }
    }

    pub fn filter_messages<'a>(messages: &'a [Message], range: &VisibleRange) -> &'a [Message] {
        let end = range.end_index.min(messages.len());
        if range.start_index >= end {
            return &[];
        }
        &messages[range.start_index..end]
    }

    pub fn calculate_token_budget(
        messages: &[Message],
        scope: &VisibilityScope,
        chars_per_token: f64,
    ) -> usize {
        let range = Self::calculate(messages, scope);
        let visible = Self::filter_messages(messages, &range);
        let total_chars: usize = visible
            .iter()
            .map(|m| match &m.content {
                wf_types::message::MessageContentValue::Text(t) => t.len(),
                wf_types::message::MessageContentValue::Rich(blocks) => blocks
                    .iter()
                    .map(|b| match b {
                        wf_types::message::MessageContent::Text { text } => text.len(),
                        _ => 0,
                    })
                    .sum(),
            })
            .sum();
        (total_chars as f64 / chars_per_token) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(role: MessageRole) -> Message {
        Message {
            id: wf_types::Id::new(),
            role,
            content: wf_types::message::MessageContentValue::Text("test".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn test_all_scope() {
        let messages = vec![
            make_msg(MessageRole::User),
            make_msg(MessageRole::Assistant),
        ];
        let range = VisibleRangeCalculator::calculate(&messages, &VisibilityScope::All);
        assert_eq!(range.start_index, 0);
        assert_eq!(range.end_index, 2);
    }

    #[test]
    fn test_no_system_scope() {
        let messages = vec![
            make_msg(MessageRole::System),
            make_msg(MessageRole::User),
            make_msg(MessageRole::Assistant),
        ];
        let range = VisibleRangeCalculator::calculate(&messages, &VisibilityScope::NoSystem);
        assert_eq!(range.start_index, 1);
        assert_eq!(range.end_index, 3);
    }

    #[test]
    fn test_last_n_scope() {
        let messages = vec![
            make_msg(MessageRole::User),
            make_msg(MessageRole::Assistant),
            make_msg(MessageRole::User),
            make_msg(MessageRole::Assistant),
        ];
        let range = VisibleRangeCalculator::calculate(&messages, &VisibilityScope::LastN(2));
        assert_eq!(range.start_index, 2);
        assert_eq!(range.end_index, 4);
    }

    #[test]
    fn test_filter_messages() {
        let messages = vec![
            make_msg(MessageRole::User),
            make_msg(MessageRole::Assistant),
        ];
        let range = VisibleRange {
            start_index: 1,
            end_index: 2,
        };
        let filtered = VisibleRangeCalculator::filter_messages(&messages, &range);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].role, MessageRole::Assistant);
    }
}
