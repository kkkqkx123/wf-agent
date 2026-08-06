use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::events::{BaseEvent, EventType};
use wf_types::ExecutionStatus;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// One node execution attempt of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct NodeExecutionRecordView {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub success: bool,
    pub error: Option<String>,
}

/// Snapshot of a workflow execution's state (TS `WorkflowExecutionStateAPI`
/// counterpart).
///
/// Data sources: the live entity in the in-memory registry (full state, up
/// to completion) or the persisted `WorkflowExecution` record (fields the
/// persistence boundary kept). `source` tells the consumer which boundary the
/// view was built from.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowExecutionStateView {
    pub execution_id: String,
    pub workflow_id: Option<String>,
    pub status: ExecutionStatus,
    pub current_node_id: Option<String>,
    pub completed_nodes: Vec<String>,
    pub node_execution_history: Vec<NodeExecutionRecordView>,
    pub variables: BTreeMap<String, Value>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub source: String,
}

/// A reconstructed state transition (derived from lifecycle events).
#[derive(Debug, Clone, Serialize)]
pub struct StateTransitionView {
    pub from: String,
    pub to: String,
    pub timestamp: i64,
}

/// One tool call recorded by an agent iteration.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallRecordView {
    pub name: String,
    pub duration_ms: i64,
    pub success: bool,
}

/// One agent loop iteration record.
#[derive(Debug, Clone, Serialize)]
pub struct IterationRecordView {
    pub iteration: u32,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub tool_call_count: u32,
    pub tool_calls: Vec<ToolCallRecordView>,
}

/// Snapshot of an agent loop's execution state (TS
/// `AgentExecutionStateAPI` counterpart).
#[derive(Debug, Clone, Serialize)]
pub struct AgentLoopStateView {
    pub agent_loop_id: String,
    pub status: ExecutionStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    pub iteration_history: Vec<IterationRecordView>,
    pub variables: BTreeMap<String, Value>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub source: String,
}

/// Execution-state queries over workflow executions.
pub struct WorkflowExecutionStateApi {
    ctx: Arc<ApiContext>,
}

impl WorkflowExecutionStateApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Full state view of an execution: live entity when present, otherwise
    /// the persisted record.
    pub async fn get_state(&self, execution_id: &str) -> ApiResult<WorkflowExecutionStateView> {
        if let Some(entity) = self.ctx.workflow_execution(execution_id) {
            let snapshot = entity
                .state
                .read()
                .await
                .create_snapshot()
                .await
                .map_err(|e| ApiError::Execution(format!("state snapshot failed: {e}")))?;
            let mut variables = BTreeMap::new();
            for entry in entity.variables().iter() {
                variables.insert(entry.key().clone(), entry.value().clone());
            }
            return Ok(WorkflowExecutionStateView {
                execution_id: execution_id.to_string(),
                workflow_id: Some(entity.workflow_id().to_string()),
                status: snapshot.status.into(),
                current_node_id: snapshot.current_node_id,
                completed_nodes: snapshot.completed_nodes,
                node_execution_history: snapshot
                    .node_execution_history
                    .into_iter()
                    .map(|r| NodeExecutionRecordView {
                        node_id: r.node_id,
                        node_name: r.node_name,
                        node_type: r.node_type,
                        start_time: r.start_time,
                        end_time: r.end_time,
                        success: r.success,
                        error: r.error,
                    })
                    .collect(),
                variables,
                start_time: snapshot.start_time,
                end_time: snapshot.end_time,
                error: snapshot.error,
                source: "live".into(),
            });
        }

        let record = self
            .ctx
            .storage
            .workflow_execution
            .load(execution_id)
            .await?
            .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
        Ok(WorkflowExecutionStateView {
            execution_id: record.id.clone(),
            workflow_id: Some(record.workflow_id.clone()),
            status: record.status,
            current_node_id: record.current_node_id,
            completed_nodes: Vec::new(),
            node_execution_history: Vec::new(),
            variables: record_variable_map(record.variables),
            start_time: record.started_at,
            end_time: record.completed_at,
            error: record.error,
            source: "persisted".into(),
        })
    }

    /// Variable snapshot of an execution (live when present, persisted
    /// otherwise). Never errors on missing live state; returns what the
    /// current boundary holds.
    pub async fn variables(&self, execution_id: &str) -> ApiResult<BTreeMap<String, Value>> {
        if let Some(entity) = self.ctx.workflow_execution(execution_id) {
            let mut variables = BTreeMap::new();
            for entry in entity.variables().iter() {
                variables.insert(entry.key().clone(), entry.value().clone());
            }
            return Ok(variables);
        }
        let record = self
            .ctx
            .storage
            .workflow_execution
            .load(execution_id)
            .await?
            .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
        Ok(record_variable_map(record.variables))
    }

    /// State transition sequence of an execution, reconstructed from the
    /// lifecycle events retained by the event bus.
    pub async fn status_transitions(
        &self,
        execution_id: &str,
    ) -> ApiResult<Vec<StateTransitionView>> {
        let mut events: Vec<BaseEvent> = self
            .ctx
            .event_bus
            .recent_events()
            .into_iter()
            .filter(|e| e.execution_id.as_deref() == Some(execution_id))
            .filter(|e| transition_status(&e.r#type).is_some())
            .collect();
        events.sort_by_key(|e| e.timestamp);

        let mut transitions = Vec::new();
        let mut previous: Option<String> = None;
        for event in events {
            let to = transition_status(&event.r#type).unwrap_or_default();
            if previous.as_deref() == Some(to.as_str()) {
                continue;
            }
            transitions.push(StateTransitionView {
                from: previous.unwrap_or_else(|| "Created".to_string()),
                to: to.clone(),
                timestamp: event.timestamp,
            });
            previous = Some(to);
        }
        Ok(transitions)
    }
}

/// Execution-state queries over agent loops.
pub struct AgentExecutionStateApi {
    ctx: Arc<ApiContext>,
}

impl AgentExecutionStateApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Full state view of an agent loop: live entity when present, otherwise
    /// the persisted record.
    pub async fn get_state(&self, agent_loop_id: &str) -> ApiResult<AgentLoopStateView> {
        if let Some(entity) = self.ctx.agent_loop(agent_loop_id) {
            let snapshot = entity
                .state
                .read()
                .await
                .create_snapshot()
                .await
                .map_err(|e| ApiError::Execution(format!("state snapshot failed: {e}")))?;
            let mut variables = BTreeMap::new();
            for (name, value) in snapshot.variable_snapshots {
                variables.insert(name, value);
            }
            return Ok(AgentLoopStateView {
                agent_loop_id: agent_loop_id.to_string(),
                status: snapshot.status.into(),
                current_iteration: snapshot.current_iteration,
                tool_call_count: snapshot.tool_call_count,
                iteration_history: snapshot
                    .iteration_history
                    .into_iter()
                    .map(iteration_record_view)
                    .collect(),
                variables,
                start_time: snapshot.start_time,
                end_time: snapshot.end_time,
                error: snapshot.error,
                source: "live".into(),
            });
        }

        if let Some(record) = self.ctx.storage.agent_execution.load(agent_loop_id).await? {
            return Ok(AgentLoopStateView {
                agent_loop_id: record.id.clone(),
                status: record.status,
                current_iteration: record.current_iteration,
                tool_call_count: record.tool_call_count,
                iteration_history: record
                    .iteration_history
                    .unwrap_or_default()
                    .into_iter()
                    .map(persisted_iteration_view)
                    .collect(),
                variables: BTreeMap::new(),
                start_time: record.started_at,
                end_time: record.completed_at,
                error: record.error,
                source: "persisted".into(),
            });
        }

        if let Some(meta) = self.ctx.storage.agent_loop.load(agent_loop_id).await? {
            return Ok(AgentLoopStateView {
                agent_loop_id: meta.id.clone(),
                status: parse_status(&meta.status),
                current_iteration: meta.current_iteration,
                tool_call_count: 0,
                iteration_history: Vec::new(),
                variables: BTreeMap::new(),
                start_time: meta.started_at,
                end_time: None,
                error: None,
                source: "persisted".into(),
            });
        }

        Err(ApiError::execution_not_found(agent_loop_id))
    }

    /// Variable snapshot of an agent loop (live only; persisted records do
    /// not retain the variable map).
    pub async fn variables(&self, agent_loop_id: &str) -> ApiResult<BTreeMap<String, Value>> {
        let view = self.get_state(agent_loop_id).await?;
        Ok(view.variables)
    }

    /// Iteration history of an agent loop (live when present, persisted
    /// otherwise).
    pub async fn iteration_history(
        &self,
        agent_loop_id: &str,
    ) -> ApiResult<Vec<IterationRecordView>> {
        Ok(self.get_state(agent_loop_id).await?.iteration_history)
    }
}

fn record_variable_map(
    variables: Option<Vec<wf_types::workflow_execution::VariableDefinition>>,
) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    if let Some(variables) = variables {
        for variable in variables {
            map.insert(variable.name, variable.value);
        }
    }
    map
}

/// Parse the persisted string status of an `AgentLoopStorageMetadata` onto
/// the typed contract, defaulting to `Created` for unknown values.
fn parse_status(status: &str) -> ExecutionStatus {
    match status {
        "created" => ExecutionStatus::Created,
        "running" => ExecutionStatus::Running,
        "paused" => ExecutionStatus::Paused,
        "stopped" => ExecutionStatus::Stopped,
        "completed" => ExecutionStatus::Completed,
        "failed" => ExecutionStatus::Failed,
        "cancelled" => ExecutionStatus::Cancelled,
        _ => ExecutionStatus::Created,
    }
}

fn iteration_record_view(record: wf_agent::state::IterationRecord) -> IterationRecordView {
    IterationRecordView {
        iteration: record.iteration,
        start_time: record.start_time,
        end_time: record.end_time,
        tool_call_count: record.tool_call_count,
        tool_calls: record
            .tool_calls
            .into_iter()
            .map(|call| ToolCallRecordView {
                name: call.name,
                duration_ms: call.duration_ms,
                success: call.success,
            })
            .collect(),
    }
}

fn persisted_iteration_view(
    record: wf_types::agent_execution::IterationRecord,
) -> IterationRecordView {
    IterationRecordView {
        iteration: record.iteration,
        start_time: record.started_at,
        end_time: record.completed_at,
        tool_call_count: record
            .tool_calls
            .as_ref()
            .map(|calls| calls.len() as u32)
            .unwrap_or(0),
        tool_calls: record
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|call| ToolCallRecordView {
                name: call.name,
                duration_ms: call
                    .completed_at
                    .map(|end| end - call.started_at)
                    .unwrap_or(0),
                success: call.error.is_none(),
            })
            .collect(),
    }
}

/// Map a lifecycle event type onto the resulting execution status.
fn transition_status(event_type: &EventType) -> Option<String> {
    let status = match event_type {
        EventType::WorkflowExecutionStarted | EventType::AgentStarted => "Running",
        EventType::WorkflowExecutionPaused | EventType::AgentPaused => "Paused",
        EventType::WorkflowExecutionResumed | EventType::AgentResumed => "Running",
        EventType::WorkflowExecutionCompleted | EventType::AgentCompleted => "Completed",
        EventType::WorkflowExecutionFailed | EventType::AgentFailed => "Failed",
        EventType::WorkflowExecutionCancelled | EventType::AgentCancelled => "Cancelled",
        EventType::ExecutionStopped => "Stopped",
        _ => return None,
    };
    Some(status.to_string())
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

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        let api = WorkflowExecutionStateApi::new(ctx);
        let err = api.get_state("missing").await.unwrap_err();
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }

    #[tokio::test]
    async fn degrades_gracefully_without_live_state() {
        let ctx = make_ctx();
        // Persisted record with no live entity -> "persisted" view, no panic.
        let record = wf_types::WorkflowExecution {
            id: "exec-p".into(),
            workflow_id: "wf-p".into(),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: Some(vec![wf_types::workflow_execution::VariableDefinition {
                name: "x".into(),
                value: serde_json::json!(1),
                r#type: None,
                scope: None,
                readonly: None,
                metadata: None,
            }]),
            input: None,
            output: None,
            node_results: None,
            errors: None,
            error: None,
            started_at: wf_common::now(),
            completed_at: Some(wf_common::now()),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&record).await.unwrap();

        let api = WorkflowExecutionStateApi::new(ctx);
        let view = api.get_state("exec-p").await.unwrap();
        assert_eq!(view.source, "persisted");
        assert_eq!(view.status, ExecutionStatus::Completed);
        assert_eq!(view.variables.get("x"), Some(&serde_json::json!(1)));
        assert!(view.node_execution_history.is_empty());

        let variables = api.variables("exec-p").await.unwrap();
        assert_eq!(variables.get("x"), Some(&serde_json::json!(1)));
    }

    #[tokio::test]
    async fn live_workflow_entity_supplies_full_state() {
        use wf_core::registry::MutableRegistry;
        use wf_workflow::entity::WorkflowExecutionEntity;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-live".to_string()),
            wf_types::Id::from("wf-live".to_string()),
        ));
        entity.set_variable("a", serde_json::json!({"n": 1}));
        ctx.workflow_executions
            .register("exec-live".to_string(), entity.clone())
            .expect("register");

        let api = WorkflowExecutionStateApi::new(ctx);
        let view = api.get_state("exec-live").await.unwrap();
        assert_eq!(view.source, "live");
        assert_eq!(view.variables.get("a"), Some(&serde_json::json!({"n": 1})));

        let transitions = api.status_transitions("exec-live").await.unwrap();
        assert!(transitions.is_empty(), "no lifecycle events published");
    }

    #[tokio::test]
    async fn agent_state_from_live_entity() {
        use wf_agent::entity::AgentLoopEntity;

        let ctx = make_ctx();
        let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
            "agent-live".to_string(),
        )));
        entity.state.write().await.start();
        entity.state.write().await.start_iteration();
        ctx.agent_loops.register(entity.clone());

        let api = AgentExecutionStateApi::new(ctx);
        let view = api.get_state("agent-live").await.unwrap();
        assert_eq!(view.source, "live");
        assert_eq!(view.current_iteration, 1);
        assert_eq!(view.status, ExecutionStatus::Running);
    }

    #[tokio::test]
    async fn agent_state_degrades_to_persisted() {
        let ctx = make_ctx();
        let meta = wf_types::AgentLoopStorageMetadata {
            id: "agent-p".into(),
            definition_id: "agent-def".into(),
            status: "completed".into(),
            current_iteration: 3,
            started_at: wf_common::now(),
            updated_at: wf_common::now(),
        };
        ctx.storage.agent_loop.save(&meta).await.unwrap();

        let api = AgentExecutionStateApi::new(ctx);
        let view = api.get_state("agent-p").await.unwrap();
        assert_eq!(view.source, "persisted");
        assert_eq!(view.current_iteration, 3);
        assert_eq!(view.status, ExecutionStatus::Completed);
    }
}
