//! API-backed recovery executor: restores the latest checkpoint of an
//! incomplete execution and resumes it through the wf-api path.

use tracing::info;

use crate::error::{RuntimeError, RuntimeResult};
use crate::recovery::{RecoveryExecutor, RecoveryItem};

/// Recovers an execution by restoring its latest checkpoint and resuming it
/// to completion (`restore_checkpoint` + `resume`). The shared `ApiContext`
/// is passed per call; restored executions register into the live registry
/// and continue through the standard coordinator path.
pub struct ApiRecoveryExecutor;

#[async_trait::async_trait]
impl RecoveryExecutor for ApiRecoveryExecutor {
    async fn recover_execution(
        &self,
        ctx: &wf_api::ApiContext,
        execution: &wf_types::WorkflowExecution,
    ) -> RuntimeResult<RecoveryItem> {
        let state_manager =
            wf_checkpoint::state::WorkflowCheckpointStateManager::new(ctx.checkpoint_store.clone());
        let latest = state_manager
            .list_latest_by_entities(std::slice::from_ref(&execution.id))
            .await
            .map_err(|e| {
                RuntimeError::Storage(wf_storage::error::StorageError::General {
                    operation: "recovery.list_latest_by_entities".into(),
                    message: e.to_string(),
                    source: None,
                })
            })?;

        let Some(latest) = latest.into_iter().next() else {
            return Ok(RecoveryItem {
                execution_id: execution.id.to_string(),
                status: format!("{:?}", execution.status),
                current_node_id: execution.current_node_id.clone(),
                recovered: false,
                note: Some("no checkpoint available for this execution".to_string()),
            });
        };

        info!(
            execution_id = %execution.id,
            checkpoint_id = %latest.id,
            "Restoring execution from checkpoint"
        );

        match wf_api::workflow::workflow_execution::restore_and_resume(ctx, &latest.id).await {
            Ok(output) => Ok(RecoveryItem {
                execution_id: output.execution_id.to_string(),
                status: "Completed".to_string(),
                current_node_id: None,
                recovered: true,
                note: None,
            }),
            Err(e) => Err(RuntimeError::Config(format!(
                "checkpoint restore/resume failed for {}: {e}",
                execution.id
            ))),
        }
    }
}

#[cfg(all(test, feature = "checkpoint"))]
mod tests {
    use std::sync::Arc;

    use super::*;
    use wf_api::ApiContext;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::adapter::execution::WorkflowExecutionStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::node::{BaseStaticNode, StaticNodeType};
    use wf_types::workflow::edge::EdgeType;
    use wf_types::workflow::WorkflowDefinition;
    use wf_types::ExecutionStatus;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    /// start -> v1 -> v2 -> end; v1 and v2 write variables so a partial run
    /// leaves genuine mid-execution state behind.
    fn make_multi_step_definition(id: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {}", id),
            description: None,
            r#type: None,
            version: Some("1.0.0".into()),
            hooks: None,
            nodes: vec![
                BaseStaticNode {
                    id: "start".into(),
                    node_type: StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "v1".into(),
                    node_type: StaticNodeType::Variable,
                    name: Some("v1".into()),
                    description: None,
                    config: Some(serde_json::json!({
                        "variable_name": "step1",
                        "expression": "${input.greeting}",
                    })),
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "v2".into(),
                    node_type: StaticNodeType::Variable,
                    name: Some("v2".into()),
                    description: None,
                    config: Some(serde_json::json!({
                        "variable_name": "final",
                        "expression": "${variables.step1}-done",
                    })),
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "end".into(),
                    node_type: StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![
                wf_types::workflow::Edge {
                    id: "e1".into(),
                    source_node_id: "start".into(),
                    target_node_id: "v1".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
                wf_types::workflow::Edge {
                    id: "e2".into(),
                    source_node_id: "v1".into(),
                    target_node_id: "v2".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
                wf_types::workflow::Edge {
                    id: "e3".into(),
                    source_node_id: "v2".into(),
                    target_node_id: "end".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
            ],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn recover_execution_restores_latest_checkpoint_and_completes() {
        let ctx = make_ctx();
        let definition = make_multi_step_definition("wf-rec-1");
        ctx.storage.workflow.save(&definition).await.unwrap();

        // Run partway (after v1) so the auto checkpoint chain holds a genuine
        // mid-execution snapshot.
        let mut partial_options = wf_types::workflow_execution::WorkflowExecutionOptions {
            input: None,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(true),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
            max_navigation_multiplier: None,
            loop_max_iterations_cap: None,
        };
        partial_options.max_steps = Some(2);
        let partial = wf_api::workflow::workflow_execution::execute(
            &ctx,
            wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
                workflow_id: "wf-rec-1".into(),
                input: Some(serde_json::json!({"greeting": "hi"})),
                options: Some(partial_options),
            },
        )
        .await
        .expect("partial run completes");
        let execution_id = partial.execution_id.to_string();

        // Simulate a crash mid-run: the persisted record is flipped back to
        // Running with its checkpoints still on the store.
        let record = ctx
            .storage
            .workflow_execution
            .load(&execution_id)
            .await
            .unwrap()
            .expect("execution record persisted");
        ctx.storage
            .workflow_execution
            .update_status(&execution_id, &ExecutionStatus::Running)
            .await
            .unwrap();

        let item = ApiRecoveryExecutor
            .recover_execution(&ctx, &record)
            .await
            .expect("recovery succeeds");
        assert!(item.recovered, "execution must be recovered: {item:?}");
        assert_eq!(item.execution_id, execution_id);
        assert_eq!(item.status, "Completed");

        // The resumed execution reached the terminal node: the persisted
        // record now reflects completion.
        let after = ctx
            .storage
            .workflow_execution
            .load(&execution_id)
            .await
            .unwrap()
            .expect("execution record still persisted");
        assert_eq!(after.status, ExecutionStatus::Completed);
    }

    #[tokio::test]
    async fn recover_execution_without_checkpoint_reports_skipped() {
        let ctx = make_ctx();
        let record = wf_types::WorkflowExecution {
            id: "no-cp-1".into(),
            workflow_id: "wf-x".into(),
            workflow_version: None,
            status: ExecutionStatus::Running,
            current_node_id: Some("v1".into()),
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: None,
            started_at: 0,
            completed_at: None,
            error: None,
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };

        let item = ApiRecoveryExecutor
            .recover_execution(&ctx, &record)
            .await
            .expect("recovery succeeds without checkpoint");
        assert!(!item.recovered);
        assert_eq!(
            item.note.as_deref(),
            Some("no checkpoint available for this execution")
        );
        assert_eq!(item.current_node_id.as_deref(), Some("v1"));
    }
}
