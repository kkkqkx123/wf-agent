//! Message resource queries and writes over the message adapter.

use std::collections::BTreeMap;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::message::{MessageListOptions, MessageStorageAdapter};
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};
use wf_types::MessageStorageMetadata;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiResult};

/// Aggregated message statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MessageStats {
    pub total: u64,
    pub by_role: BTreeMap<String, u64>,
    /// Estimated total token count across retained messages (heuristic).
    pub estimated_tokens: u64,
    /// Total token usage aggregated across executions.
    pub total_token_usage: u64,
    pub by_execution: BTreeMap<String, u64>,
    /// Distribution by content shape (`text` | `rich`).
    pub by_type: BTreeMap<String, u64>,
}

/// Sort order for paginated message queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageOrder {
    /// Oldest first (default).
    Asc,
    /// Newest first.
    Desc,
}

/// One page of a cursor query. Records are returned newest first; `has_more`
/// tells whether even older messages exist, so the next page continues with
/// the oldest record of this page as its `before_timestamp`.
#[derive(Debug, Clone, Default)]
pub struct MessagePage {
    pub records: Vec<MessageStorageMetadata>,
    pub has_more: bool,
}

/// Default number of recent messages returned when no explicit limit is given.
const DEFAULT_RECENT_LIMIT: usize = 20;
/// Default maximum search results when no explicit limit is given.
const DEFAULT_SEARCH_LIMIT: usize = 50;

/// Persist a message record (upsert by message id).
pub async fn save(ctx: &ApiContext, record: &MessageStorageMetadata) -> ApiResult<()> {
    ctx.storage.message.save(record).await?;
    Ok(())
}

/// Convenience: build a record from a message + execution scoping and
/// persist it.
pub async fn add_message(
    ctx: &ApiContext,
    execution_id: &str,
    agent_loop_id: Option<&str>,
    message: Message,
) -> ApiResult<()> {
    let record = MessageStorageMetadata {
        id: message.id.clone(),
        execution_id: execution_id.to_string(),
        agent_loop_id: agent_loop_id.map(ToOwned::to_owned),
        message,
    };
    save(ctx, &record).await
}

pub async fn get(ctx: &ApiContext, id: &str) -> ApiResult<MessageStorageMetadata> {
    ctx.storage
        .message
        .load(id)
        .await?
        .ok_or_else(|| not_found("message", id))
}

pub async fn delete(ctx: &ApiContext, id: &str) -> ApiResult<bool> {
    ctx.storage.message.delete(id).await.map_err(Into::into)
}

/// Paginated message list with optional execution / role filters.
pub async fn list(
    ctx: &ApiContext,
    options: &MessageListOptions,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    ctx.storage
        .message
        .list(Some(options.clone()))
        .await
        .map_err(Into::into)
}

/// Most recent messages, newest first.
pub async fn recent(
    ctx: &ApiContext,
    limit: Option<usize>,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    let limit = limit.unwrap_or(DEFAULT_RECENT_LIMIT);
    let options = MessageListOptions {
        offset: None,
        limit: Some(limit as u64),
        before_timestamp: None,
        execution_id_filter: None,
        agent_loop_id_filter: None,
        role_filter: None,
    };
    let mut messages = ctx.storage.message.list(Some(options)).await?;
    messages.sort_by_key(|r| std::cmp::Reverse(r.message.timestamp));
    messages.truncate(limit);
    Ok(messages)
}

/// Messages of one workflow execution, oldest first.
pub async fn by_execution(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    ctx.storage
        .message
        .list_by_execution(execution_id, None)
        .await
        .map_err(Into::into)
}

/// Paginated messages of one execution with an explicit sort order.
pub async fn by_execution_paginated(
    ctx: &ApiContext,
    execution_id: &str,
    offset: u64,
    limit: u64,
    order: MessageOrder,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    let options = MessageListOptions {
        offset: Some(offset),
        limit: Some(limit),
        before_timestamp: None,
        execution_id_filter: Some(execution_id.to_string()),
        agent_loop_id_filter: None,
        role_filter: None,
    };
    let mut messages = ctx.storage.message.list(Some(options)).await?;
    match order {
        MessageOrder::Asc => messages.sort_by_key(|r| r.message.timestamp),
        MessageOrder::Desc => {
            messages.sort_by_key(|r| std::cmp::Reverse(r.message.timestamp));
        }
    }
    Ok(messages)
}

/// Fetch up to `limit` messages of one execution, newest first. When
/// `before_timestamp` is `Some`, only messages strictly older than it are
/// returned; `None` starts at the newest message.
pub async fn page_by_execution(
    ctx: &ApiContext,
    execution_id: &str,
    before_timestamp: Option<i64>,
    limit: u64,
) -> ApiResult<MessagePage> {
    let options = MessageListOptions {
        offset: None,
        limit: Some(limit.saturating_add(1)),
        before_timestamp: Some(before_timestamp.unwrap_or(i64::MAX)),
        execution_id_filter: None,
        agent_loop_id_filter: None,
        role_filter: None,
    };
    let mut records = ctx
        .storage
        .message
        .list_by_execution(execution_id, Some(options))
        .await?;
    let has_more = records.len() as u64 > limit;
    records.truncate(limit as usize);
    Ok(MessagePage { records, has_more })
}

/// Cursor variant of [`by_agent_loop`]: fetch up to `limit` messages of one
/// agent loop, newest first, optionally strictly older than
/// `before_timestamp`.
pub async fn page_by_agent_loop(
    ctx: &ApiContext,
    agent_loop_id: &str,
    before_timestamp: Option<i64>,
    limit: u64,
) -> ApiResult<MessagePage> {
    let options = MessageListOptions {
        offset: None,
        limit: Some(limit.saturating_add(1)),
        before_timestamp: Some(before_timestamp.unwrap_or(i64::MAX)),
        execution_id_filter: None,
        agent_loop_id_filter: None,
        role_filter: None,
    };
    let mut records = ctx
        .storage
        .message
        .list_by_agent_loop(agent_loop_id, Some(options))
        .await?;
    let has_more = records.len() as u64 > limit;
    records.truncate(limit as usize);
    Ok(MessagePage { records, has_more })
}

/// Normalized conversation history of an execution: messages deduplicated by
/// id and sorted by timestamp.
pub async fn normalize_history(ctx: &ApiContext, execution_id: &str) -> ApiResult<Vec<Message>> {
    let mut records = by_execution(ctx, execution_id).await?;
    records.sort_by_key(|r| (r.message.timestamp, r.id.clone()));
    records.dedup_by_key(|r| r.id.clone());
    Ok(records.into_iter().map(|r| r.message).collect())
}

/// Total number of messages retained for one execution.
pub async fn get_message_count(ctx: &ApiContext, execution_id: &str) -> ApiResult<u64> {
    let records = by_execution(ctx, execution_id).await?;
    Ok(records.len() as u64)
}

/// Messages of one agent loop, oldest first.
pub async fn by_agent_loop(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    ctx.storage
        .message
        .list_by_agent_loop(agent_loop_id, None)
        .await
        .map_err(Into::into)
}

/// Keyword search over the message text content.
pub async fn search(
    ctx: &ApiContext,
    keyword: &str,
    limit: Option<usize>,
) -> ApiResult<Vec<MessageStorageMetadata>> {
    let keyword = keyword.trim().to_lowercase();
    if keyword.is_empty() {
        return Ok(Vec::new());
    }
    let all = ctx.storage.message.list(None).await?;
    let mut matches: Vec<MessageStorageMetadata> = all
        .into_iter()
        .filter(|record| {
            message_text(&record.message)
                .to_lowercase()
                .contains(&keyword)
        })
        .collect();
    matches.sort_by_key(|r| std::cmp::Reverse(r.message.timestamp));
    matches.truncate(limit.unwrap_or(DEFAULT_SEARCH_LIMIT));
    Ok(matches)
}

/// Message statistics (count by role, estimated tokens, count by
/// execution, content-type distribution).
pub async fn stats(ctx: &ApiContext) -> ApiResult<MessageStats> {
    let all = ctx.storage.message.list(None).await?;
    let mut stats = MessageStats {
        total: all.len() as u64,
        ..MessageStats::default()
    };
    for record in &all {
        let role = role_name(&record.message.role).to_string();
        *stats.by_role.entry(role).or_insert(0) += 1;
        let tokens = estimate_tokens(&record.message) as u64;
        stats.estimated_tokens += tokens;
        stats.total_token_usage += tokens;
        *stats
            .by_execution
            .entry(record.execution_id.clone())
            .or_insert(0) += 1;
        let content_type = match &record.message.content {
            MessageContentValue::Text(_) => "text",
            MessageContentValue::Rich(_) => "rich",
        };
        *stats.by_type.entry(content_type.to_string()).or_insert(0) += 1;
    }
    Ok(stats)
}

/// Normalized session history of an execution: messages sorted by
/// timestamp as a plain `Vec<Message>`.
pub async fn conversation_history(ctx: &ApiContext, execution_id: &str) -> ApiResult<Vec<Message>> {
    let mut records = by_execution(ctx, execution_id).await?;
    records.sort_by_key(|r| r.message.timestamp);
    Ok(records.into_iter().map(|r| r.message).collect())
}

pub(crate) fn role_name(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

/// All text pieces of a message content value.
pub(crate) fn message_text(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(parts) => parts
            .iter()
            .filter_map(|part| match part {
                MessageContent::Text { text } => Some(text.clone()),
                MessageContent::Thinking { thinking, .. } => Some(thinking.clone()),
                MessageContent::ToolResult { tool_result } => Some(tool_result.content.clone()),
                MessageContent::ToolUse { .. } | MessageContent::ImageUrl { .. } => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Heuristic token estimate: whitespace words plus one token per 4 remaining
/// characters (approximates both latin and CJK text).
pub(crate) fn estimate_tokens(message: &Message) -> usize {
    let text = message_text(message);
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    words + chars.saturating_sub(words * 5) / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::error::ApiError;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
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
    async fn message_crud_and_paging() {
        let ctx = make_ctx();
        add_message(
            &ctx,
            "exec-1",
            None,
            make_message("m1", MessageRole::User, "hello world", 100),
        )
        .await
        .unwrap();
        add_message(
            &ctx,
            "exec-1",
            None,
            make_message("m2", MessageRole::Assistant, "hi there", 200),
        )
        .await
        .unwrap();

        let all = list(
            &ctx,
            &MessageListOptions {
                limit: Some(10),
                ..MessageListOptions::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(all.len(), 2);

        let exec_messages = by_execution(&ctx, "exec-1").await.unwrap();
        assert_eq!(exec_messages.len(), 2);

        let history = conversation_history(&ctx, "exec-1").await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, "m1");

        let loaded = get(&ctx, "m1").await.unwrap();
        assert_eq!(loaded.execution_id, "exec-1");
        assert!(delete(&ctx, "m1").await.unwrap());
    }

    #[tokio::test]
    async fn search_and_stats() {
        let ctx = make_ctx();
        add_message(
            &ctx,
            "exec-1",
            None,
            make_message("s1", MessageRole::User, "deploy the service", 100),
        )
        .await
        .unwrap();
        add_message(
            &ctx,
            "exec-1",
            None,
            make_message("s2", MessageRole::Assistant, "deploying now", 200),
        )
        .await
        .unwrap();
        add_message(
            &ctx,
            "exec-2",
            Some("agent-9"),
            make_message("s3", MessageRole::Tool, "log output", 300),
        )
        .await
        .unwrap();

        let matches = search(&ctx, "deploy", Some(10)).await.unwrap();
        assert_eq!(matches.len(), 2);

        let agent = by_agent_loop(&ctx, "agent-9").await.unwrap();
        assert_eq!(agent.len(), 1);

        let stats = stats(&ctx).await.unwrap();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.by_role.get("user"), Some(&1));
        assert_eq!(stats.by_role.get("tool"), Some(&1));
        assert!(stats.estimated_tokens > 0);
    }

    #[tokio::test]
    async fn unknown_message_is_not_found() {
        let ctx = make_ctx();
        let err = get(&ctx, "missing").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn pagination_normalize_count_and_extended_stats() {
        let ctx = make_ctx();
        for i in 0..5 {
            add_message(
                &ctx,
                "exec-6",
                None,
                make_message(&format!("p{i}"), MessageRole::User, &format!("msg {i}"), i),
            )
            .await
            .unwrap();
        }
        // Re-saving the same id upserts (deduplicated storage), so normalize
        // keeps the unique ids.
        add_message(
            &ctx,
            "exec-6",
            None,
            make_message("p0", MessageRole::User, "duplicate", 0),
        )
        .await
        .unwrap();

        let desc = by_execution_paginated(&ctx, "exec-6", 0, 10, MessageOrder::Desc)
            .await
            .unwrap();
        assert_eq!(desc[0].id, "p4");
        assert_eq!(desc[desc.len() - 1].id, "p0");

        let asc = by_execution_paginated(&ctx, "exec-6", 0, 10, MessageOrder::Asc)
            .await
            .unwrap();
        assert_eq!(asc[0].id, "p0");
        assert_eq!(asc[asc.len() - 1].id, "p4");

        let normalized = normalize_history(&ctx, "exec-6").await.unwrap();
        assert_eq!(normalized.len(), 5);

        assert_eq!(get_message_count(&ctx, "exec-6").await.unwrap(), 5);

        let stats = stats(&ctx).await.unwrap();
        assert_eq!(stats.total, 5);
        assert_eq!(stats.by_type.get("text"), Some(&5));
        assert!(stats.total_token_usage > 0);
    }

    #[tokio::test]
    async fn cursor_page_newest_first_with_more() {
        let ctx = make_ctx();
        for i in 0..5 {
            add_message(
                &ctx,
                "exec-page",
                None,
                make_message(&format!("q{i}"), MessageRole::User, &format!("msg {i}"), 100 + i),
            )
            .await
            .unwrap();
        }
        // First page starts at the newest message.
        let page = page_by_execution(&ctx, "exec-page", None, 2).await.unwrap();
        assert!(page.has_more);
        let ids: Vec<String> = page.records.iter().map(|r| r.id.to_string()).collect();
        assert_eq!(ids, vec!["q4", "q3"]);
        // Cursor continues strictly before the oldest record of the page.
        let cursor = page.records.last().unwrap().message.timestamp;
        let next = page_by_execution(&ctx, "exec-page", Some(cursor), 2)
            .await
            .unwrap();
        assert!(next.has_more);
        let ids: Vec<String> = next.records.iter().map(|r| r.id.to_string()).collect();
        assert_eq!(ids, vec!["q2", "q1"]);
        // Last page: has_more turns false when no older message remains.
        let last = page_by_execution(&ctx, "exec-page", Some(next.records.last().unwrap().message.timestamp), 2)
            .await
            .unwrap();
        assert!(!last.has_more);
        let ids: Vec<String> = last.records.iter().map(|r| r.id.to_string()).collect();
        assert_eq!(ids, vec!["q0"]);
    }

    #[tokio::test]
    async fn cursor_page_boundaries() {
        let ctx = make_ctx();
        // No records at all: an empty, complete page.
        let empty = page_by_execution(&ctx, "exec-void", None, 5).await.unwrap();
        assert!(!empty.has_more);
        assert!(empty.records.is_empty());

        // Exactly `limit` records: the page is complete.
        for i in 0..3 {
            add_message(
                &ctx,
                "exec-exact",
                None,
                make_message(&format!("e{i}"), MessageRole::User, "x", i),
            )
            .await
            .unwrap();
        }
        let exact = page_by_execution(&ctx, "exec-exact", None, 3).await.unwrap();
        assert!(!exact.has_more);
        assert_eq!(exact.records.len(), 3);

        // A cursor older than every record yields an empty tail page.
        let tail = page_by_execution(&ctx, "exec-exact", Some(-1), 3)
            .await
            .unwrap();
        assert!(!tail.has_more);
        assert!(tail.records.is_empty());

        // Agent-loop scoped page returns only its own records.
        add_message(
            &ctx,
            "exec-loop",
            Some("loop-9"),
            make_message("a0", MessageRole::User, "agent", 100),
        )
        .await
        .unwrap();
        let loop_page = page_by_agent_loop(&ctx, "loop-9", None, 1)
            .await
            .unwrap();
        assert!(!loop_page.has_more);
        assert_eq!(loop_page.records.len(), 1);
        let missing = page_by_agent_loop(&ctx, "loop-nope", None, 1)
            .await
            .unwrap();
        assert!(!missing.has_more);
        assert!(missing.records.is_empty());
    }
}
