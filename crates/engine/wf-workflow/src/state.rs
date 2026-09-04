use std::collections::HashMap;

use wf_common::error_chain::ErrorRecord;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_execution_shared::types::state_manager::StateManager;
use wf_types::checkpoint::workflow::snapshot::OperationState;

/// One node execution attempt (retries produce independent records).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeExecutionRecord {
    pub node_id: String,
    /// Node name from the graph definition, falls back to `node_id`.
    pub node_name: String,
    pub node_type: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub success: bool,
    pub error: Option<String>,
    /// Input passed to the node handler (payload-capped). Older
    /// states have no such field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    /// Result produced by the node (payload-capped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Fork/join branch the node ran under; `None` in linear flows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

/// Statistics summarising interruption events captured during execution.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WorkflowInterruptionStatistics {
    pub total: u64,
    /// Interruption type (e.g. `stop`/`pause`/`timeout`) -> occurrence count.
    pub type_distribution: std::collections::HashMap<String, u64>,
    pub avg_duration_ms: i64,
    pub recovery_rate: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowExecutionStateSnapshot {
    pub status: ExecutionStatus,
    pub current_node_id: Option<String>,
    pub completed_nodes: Vec<String>,
    pub node_execution_history: Vec<NodeExecutionRecord>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub error_records: Vec<ErrorRecord>,
    pub operation_state: Option<OperationState>,
    /// Interruption/event audit records captured at snapshot time. Older
    /// states have no such fields, so they default to empty.
    #[serde(default)]
    pub interruption_records: Vec<serde_json::Value>,
    #[serde(default)]
    pub event_records: Vec<serde_json::Value>,
    #[serde(default)]
    pub timeout_count: u32,
}

pub struct WorkflowExecutionState {
    status: ExecutionStatus,
    current_node_id: Option<String>,
    completed_nodes: Vec<String>,
    node_execution_history: Vec<NodeExecutionRecord>,
    start_time: i64,
    end_time: Option<i64>,
    error: Option<String>,
    error_records: Vec<ErrorRecord>,
    operation_state: Option<OperationState>,
    interruption_records: Vec<serde_json::Value>,
    event_records: Vec<serde_json::Value>,
    timeout_count: u32,
}

impl Default for WorkflowExecutionState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowExecutionState {
    pub fn new() -> Self {
        Self {
            status: ExecutionStatus::Created,
            current_node_id: None,
            completed_nodes: Vec::new(),
            node_execution_history: Vec::new(),
            start_time: wf_common::now(),
            end_time: None,
            error: None,
            error_records: Vec::new(),
            operation_state: None,
            interruption_records: Vec::new(),
            event_records: Vec::new(),
            timeout_count: 0,
        }
    }

    pub fn status(&self) -> ExecutionStatus {
        self.status.clone()
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, ExecutionStatus::Running)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self.status, ExecutionStatus::Paused)
    }

    pub fn is_completed(&self) -> bool {
        matches!(self.status, ExecutionStatus::Completed)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.status, ExecutionStatus::Failed)
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatus::Cancelled | ExecutionStatus::Stopped
        )
    }

    pub fn current_node_id(&self) -> Option<&str> {
        self.current_node_id.as_deref()
    }

    pub fn set_current_node(&mut self, node_id: Option<String>) {
        self.current_node_id = node_id;
    }

    pub fn completed_nodes(&self) -> &[String] {
        &self.completed_nodes
    }

    pub fn mark_node_completed(&mut self, node_id: String) {
        self.completed_nodes.push(node_id);
    }

    pub fn node_execution_history(&self) -> &[NodeExecutionRecord] {
        &self.node_execution_history
    }

    pub fn record_node_execution(&mut self, record: NodeExecutionRecord) {
        self.node_execution_history.push(record);
    }

    /// Replace the recorded node execution history wholesale (used when a
    /// checkpoint restore replays the captured audit trail).
    pub fn restore_node_execution_history(&mut self, records: Vec<NodeExecutionRecord>) {
        self.node_execution_history = records;
    }

    pub fn error_records(&self) -> &[ErrorRecord] {
        &self.error_records
    }

    pub fn add_error_record(&mut self, record: ErrorRecord) {
        self.error_records.push(record);
    }

    pub fn operation_state(&self) -> Option<&OperationState> {
        self.operation_state.as_ref()
    }

    pub fn set_operation_state(&mut self, state: Option<OperationState>) {
        self.operation_state = state;
    }

    pub fn start(&mut self) -> crate::WorkflowResult<()> {
        self.transition(ExecutionStatus::Running)
    }

    pub fn pause(&mut self) -> crate::WorkflowResult<()> {
        self.transition(ExecutionStatus::Paused)
    }

    pub fn resume(&mut self) -> crate::WorkflowResult<()> {
        self.transition(ExecutionStatus::Running)
    }

    pub fn complete(&mut self) -> crate::WorkflowResult<()> {
        self.transition(ExecutionStatus::Completed)
    }

    pub fn fail(&mut self, error: String) -> crate::WorkflowResult<()> {
        self.transition(ExecutionStatus::Failed)?;
        self.error = Some(error);
        Ok(())
    }

    pub fn cancel(&mut self) -> crate::WorkflowResult<()> {
        self.transition(ExecutionStatus::Cancelled)
    }

    /// Settle the execution as timed out (wall-clock `max_execution_time`
    /// exceeded). Kept distinct from `fail` so the terminal status records a
    /// timeout rather than a generic failure.
    pub fn timeout(&mut self, error: String) -> crate::WorkflowResult<()> {
        self.transition(ExecutionStatus::Timeout)?;
        self.error = Some(error);
        Ok(())
    }

    /// Apply a status transition with a source-state guard. Illegal
    /// transitions (e.g. `Completed -> Paused`) return an error instead of
    /// silently corrupting the machine. All status changes in the workflow
    /// execution go through this single entry point.
    pub fn transition(&mut self, target: ExecutionStatus) -> crate::WorkflowResult<()> {
        let source = self.status.clone();
        if !Self::transition_allowed(&source, &target) {
            return Err(crate::error::WorkflowError::StateTransitionError(format!(
                "{source:?} -> {target:?}"
            )));
        }
        self.apply(target);
        Ok(())
    }

    /// The legal transition table. `Running -> Running` is idempotent so a
    /// checkpoint restore that rebuilds the entity in its snapshotted
    /// `Running` state and re-drives the loop through `start` is accepted.
    /// Terminal states (Completed/Failed/Cancelled/Stopped/Timeout) never
    /// transition again.
    fn transition_allowed(source: &ExecutionStatus, target: &ExecutionStatus) -> bool {
        use ExecutionStatus::*;
        matches!(
            (source, target),
            (Created, Running)
                | (Created, Paused)
                | (Created, Cancelled)
                | (Created, Stopped)
                // Idempotent re-entry for checkpoint resumes.
                | (Running, Running)
                | (Running, Paused)
                | (Running, Completed)
                | (Running, Failed)
                | (Running, Cancelled)
                | (Running, Stopped)
                | (Running, Timeout)
                | (Paused, Running)
                | (Paused, Failed)
                | (Paused, Cancelled)
                | (Paused, Stopped)
                | (Paused, Timeout)
        )
    }

    /// Mutate the status field and its side effects for an already-validated
    /// transition.
    fn apply(&mut self, target: ExecutionStatus) {
        match target {
            ExecutionStatus::Running => {
                if self.status == ExecutionStatus::Created {
                    self.start_time = wf_common::now();
                }
                self.status = ExecutionStatus::Running;
            }
            ExecutionStatus::Paused => {
                self.status = ExecutionStatus::Paused;
            }
            ExecutionStatus::Completed => {
                self.status = ExecutionStatus::Completed;
                self.end_time = Some(wf_common::now());
            }
            ExecutionStatus::Failed => {
                self.status = ExecutionStatus::Failed;
                self.end_time = Some(wf_common::now());
            }
            ExecutionStatus::Cancelled | ExecutionStatus::Stopped | ExecutionStatus::Timeout => {
                self.status = target;
                self.end_time = Some(wf_common::now());
            }
            ExecutionStatus::Created => {}
        }
    }

    // ── Execution record management ──────────────────────────────────────

    pub fn interruption_records(&self) -> &[serde_json::Value] {
        &self.interruption_records
    }

    pub fn event_records(&self) -> &[serde_json::Value] {
        &self.event_records
    }

    pub fn record_interruption(&mut self, record: serde_json::Value) {
        self.interruption_records.push(record);
    }

    pub fn record_event(&mut self, record: serde_json::Value) {
        self.event_records.push(record);
    }

    /// Number of timeout events that have occurred during execution.
    pub fn timeout_count(&self) -> u32 {
        self.timeout_count
    }

    /// Increment the timeout counter by one.
    pub fn increment_timeout_count(&mut self) {
        self.timeout_count += 1;
    }

    /// Interruption statistics: total count, type distribution, average
    /// duration and recovery rate derived from the recorded records.
    pub fn interruption_statistics(&self) -> WorkflowInterruptionStatistics {
        let total = self.interruption_records.len() as u64;
        if total == 0 {
            return WorkflowInterruptionStatistics::default();
        }
        let mut type_distribution: HashMap<String, u64> = HashMap::new();
        let mut total_duration_ms: i64 = 0;
        let mut recovered: u64 = 0;
        for record in &self.interruption_records {
            if let Some(obj) = record.as_object() {
                if let Some(typ) = obj.get("type").and_then(|v| v.as_str()) {
                    *type_distribution.entry(typ.to_string()).or_insert(0) += 1;
                }
                if let Some(dur) = obj.get("duration_ms").and_then(|v| v.as_i64()) {
                    total_duration_ms += dur;
                }
                if let Some(rec) = obj.get("recovered").and_then(|v| v.as_bool()) {
                    if rec {
                        recovered += 1;
                    }
                }
            }
        }
        WorkflowInterruptionStatistics {
            total,
            type_distribution,
            avg_duration_ms: if total > 0 {
                (total_duration_ms as f64 / total as f64) as i64
            } else {
                0
            },
            recovery_rate: if total > 0 {
                recovered as f64 / total as f64
            } else {
                0.0
            },
        }
    }
}

impl StateManager<WorkflowExecutionStateSnapshot> for WorkflowExecutionState {
    async fn cleanup(&mut self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.completed_nodes.clear();
        self.error = None;
        Ok(())
    }

    async fn create_snapshot(
        &self,
    ) -> Result<WorkflowExecutionStateSnapshot, wf_execution_shared::error::ExecutionSharedError>
    {
        Ok(WorkflowExecutionStateSnapshot {
            status: self.status.clone(),
            current_node_id: self.current_node_id.clone(),
            completed_nodes: self.completed_nodes.clone(),
            node_execution_history: self.node_execution_history.clone(),
            start_time: self.start_time,
            end_time: self.end_time,
            error: self.error.clone(),
            error_records: self.error_records.clone(),
            operation_state: self.operation_state.clone(),
            interruption_records: self.interruption_records.clone(),
            event_records: self.event_records.clone(),
            timeout_count: self.timeout_count,
        })
    }

    async fn restore_from_snapshot(
        &mut self,
        snapshot: WorkflowExecutionStateSnapshot,
    ) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.status = snapshot.status;
        self.current_node_id = snapshot.current_node_id;
        self.completed_nodes = snapshot.completed_nodes;
        self.node_execution_history = snapshot.node_execution_history;
        self.start_time = snapshot.start_time;
        self.end_time = snapshot.end_time;
        self.error = snapshot.error;
        self.error_records = snapshot.error_records;
        self.operation_state = snapshot.operation_state;
        self.interruption_records = snapshot.interruption_records;
        self.event_records = snapshot.event_records;
        self.timeout_count = snapshot.timeout_count;
        Ok(())
    }

    fn size(&self) -> usize {
        self.completed_nodes.len()
    }

    fn is_empty(&self) -> bool {
        self.completed_nodes.is_empty()
    }
}
