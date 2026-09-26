use std::any::Any;
use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_metrics::collectors::node::NodeExecutionRecord as MetricsNodeExecutionRecord;
use wf_metrics::collectors::node::NodeMetricsCollector;
use wf_types::checkpoint::NodeCheckpointConfig;
use wf_types::node::StaticNodeType;

use crate::coordinator::NodeCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::error_analysis::workflow_error_record;
use crate::hook::WorkflowHookEmitter;
use crate::state::NodeExecutionRecord;

use super::timeout::{json_size, panic_message, resolve_node_timeout};
use super::WorkflowCoordinator;

/// Identity of the node being executed in the coordinator loop: the execution
/// entity it runs under plus the node's id and parsed type. Grouped so the
/// execute / record / retry helpers stay small.
pub(super) struct NodeAttempt<'a> {
    pub entity: &'a WorkflowExecutionEntity,
    pub node_id: &'a str,
    pub node_type: &'a StaticNodeType,
}

/// Timing and accounting inputs captured once a node execution has a result.
/// Shared by the success/failure recording and retry paths.
pub(super) struct NodeOutcome<'a> {
    pub node_type_str: &'a str,
    pub metrics: Option<&'a NodeMetricsCollector>,
    pub start: i64,
    pub duration_ms: f64,
    pub checkpoint_config: Option<NodeCheckpointConfig>,
    /// Node-level `checkpoint_after_execute` force flag, carried from the
    /// pre-execution read so the completion path needs no second lookup.
    pub force_checkpoint_after: bool,
}

/// One completed node execution attempt, shared by the main execution path
/// and the retry path; each attempt yields an independent record.
pub(super) struct ExecutionAttempt<'a> {
    pub node_id: &'a str,
    pub node_type: &'a str,
    pub start_time: i64,
    pub success: bool,
    pub error: Option<String>,
    /// Input passed to the node handler (audit detail).
    pub input: Option<Value>,
    /// Result produced by the node (audit detail).
    pub result: Option<Value>,
    /// Fork/join branch the node ran under (audit detail).
    pub branch_id: Option<String>,
}

impl WorkflowCoordinator {
    /// Record a node completion on the innermost active loop's current
    /// iteration, so the completed-skip decision can tell iterations apart.
    pub(super) fn record_loop_iteration_completion(&self, node_id: &str) {
        let is_loop_control = self
            .traversal
            .get_node(node_id)
            .is_some_and(|n| matches!(n.node_type.as_str(), "LOOP_START" | "LOOP_END"));
        if !is_loop_control {
            crate::loop_state::record_iteration_completion(&self.ctx.variables, node_id);
        }
    }

    /// Append one node execution record to the shared entity state.
    pub(super) async fn record_node_execution(
        &self,
        entity: &WorkflowExecutionEntity,
        attempt: ExecutionAttempt<'_>,
    ) {
        let node_name = self
            .traversal
            .get_node(attempt.node_id)
            .and_then(|n| n.name.clone())
            .unwrap_or_else(|| attempt.node_id.to_string());

        entity
            .state
            .write()
            .await
            .record_node_execution(NodeExecutionRecord {
                node_id: attempt.node_id.to_string(),
                node_name,
                node_type: attempt.node_type.to_string(),
                start_time: attempt.start_time,
                end_time: Some(wf_common::now()),
                success: attempt.success,
                error: attempt.error,
                // Pre-capped payload audit detail (truncation footprint
                // marking stays with the checkpoint type).
                input: attempt
                    .input
                    .as_ref()
                    .map(wf_types::checkpoint::workflow::cap_node_payload),
                result: attempt
                    .result
                    .as_ref()
                    .map(wf_types::checkpoint::workflow::cap_node_payload),
                branch_id: attempt.branch_id,
            });
    }

    /// Persist a structured error record for a failed node attempt. Each
    /// record is its own root cause with a single-entry chain.
    pub(super) async fn record_workflow_error(
        entity: &WorkflowExecutionEntity,
        error: &WorkflowError,
        node_id: &str,
    ) {
        let mut state = entity.state.write().await;
        let mut record = workflow_error_record(error, entity.id(), node_id);
        record.error_chain = vec![record.id.clone()];
        record.root_cause_id = record.id.clone();
        state.add_error_record(record);
    }

    /// Execute one node through the `NodeCoordinator`, wrapped in the
    /// node-level timeout when configured.
    pub(super) async fn execute_node_once(
        &self,
        attempt: &NodeAttempt<'_>,
        node: &wf_types::workflow_execution::WorkflowNode,
        node_ctx: &mut NodeExecutionContext,
        event_bus: Option<&EventBus>,
        node_timeout: Option<u64>,
    ) -> WorkflowResult<NodeExecutionResult> {
        let entity = attempt.entity;
        let node_id = attempt.node_id;
        let node_type = attempt.node_type;

        let handler =
            self.resolve_node_handler(node_type)
                .ok_or_else(|| WorkflowError::HandlerNotFound {
                    node_type: node.node_type.clone(),
                })?;

        let coordinator = NodeCoordinator::new();
        let timeout_dur = resolve_node_timeout(node, node_type, node_timeout);

        let fut = coordinator.execute_node(
            entity,
            handler,
            node_ctx,
            event_bus,
            &self.hooks,
            self.ctx.hook_handler_registry.as_deref(),
        );
        // Panic isolation: a panicking handler must surface as a routed node
        // failure instead of aborting the whole execution task.
        let guarded = AssertUnwindSafe(fut).catch_unwind();
        let panic_failure = |payload: Box<dyn Any + Send>| {
            tracing::error!(node_id = %node_id, "node handler panicked");
            WorkflowError::NodeFailure {
                node_id: node_id.to_string(),
                category: wf_types::workflow::error_branch::NodeErrorCategory::BusinessFailure,
                detail: format!("node handler panicked: {}", panic_message(&payload)),
            }
        };

        match timeout_dur {
            Some(tout_dur) => {
                let timeout_metrics = self.ctx.metrics.as_ref().map(|m| m.timeout());
                let execution_id = self.ctx.execution_id.to_string();
                if let Some(ref metrics) = timeout_metrics {
                    metrics.record_registration(
                        "workflow_node",
                        tout_dur.as_millis() as f64,
                        &execution_id,
                    );
                }
                let node_start = wf_common::now();
                let result = tokio::time::timeout(tout_dur, guarded)
                    .await
                    .map_err(|_| WorkflowError::NodeFailure {
                        node_id: node_id.to_string(),
                        category:
                            wf_types::workflow::error_branch::NodeErrorCategory::TransportTimeout,
                        detail: format!("timed out after {:?}", tout_dur),
                    })
                    .and_then(|handler_result| handler_result.map_err(panic_failure));
                match &result {
                    Err(_) => {
                        if let Some(ref metrics) = timeout_metrics {
                            metrics.record_expiration(
                                "workflow_node",
                                (wf_common::now() - node_start) as f64,
                                &execution_id,
                            );
                        }
                    }
                    Ok(_) => {
                        if let Some(ref metrics) = timeout_metrics {
                            metrics.record_cancellation("workflow_node", "complete", &execution_id);
                        }
                    }
                }
                result?
            }
            None => guarded.await.map_err(panic_failure)?,
        }
    }

    /// Record a successful node execution: outputs, completion state, audit
    /// record, metrics and node-level checkpoint.
    pub(super) async fn record_node_success(
        &mut self,
        attempt: &NodeAttempt<'_>,
        outcome: &NodeOutcome<'_>,
        node_ctx: &NodeExecutionContext,
        output: &NodeExecutionResult,
    ) {
        let entity = attempt.entity;
        let node_id = attempt.node_id;
        let node_type_str = outcome.node_type_str;
        let node_metrics = outcome.metrics;
        let node_start = outcome.start;
        let node_duration_ms = outcome.duration_ms;
        let checkpoint_config = &outcome.checkpoint_config;

        self.node_outputs
            .insert(node_id.to_string(), output.output.clone());
        self.completed_nodes.push(node_id.to_string());
        self.record_loop_iteration_completion(node_id);
        entity.set_node_result(node_id.to_string(), output.output.clone());

        for (k, v) in &output.metadata {
            self.ctx.variables.insert(k.clone(), v.clone());
        }

        entity
            .state
            .write()
            .await
            .mark_node_completed(node_id.to_string());

        self.record_node_execution(
            entity,
            ExecutionAttempt {
                node_id,
                node_type: node_type_str,
                start_time: node_start,
                success: true,
                error: None,
                input: Some(node_ctx.input.clone()),
                result: Some(output.output.clone()),
                branch_id: None,
            },
        )
        .await;

        if let Some(ref mut cp) = self.checkpoint {
            cp.on_node_completed(
                entity,
                checkpoint_config.as_ref(),
                outcome.force_checkpoint_after,
            )
            .await;
        }
        WorkflowHookEmitter::maybe_hook_checkpoint(
            &self.hooks,
            "AFTER_EXECUTE",
            self.checkpoint.as_ref(),
            entity,
        )
        .await;

        if let Some(node_metrics) = node_metrics {
            node_metrics.record_execution(MetricsNodeExecutionRecord {
                node_id,
                node_type: node_type_str,
                execution_id: &self.ctx.execution_id,
                success: true,
                duration_ms: node_duration_ms,
                input_size: json_size(&node_ctx.input),
                output_size: json_size(&output.output),
                error_type: None,
            });
        }

        // Publish the branch's public variables into the fork registry after
        // every completed node (SYNC reads the source branch's live state).
        if let Some((registry, path_id)) = &self.fork_branch_progress {
            let snapshot = self
                .ctx
                .variables
                .iter()
                .filter(|entry| !entry.key().starts_with("__"))
                .map(|entry| (entry.key().clone(), entry.value().clone()))
                .collect();
            registry.update_variables(path_id, snapshot);
        }

        // An explicit merge point brings isolated error-branch writes back to
        // the main path (last-writer-wins with audit) and reclaims the error
        // namespace; the run continues as a normal execution.
        if self.error_scope.is_some()
            && self
                .traversal
                .get_node(node_id)
                .is_some_and(|node| wf_types::workflow::error_branch::is_merge_point(&node.inner))
        {
            self.merge_error_scope(entity).await;
        }
    }

    /// Record a failed node execution: error chain, audit record, metrics and
    /// node-level checkpoint.
    pub(super) async fn record_node_failure(
        &mut self,
        attempt: &NodeAttempt<'_>,
        outcome: &NodeOutcome<'_>,
        node_ctx: &NodeExecutionContext,
        error: &WorkflowError,
    ) {
        let entity = attempt.entity;
        let node_id = attempt.node_id;
        let node_type_str = outcome.node_type_str;
        let node_metrics = outcome.metrics;
        let node_start = outcome.start;
        let node_duration_ms = outcome.duration_ms;
        let checkpoint_config = &outcome.checkpoint_config;

        self.node_errors
            .push(format!("Node {}: {}", node_id, error));

        Self::record_workflow_error(entity, error, node_id).await;

        self.record_node_execution(
            entity,
            ExecutionAttempt {
                node_id,
                node_type: node_type_str,
                start_time: node_start,
                success: false,
                error: Some(error.to_string()),
                input: Some(node_ctx.input.clone()),
                result: None,
                branch_id: None,
            },
        )
        .await;

        if let Some(ref mut cp) = self.checkpoint {
            cp.on_node_failed(entity, checkpoint_config.as_ref()).await;
        }
        WorkflowHookEmitter::maybe_hook_checkpoint(
            &self.hooks,
            "ON_ERROR",
            self.checkpoint.as_ref(),
            entity,
        )
        .await;

        if let Some(node_metrics) = node_metrics {
            node_metrics.record_execution(MetricsNodeExecutionRecord {
                node_id,
                node_type: node_type_str,
                execution_id: &self.ctx.execution_id,
                success: false,
                duration_ms: node_duration_ms,
                input_size: json_size(&node_ctx.input),
                output_size: 0,
                error_type: Some("node_failed"),
            });
        }
    }
}
