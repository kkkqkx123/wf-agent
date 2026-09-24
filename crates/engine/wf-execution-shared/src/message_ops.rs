use wf_types::message::{
    AppendMessageOperation, FilterMessageOperation, InsertMessageOperation, Message,
    MessageContent, MessageContentValue, MessageOperationConfig, MessageOperationStats,
    MessageRole, ReplaceMessageOperation, RollbackMessageOperation,
};

/// Apply one message-array operation as a pure function: the input slice is
/// never mutated, a new array plus stats is returned.
///
/// Supported operations are the user-facing context operations (append, insert,
/// replace, filter) executed by the workflow context processor and trigger
/// actions against a named message context. Snapshot operations (`Rollback`,
/// `BatchManagement`) are owned by checkpoints, not by message arrays: they are
/// no-ops here and return the input unchanged.
///
/// Isolation rule: the agent conversation session never calls this helper.
/// It only appends through `ConversationSession::add_message` and switches
/// the `MessageView` for compression. `Insert` and `Replace` are reserved
/// for workflow draft contexts; applying them to an agent history would
/// break sequence stability and branch lineage.
pub fn apply(
    messages: &[Message],
    operation: &MessageOperationConfig,
) -> (Vec<Message>, MessageOperationStats) {
    match operation {
        MessageOperationConfig::Append(op) => apply_append(messages, op),
        MessageOperationConfig::Insert(op) => apply_insert(messages, op),
        MessageOperationConfig::Replace(op) => apply_replace(messages, op),
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

/// Merge a compressed summary batch with the retained tail of the active
/// array. Shared by workflow write-back and agent view construction so both
/// sides keep the same tail semantics: summary first, then the last
/// `tail_keep` active messages, deduplicated by message id.
pub fn merge_compressed_with_tail(
    mut summary: Vec<Message>,
    active: &[Message],
    tail_keep: usize,
) -> Vec<Message> {
    if tail_keep == 0 || active.is_empty() {
        return summary;
    }
    let known: std::collections::HashSet<String> = summary.iter().map(|m| m.id.clone()).collect();
    let start = active.len().saturating_sub(tail_keep);
    for message in active.iter().skip(start) {
        if !known.contains(&message.id) {
            summary.push(message.clone());
        }
    }
    summary
}

/// Whether an operation is safe for agent histories. Only `Append` (and
/// read-only `Filter` projections built at request time) qualify; `Insert`
/// and `Replace` would rewrite coordinates and are rejected on the agent
/// path. Workflow draft contexts may still use them.
pub fn is_agent_safe(operation: &MessageOperationConfig) -> bool {
    matches!(
        operation,
        MessageOperationConfig::Append(_) | MessageOperationConfig::BatchManagement(_)
    )
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
        let base = vec![
            msg(MessageRole::User, "deploy x"),
            msg(MessageRole::User, "other"),
        ];
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
        assert_eq!(extract_by_role(&base, MessageRole::User, false).len(), 1);
        assert_eq!(extract_by_role(&base, MessageRole::User, true).len(), 1);
        assert_eq!(
            extract_by_role(&base, MessageRole::User, true)[0].role,
            MessageRole::Assistant
        );
    }

    #[test]
    fn merge_compressed_keeps_tail_deduped() {
        let summary = vec![msg(MessageRole::Assistant, "summary")];
        let active = vec![
            msg(MessageRole::User, "first"),
            msg(MessageRole::User, "second"),
        ];
        let merged = merge_compressed_with_tail(summary, &active, 1);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].role, MessageRole::User);

        let summary = vec![msg(MessageRole::Assistant, "summary")];
        let merged = merge_compressed_with_tail(summary, &active, 0);
        assert_eq!(merged.len(), 1);
    }
}
