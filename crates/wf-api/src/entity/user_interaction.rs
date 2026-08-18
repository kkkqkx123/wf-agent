//! Shared user interaction management: handler registration and typed
//! notification hooks.
//!
//! The handler slot is process-scoped through the `ApiContext`, so the same
//! handler serves workflow `USER_INTERACTION` nodes, agent loops and the
//! `ApprovalCoordinator` approval requests.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::user_interaction::{
    UserInteractionListOptions, UserInteractionStorageAdapter,
};
use wf_storage::context::StorageContext;
use wf_types::UserInteractionStorageMetadata;

use crate::infra::context::ApiContext;
use crate::not_found;

/// Event record of a user interaction.
#[derive(Debug, Clone, Serialize)]
pub struct AgentUserInteractionEventRecord {
    pub id: String,
    pub execution_id: String,
    pub interaction_type: String,
    pub status: String,
    pub request_data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_data: Option<serde_json::Value>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responded_at: Option<i64>,
}

/// Handler invoked when a user interaction of a config is created. Defined
/// here so the shared interaction plumbing never depends on the agent module.
pub trait UserInteractionHandler: Send + Sync {
    /// Generic interaction hook; fired for every created interaction.
    fn on_interaction(&self, record: &AgentUserInteractionEventRecord);

    /// Fired when a tool approval request is opened. Defaults to no-op so
    /// existing implementers keep compiling; `execution_id` and `request`
    /// carry the serialized `ToolApprovalRequestData`.
    fn on_tool_approval_requested(&self, _execution_id: &str, _request: &serde_json::Value) {}

    /// Fired when a follow-up question request is opened. Defaults to no-op.
    fn on_followup_question_requested(&self, _execution_id: &str, _request: &serde_json::Value) {}
}

/// Register the shared interaction handler; a previous handler is
/// replaced.
pub async fn register_handler(ctx: &ApiContext, handler: Arc<dyn UserInteractionHandler>) {
    *ctx.user_interaction_handler.write().await = Some(handler);
}

/// Clear the registered handler.
pub async fn clear_handler(ctx: &ApiContext) {
    *ctx.user_interaction_handler.write().await = None;
}

/// Whether a handler is registered.
pub async fn has_handler(ctx: &ApiContext) -> bool {
    ctx.user_interaction_handler.read().await.is_some()
}

/// Notify the registered handler of a newly created interaction.
pub async fn on_interaction_created(ctx: &ApiContext, record: &AgentUserInteractionEventRecord) {
    if let Some(handler) = ctx.user_interaction_handler.read().await.as_ref() {
        handler.on_interaction(record);
    }
}

/// Notify the registered handler of a tool approval request.
pub async fn on_tool_approval_requested(
    ctx: &ApiContext,
    execution_id: &str,
    request: &serde_json::Value,
) {
    if let Some(handler) = ctx.user_interaction_handler.read().await.as_ref() {
        handler.on_tool_approval_requested(execution_id, request);
    }
}

/// Notify the registered handler of a follow-up question request.
pub async fn on_followup_question_requested(
    ctx: &ApiContext,
    execution_id: &str,
    request: &serde_json::Value,
) {
    if let Some(handler) = ctx.user_interaction_handler.read().await.as_ref() {
        handler.on_followup_question_requested(execution_id, request);
    }
}

pub async fn save_interaction(
    ctx: &StorageContext,
    interaction: &UserInteractionStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.user_interaction.save(interaction).await?;
    Ok(())
}

pub async fn get_interaction(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<UserInteractionStorageMetadata> {
    ctx.user_interaction
        .load(id)
        .await?
        .ok_or_else(|| not_found("user_interaction", id))
}

pub async fn list_interactions(
    ctx: &StorageContext,
    options: Option<UserInteractionListOptions>,
) -> crate::ApiResult<Vec<UserInteractionStorageMetadata>> {
    ctx.user_interaction.list(options).await.map_err(Into::into)
}

pub async fn list_interactions_by_execution(
    ctx: &StorageContext,
    execution_id: &str,
) -> crate::ApiResult<Vec<UserInteractionStorageMetadata>> {
    ctx.user_interaction
        .list_by_execution(execution_id)
        .await
        .map_err(Into::into)
}

pub async fn list_interactions_by_status(
    ctx: &StorageContext,
    status: &str,
) -> crate::ApiResult<Vec<UserInteractionStorageMetadata>> {
    ctx.user_interaction
        .list_by_status(status)
        .await
        .map_err(Into::into)
}

pub async fn get_interaction_stats(ctx: &StorageContext) -> crate::ApiResult<HashMap<String, u64>> {
    ctx.user_interaction.get_stats().await.map_err(Into::into)
}

pub async fn delete_interaction(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.user_interaction.delete(id).await.map_err(Into::into)
}

/// Submit a response to a pending user interaction.
///
/// Persists the response/result payloads, flips the interaction status to
/// `responded` (recording `responded_at`) and resolves the in-process wait of
/// a live `USER_INTERACTION` node through `wf-workflow`'s interaction
/// registry, closing the create -> respond -> status-flip loop. Responding an
/// already-responded interaction is a no-op conflict (`ApiError::Conflict`).
pub async fn respond_interaction(
    ctx: &StorageContext,
    id: &str,
    response_data: Option<serde_json::Value>,
    result_data: Option<serde_json::Value>,
) -> crate::ApiResult<()> {
    let mut interaction = get_interaction(ctx, id).await?;
    if interaction.status == "responded" {
        return Err(crate::ApiError::Conflict(format!(
            "user interaction {} already responded",
            id
        )));
    }
    interaction.response_data = response_data;
    interaction.result_data = result_data;
    interaction.status = "responded".to_string();
    interaction.responded_at = Some(wf_common::now());

    // Persist first so the response is durable even if the live wait has
    // already been resolved (or the execution is gone).
    ctx.user_interaction.save(&interaction).await?;

    // Resolve the in-process wait of a live USER_INTERACTION node, if any.
    // Returns false when no node is currently waiting on this interaction.
    let _ = wf_workflow::complete_interaction(
        id,
        interaction.response_data.clone().unwrap_or(
            interaction
                .result_data
                .clone()
                .unwrap_or(serde_json::Value::Null),
        ),
    );

    Ok(())
}

/// Whether a user interaction is still pending a response.
pub async fn is_interaction_pending(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    let interaction = get_interaction(ctx, id).await?;
    Ok(interaction.status == "pending")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_interaction(
        id: &str,
        execution_id: &str,
        interaction_type: &str,
        status: &str,
    ) -> UserInteractionStorageMetadata {
        UserInteractionStorageMetadata {
            id: id.into(),
            execution_id: execution_id.into(),
            interaction_type: interaction_type.into(),
            status: status.into(),
            request_data: json!({ "prompt": "confirm" }),
            response_data: None,
            result_data: None,
            error: None,
            created_at: 1000,
            responded_at: None,
        }
    }

    #[tokio::test]
    async fn interaction_crud() {
        let ctx = StorageContext::new_memory();
        save_interaction(
            &ctx,
            &make_interaction("ui-1", "ex-1", "confirm", "pending"),
        )
        .await
        .unwrap();

        let loaded = get_interaction(&ctx, "ui-1").await.unwrap();
        assert_eq!(loaded.interaction_type, "confirm");

        let err = get_interaction(&ctx, "ui-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_interaction(&ctx, "ui-1").await.unwrap());
        assert!(!delete_interaction(&ctx, "ui-1").await.unwrap());
    }

    #[tokio::test]
    async fn interaction_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_interaction(
            &ctx,
            &make_interaction("ui-1", "ex-1", "confirm", "pending"),
        )
        .await
        .unwrap();
        save_interaction(
            &ctx,
            &make_interaction("ui-2", "ex-1", "input", "responded"),
        )
        .await
        .unwrap();
        save_interaction(
            &ctx,
            &make_interaction("ui-3", "ex-2", "confirm", "pending"),
        )
        .await
        .unwrap();

        let by_execution = list_interactions_by_execution(&ctx, "ex-1").await.unwrap();
        assert_eq!(by_execution.len(), 2);

        let by_status = list_interactions_by_status(&ctx, "pending").await.unwrap();
        assert_eq!(by_status.len(), 2);

        let listed = list_interactions(&ctx, None).await.unwrap();
        assert_eq!(listed.len(), 3);

        let stats = get_interaction_stats(&ctx).await.unwrap();
        assert_eq!(stats.get("pending"), Some(&2));
        assert_eq!(stats.get("responded"), Some(&1));
    }

    #[tokio::test]
    async fn respond_interaction_flips_status_and_payloads() {
        let ctx = StorageContext::new_memory();
        save_interaction(
            &ctx,
            &make_interaction("ui-r1", "ex-r1", "input", "pending"),
        )
        .await
        .unwrap();

        respond_interaction(
            &ctx,
            "ui-r1",
            Some(json!({"value": "approved"})),
            Some(json!({"ok": true})),
        )
        .await
        .expect("respond should succeed");

        let stored = get_interaction(&ctx, "ui-r1").await.unwrap();
        assert_eq!(stored.status, "responded");
        assert!(stored.responded_at.is_some());
        assert_eq!(stored.response_data, Some(json!({"value": "approved"})));
        assert_eq!(stored.result_data, Some(json!({"ok": true})));

        // A second response is rejected as a conflict.
        let err = respond_interaction(&ctx, "ui-r1", None, None)
            .await
            .expect_err("double respond must fail");
        assert!(matches!(err, crate::ApiError::Conflict(_)));
    }

    #[tokio::test]
    async fn respond_unknown_interaction_is_not_found() {
        let ctx = StorageContext::new_memory();
        let err = respond_interaction(&ctx, "ui-missing", None, None)
            .await
            .expect_err("unknown interaction");
        assert!(matches!(err, crate::ApiError::NotFound { .. }));
    }

    // ── UserInteractionApi handler wiring ───────────────────────────

    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingHandler {
        interactions: Arc<AtomicUsize>,
        approvals: Arc<AtomicUsize>,
        followups: Arc<AtomicUsize>,
    }

    impl crate::agent::agent_user_interaction::UserInteractionHandler for CountingHandler {
        fn on_interaction(
            &self,
            _record: &crate::agent::agent_user_interaction::AgentUserInteractionEventRecord,
        ) {
            self.interactions.fetch_add(1, Ordering::SeqCst);
        }
        fn on_tool_approval_requested(&self, _execution_id: &str, _request: &serde_json::Value) {
            self.approvals.fetch_add(1, Ordering::SeqCst);
        }
        fn on_followup_question_requested(
            &self,
            _execution_id: &str,
            _request: &serde_json::Value,
        ) {
            self.followups.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn user_interaction_api_handler_roundtrip_and_notify() {
        let ctx = Arc::new(crate::ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
            Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        ));
        assert!(!has_handler(&ctx).await);

        let interactions = Arc::new(AtomicUsize::new(0));
        let approvals = Arc::new(AtomicUsize::new(0));
        let followups = Arc::new(AtomicUsize::new(0));
        register_handler(
            &ctx,
            Arc::new(CountingHandler {
                interactions: interactions.clone(),
                approvals: approvals.clone(),
                followups: followups.clone(),
            }),
        )
        .await;
        assert!(has_handler(&ctx).await);

        on_tool_approval_requested(&ctx, "exec-1", &serde_json::json!({"tool": "read_file"})).await;
        on_followup_question_requested(&ctx, "exec-1", &serde_json::json!({"prompt": "?"})).await;
        assert_eq!(approvals.load(Ordering::SeqCst), 1);
        assert_eq!(followups.load(Ordering::SeqCst), 1);

        clear_handler(&ctx).await;
        assert!(!has_handler(&ctx).await);
        on_tool_approval_requested(&ctx, "exec-2", &serde_json::json!({})).await;
        assert_eq!(approvals.load(Ordering::SeqCst), 1);
    }
}
