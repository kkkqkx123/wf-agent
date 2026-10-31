use std::sync::Arc;

use serde_json::{json, Value};
use wf_types::message::IndexEntry;
use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::trait_def::ToolExecutionContext;
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

pub static SEARCH_HISTORY: ToolDefinition = ToolDefinition {
    id: "search_history",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "context",
    tags: &["history", "archive", "compaction"],
    description: "Search the compacted conversation archive index by keyword or topic. Returns matching block ids, topics and archive pointers without loading the archived content.",
    parameters: &[
        ToolParameter { name: "query", r#type: "string", required: true, description: "Keyword or topic to search for", default_json: None, constraints: None },
        ToolParameter { name: "limit", r#type: "number", required: false, description: "Maximum entries to return", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["search_history(\"storage layout\")"]),
};

pub static READ_ARCHIVE: ToolDefinition = ToolDefinition {
    id: "read_archive",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "context",
    tags: &["history", "archive", "compaction"],
    description: "Read archived block content by archive pointer (a:<blob-id>). Supports line-range slicing. Use after search_history to recover cold context.",
    parameters: &[
        ToolParameter { name: "archive_ref", r#type: "string", required: true, description: "Archive pointer as returned by search_history", default_json: None, constraints: None },
        ToolParameter { name: "start_line", r#type: "number", required: false, description: "First line to return (1-indexed)", default_json: None, constraints: None },
        ToolParameter { name: "end_line", r#type: "number", required: false, description: "Last line to return (inclusive)", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["read_archive(\"a:abc123\")"]),
};

pub const ALL: &[&ToolDefinition] = &[&SEARCH_HISTORY, &READ_ARCHIVE];

pub fn register(registry: &ToolRegistry) -> ToolResult<()> {
    registry.register_stateless_async_handler("search_history", search_history_handler());
    registry.register_stateless_async_handler("read_archive", read_archive_handler());
    Ok(())
}

fn archive_session(ctx: &ToolExecutionContext) -> ToolResult<&wf_checkpoint::CheckpointSession> {
    ctx.checkpoint_session.as_ref().ok_or_else(|| {
        ToolError::ExecutionError(
            "context archive tools need a checkpoint session; the execution runs without file checkpointing"
                .to_string(),
        )
    })
}

pub fn search_history_handler() -> StatelessAsyncHandler {
    Arc::new(move |parameters: Value, ctx: ToolExecutionContext| {
        Box::pin(async move {
            let session = archive_session(&ctx)?;
            let query = parameters
                .get("query")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ToolError::ValidationFailed("query is required".to_string()))?;
            let limit = parameters
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as usize;
            let bytes = session
                .archive_read_file("index.json")
                .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
            let index: Vec<IndexEntry> = serde_json::from_slice(&bytes)
                .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
            let needle = query.to_lowercase();
            let entries: Vec<Value> = index
                .into_iter()
                .filter(|entry| {
                    entry.block_id.to_lowercase().contains(&needle)
                        || entry.topic.to_lowercase().contains(&needle)
                        || entry
                            .keywords
                            .iter()
                            .any(|k| k.to_lowercase().contains(&needle))
                })
                .take(limit.max(1))
                .map(|entry| {
                    json!({
                        "block_id": entry.block_id,
                        "topic": entry.topic,
                        "time_range": entry.time_range,
                        "archive_ref": entry.archive_ref,
                        "size_tokens": entry.size_tokens,
                    })
                })
                .collect();
            Ok(json!({ "entries": entries }))
        })
    })
}

pub fn read_archive_handler() -> StatelessAsyncHandler {
    Arc::new(move |parameters: Value, ctx: ToolExecutionContext| {
        Box::pin(async move {
            let pointer = parameters
                .get("archive_ref")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ToolError::ValidationFailed("archive_ref is required".to_string())
                })?;
            let blob = pointer.strip_prefix("a:").ok_or_else(|| {
                ToolError::ValidationFailed("archive_ref must look like a:<blob-id>".to_string())
            })?;
            let session = archive_session(&ctx)?;
            let bytes = session
                .archive_read_blob(blob)
                .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();
            let total = lines.len();
            let start = parameters
                .get("start_line")
                .and_then(|v| v.as_u64())
                .unwrap_or(1)
                .max(1) as usize;
            let end = parameters
                .get("end_line")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(total)
                .min(total);
            if start > end || total == 0 {
                return Ok(json!({ "content": "", "total_lines": total }));
            }
            Ok(json!({
                "content": lines[start.saturating_sub(1)..end].join("\n"),
                "total_lines": total,
            }))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::trait_def::ToolExecutionContext;

    #[tokio::test]
    async fn search_history_needs_a_session() {
        let handler = search_history_handler();
        let ctx = ToolExecutionContext::new("exec-1".into());
        let result = handler(json!({ "query": "x" }), ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_archive_rejects_pointer_shape() {
        let handler = read_archive_handler();
        let ctx = ToolExecutionContext::new("exec-1".into());
        let result = handler(json!({ "archive_ref": "nope" }), ctx).await;
        assert!(matches!(result, Err(ToolError::ValidationFailed(_))));
    }

    #[test]
    fn context_definitions_register() {
        assert!(ALL.iter().any(|d| d.id == "search_history"));
        assert!(ALL.iter().any(|d| d.id == "read_archive"));
    }
}
