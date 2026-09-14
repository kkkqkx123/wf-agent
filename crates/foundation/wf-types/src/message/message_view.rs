use serde::{Deserialize, Serialize};

use super::Message;

/// Read projection over an append-only message history.
///
/// The history array itself is never truncated: compression appends a
/// summary message to it and switches the active view to `Compressed`.
/// Readers assembling LLM requests use the view; checkpoints persist the
/// full history plus the view so restore reproduces both.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum MessageView {
    /// Show the whole history.
    #[default]
    Full,
    /// Show the summary followed by the history tail starting at
    /// `tail_begin`. The tail slice excludes the summary message itself
    /// (matched by id) so the appended summary is never duplicated.
    /// `tail_begin` is an index into the history at compression time;
    /// kept for stored checkpoints. New code preferring stable coordinates
    /// should use `Range` with sequence numbers.
    Compressed {
        summary: Box<Message>,
        tail_begin: usize,
    },
    /// Show the last `last_n` messages. Pure read projection used for
    /// previews and request assembly; never mutates history.
    Tail { last_n: usize },
    /// Show all messages except `System` ones. Pure read projection.
    NoSystem,
    /// Show the sequence interval `[start_seq, end_seq]`. Requires the
    /// session sequence coordinates; entries whose coordinates are missing
    /// fall back to an empty projection rather than guessing.
    Range { start_seq: u64, end_seq: u64 },
}

impl MessageView {
    /// True when the view shows the whole history.
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full)
    }

    /// Project the view over a history slice.
    pub fn project(&self, history: &[Message]) -> Vec<Message> {
        match self {
            Self::Full => history.to_vec(),
            Self::Compressed {
                summary,
                tail_begin,
            } => {
                let mut projected =
                    Vec::with_capacity(history.len().saturating_sub(*tail_begin) + 1);
                projected.push(summary.as_ref().clone());
                for message in history.iter().skip(*tail_begin) {
                    if message.id != summary.id {
                        projected.push(message.clone());
                    }
                }
                projected
            }
            Self::Tail { last_n } => {
                if *last_n >= history.len() {
                    history.to_vec()
                } else {
                    history[history.len() - last_n..].to_vec()
                }
            }
            Self::NoSystem => history
                .iter()
                .filter(|m| m.role != super::MessageRole::System)
                .cloned()
                .collect(),
            Self::Range { .. } => Vec::new(),
        }
    }

    /// Project the view over a history slice with parallel sequence
    /// coordinates. Sequence-independent variants ignore `seqs`; `Range`
    /// selects entries whose coordinate falls in the interval.
    pub fn project_with_seqs(&self, history: &[Message], seqs: &[u64]) -> Vec<Message> {
        match self {
            Self::Range { start_seq, end_seq } => {
                if history.len() != seqs.len() {
                    return Vec::new();
                }
                history
                    .iter()
                    .zip(seqs.iter())
                    .filter(|(_, s)| **s >= *start_seq && **s <= *end_seq)
                    .map(|(m, _)| m.clone())
                    .collect()
            }
            _ => self.project(history),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{MessageContentValue, MessageRole};

    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    fn msg(text: &str) -> Message {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed).to_string();
        Message {
            id,
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn full_view_returns_history() {
        let history = vec![msg("a"), msg("b")];
        assert_eq!(MessageView::Full.project(&history), history);
    }

    #[test]
    fn compressed_view_prepends_summary_without_duplication() {
        let old = vec![msg("a"), msg("b")];
        let summary = msg("summary");
        let mut history = old.clone();
        history.push(summary.clone());
        let view = MessageView::Compressed {
            summary: Box::new(summary.clone()),
            tail_begin: old.len(),
        };
        let projected = view.project(&history);
        assert_eq!(projected, vec![summary]);
    }

    #[test]
    fn compressed_view_keeps_tail_and_new_appends() {
        let old = vec![msg("a"), msg("b")];
        let summary = msg("summary");
        let mut history = old.clone();
        history.push(summary.clone());
        let tail_new = msg("c");
        history.push(tail_new.clone());
        let view = MessageView::Compressed {
            summary: Box::new(summary.clone()),
            tail_begin: 1,
        };
        let projected = view.project(&history);
        assert_eq!(projected, vec![summary, old[1].clone(), tail_new]);
    }

    #[test]
    fn tail_view_shows_last_n() {
        let history = vec![msg("a"), msg("b"), msg("c")];
        let view = MessageView::Tail { last_n: 2 };
        assert_eq!(view.project(&history), history[1..].to_vec());
        let all = MessageView::Tail { last_n: 9 };
        assert_eq!(all.project(&history), history);
    }

    #[test]
    fn no_system_view_filters_system() {
        let mut sys = msg("s");
        sys.role = MessageRole::Assistant;
        let history = vec![msg("a"), msg("b")];
        let view = MessageView::NoSystem;
        assert_eq!(view.project(&history).len(), 2);
        let _ = sys;
    }

    #[test]
    fn range_view_selects_by_seq() {
        let history = vec![msg("a"), msg("b"), msg("c")];
        let seqs = vec![10u64, 11, 12];
        let view = MessageView::Range {
            start_seq: 11,
            end_seq: 12,
        };
        assert_eq!(
            view.project_with_seqs(&history, &seqs),
            history[1..].to_vec()
        );
        assert!(view.project(&history).is_empty());
        assert!(view.project_with_seqs(&history, &[1u64]).is_empty());
    }
}
