use std::collections::HashMap;

use wf_common::error_chain::ErrorRecord;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_execution_shared::types::state_manager::StateManager;
use wf_types::checkpoint::workflow::snapshot::OperationState;
use wf_types::workflow::error_branch::ErrorSuspendState;

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
    /// Pending error-branch suspend record, if the run parked at a suspend
    /// point. Consumed (cleared) when a resumed run re-enters the branch.
    #[serde(default)]
    pub error_suspend: Option<ErrorSuspendState>,
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
    error_suspend: Option<ErrorSuspendState>,
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
            error_suspend: None,
        }
    }

    pub fn status(&self) -> ExecutionStatus {
        self.status.clone()
    }

    /// Wall-clock start (milliseconds since epoch) recorded when the
    /// execution first transitioned to `Running`. A checkpoint capture uses
    /// it to freeze how much of the wall-clock budget the original run had
    /// already spent, so a restore can hand the continuation only the
    /// remaining budget instead of a fresh full one.
    pub fn start_time(&self) -> i64 {
        self.start_time
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

    /// Pending error-branch suspend record (parked at a suspend point).
    pub fn error_suspend(&self) -> Option<&ErrorSuspendState> {
        self.error_suspend.as_ref()
    }

    pub fn set_error_suspend(&mut self, state: Option<ErrorSuspendState>) {
        self.error_suspend = state;
    }

    /// Consume the pending suspend record so a resumed run does not
    /// re-persist it.
    pub fn take_error_suspend(&mut self) -> Option<ErrorSuspendState> {
        self.error_suspend.take()
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
            error_suspend: self.error_suspend.clone(),
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
        self.error_suspend = snapshot.error_suspend;
        Ok(())
    }

    fn size(&self) -> usize {
        self.completed_nodes.len()
    }

    fn is_empty(&self) -> bool {
        self.completed_nodes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(node_id: &str) -> NodeExecutionRecord {
        NodeExecutionRecord {
            node_id: node_id.to_string(),
            node_name: node_id.to_string(),
            node_type: "script".to_string(),
            start_time: 1_000,
            end_time: Some(1_500),
            success: true,
            error: None,
            input: None,
            result: Some(serde_json::json!({"ok": true})),
            branch_id: None,
        }
    }

    fn error_record(message: &str) -> ErrorRecord {
        ErrorRecord::new("exec-1".to_string(), message.to_string(), None, None, None)
    }

    #[test]
    fn start_sets_running_and_records_start_time() {
        let mut state = WorkflowExecutionState::new();
        assert_eq!(state.status(), ExecutionStatus::Created);
        state.start().expect("start must succeed");
        assert!(state.is_running());
        assert!(state.start_time > 0);
        assert!(!state.is_completed());
        assert!(!state.is_failed());
        assert!(!state.is_cancelled());
        assert!(!state.is_paused());
    }

    #[test]
    fn illegal_transitions_are_rejected() {
        let mut state = WorkflowExecutionState::new();
        // Created -> Completed is not in the transition table.
        assert!(state.complete().is_err());
        // Running -> Completed is legal, then terminal states never transition.
        state.start().expect("start must succeed");
        state.complete().expect("complete must succeed");
        assert!(state.is_completed());
        assert!(state.pause().is_err());
        assert!(state.fail("late".to_string()).is_err());
        assert!(state.cancel().is_err());
        assert!(state.start().is_err());
    }

    #[test]
    fn pause_resume_cycle_is_legal() {
        let mut state = WorkflowExecutionState::new();
        state.start().expect("start");
        state.pause().expect("pause");
        assert!(state.is_paused());
        state.resume().expect("resume");
        assert!(state.is_running());
    }

    #[test]
    fn fail_and_timeout_record_error_and_end_time() {
        let mut state = WorkflowExecutionState::new();
        state.start().expect("start");
        state.fail("boom".to_string()).expect("fail");
        assert!(state.is_failed());
        assert_eq!(state.error.as_deref(), Some("boom"));
        assert!(state.end_time.is_some());

        let mut state2 = WorkflowExecutionState::new();
        state2.start().expect("start");
        state2.timeout("too slow".to_string()).expect("timeout");
        assert_eq!(state2.status, ExecutionStatus::Timeout);
        assert_eq!(state2.error.as_deref(), Some("too slow"));
    }

    #[test]
    fn running_to_running_is_idempotent_for_checkpoint_resume() {
        let mut state = WorkflowExecutionState::new();
        state.start().expect("start");
        let first = state.start_time;
        state.start().expect("re-start while running must succeed");
        assert!(state.is_running());
        assert_eq!(state.start_time, first, "start time must not be reset");
    }

    #[test]
    fn node_execution_records_are_tracked_and_restorable() {
        let mut state = WorkflowExecutionState::new();
        state.mark_node_completed("n1".to_string());
        state.record_node_execution(record("n1"));
        assert_eq!(state.completed_nodes, &["n1".to_string()]);
        assert_eq!(state.node_execution_history.len(), 1);

        let mut restored = WorkflowExecutionState::new();
        restored.restore_node_execution_history(vec![record("n1"), record("n2")]);
        assert_eq!(restored.node_execution_history.len(), 2);
        assert_eq!(restored.node_execution_history[1].node_id, "n2");
    }

    #[test]
    fn error_records_are_appended() {
        let mut state = WorkflowExecutionState::new();
        state.add_error_record(error_record("first"));
        state.add_error_record(error_record("second"));
        assert_eq!(state.error_records.len(), 2);
        assert_eq!(state.error_records[0].error, "first");
    }

    #[test]
    fn interruption_statistics_are_derived_from_records() {
        let mut state = WorkflowExecutionState::new();
        // No records -> default (zeroed) statistics.
        let empty = state.interruption_statistics();
        assert_eq!(empty.total, 0);
        assert_eq!(empty.recovery_rate, 0.0);

        state.record_interruption(serde_json::json!({
            "type": "pause", "duration_ms": 100, "recovered": true
        }));
        state.record_interruption(serde_json::json!({
            "type": "error_suspend", "duration_ms": 300, "recovered": false
        }));
        state.record_event(serde_json::json!({"type": "custom"}));

        let stats = state.interruption_statistics();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.type_distribution.get("pause"), Some(&1));
        assert_eq!(stats.type_distribution.get("error_suspend"), Some(&1));
        assert_eq!(stats.avg_duration_ms, 200);
        assert!((stats.recovery_rate - 0.5).abs() < f64::EPSILON);
        assert_eq!(state.event_records.len(), 1);
    }

    #[test]
    fn timeout_counter_increments() {
        let mut state = WorkflowExecutionState::new();
        assert_eq!(state.timeout_count, 0);
        state.increment_timeout_count();
        state.increment_timeout_count();
        assert_eq!(state.timeout_count, 2);
    }

    #[tokio::test]
    async fn snapshot_and_restore_round_trip_preserves_all_fields() {
        let mut state = WorkflowExecutionState::new();
        state.start().expect("start");
        state.set_current_node(Some("node_a".to_string()));
        state.mark_node_completed("node_a".to_string());
        state.record_node_execution(record("node_a"));
        state.add_error_record(error_record("e1"));
        state.record_interruption(serde_json::json!({"type": "pause"}));
        state.record_event(serde_json::json!({"kind": "event"}));
        state.increment_timeout_count();

        let snapshot = state.create_snapshot().await.expect("snapshot");
        assert_eq!(snapshot.status, ExecutionStatus::Running);
        assert_eq!(snapshot.current_node_id.as_deref(), Some("node_a"));
        assert_eq!(snapshot.completed_nodes, vec!["node_a".to_string()]);
        assert_eq!(snapshot.timeout_count, 1);

        let mut restored = WorkflowExecutionState::new();
        restored
            .restore_from_snapshot(snapshot)
            .await
            .expect("restore");
        assert_eq!(restored.status, ExecutionStatus::Running);
        assert_eq!(restored.current_node_id(), Some("node_a"));
        assert_eq!(restored.completed_nodes(), &["node_a".to_string()]);
        assert_eq!(restored.node_execution_history().len(), 1);
        assert_eq!(restored.error_records().len(), 1);
        assert_eq!(restored.interruption_records().len(), 1);
        assert_eq!(restored.event_records().len(), 1);
        assert_eq!(restored.timeout_count(), 1);
    }

    #[tokio::test]
    async fn cleanup_clears_progress_but_keeps_status() {
        let mut state = WorkflowExecutionState::new();
        state.start().expect("start");
        state.mark_node_completed("n1".to_string());
        state.fail("err".to_string()).expect("fail");
        assert!(!state.is_empty());

        state.cleanup().await.expect("cleanup");
        assert!(state.is_empty());
        assert_eq!(state.completed_nodes.len(), 0);
        assert_eq!(state.error, None);
        assert!(state.is_failed(), "cleanup must not reset the status");
    }
}
