//! User interaction queries and handler wiring scoped to agent loops (TS
//! `AgentUserInteractionResourceAPI` counterpart).

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use wf_types::UserInteractionStorageMetadata;

use crate::entity::user_interaction::{
    get_interaction, list_interactions_by_execution, respond_interaction,
};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Event record of a user interaction (TS `AgentUserInteractionEventRecord`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentUserInteractionEventRecord {
    pub id: String,
    pub execution_id: String,
    pub interaction_type: String,
    pub status: String,
    pub request_data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_data: Option<Value>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responded_at: Option<i64>,
}

/// Handler invoked when a user interaction of a config is created (TS
/// `UserInteractionHandler` counterpart).
pub trait UserInteractionHandler: Send + Sync {
    /// Generic interaction hook; fired for every created interaction.
    fn on_interaction(&self, record: &AgentUserInteractionEventRecord);

    /// Fired when a tool approval request is opened (TS
    /// `onToolApprovalRequested` counterpart). Defaults to no-op so existing
    /// implementers keep compiling; `execution_id` and `request` carry the
    /// serialized `ToolApprovalRequestData`.
    fn on_tool_approval_requested(&self, _execution_id: &str, _request: &Value) {}

    /// Fired when a follow-up question request is opened (TS
    /// `onFollowupQuestionRequested` counterpart). Defaults to no-op.
    fn on_followup_question_requested(&self, _execution_id: &str, _request: &Value) {}
}

/// Interactions of an agent loop, newest first.
pub async fn list(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentUserInteractionEventRecord>> {
    let mut records = list_interactions_by_execution(&ctx.storage, agent_loop_id)
        .await?
        .into_iter()
        .map(record_from_storage)
        .collect::<Vec<_>>();
    records.sort_by_key(|r| std::cmp::Reverse(r.created_at));
    Ok(records)
}

/// Interactions of an agent loop with an optional status filter.
pub async fn list_filtered(
    ctx: &ApiContext,
    agent_loop_id: &str,
    status: Option<&str>,
    limit: Option<usize>,
) -> ApiResult<Vec<AgentUserInteractionEventRecord>> {
    let mut records = list(ctx, agent_loop_id).await?;
    if let Some(status) = status {
        records.retain(|r| r.status == status);
    }
    if let Some(limit) = limit {
        records.truncate(limit);
    }
    Ok(records)
}

/// One interaction by id.
pub async fn get(ctx: &ApiContext, id: &str) -> ApiResult<AgentUserInteractionEventRecord> {
    Ok(record_from_storage(
        get_interaction(&ctx.storage, id).await?,
    ))
}

/// Respond to a pending interaction of an agent loop (TS
/// `handleUserInteraction` counterpart).
pub async fn respond(
    ctx: &ApiContext,
    agent_loop_id: &str,
    interaction_id: &str,
    response_data: Option<Value>,
    result_data: Option<Value>,
) -> ApiResult<()> {
    let interaction = get_interaction(&ctx.storage, interaction_id).await?;
    if interaction.execution_id != agent_loop_id {
        return Err(ApiError::Validation(format!(
            "interaction {interaction_id} does not belong to agent loop {agent_loop_id}"
        )));
    }
    respond_interaction(&ctx.storage, interaction_id, response_data, result_data).await
}

/// Register the shared handler for an agent config; a previous handler is
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

/// Notify the registered handler (if any) of a new interaction.
pub async fn on_interaction_created(ctx: &ApiContext, record: &AgentUserInteractionEventRecord) {
    if let Some(handler) = ctx.user_interaction_handler.read().await.as_ref() {
        handler.on_interaction(record);
    }
}

/// Notify the registered handler (if any) of a tool approval request.
pub async fn notify_tool_approval_requested(ctx: &ApiContext, execution_id: &str, request: &Value) {
    if let Some(handler) = ctx.user_interaction_handler.read().await.as_ref() {
        handler.on_tool_approval_requested(execution_id, request);
    }
}

/// Notify the registered handler (if any) of a follow-up question request.
pub async fn notify_followup_question_requested(
    ctx: &ApiContext,
    execution_id: &str,
    request: &Value,
) {
    if let Some(handler) = ctx.user_interaction_handler.read().await.as_ref() {
        handler.on_followup_question_requested(execution_id, request);
    }
}

/// Interaction history of a config (agent loop), newest first.
pub async fn get_configuration_interaction_history(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentUserInteractionEventRecord>> {
    list(ctx, agent_loop_id).await
}

fn record_from_storage(record: UserInteractionStorageMetadata) -> AgentUserInteractionEventRecord {
    AgentUserInteractionEventRecord {
        id: record.id.to_string(),
        execution_id: record.execution_id.to_string(),
        interaction_type: record.interaction_type,
        status: record.status,
        request_data: record.request_data,
        response_data: record.response_data,
        result_data: record.result_data,
        created_at: record.created_at,
        responded_at: record.responded_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::UserInteractionStorageMetadata;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    async fn save_interaction(ctx: &StorageContext, id: &str, execution_id: &str, status: &str) {
        ctx.user_interaction
            .save(&UserInteractionStorageMetadata {
                id: id.into(),
                execution_id: execution_id.into(),
                interaction_type: "confirm".into(),
                status: status.into(),
                request_data: serde_json::json!({ "prompt": "approve?" }),
                response_data: None,
                result_data: None,
                error: None,
                created_at: 1000,
                responded_at: None,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn list_get_and_respond() {
        let ctx = make_ctx();
        save_interaction(&ctx.storage, "ui-1", "loop-ui", "pending").await;
        save_interaction(&ctx.storage, "ui-2", "loop-ui", "pending").await;
        save_interaction(&ctx.storage, "ui-3", "other-loop", "pending").await;

        let records = list(&ctx, "loop-ui").await.unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.execution_id == "loop-ui"));

        let filtered = list_filtered(&ctx, "loop-ui", Some("pending"), None)
            .await
            .unwrap();
        assert_eq!(filtered.len(), 2);

        let one = get(&ctx, "ui-1").await.unwrap();
        assert_eq!(one.interaction_type, "confirm");

        respond(&ctx, "loop-ui", "ui-1", Some(json!({"value": "yes"})), None)
            .await
            .unwrap();
        let updated = get(&ctx, "ui-1").await.unwrap();
        assert_eq!(updated.status, "responded");
        assert_eq!(updated.response_data, Some(json!({"value": "yes"})));

        // Responding an interaction of another loop is rejected.
        let err = respond(&ctx, "loop-ui", "ui-3", None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));

        // Double respond conflicts.
        let err = respond(&ctx, "loop-ui", "ui-1", None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)));
    }

    #[tokio::test]
    async fn handler_registration_and_notify() {
        struct CountingHandler(Arc<AtomicUsize>);
        impl UserInteractionHandler for CountingHandler {
            fn on_interaction(&self, _record: &AgentUserInteractionEventRecord) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let ctx = make_ctx();
        assert!(!has_handler(&ctx).await);

        let counter = Arc::new(AtomicUsize::new(0));
        register_handler(&ctx, Arc::new(CountingHandler(counter.clone()))).await;
        assert!(has_handler(&ctx).await);

        let record = AgentUserInteractionEventRecord {
            id: "ui-h".into(),
            execution_id: "loop-h".into(),
            interaction_type: "confirm".into(),
            status: "pending".into(),
            request_data: json!({}),
            response_data: None,
            result_data: None,
            created_at: 1,
            responded_at: None,
        };
        on_interaction_created(&ctx, &record).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        clear_handler(&ctx).await;
        assert!(!has_handler(&ctx).await);
        on_interaction_created(&ctx, &record).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
