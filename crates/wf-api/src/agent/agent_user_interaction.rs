//! User interaction queries and handler wiring scoped to agent loops.

use serde_json::Value;

use wf_types::UserInteractionStorageMetadata;

use crate::entity::user_interaction::{
    get_interaction, list_interactions_by_execution, respond_interaction,
};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

// Re-export the shared interaction contract so the agent-facing module path
// keeps working; the trait and event record live in the entity module to
// avoid a entity -> agent dependency.
pub use crate::entity::user_interaction::{
    AgentUserInteractionEventRecord, UserInteractionHandler,
};

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

/// Respond to a pending interaction of an agent loop.
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
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::UserInteractionStorageMetadata;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
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
        assert!(!crate::entity::user_interaction::has_handler(&ctx).await);

        let counter = Arc::new(AtomicUsize::new(0));
        crate::entity::user_interaction::register_handler(
            &ctx,
            Arc::new(CountingHandler(counter.clone())),
        )
        .await;
        assert!(crate::entity::user_interaction::has_handler(&ctx).await);

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
        crate::entity::user_interaction::on_interaction_created(&ctx, &record).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        crate::entity::user_interaction::clear_handler(&ctx).await;
        assert!(!crate::entity::user_interaction::has_handler(&ctx).await);
        crate::entity::user_interaction::on_interaction_created(&ctx, &record).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
