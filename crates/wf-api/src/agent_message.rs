use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::message::MessageStorageAdapter;
use wf_types::message::{Message, MessageRole};
use wf_types::MessageStorageMetadata;

use crate::context::ApiContext;
use crate::error::ApiResult;
use crate::message::{MessageApi, MessageStats};

/// Per-agent-loop message statistics (TS `AgentLoopMessageStats`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentLoopMessageStats {
    pub total: u64,
    pub by_role: BTreeMap<String, u64>,
    pub estimated_tokens: u64,
}

/// Message queries scoped to one agent loop (TS `AgentLoopMessageResourceAPI`
/// counterpart).
pub struct AgentLoopMessageApi {
    ctx: Arc<ApiContext>,
}

impl AgentLoopMessageApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Most recent messages of an agent loop, newest first.
    pub async fn recent(&self, agent_loop_id: &str, count: usize) -> ApiResult<Vec<MessageStorageMetadata>> {
        let count = if count == 0 { 20 } else { count };
        let mut messages = self
            .ctx
            .storage
            .message
            .list_by_agent_loop(agent_loop_id, None)
            .await?;
        messages.sort_by_key(|r| std::cmp::Reverse(r.message.timestamp));
        messages.truncate(count);
        Ok(messages)
    }

    /// Keyword search over the messages of one agent loop.
    pub async fn search(&self, agent_loop_id: &str, query: &str) -> ApiResult<Vec<MessageStorageMetadata>> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let all = self
            .ctx
            .storage
            .message
            .list_by_agent_loop(agent_loop_id, None)
            .await?;
        let mut matches: Vec<MessageStorageMetadata> = all
            .into_iter()
            .filter(|record| {
                message_text(&record.message)
                    .to_lowercase()
                    .contains(&query)
            })
            .collect();
        matches.sort_by_key(|r| std::cmp::Reverse(r.message.timestamp));
        Ok(matches)
    }

    /// Statistics of one agent loop's messages.
    pub async fn stats(&self, agent_loop_id: &str) -> ApiResult<AgentLoopMessageStats> {
        let messages = self
            .ctx
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
        &self,
        agent_loop_id: &str,
        max_messages: Option<usize>,
    ) -> ApiResult<Vec<Message>> {
        let mut messages = self
            .ctx
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
    pub async fn count(&self, agent_loop_id: &str) -> ApiResult<u64> {
        Ok(self
            .ctx
            .storage
            .message
            .list_by_agent_loop(agent_loop_id, None)
            .await?
            .len() as u64)
    }

    /// Global message statistics (shared with `MessageApi`).
    pub async fn global_stats(&self) -> ApiResult<MessageStats> {
        MessageApi::new(self.ctx.clone()).stats().await
    }

    /// Message count of one agent loop (alias for `count`).
    pub async fn get_message_count(&self, agent_loop_id: &str) -> ApiResult<u64> {
        self.count(agent_loop_id).await
    }

    /// Remove stale duplicate messages of an agent loop, keeping the latest
    /// copy per message id; returns the number of records removed.
    pub async fn normalize_history(&self, agent_loop_id: &str) -> ApiResult<usize> {
        let messages = self
            .ctx
            .storage
            .message
            .list_by_agent_loop(agent_loop_id, None)
            .await?;
        let mut seen: BTreeMap<String, MessageStorageMetadata> = BTreeMap::new();
        let mut removed = 0usize;
        for record in messages {
            match seen.get(&record.message.id) {
                Some(existing) if existing.message.timestamp <= record.message.timestamp => {
                    if self.ctx.storage.message.delete(&existing.id).await? {
                        removed += 1;
                    }
                    seen.insert(record.message.id.clone(), record);
                }
                Some(_) => {
                    if self.ctx.storage.message.delete(&record.id).await? {
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
    pub async fn get_recent_messages(&self, agent_loop_id: &str, count: usize) -> ApiResult<Vec<MessageStorageMetadata>> {
        self.recent(agent_loop_id, count).await
    }

    pub async fn search_messages(&self, agent_loop_id: &str, query: &str) -> ApiResult<Vec<MessageStorageMetadata>> {
        self.search(agent_loop_id, query).await
    }

    pub async fn get_message_stats(&self, agent_loop_id: &str) -> ApiResult<AgentLoopMessageStats> {
        self.stats(agent_loop_id).await
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

fn message_text(message: &Message) -> String {
    use wf_types::message::{MessageContent, MessageContentValue};
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
        let api = AgentLoopMessageApi::new(ctx.clone());
        api.ctx
            .storage
            .message
            .save(&MessageStorageMetadata {
                id: "m1".into(),
                execution_id: "loop-m".into(),
                agent_loop_id: Some("loop-m".into()),
                message: make_message("m1", MessageRole::User, "deploy the service", 100),
            })
            .await
            .unwrap();
        api.ctx
            .storage
            .message
            .save(&MessageStorageMetadata {
                id: "m2".into(),
                execution_id: "loop-m".into(),
                agent_loop_id: Some("loop-m".into()),
                message: make_message("m2", MessageRole::Assistant, "deploying now", 200),
            })
            .await
            .unwrap();
        api.ctx
            .storage
            .message
            .save(&MessageStorageMetadata {
                id: "m3".into(),
                execution_id: "loop-other".into(),
                agent_loop_id: Some("loop-other".into()),
                message: make_message("m3", MessageRole::User, "unrelated", 300),
            })
            .await
            .unwrap();

        let recent = api.recent("loop-m", 1).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].message.id, "m2");

        let matches = api.search("loop-m", "deploy").await.unwrap();
        assert_eq!(matches.len(), 2);

        let stats = api.stats("loop-m").await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_role.get("user"), Some(&1));
        assert!(stats.estimated_tokens > 0);

        assert_eq!(api.count("loop-m").await.unwrap(), 2);

        let history = api.conversation_history("loop-m", Some(10)).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, "m1");

        let global = api.global_stats().await.unwrap();
        assert_eq!(global.total, 3);
    }

    #[tokio::test]
    async fn normalize_history_keeps_latest() {
        let ctx = make_ctx();
        let api = AgentLoopMessageApi::new(ctx.clone());
        // Two records with the same message id (replayed loop), different ts.
        api.ctx
            .storage
            .message
            .save(&MessageStorageMetadata {
                id: "rec-1".into(),
                execution_id: "loop-n".into(),
                agent_loop_id: Some("loop-n".into()),
                message: make_message("msg-1", MessageRole::User, "old", 100),
            })
            .await
            .unwrap();
        api.ctx
            .storage
            .message
            .save(&MessageStorageMetadata {
                id: "rec-2".into(),
                execution_id: "loop-n".into(),
                agent_loop_id: Some("loop-n".into()),
                message: make_message("msg-1", MessageRole::User, "new", 200),
            })
            .await
            .unwrap();

        let removed = api.normalize_history("loop-n").await.unwrap();
        assert!(removed >= 1);
        let remaining = api
            .ctx
            .storage
            .message
            .list_by_agent_loop("loop-n", None)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].message.timestamp, 200);
    }
}
