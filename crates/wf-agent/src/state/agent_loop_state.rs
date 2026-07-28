use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use wf_common::now;
use wf_execution_shared::error_chain::manager::ErrorRecord;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_execution_shared::types::state_manager::StateManager;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IterationRecord {
    pub iteration: u32,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub tool_call_count: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentLoopStateSnapshot {
    pub status: ExecutionStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    pub iteration_history: Vec<IterationRecord>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub error_records: Vec<ErrorRecord>,
    pub variable_snapshots: HashMap<String, Value>,
}

pub struct AgentLoopState {
    status: ExecutionStatus,
    current_iteration: u32,
    tool_call_count: u32,
    iteration_history: Vec<IterationRecord>,
    start_time: i64,
    end_time: Option<i64>,
    error: Option<String>,
    error_records: Vec<ErrorRecord>,
    variable_snapshots: HashMap<String, Value>,
}

impl Default for AgentLoopState {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentLoopState {
    pub fn new() -> Self {
        Self {
            status: ExecutionStatus::Created,
            current_iteration: 0,
            tool_call_count: 0,
            iteration_history: Vec::new(),
            start_time: now(),
            end_time: None,
            error: None,
            error_records: Vec::new(),
            variable_snapshots: HashMap::new(),
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
        matches!(self.status, ExecutionStatus::Cancelled | ExecutionStatus::Stopped)
    }

    pub fn current_iteration(&self) -> u32 {
        self.current_iteration
    }

    pub fn tool_call_count(&self) -> u32 {
        self.tool_call_count
    }

    pub fn iteration_history(&self) -> &[IterationRecord] {
        &self.iteration_history
    }

    pub fn error_records(&self) -> &[ErrorRecord] {
        &self.error_records
    }

    pub fn variable_snapshots(&self) -> &HashMap<String, Value> {
        &self.variable_snapshots
    }

    pub fn start_time(&self) -> i64 {
        self.start_time
    }

    pub fn end_time(&self) -> Option<i64> {
        self.end_time
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn start(&mut self) {
        self.status = ExecutionStatus::Running;
        self.start_time = now();
    }

    pub fn start_iteration(&mut self) {
        self.current_iteration += 1;
        self.iteration_history.push(IterationRecord {
            iteration: self.current_iteration,
            start_time: now(),
            end_time: None,
            tool_call_count: 0,
        });
    }

    pub fn end_iteration(&mut self) {
        if let Some(record) = self.iteration_history.last_mut() {
            record.end_time = Some(now());
        }
    }

    pub fn record_tool_call(&mut self) {
        self.tool_call_count += 1;
        if let Some(record) = self.iteration_history.last_mut() {
            record.tool_call_count += 1;
        }
    }

    pub fn record_error(&mut self, record: ErrorRecord) {
        self.error_records.push(record);
    }

    pub fn set_variable_snapshot(&mut self, name: String, value: Value) {
        self.variable_snapshots.insert(name, value);
    }

    pub fn pause(&mut self) {
        self.status = ExecutionStatus::Paused;
    }

    pub fn resume(&mut self) {
        self.status = ExecutionStatus::Running;
    }

    pub fn complete(&mut self) {
        self.status = ExecutionStatus::Completed;
        self.end_time = Some(now());
    }

    pub fn fail(&mut self, error: String) {
        self.status = ExecutionStatus::Failed;
        self.end_time = Some(now());
        self.error = Some(error);
    }

    pub fn cancel(&mut self) {
        self.status = ExecutionStatus::Cancelled;
        self.end_time = Some(now());
    }
}

#[async_trait]
impl StateManager<AgentLoopStateSnapshot> for AgentLoopState {
    async fn cleanup(&mut self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.iteration_history.clear();
        self.error = None;
        self.error_records.clear();
        self.variable_snapshots.clear();
        Ok(())
    }

    async fn create_snapshot(&self) -> Result<AgentLoopStateSnapshot, wf_execution_shared::error::ExecutionSharedError> {
        Ok(AgentLoopStateSnapshot {
            status: self.status.clone(),
            current_iteration: self.current_iteration,
            tool_call_count: self.tool_call_count,
            iteration_history: self.iteration_history.clone(),
            start_time: self.start_time,
            end_time: self.end_time,
            error: self.error.clone(),
            error_records: self.error_records.clone(),
            variable_snapshots: self.variable_snapshots.clone(),
        })
    }

    async fn restore_from_snapshot(&mut self, snapshot: AgentLoopStateSnapshot) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.status = snapshot.status;
        self.current_iteration = snapshot.current_iteration;
        self.tool_call_count = snapshot.tool_call_count;
        self.iteration_history = snapshot.iteration_history;
        self.start_time = snapshot.start_time;
        self.end_time = snapshot.end_time;
        self.error = snapshot.error;
        self.error_records = snapshot.error_records;
        self.variable_snapshots = snapshot.variable_snapshots;
        Ok(())
    }

    fn size(&self) -> usize {
        self.iteration_history.len()
    }

    fn is_empty(&self) -> bool {
        self.iteration_history.is_empty()
    }
}
