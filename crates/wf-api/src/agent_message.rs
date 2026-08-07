use std::collections::BTreeMap;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::message::MessageStorageAdapter;
use wf_types::message::Message;
use wf_types::MessageStorageMetadata;

use crate::context::ApiContext;
use crate::error::ApiResult;
use crate::message::MessageStats;
use crate::message::{estimate_tokens, message_text, role_name};

/// Per-agent-loop message statistics (TS `AgentLoopMessageStats`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentLoopMessageStats {
    pub total: u64,
    pub by_role: BTreeMap<String, u64>,
    pub estimated_tokens: u64,
}

/// Most recent messages of an agent loop, newest first.
pub async fn recent(
    ctx: &ApiContext,
    agent_loop_id: &str,
    count: usize,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    let count = if count == 0 { 20 } else { count };
    let mut messages = ctx
        .storage
        .message
        .list_by_agent_loop(agent_loop_id, None)
        .await?;
    messages.sort_by_key(|r| std::cmp::Reverse(r.message.timestamp));
    messages.truncate(count);
    Ok(messages)
}

/// Keyword search over the messages of one agent loop.
pub async fn search(
    ctx: &ApiContext,
    agent_loop_id: &str,
    query: &str,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let all = ctx
        .storage
        .message
        .list_by_agent_loop(agent_loop_id, None)
        .await?;
    let mut matches: Vec<MessageStorageMetadata> = all
        .into_iter()
        .filter(|record| message_text(&record.message).to_lowercase().contains(&query))
        .collect();
    matches.sort_by_key(|r| std::cmp::Reverse(r.message.timestamp));
    Ok(matches)
}

/// Statistics of one agent loop's messages.
pub async fn stats(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<AgentLoopMessageStats> {
    let messages = ctx
        .storage
        .message
        .list_by_agent_loop(agent_loop_id, None)
        .await?;
    let mut stats = AgentLoopMessageStats {
        total: messages.len() as u64,
        ..AgentLoopMessageStats::default()
    };
    for record in &messages {
        let role = role_name(&record.message.role).to_string();
        *stats.by_role.entry(role).or_insert(0) += 1;
        stats.estimated_tokens += estimate_tokens(&record.message) as u64;
    }
    Ok(stats)
}

/// Normalized conversation history of an agent loop: messages sorted by
/// timestamp, replayed through the shared message API for the caller.
pub async fn conversation_history(
    ctx: &ApiContext,
    agent_loop_id: &str,
    max_messages: Option<usize>,
) -> ApiResult<Vec<Message>> {
    let mut messages = ctx
        .storage
        .message
        .list_by_agent_loop(agent_loop_id, None)
        .await?;
    messages.sort_by_key(|r| r.message.timestamp);
    if let Some(max) = max_messages {
        if max > 0 {
            messages.truncate(max);
        }
    }
    Ok(messages.into_iter().map(|r| r.message).collect())
}

/// Message count of one agent loop.
pub async fn count(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<u64> {
    Ok(ctx
        .storage
        .message
        .list_by_agent_loop(agent_loop_id, None)
        .await?
        .len() as u64)
}

/// Global message statistics (shared with `message::stats`).
pub async fn global_stats(ctx: &ApiContext) -> ApiResult<MessageStats> {
    crate::message::stats(ctx).await
}

/// Message count of one agent loop (alias for `count`).
pub async fn get_message_count(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<u64> {
    count(ctx, agent_loop_id).await
}

/// Remove stale duplicate messages of an agent loop, keeping the latest
/// copy per message id; returns the number of records removed.
pub async fn normalize_history(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<usize> {
    let messages = ctx
        .storage
        .message
        .list_by_agent_loop(agent_loop_id, None)
        .await?;
    let mut seen: BTreeMap<String, MessageStorageMetadata> = BTreeMap::new();
    let mut removed = 0usize;
    for record in messages {
        match seen.get(&record.message.id) {
            Some(existing) if existing.message.timestamp <= record.message.timestamp => {
                if ctx.storage.message.delete(&existing.id).await? {
                    removed += 1;
                }
                seen.insert(record.message.id.clone(), record);
            }
            Some(_) => {
                if ctx.storage.message.delete(&record.id).await? {
                    removed += 1;
                }
            }
            None => {
                seen.insert(record.message.id.clone(), record);
            }
        }
    }
    Ok(removed)
}

/// Unknown agent loop is an empty result set, not an error.
pub async fn get_recent_messages(
    ctx: &ApiContext,
    agent_loop_id: &str,
    count: usize,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    recent(ctx, agent_loop_id, count).await
}

pub async fn search_messages(
    ctx: &ApiContext,
    agent_loop_id: &str,
    query: &str,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    search(ctx, agent_loop_id, query).await
}

pub async fn get_message_stats(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<AgentLoopMessageStats> {
    stats(ctx, agent_loop_id).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::message::{MessageContentValue, MessageRole};

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn make_message(id: &str, role: MessageRole, text: &str, ts: i64) -> Message {
        Message {
            id: id.into(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: ts,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn recent_search_stats_and_count() {
        let ctx = make_ctx();
        ctx.storage
            .message
            .save(&MessageStorageMetadata {
                id: "m1".into(),
                execution_id: "loop-m".into(),
                agent_loop_id: Some("loop-m".into()),
                message: make_message("m1", MessageRole::User, "deploy the service", 100),
            })
            .await
            .unwrap();
        ctx.storage
            .message
            .save(&MessageStorageMetadata {
                id: "m2".into(),
                execution_id: "loop-m".into(),
                agent_loop_id: Some("loop-m".into()),
                message: make_message("m2", MessageRole::Assistant, "deploying now", 200),
            })
            .await
            .unwrap();
        ctx.storage
            .message
            .save(&MessageStorageMetadata {
                id: "m3".into(),
                execution_id: "loop-other".into(),
                agent_loop_id: Some("loop-other".into()),
                message: make_message("m3", MessageRole::User, "unrelated", 300),
            })
            .await
            .unwrap();

        let recent = recent(&ctx, "loop-m", 1).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].message.id, "m2");

        let matches = search(&ctx, "loop-m", "deploy").await.unwrap();
        assert_eq!(matches.len(), 2);

        let stats = stats(&ctx, "loop-m").await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_role.get("user"), Some(&1));
        assert!(stats.estimated_tokens > 0);

        assert_eq!(count(&ctx, "loop-m").await.unwrap(), 2);

        let history = conversation_history(&ctx, "loop-m", Some(10)).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, "m1");

        let global = global_stats(&ctx).await.unwrap();
        assert_eq!(global.total, 3);
    }

    #[tokio::test]
    async fn normalize_history_keeps_latest() {
        let ctx = make_ctx();
        // Two records with the same message id (replayed loop), different ts.
        ctx.storage
            .message
            .save(&MessageStorageMetadata {
                id: "rec-1".into(),
                execution_id: "loop-n".into(),
                agent_loop_id: Some("loop-n".into()),
                message: make_message("msg-1", MessageRole::User, "old", 100),
            })
            .await
            .unwrap();
        ctx.storage
            .message
            .save(&MessageStorageMetadata {
                id: "rec-2".into(),
                execution_id: "loop-n".into(),
                agent_loop_id: Some("loop-n".into()),
                message: make_message("msg-1", MessageRole::User, "new", 200),
            })
            .await
            .unwrap();

        let removed = normalize_history(&ctx, "loop-n").await.unwrap();
        assert!(removed >= 1);
        let remaining = ctx
            .storage
            .message
            .list_by_agent_loop("loop-n", None)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].message.timestamp, 200);
    }
}
