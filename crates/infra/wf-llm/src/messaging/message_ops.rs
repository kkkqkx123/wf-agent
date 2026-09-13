use wf_types::message::{
    AppendMessageOperation, ClearMessageOperation, FilterMessageOperation, InsertMessageOperation,
    Message, MessageContent, MessageContentValue, MessageOperationConfig, MessageOperationStats,
    MessageRole, ReplaceMessageOperation, RollbackMessageOperation, TruncateMessageOperation,
};

/// Apply one message-array operation as a pure function: the input slice is
/// never mutated, a new array plus stats is returned.
///
/// Supported operations are the user-facing context operations (append, insert,
/// replace, truncate, clear, filter) executed by the workflow context
/// processor and trigger actions against a named message context. Snapshot
/// operations (`Rollback`, `BatchManagement`) are owned by checkpoints, not
/// by message arrays: they are no-ops here and return the input unchanged.
pub fn apply(
    messages: &[Message],
    operation: &MessageOperationConfig,
) -> (Vec<Message>, MessageOperationStats) {
    match operation {
        MessageOperationConfig::Append(op) => apply_append(messages, op),
        MessageOperationConfig::Insert(op) => apply_insert(messages, op),
        MessageOperationConfig::Replace(op) => apply_replace(messages, op),
        MessageOperationConfig::Truncate(op) => apply_truncate(messages, op),
        MessageOperationConfig::Clear(op) => apply_clear(messages, op),
        MessageOperationConfig::Filter(op) => apply_filter(messages, op),
        MessageOperationConfig::Rollback(op) => apply_rollback(messages, op),
        MessageOperationConfig::BatchManagement(_) => {
            (messages.to_vec(), unchanged_stats(messages.len()))
        }
    }
}

/// Extract messages of one role (`exclude` inverts the selection). Shared by
/// the `Filter` operation and by compression sub-workflows preparing an
/// explicit summary input.
pub fn extract_by_role(messages: &[Message], role: MessageRole, exclude: bool) -> Vec<Message> {
    messages
        .iter()
        .filter(|m| (m.role == role) != exclude)
        .cloned()
        .collect()
}

/// All text pieces of one message (text content plus rich text parts).
fn message_text(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(parts) => parts
            .iter()
            .filter_map(|part| match part {
                MessageContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn unchanged_stats(total_after: usize) -> MessageOperationStats {
    MessageOperationStats {
        added: 0,
        removed: 0,
        modified: 0,
        total_after: total_after as u32,
    }
}

fn apply_append(
    messages: &[Message],
    op: &AppendMessageOperation,
) -> (Vec<Message>, MessageOperationStats) {
    let mut out = messages.to_vec();
    out.extend(op.messages.iter().cloned());
    let added = op.messages.len() as u32;
    (
        out,
        MessageOperationStats {
            added,
            removed: 0,
            modified: 0,
            total_after: messages.len() as u32 + added,
        },
    )
}

fn apply_insert(
    messages: &[Message],
    op: &InsertMessageOperation,
) -> (Vec<Message>, MessageOperationStats) {
    let mut out = messages.to_vec();
    let index = (op.index as usize).min(out.len());
    for (offset, message) in op.messages.iter().enumerate() {
        out.insert(index + offset, message.clone());
    }
    let added = op.messages.len() as u32;
    (
        out,
        MessageOperationStats {
            added,
            removed: 0,
            modified: 0,
            total_after: messages.len() as u32 + added,
        },
    )
}

fn apply_replace(
    messages: &[Message],
    op: &ReplaceMessageOperation,
) -> (Vec<Message>, MessageOperationStats) {
    let mut out = messages.to_vec();
    let modified = if (op.index as usize) < out.len() {
        out[op.index as usize] = op.message.clone();
        1
    } else {
        0
    };
    (
        out,
        MessageOperationStats {
            added: 0,
            removed: 0,
            modified,
            total_after: messages.len() as u32,
        },
    )
}

fn apply_truncate(
    messages: &[Message],
    op: &TruncateMessageOperation,
) -> (Vec<Message>, MessageOperationStats) {
    let keep = op.keep_count as usize;
    let out = if keep >= messages.len() {
        messages.to_vec()
    } else if op.from_end.unwrap_or(false) {
        messages[messages.len() - keep..].to_vec()
    } else {
        messages[..keep].to_vec()
    };
    let removed = messages.len() as u32 - out.len() as u32;
    (
        out,
        MessageOperationStats {
            added: 0,
            removed,
            modified: 0,
            total_after: messages.len() as u32 - removed,
        },
    )
}

fn apply_clear(
    messages: &[Message],
    _op: &ClearMessageOperation,
) -> (Vec<Message>, MessageOperationStats) {
    (
        Vec::new(),
        MessageOperationStats {
            added: 0,
            removed: messages.len() as u32,
            modified: 0,
            total_after: 0,
        },
    )
}

fn apply_filter(
    messages: &[Message],
    op: &FilterMessageOperation,
) -> (Vec<Message>, MessageOperationStats) {
    let mut out: Vec<Message> = messages.to_vec();
    if let Some(role) = &op.role {
        let exclude = op.exclude.unwrap_or(false);
        out.retain(|m| (m.role != *role) == exclude);
    }
    if let Some(custom) = &op.custom_filter {
        out.retain(|m| message_text(m).contains(custom));
    }
    let removed = messages.len() as u32 - out.len() as u32;
    let total_after = out.len() as u32;
    (
        out,
        MessageOperationStats {
            added: 0,
            removed,
            modified: 0,
            total_after,
        },
    )
}

fn apply_rollback(
    messages: &[Message],
    _op: &RollbackMessageOperation,
) -> (Vec<Message>, MessageOperationStats) {
    (messages.to_vec(), unchanged_stats(messages.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::MessageContentValue;

    fn msg(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_common::generate_id(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::time::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn config(op: MessageOperationConfig) -> MessageOperationConfig {
        op
    }

    #[test]
    fn append_adds_messages() {
        let base = vec![msg(MessageRole::User, "q")];
        let (out, stats) = apply(
            &base,
            &config(MessageOperationConfig::Append(AppendMessageOperation {
                messages: vec![msg(MessageRole::Assistant, "a")],
                batch_index: None,
            })),
        );
        assert_eq!(out.len(), 2);
        assert_eq!(stats.added, 1);
        assert_eq!(base.len(), 1, "input slice is never mutated");
    }

    #[test]
    fn insert_clamps_out_of_range_index() {
        let base = vec![msg(MessageRole::User, "q")];
        let (out, _) = apply(
            &base,
            &config(MessageOperationConfig::Insert(InsertMessageOperation {
                messages: vec![msg(MessageRole::Assistant, "a")],
                index: 99,
            })),
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].role, MessageRole::Assistant);
    }

    #[test]
    fn replace_out_of_range_is_noop() {
        let base = vec![msg(MessageRole::User, "q")];
        let (out, stats) = apply(
            &base,
            &config(MessageOperationConfig::Replace(ReplaceMessageOperation {
                index: 5,
                message: msg(MessageRole::User, "new"),
            })),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(stats.modified, 0);
    }

    #[test]
    fn truncate_keeps_head_or_tail() {
        let base = vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, "q1"),
            msg(MessageRole::Assistant, "a1"),
            msg(MessageRole::User, "q2"),
        ];
        let (head, _) = apply(
            &base,
            &config(MessageOperationConfig::Truncate(TruncateMessageOperation {
                keep_count: 2,
                from_end: None,
            })),
        );
        assert_eq!(head.len(), 2);
        assert_eq!(head[0].role, MessageRole::System);
        let (tail, stats) = apply(
            &base,
            &config(MessageOperationConfig::Truncate(TruncateMessageOperation {
                keep_count: 2,
                from_end: Some(true),
            })),
        );
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].role, MessageRole::Assistant);
        assert_eq!(stats.removed, 2);
    }

    #[test]
    fn filter_by_role_and_exclude() {
        let base = vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, "q1"),
            msg(MessageRole::Assistant, "a1"),
        ];
        let (users, _) = apply(
            &base,
            &config(MessageOperationConfig::Filter(FilterMessageOperation {
                role: Some(MessageRole::User),
                exclude: None,
                custom_filter: None,
            })),
        );
        assert_eq!(users.len(), 1);
        let (no_users, _) = apply(
            &base,
            &config(MessageOperationConfig::Filter(FilterMessageOperation {
                role: Some(MessageRole::User),
                exclude: Some(true),
                custom_filter: None,
            })),
        );
        assert_eq!(no_users.len(), 2);
    }

    #[test]
    fn filter_by_custom_text() {
        let base = vec![msg(MessageRole::User, "deploy x"), msg(MessageRole::User, "other")];
        let (out, stats) = apply(
            &base,
            &config(MessageOperationConfig::Filter(FilterMessageOperation {
                role: None,
                exclude: None,
                custom_filter: Some("deploy".to_string()),
            })),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(stats.removed, 1);
    }

    #[test]
    fn clear_empties_and_reports_removed() {
        let base = vec![msg(MessageRole::User, "q")];
        let (out, stats) = apply(
            &base,
            &config(MessageOperationConfig::Clear(ClearMessageOperation {
                batch_index: None,
            })),
        );
        assert!(out.is_empty());
        assert_eq!(stats.removed, 1);
    }

    #[test]
    fn rollback_is_owned_by_checkpoints_and_noops() {
        let base = vec![msg(MessageRole::User, "q")];
        let (out, stats) = apply(
            &base,
            &config(MessageOperationConfig::Rollback(
                wf_types::message::RollbackMessageOperation { target_batch: 0 },
            )),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(stats.total_after, 1);
    }

    #[test]
    fn extract_by_role_selects_and_inverts() {
        let base = vec![
            msg(MessageRole::User, "q"),
            msg(MessageRole::Assistant, "a"),
        ];
        assert_eq!(
            extract_by_role(&base, MessageRole::User, false).len(),
            1
        );
        assert_eq!(extract_by_role(&base, MessageRole::User, true).len(), 1);
        assert_eq!(
            extract_by_role(&base, MessageRole::User, true)[0].role,
            MessageRole::Assistant
        );
    }
}
