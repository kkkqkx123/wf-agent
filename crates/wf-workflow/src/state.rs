use wf_common::error_chain::ErrorRecord;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_execution_shared::types::state_manager::StateManager;
use wf_types::checkpoint::workflow::snapshot::OperationState;

/// One node execution attempt (retries produce independent records).
///
/// `end_time` is `None` for executions still in flight; only records with
/// `end_time` set participate in duration statistics.
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
    /// Aggregated from the agent_loop node's `metadata["performance"].total_tool_calls`
    /// at record time; non-agent nodes contribute 0.
    pub tool_call_count: u32,
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

    pub fn start(&mut self) {
        self.status = ExecutionStatus::Running;
        self.start_time = wf_common::now();
    }

    pub fn pause(&mut self) {
        self.status = ExecutionStatus::Paused;
    }

    pub fn resume(&mut self) {
        self.status = ExecutionStatus::Running;
    }

    pub fn complete(&mut self) {
        self.status = ExecutionStatus::Completed;
        self.end_time = Some(wf_common::now());
    }

    pub fn fail(&mut self, error: String) {
        self.status = ExecutionStatus::Failed;
        self.end_time = Some(wf_common::now());
        self.error = Some(error);
    }

    pub fn cancel(&mut self) {
        self.status = ExecutionStatus::Cancelled;
        self.end_time = Some(wf_common::now());
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
        Ok(())
    }

    fn size(&self) -> usize {
        self.completed_nodes.len()
    }

    fn is_empty(&self) -> bool {
        self.completed_nodes.is_empty()
    }
}
