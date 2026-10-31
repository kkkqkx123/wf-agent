use std::collections::HashMap;

use wf_types::llm::LlmRequest;
use wf_types::message::{IndexEntry, Message};

use super::AgentIterationCoordinator;
use crate::entity::AgentLoopEntity;

/// Profile the handoff document call runs against. The summary chain uses
/// the same default profile, so handoff quality tracks summary quality.
pub const HANDOFF_PROFILE_ID: &str = "DEFAULT";

const HANDOFF_RETRY_NOTE: &str =
    "\nThe previous draft missed these KEEP facts; include every one of them:";

/// KEEP-class facts are file references and error-like lines; everything
/// else may be rephrased by the document.
fn keep_facts(text: &str) -> Vec<String> {
    let mut facts = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(reference) = find_file_ref(trimmed) {
            facts.push(reference);
            continue;
        }
        let lower = trimmed.to_lowercase();
        if trimmed.len() < 200
            && (lower.contains("error")
                || lower.contains("fail")
                || lower.contains("panic")
                || lower.contains("assert")
                || lower.contains("traceback"))
        {
            facts.push(trimmed.to_string());
        }
    }
    facts.sort();
    facts.dedup();
    facts.truncate(64);
    facts
}

/// Find the first `path:line` reference in a line, if any.
fn find_file_ref(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b':'
            && index > 0
            && index + 1 < bytes.len()
            && bytes[index + 1].is_ascii_digit()
        {
            let mut start = index;
            while start > 0 {
                let c = bytes[start - 1] as char;
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '+' | '@') {
                    start -= 1;
                } else {
                    break;
                }
            }
            let mut end = index + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end - start > 3 {
                return Some(line[start..end].to_string());
            }
        }
        index += 1;
    }
    None
}

/// KEEP facts present in the sources but missing from the document.
fn keep_fact_gaps(sources: &str, document: &str) -> Vec<String> {
    keep_facts(sources)
        .into_iter()
        .filter(|fact| !document.contains(fact))
        .collect()
}

fn handoff_user_text(blocks: &[(String, String, String)], reason: &str) -> String {
    let mut out = format!("Handoff reason: {reason}\n\nBlock summaries:\n");
    for (block_id, topic, summary) in blocks {
        out.push_str(&format!("\n[{block_id}] {topic}\n{summary}\n"));
    }
    out
}

impl AgentIterationCoordinator {
    /// Enter handoff: generate the handoff document, validate it against
    /// KEEP facts, archive block sources plus the index, and cut the hot
    /// view. A document-generation failure parks the loop for external
    /// handling instead of degrading silently.
    pub(super) async fn enter_handoff_flow(&self, entity: &AgentLoopEntity, reason: &'static str) {
        let summaries: Vec<(String, String, String)> = {
            let session = entity.conversation().read().await;
            let history = session.history();
            session
                .compaction_blocks()
                .iter()
                .filter(|block| block.active && !block.summary_message_id.is_empty())
                .filter_map(|block| {
                    let summary = history.iter().find(|m| m.id == block.summary_message_id)?;
                    Some((
                        block.block_id.clone(),
                        block.topic.clone(),
                        super::ingest::message_text(summary),
                    ))
                })
                .collect()
        };
        if summaries.is_empty() {
            self.compression_failure_park(
                entity,
                entity.conversation().read().await.conversation_version(),
                &format!("handoff requested ({reason}) with no compressible summaries"),
            )
            .await;
            return;
        }
        let sources: String = summaries
            .iter()
            .map(|(_, _, text)| text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        // Single source of truth: the `@standard/handoff` asset, not a
        // duplicated constant. The version travels into the validation
        // note with the artifact.
        let (handoff_prompt, handoff_prompt_version) =
            wf_execution_shared::compaction::handoff_system_prompt();
        let request = LlmRequest {
            profile_id: HANDOFF_PROFILE_ID.to_string(),
            messages: vec![
                Message::system_text(handoff_prompt.to_string()),
                Message::user_text(handoff_user_text(&summaries, reason)),
            ],
            parameters: None,
            generation: None,
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
            timeout_ms: None,
            prompt_caching: None,
            cached_content: None,
            max_retries: None,
            retry_delay_ms: None,
            extra_headers: None,
        };
        let first = wf_execution_shared::generate_text_once(&self.gateway, &request, None).await;
        let mut document = match first {
            Ok(result) => result.content.unwrap_or_default(),
            Err(error) => {
                self.compression_failure_park(
                    entity,
                    entity.conversation().read().await.conversation_version(),
                    &format!("handoff document generation failed: {error}"),
                )
                .await;
                return;
            }
        };
        let mut validation_note: Option<String> = None;
        let mut missing = keep_fact_gaps(&sources, &document);
        if !missing.is_empty() {
            let retry = LlmRequest {
                profile_id: HANDOFF_PROFILE_ID.to_string(),
                messages: vec![
                    Message::system_text(handoff_prompt.to_string()),
                    Message::user_text(format!(
                        "{}\n{HANDOFF_RETRY_NOTE}\n{}",
                        handoff_user_text(&summaries, reason),
                        missing.join("\n")
                    )),
                ],
                parameters: None,
                generation: None,
                tools: None,
                tool_call_protocol: None,
                locked_tool_call_protocol: None,
                violation_policy: None,
                execution_id: None,
                stream: None,
                dead_loop_detection: None,
                protocol_auto_converted: None,
                timeout_ms: None,
                prompt_caching: None,
                cached_content: None,
                max_retries: None,
                retry_delay_ms: None,
                extra_headers: None,
            };
            if let Ok(result) =
                wf_execution_shared::generate_text_once(&self.gateway, &retry, None).await
            {
                document = result.content.unwrap_or(document);
            }
            missing = keep_fact_gaps(&sources, &document);
            if !missing.is_empty() {
                if let Some(ref metrics) = self.metrics {
                    metrics
                        .compaction()
                        .record_keep_facts_appended(missing.len() as u64);
                }
                document.push_str(&format!(
                    "\n\ndo_not_forget (auto-appended, model omitted):\n{}",
                    missing.join("\n")
                ));
                validation_note = Some(format!(
                    "handoff validation: {} KEEP fact(s) appended after retry",
                    missing.len()
                ));
            } else {
                validation_note =
                    Some("handoff validation: retry recovered all KEEP facts".to_string());
            }
        }
        let (index, archive_note) = self.archive_handoff_blocks(entity, &summaries).await;
        // The prompt version stamps every artifact, including the clean
        // path, so documents stay attributable to the prompt version.
        let prompt_note = format!("handoff prompt v{handoff_prompt_version}");
        let validation_note = match (validation_note, archive_note) {
            (Some(note), Some(extra)) => Some(format!("{note}; {extra}; {prompt_note}")),
            (Some(note), None) => Some(format!("{note}; {prompt_note}")),
            (None, Some(extra)) => Some(format!("{extra}; {prompt_note}")),
            (None, None) => Some(prompt_note),
        };
        if let Some(ref metrics) = self.metrics {
            metrics.compaction().record_handoff_entered(reason);
        }
        let document_message = Message::system_text(format!(
            "[handoff] context compacted to handoff ({reason})\n{document}"
        ));
        {
            let mut session = entity.conversation().write().await;
            let recent = wf_execution_shared::process_compaction_limits()
                .recent_raw_turns
                .unwrap_or(wf_execution_shared::RECENT_RAW_TURNS_DEFAULT)
                .saturating_mul(2);
            session.enter_handoff(document_message, index, reason, validation_note, recent);
        }
        self.boundary_checkpoint(
            entity,
            wf_types::checkpoint::CheckpointTiming::AfterCompression,
        )
        .await;
    }

    /// Archive active block sources plus the handoff index. Returns the
    /// index and an optional validation note when archiving degraded.
    async fn archive_handoff_blocks(
        &self,
        entity: &AgentLoopEntity,
        summaries: &[(String, String, String)],
    ) -> (Vec<wf_types::message::IndexEntry>, Option<String>) {
        let archive = self.tool_coordinator.checkpoint_session_config();
        let Some(archive) = archive.as_ref() else {
            return (
                summaries
                    .iter()
                    .map(|(block_id, topic, _)| IndexEntry {
                        block_id: block_id.clone(),
                        topic: topic.clone(),
                        time_range: String::new(),
                        archive_ref: String::new(),
                        keywords: Vec::new(),
                        size_tokens: 0,
                    })
                    .collect(),
                Some("archive unavailable; index entries carry no pointers".to_string()),
            );
        };
        let blocks: Vec<(String, String, Vec<String>, u64, String)> = {
            let session = entity.conversation().read().await;
            let history = session.history();
            session
                .compaction_blocks()
                .iter()
                .filter(|block| block.active && !block.summary_message_id.is_empty())
                .map(|block| {
                    let sources: Vec<String> = block
                        .spans
                        .iter()
                        .flat_map(|span| {
                            session
                                .message_seqs()
                                .iter()
                                .zip(history.iter())
                                .filter(|(seq, _)| **seq >= span.start && **seq <= span.end)
                                .map(|(_, message)| super::ingest::message_text(message))
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    let time_range = block
                        .spans
                        .iter()
                        .map(|span| format!("{}..={}", span.start, span.end))
                        .collect::<Vec<_>>()
                        .join(",");
                    (
                        block.block_id.clone(),
                        block.topic.clone(),
                        sources,
                        block.size_tokens,
                        time_range,
                    )
                })
                .collect()
        };
        let mut files: HashMap<String, Vec<u8>> = HashMap::new();
        for (block_id, topic, sources, _, _) in &blocks {
            let summary = summaries
                .iter()
                .find(|(id, _, _)| id == block_id)
                .map(|(_, _, text)| text.clone())
                .unwrap_or_default();
            let payload = serde_json::json!({
                "block_id": block_id,
                "topic": topic,
                "summary": summary,
                "sources": sources,
            });
            files.insert(
                format!("blocks/{block_id}.json"),
                serde_json::to_vec(&payload).unwrap_or_default(),
            );
        }
        let outcome = archive.archive_write_files(&files);
        let blobs: HashMap<String, String> = match outcome {
            Ok(outcome) => outcome.blobs.into_iter().collect(),
            Err(error) => {
                tracing::warn!(
                    entity_id = %entity.id(),
                    error = %error,
                    "handoff block archive write failed; index entries carry no pointers"
                );
                return (
                    blocks
                        .into_iter()
                        .map(|(block_id, topic, _, size_tokens, time_range)| IndexEntry {
                            block_id,
                            topic,
                            time_range,
                            archive_ref: String::new(),
                            keywords: Vec::new(),
                            size_tokens,
                        })
                        .collect(),
                    Some("block archive write failed; index entries carry no pointers".to_string()),
                );
            }
        };
        let mut index = Vec::with_capacity(blocks.len());
        for (block_id, topic, _, size_tokens, time_range) in &blocks {
            let path = format!("blocks/{block_id}.json");
            let keywords: Vec<String> = topic
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| word.len() > 3)
                .take(8)
                .map(str::to_lowercase)
                .collect();
            index.push(IndexEntry {
                block_id: block_id.clone(),
                topic: topic.clone(),
                time_range: time_range.clone(),
                archive_ref: blobs
                    .get(&path)
                    .map(|blob| format!("a:{blob}"))
                    .unwrap_or_default(),
                keywords,
                size_tokens: *size_tokens,
            });
        }
        files.clear();
        files.insert(
            "index.json".to_string(),
            serde_json::to_vec(&index).unwrap_or_default(),
        );
        match archive.archive_write_files(&files) {
            Ok(_) => (index, None),
            Err(error) => {
                tracing::warn!(
                    entity_id = %entity.id(),
                    error = %error,
                    "handoff index archive write failed"
                );
                (
                    index,
                    Some(
                        "index archive write failed; search_history cannot see this handoff"
                            .to_string(),
                    ),
                )
            }
        }
    }
}
