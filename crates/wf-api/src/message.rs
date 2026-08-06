use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::message::{MessageListOptions, MessageStorageAdapter};
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};
use wf_types::MessageStorageMetadata;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// Aggregated message statistics (TS `MessageResourceAPI` counterpart).
#[derive(Debug, Clone, Default, Serialize)]
pub struct MessageStats {
    pub total: u64,
    pub by_role: BTreeMap<String, u64>,
    /// Estimated total token count across retained messages (heuristic).
    pub estimated_tokens: u64,
    pub by_execution: BTreeMap<String, u64>,
}

/// Message resource queries and writes over the message adapter.
pub struct MessageApi {
    ctx: Arc<ApiContext>,
}

impl MessageApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Persist a message record (upsert by message id).
    pub async fn save(&self, record: &MessageStorageMetadata) -> ApiResult<()> {
        self.ctx.storage.message.save(record).await?;
        Ok(())
    }

    /// Convenience: build a record from a message + execution scoping and
    /// persist it.
    pub async fn add_message(
        &self,
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
        self.save(&record).await
    }

    pub async fn get(&self, id: &str) -> ApiResult<MessageStorageMetadata> {
        self.ctx
            .storage
            .message
            .load(id)
            .await?
            .ok_or_else(|| ApiError::not_found("message", id))
    }

    pub async fn delete(&self, id: &str) -> ApiResult<bool> {
        self.ctx
            .storage
            .message
            .delete(id)
            .await
            .map_err(Into::into)
    }

    /// Paginated message list with optional execution / role filters.
    pub async fn list(
        &self,
        options: &MessageListOptions,
    ) -> ApiResult<Vec<MessageStorageMetadata>> {
        self.ctx
            .storage
            .message
            .list(Some(options.clone()))
            .await
            .map_err(Into::into)
    }

    /// Most recent messages, newest first.
    pub async fn recent(&self, limit: usize) -> ApiResult<Vec<MessageStorageMetadata>> {
        let options = MessageListOptions {
            offset: None,
            limit: Some(if limit == 0 { 20 } else { limit as u64 }),
            execution_id_filter: None,
            agent_loop_id_filter: None,
            role_filter: None,
        };
        let mut messages = self.ctx.storage.message.list(Some(options)).await?;
        messages.sort_by_key(|r| std::cmp::Reverse(r.message.timestamp));
        messages.truncate(if limit == 0 { 20 } else { limit });
        Ok(messages)
    }

    /// Messages of one workflow execution, oldest first.
    pub async fn by_execution(&self, execution_id: &str) -> ApiResult<Vec<MessageStorageMetadata>> {
        self.ctx
            .storage
            .message
            .list_by_execution(execution_id, None)
            .await
            .map_err(Into::into)
    }

    /// Messages of one agent loop, oldest first.
    pub async fn by_agent_loop(
        &self,
        agent_loop_id: &str,
    ) -> ApiResult<Vec<MessageStorageMetadata>> {
        self.ctx
            .storage
            .message
            .list_by_agent_loop(agent_loop_id, None)
            .await
            .map_err(Into::into)
    }

    /// Keyword search over the message text content.
    pub async fn search(
        &self,
        keyword: &str,
        limit: usize,
    ) -> ApiResult<Vec<MessageStorageMetadata>> {
        let keyword = keyword.trim().to_lowercase();
        if keyword.is_empty() {
            return Ok(Vec::new());
        }
        let all = self.ctx.storage.message.list(None).await?;
        let mut matches: Vec<MessageStorageMetadata> = all
            .into_iter()
            .filter(|record| {
                message_text(&record.message)
                    .to_lowercase()
                    .contains(&keyword)
            })
            .collect();
        matches.sort_by_key(|r| std::cmp::Reverse(r.message.timestamp));
        matches.truncate(if limit == 0 { 50 } else { limit });
        Ok(matches)
    }

    /// Message statistics (count by role, estimated tokens, count by
    /// execution).
    pub async fn stats(&self) -> ApiResult<MessageStats> {
        let all = self.ctx.storage.message.list(None).await?;
        let mut stats = MessageStats {
            total: all.len() as u64,
            ..MessageStats::default()
        };
        for record in &all {
            let role = role_name(&record.message.role).to_string();
            *stats.by_role.entry(role).or_insert(0) += 1;
            stats.estimated_tokens += estimate_tokens(&record.message) as u64;
            *stats
                .by_execution
                .entry(record.execution_id.clone())
                .or_insert(0) += 1;
        }
        Ok(stats)
    }

    /// Normalized session history of an execution: messages sorted by
    /// timestamp as a plain `Vec<Message>`.
    pub async fn conversation_history(&self, execution_id: &str) -> ApiResult<Vec<Message>> {
        let mut records = self.by_execution(execution_id).await?;
        records.sort_by_key(|r| r.message.timestamp);
        Ok(records.into_iter().map(|r| r.message).collect())
    }
}

fn role_name(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

/// All text pieces of a message content value.
fn message_text(message: &Message) -> String {
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
fn estimate_tokens(message: &Message) -> usize {
    let text = message_text(message);
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    words + chars.saturating_sub(words * 5) / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

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
    async fn message_crud_and_paging() {
        let ctx = make_ctx();
        let api = MessageApi::new(ctx.clone());
        api.add_message(
            "exec-1",
            None,
            make_message("m1", MessageRole::User, "hello world", 100),
        )
        .await
        .unwrap();
        api.add_message(
            "exec-1",
            None,
            make_message("m2", MessageRole::Assistant, "hi there", 200),
        )
        .await
        .unwrap();

        let all = api
            .list(&MessageListOptions {
                limit: Some(10),
                ..MessageListOptions::default()
            })
            .await
            .unwrap();
        assert_eq!(all.len(), 2);

        let exec_messages = api.by_execution("exec-1").await.unwrap();
        assert_eq!(exec_messages.len(), 2);

        let history = api.conversation_history("exec-1").await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, "m1");

        let loaded = api.get("m1").await.unwrap();
        assert_eq!(loaded.execution_id, "exec-1");
        assert!(api.delete("m1").await.unwrap());
    }

    #[tokio::test]
    async fn search_and_stats() {
        let ctx = make_ctx();
        let api = MessageApi::new(ctx);
        api.add_message(
            "exec-1",
            None,
            make_message("s1", MessageRole::User, "deploy the service", 100),
        )
        .await
        .unwrap();
        api.add_message(
            "exec-1",
            None,
            make_message("s2", MessageRole::Assistant, "deploying now", 200),
        )
        .await
        .unwrap();
        api.add_message(
            "exec-2",
            Some("agent-9"),
            make_message("s3", MessageRole::Tool, "log output", 300),
        )
        .await
        .unwrap();

        let matches = api.search("deploy", 10).await.unwrap();
        assert_eq!(matches.len(), 2);

        let agent = api.by_agent_loop("agent-9").await.unwrap();
        assert_eq!(agent.len(), 1);

        let stats = api.stats().await.unwrap();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.by_role.get("user"), Some(&1));
        assert_eq!(stats.by_role.get("tool"), Some(&1));
        assert!(stats.estimated_tokens > 0);
    }

    #[tokio::test]
    async fn unknown_message_is_not_found() {
        let ctx = make_ctx();
        let api = MessageApi::new(ctx);
        let err = api.get("missing").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }
}
