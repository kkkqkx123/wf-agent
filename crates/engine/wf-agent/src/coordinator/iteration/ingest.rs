use std::collections::HashMap;

use wf_types::config::limits::CompactionLimits;
use wf_types::message::{FileReadEntry, LlmToolCall, Message, MessageContent, MessageContentValue};
use wf_types::Metadata;

use wf_checkpoint::CheckpointSession;
use wf_execution_shared::compaction::{
    blake3_hex, classify_tool, distill_tool_output, file_pointer_text, ToolOutputKind,
    ARCHIVE_REF_KEY, ARCHIVE_REF_PREFIX,
};
use wf_execution_shared::conversation_session::ConversationSession;

/// Prepare tool result messages for history: distill oversized outputs and
/// deduplicate file reads by content hash. Originals are archived first and
/// the message keeps an archive pointer. Best-effort throughout: an
/// unavailable archive keeps the original text instead of replacing it.
pub fn prepare_tool_messages(
    tool_calls: &[LlmToolCall],
    tool_messages: Vec<Message>,
    session: &mut ConversationSession,
    archive: Option<&CheckpointSession>,
    limits: &CompactionLimits,
) -> Vec<Message> {
    let distill = limits.tool_distill.unwrap_or(true);
    let dedup = limits.file_dedup.unwrap_or(true);
    let base_seq = session.next_seq();
    tool_messages
        .into_iter()
        .enumerate()
        .map(|(index, mut message)| {
            let tool_name = tool_calls
                .iter()
                .find(|call| {
                    message
                        .tool_call_id
                        .as_ref()
                        .is_some_and(|id| id == &call.id)
                })
                .map(|call| call.function.name.clone())
                .or_else(|| message.tool_name.clone())
                .unwrap_or_default();
            let text = message_text(&message);
            if dedup && classify_tool(&tool_name) == ToolOutputKind::File {
                let path = tool_calls
                    .iter()
                    .find(|call| {
                        message
                            .tool_call_id
                            .as_ref()
                            .is_some_and(|id| id == &call.id)
                    })
                    .and_then(|call| read_file_path(&call.function.arguments));
                if let Some(path) = path {
                    let hash = blake3_hex(&text);
                    let prospective = base_seq.saturating_add(index as u64);
                    return dedupe_file_read(
                        &mut message,
                        session,
                        archive,
                        &path,
                        &hash,
                        &text,
                        prospective,
                    );
                }
            }
            if distill && classify_tool(&tool_name) != ToolOutputKind::File {
                let distilled = distill_tool_output(classify_tool(&tool_name), &text);
                if distilled.len() < text.len() {
                    let blob = archive_original(archive, &message.id, &text);
                    if let Some(blob) = blob {
                        set_archive_ref(&mut message, &blob);
                    }
                    message.content = MessageContentValue::Text(distilled);
                }
            }
            message
        })
        .collect()
}

pub(super) fn message_text(message: &Message) -> String {
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

fn read_file_path(arguments: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()?
        .get("path")?
        .as_str()
        .map(String::from)
}

fn set_archive_ref(message: &mut Message, blob_id: &str) {
    let metadata: &mut Metadata = message.metadata.get_or_insert_with(Default::default);
    metadata.insert(
        ARCHIVE_REF_KEY.to_string(),
        serde_json::Value::String(format!("{ARCHIVE_REF_PREFIX}{blob_id}")),
    );
}

fn archive_original(
    archive: Option<&CheckpointSession>,
    message_id: &str,
    text: &str,
) -> Option<String> {
    let archive = archive?;
    let mut files = HashMap::new();
    files.insert(format!("tools/{message_id}.json"), text.as_bytes().to_vec());
    let outcome = archive.archive_write_files(&files).ok()?;
    outcome.blobs.into_iter().next().map(|(_, blob)| blob)
}

fn dedupe_file_read(
    message: &mut Message,
    session: &mut ConversationSession,
    archive: Option<&CheckpointSession>,
    path: &str,
    hash: &str,
    text: &str,
    prospective_seq: u64,
) -> Message {
    if let Some(existing) = session.file_read_entry(path).cloned() {
        if existing.content_hash == hash {
            if !existing.archive_ref.is_empty() {
                let metadata: &mut Metadata = message.metadata.get_or_insert_with(Default::default);
                metadata.insert(
                    ARCHIVE_REF_KEY.to_string(),
                    serde_json::Value::String(existing.archive_ref.clone()),
                );
            }
            message.content = MessageContentValue::Text(file_pointer_text(
                path,
                &existing.commit,
                existing.message_seq,
            ));
            return message.clone();
        }
    }
    let blob = archive_original(archive, &message.id, text);
    session.record_file_read(FileReadEntry {
        path: path.to_string(),
        content_hash: hash.to_string(),
        commit: String::new(),
        last_read_at: wf_common::now(),
        message_seq: prospective_seq,
        archive_ref: blob
            .map(|id| format!("{ARCHIVE_REF_PREFIX}{id}"))
            .unwrap_or_default(),
    });
    message.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_execution_shared::conversation_session::ConversationSession;

    fn result_message(call_id: &str, tool_name: &str, text: &str) -> Message {
        Message::tool_result(
            call_id.to_string(),
            Some(tool_name.to_string()),
            text.to_string(),
            false,
        )
    }

    fn read_call(id: &str, path: &str) -> LlmToolCall {
        LlmToolCall {
            id: id.to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: "read_file".to_string(),
                arguments: format!(r#"{{"path":"{path}"}}"#),
            },
            call_id: None,
            thought_signature: None,
        }
    }

    #[test]
    fn repeated_file_read_collapses_to_pointer() {
        let mut session = ConversationSession::new();
        let limits = wf_execution_shared::compaction::builtin_compaction_limits();
        let calls = [read_call("c1", "/a.txt"), read_call("c2", "/a.txt")];
        let first = prepare_tool_messages(
            &calls[..1],
            vec![result_message("c1", "read_file", "hello")],
            &mut session,
            None,
            &limits,
        );
        assert_eq!(message_text(&first[0]), "hello");
        for message in first {
            session.add_message(message);
        }
        let second = prepare_tool_messages(
            &calls[1..],
            vec![result_message("c2", "read_file", "hello")],
            &mut session,
            None,
            &limits,
        );
        assert!(message_text(&second[0]).contains("already read"));
        assert!(message_text(&second[0]).contains("m0"));
    }

    #[test]
    fn changed_file_content_reads_again() {
        let mut session = ConversationSession::new();
        let limits = wf_execution_shared::compaction::builtin_compaction_limits();
        let calls = [read_call("c1", "/a.txt"), read_call("c2", "/a.txt")];
        let first = prepare_tool_messages(
            &calls[..1],
            vec![result_message("c1", "read_file", "v1")],
            &mut session,
            None,
            &limits,
        );
        for message in first {
            session.add_message(message);
        }
        let second = prepare_tool_messages(
            &calls[1..],
            vec![result_message("c2", "read_file", "v2 changed")],
            &mut session,
            None,
            &limits,
        );
        assert_eq!(message_text(&second[0]), "v2 changed");
    }
}
