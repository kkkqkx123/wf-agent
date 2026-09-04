use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::Value;

use wf_common::error_chain::ErrorRecord;
use wf_common::now;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_execution_shared::types::state_manager::StateManager;
use wf_llm::messaging::conversation_session::{ConversationSession, ConversationState};

use crate::error::AgentResult;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    /// Tool call arguments as passed to the tool executor. Kept for audit /
    /// replay; the agent checkpoint snapshot predates this field, so it
    /// defaults to `Null` when reading older states.
    #[serde(default)]
    pub arguments: serde_json::Value,
    /// Tool result payload when the call completed successfully.
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    /// Raw error message when the call failed; `None` on success.
    #[serde(default)]
    pub error: Option<String>,
    /// The LLM tool call id this record belongs to. Used as the replay
    /// idempotency key: a restored execution re-issuing the same id is
    /// served from `completed_tool_results` instead of re-executing.
    #[serde(default)]
    pub tool_call_id: Option<String>,
    pub duration_ms: i64,
    pub success: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IterationRecord {
    pub iteration: u32,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub tool_call_count: u32,
    pub tool_calls: Vec<ToolCallRecord>,
    /// The assistant's reply content for this iteration (empty for pure
    /// tool-call iterations). Older states have no such field.
    #[serde(default)]
    pub response_content: Option<String>,
    /// LLM calls issued by this iteration (audit trail). Older states
    /// have no such field; the runtime type reuses the persisted
    /// `wf_types` record shape so the audit query sees identical data.
    #[serde(default)]
    pub llm_calls: Vec<wf_types::agent_execution::LlmCallRecord>,
}

/// Runtime tool-discovery tracking: tools formally activated via
/// TOOL_VISIBILITY unblock (gated → activated) and tools first invoked
/// through the `general` tool. Serialized with the agent loop state so
/// checkpoints can replay activation and observability.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolDiscoveryState {
    /// Tools formally activated (gated → activated) via TOOL_VISIBILITY
    /// unblock. Activated tools enter the visible schema.
    pub activated_tools: HashSet<String>,
    /// Tools invoked through the `general` tool (metrics/audit).
    pub discovered_via_general: HashSet<String>,
}

impl ToolDiscoveryState {
    /// Mark a tool as formally activated. Returns `true` when the tool was
    /// not previously activated (a state change for metrics/events).
    pub fn activate_tool(&mut self, name: &str) -> bool {
        self.activated_tools.insert(name.to_string())
    }

    /// Record that a tool was invoked through `general`. Returns `true` on
    /// first discovery (a state change for metrics/events).
    pub fn record_general_discovery(&mut self, name: &str) -> bool {
        self.discovered_via_general.insert(name.to_string())
    }

    pub fn is_activated(&self, name: &str) -> bool {
        self.activated_tools.contains(name)
    }
}

/// A single entry in a variable's change history.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VariableHistoryEntry {
    pub value: serde_json::Value,
    pub timestamp: i64,
    pub source: String,
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
    /// Interruption records for audit and diagnostics.
    #[serde(default)]
    pub interruption_records: Vec<serde_json::Value>,
    /// Event records for audit and diagnostics.
    #[serde(default)]
    pub event_records: Vec<serde_json::Value>,
    pub variable_snapshots: HashMap<String, Value>,
    pub tool_discovery: ToolDiscoveryState,
    /// Tool call ids still in flight when the snapshot was taken; a restored
    /// execution re-executes them (they were never completed).
    #[serde(default)]
    pub pending_tool_calls: HashSet<String>,
    /// Completed tool call id -> cached result payload. The replay
    /// idempotency table: after a restore, a replayed tool call with a
    /// cached id returns the cached result without executing the tool again.
    #[serde(default)]
    pub completed_tool_results: HashMap<String, Value>,
    /// Locked tool call format once the first LLM request is assembled.
    /// Ensures checkpoint restore uses the same protocol format as the
    /// original run.
    #[serde(default)]
    pub locked_tool_call_format: Option<wf_types::llm::ToolCallFormatConfig>,
    /// Number of timeout events that have occurred.
    #[serde(default)]
    pub timeout_count: u32,
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
    interruption_records: Vec<serde_json::Value>,
    event_records: Vec<serde_json::Value>,
    variable_snapshots: HashMap<String, Value>,
    variable_history: HashMap<String, Vec<VariableHistoryEntry>>,
    tool_discovery: ToolDiscoveryState,
    pending_tool_calls: HashSet<String>,
    completed_tool_results: HashMap<String, Value>,
    locked_tool_call_format: Option<wf_types::llm::ToolCallFormatConfig>,
    timeout_count: u32,
    /// Streaming message buffer: content accumulated while streaming.
    streaming_message_buffer: Option<String>,
    /// Whether the stream is currently active.
    is_streaming: bool,
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
            interruption_records: Vec::new(),
            event_records: Vec::new(),
            variable_snapshots: HashMap::new(),
            variable_history: HashMap::new(),
            tool_discovery: ToolDiscoveryState::default(),
            pending_tool_calls: HashSet::new(),
            completed_tool_results: HashMap::new(),
            locked_tool_call_format: None,
            timeout_count: 0,
            streaming_message_buffer: None,
            is_streaming: false,
        }
    }

    pub fn with_activated_tools(mut self, activated: HashSet<String>) -> Self {
        self.tool_discovery.activated_tools = activated;
        self
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

    /// Live tool discovery state (activated tools / general discoveries).
    pub fn tool_discovery(&self) -> &ToolDiscoveryState {
        &self.tool_discovery
    }

    /// Mutably borrow the tool discovery state for a state transition.
    pub fn tool_discovery_mut(&mut self) -> &mut ToolDiscoveryState {
        &mut self.tool_discovery
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

    pub fn start_iteration(&mut self) {
        self.current_iteration += 1;
        self.iteration_history.push(IterationRecord {
            iteration: self.current_iteration,
            start_time: now(),
            end_time: None,
            tool_call_count: 0,
            tool_calls: Vec::new(),
            response_content: None,
            llm_calls: Vec::new(),
        });
    }

    pub fn end_iteration(&mut self) {
        self.end_iteration_with_content(None);
    }

    pub fn end_iteration_with_content(&mut self, response_content: Option<String>) {
        if let Some(record) = self.iteration_history.last_mut() {
            record.end_time = Some(now());
            record.response_content = response_content;
        }
    }

    pub fn record_tool_call(&mut self, name: &str, duration_ms: i64, success: bool) {
        self.record_tool_call_with_details(ToolCallRecord {
            name: name.to_string(),
            arguments: serde_json::Value::Null,
            result: None,
            error: None,
            tool_call_id: None,
            duration_ms,
            success,
        });
    }

    /// Record a tool call with its full audit payload (arguments, result,
    /// error, tool call id). The argument-free variant keeps callers that
    /// only track counts/durations working unchanged.
    pub fn record_tool_call_with_details(&mut self, record: ToolCallRecord) {
        self.tool_call_count += 1;
        if let Some(iteration) = self.iteration_history.last_mut() {
            iteration.tool_call_count += 1;
            iteration.tool_calls.push(record);
        }
    }

    /// Record a completed (or failed) LLM call into the current iteration's
    /// audit trail. The per-iteration `seq` is assigned here so
    /// callers never have to track call counts.
    pub fn record_llm_call(&mut self, mut call: wf_types::agent_execution::LlmCallRecord) {
        if let Some(record) = self.iteration_history.last_mut() {
            call.seq = record.llm_calls.len() as u32;
            record.llm_calls.push(call);
        }
    }

    /// Mark a tool call as in flight. The id stays pending until
    /// [`Self::finish_tool_call`] runs, so a checkpoint taken mid-execution
    /// records it as pending and the restored run re-executes it.
    pub fn begin_tool_call(&mut self, tool_call_id: &str) {
        self.pending_tool_calls.insert(tool_call_id.to_string());
    }

    /// Clear the in-flight marker and cache the result for replay
    /// idempotency. Successful results are cached; failed calls are not, so
    /// a retry after restore re-executes the tool.
    pub fn finish_tool_call(&mut self, tool_call_id: &str, result: Option<Value>) {
        self.pending_tool_calls.remove(tool_call_id);
        if let Some(result) = result {
            self.completed_tool_results
                .insert(tool_call_id.to_string(), result);
        }
    }

    /// Tool call ids still in flight (checkpoint visibility).
    pub fn pending_tool_calls(&self) -> &HashSet<String> {
        &self.pending_tool_calls
    }

    /// Whether a tool call id already produced a result (replay idempotency).
    pub fn has_completed_tool_call(&self, tool_call_id: &str) -> bool {
        self.completed_tool_results.contains_key(tool_call_id)
    }

    /// Cached result of a completed tool call id, if any.
    pub fn completed_tool_result(&self, tool_call_id: &str) -> Option<Value> {
        self.completed_tool_results.get(tool_call_id).cloned()
    }

    pub fn record_error(&mut self, record: ErrorRecord) {
        self.error_records.push(record);
    }

    pub fn set_variable_snapshot(&mut self, name: String, value: Value) {
        self.variable_snapshots.insert(name, value);
    }

    /// Apply a status transition with a source-state guard. Illegal
    /// transitions (e.g. `Completed -> Paused`) return an error instead of
    /// silently corrupting the machine. All status changes in the agent loop
    /// go through this single entry point.
    pub fn transition(&mut self, target: ExecutionStatus) -> AgentResult<()> {
        let source = self.status.clone();
        if !Self::transition_allowed(&source, &target) {
            return Err(crate::error::AgentError::IllegalStateTransition(format!(
                "{source:?} -> {target:?}"
            )));
        }
        self.apply(target);
        Ok(())
    }

    /// The legal transition table. `Timeout` is a terminal state reached when
    /// a wall-clock or pause budget expires; explicit `stop`/`cancel` land on
    /// `Cancelled`. Terminal states (Completed/Failed/Cancelled/Stopped/
    /// Timeout) never transition again.
    fn transition_allowed(source: &ExecutionStatus, target: &ExecutionStatus) -> bool {
        use ExecutionStatus::*;
        matches!(
            (source, target),
            (Created, Running)
                | (Created, Paused)
                | (Created, Cancelled)
                | (Created, Stopped)
                // Idempotent re-entry: a checkpoint restore rebuilds the entity
                // in its snapshotted `Running` state and re-drives the loop, so
                // `start` must be legal from `Running` (it only resets nothing
                // and re-fires the start transition).
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
                    self.start_time = now();
                }
                self.status = ExecutionStatus::Running;
            }
            ExecutionStatus::Paused => {
                self.status = ExecutionStatus::Paused;
            }
            ExecutionStatus::Completed => {
                self.status = ExecutionStatus::Completed;
                self.end_time = Some(now());
            }
            ExecutionStatus::Failed => {
                self.status = ExecutionStatus::Failed;
                self.end_time = Some(now());
            }
            ExecutionStatus::Cancelled | ExecutionStatus::Stopped | ExecutionStatus::Timeout => {
                self.status = target;
                self.end_time = Some(now());
            }
            ExecutionStatus::Created => {}
        }
    }

    pub fn start(&mut self) -> AgentResult<()> {
        self.transition(ExecutionStatus::Running)
    }

    pub fn pause(&mut self) -> AgentResult<()> {
        self.transition(ExecutionStatus::Paused)
    }

    pub fn resume(&mut self) -> AgentResult<()> {
        self.transition(ExecutionStatus::Running)
    }

    pub fn complete(&mut self) -> AgentResult<()> {
        self.transition(ExecutionStatus::Completed)
    }

    pub fn fail(&mut self, error: String) -> AgentResult<()> {
        self.transition(ExecutionStatus::Failed)?;
        self.error = Some(error);
        Ok(())
    }

    pub fn cancel(&mut self) -> AgentResult<()> {
        self.transition(ExecutionStatus::Cancelled)
    }

    pub fn timeout(&mut self) -> AgentResult<()> {
        self.transition(ExecutionStatus::Timeout)
    }

    // ── Tool Call Format persistence ──────────────────────────────────────

    pub fn locked_tool_call_format(&self) -> Option<&wf_types::llm::ToolCallFormatConfig> {
        self.locked_tool_call_format.as_ref()
    }

    pub fn set_locked_tool_call_format(&mut self, format: wf_types::llm::ToolCallFormatConfig) {
        self.locked_tool_call_format = Some(format);
    }

    // ── Timeout counting ──────────────────────────────────────────────────

    /// Return the total number of timeout events that have occurred.
    pub fn timeout_count(&self) -> u32 {
        self.timeout_count
    }

    /// Increment the timeout counter by one.
    pub fn increment_timeout_count(&mut self) {
        self.timeout_count += 1;
    }

    // ── Streaming message buffer ─────────────────────────────────────────

    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    pub fn start_streaming(&mut self) {
        self.is_streaming = true;
        self.streaming_message_buffer = Some(String::new());
    }

    pub fn update_stream_message(&mut self, delta: &str) {
        if let Some(ref mut buf) = self.streaming_message_buffer {
            buf.push_str(delta);
        } else {
            self.streaming_message_buffer = Some(delta.to_string());
        }
    }

    pub fn end_streaming(&mut self) {
        self.is_streaming = false;
        // Keep the buffer for checkpoint/snapshot, cleared on next start.
    }

    pub fn streaming_message_buffer(&self) -> Option<&str> {
        self.streaming_message_buffer.as_deref()
    }

    /// Consume and clear the streaming message buffer.
    pub fn take_streaming_message(&mut self) -> Option<String> {
        self.streaming_message_buffer.take()
    }

    // ── Variable snapshot history ────────────────────────────────────────

    /// Set a variable snapshot and record its history entry.
    pub fn set_variable_snapshot_with_history(
        &mut self,
        name: String,
        value: Value,
        source: String,
    ) {
        self.variable_snapshots.insert(name.clone(), value.clone());
        let entry = VariableHistoryEntry {
            value,
            timestamp: now(),
            source,
        };
        self.variable_history.entry(name).or_default().push(entry);
    }

    /// Get the change history for a specific variable.
    pub fn get_variable_history(&self, name: &str) -> Vec<&VariableHistoryEntry> {
        self.variable_history
            .get(name)
            .map(|entries| entries.iter().collect())
            .unwrap_or_default()
    }

    /// Prune variable history for all variables to the most recent N entries.
    pub fn prune_variable_history(&mut self, max_entries: usize) {
        for entries in self.variable_history.values_mut() {
            if entries.len() > max_entries {
                let start = entries.len() - max_entries;
                entries.drain(0..start);
            }
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

    /// Interruption statistics: total count, type distribution, average
    /// duration, and recovery rate.
    pub fn interruption_statistics(&self) -> InterruptionStatistics {
        let total = self.interruption_records.len() as u64;
        if total == 0 {
            return InterruptionStatistics::default();
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
        InterruptionStatistics {
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

/// Statistics about interruption events during execution.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct InterruptionStatistics {
    pub total: u64,
    pub type_distribution: HashMap<String, u64>,
    pub avg_duration_ms: i64,
    pub recovery_rate: f64,
}

impl StateManager<AgentLoopStateSnapshot> for AgentLoopState {
    async fn cleanup(&mut self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.iteration_history.clear();
        self.error = None;
        self.error_records.clear();
        self.variable_snapshots.clear();
        Ok(())
    }

    async fn create_snapshot(
        &self,
    ) -> Result<AgentLoopStateSnapshot, wf_execution_shared::error::ExecutionSharedError> {
        Ok(AgentLoopStateSnapshot {
            status: self.status.clone(),
            current_iteration: self.current_iteration,
            tool_call_count: self.tool_call_count,
            iteration_history: self.iteration_history.clone(),
            start_time: self.start_time,
            end_time: self.end_time,
            error: self.error.clone(),
            error_records: self.error_records.clone(),
            interruption_records: self.interruption_records.clone(),
            event_records: self.event_records.clone(),
            variable_snapshots: self.variable_snapshots.clone(),
            tool_discovery: self.tool_discovery.clone(),
            pending_tool_calls: self.pending_tool_calls.clone(),
            completed_tool_results: self.completed_tool_results.clone(),
            locked_tool_call_format: self.locked_tool_call_format.clone(),
            timeout_count: self.timeout_count,
        })
    }

    async fn restore_from_snapshot(
        &mut self,
        snapshot: AgentLoopStateSnapshot,
    ) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.status = snapshot.status;
        self.current_iteration = snapshot.current_iteration;
        self.tool_call_count = snapshot.tool_call_count;
        self.iteration_history = snapshot.iteration_history;
        self.start_time = snapshot.start_time;
        self.end_time = snapshot.end_time;
        self.error = snapshot.error;
        self.error_records = snapshot.error_records;
        self.interruption_records = snapshot.interruption_records;
        self.event_records = snapshot.event_records;
        self.variable_snapshots = snapshot.variable_snapshots;
        self.tool_discovery = snapshot.tool_discovery;
        self.pending_tool_calls = snapshot.pending_tool_calls;
        self.completed_tool_results = snapshot.completed_tool_results;
        self.locked_tool_call_format = snapshot.locked_tool_call_format;
        self.timeout_count = snapshot.timeout_count;
        Ok(())
    }

    fn size(&self) -> usize {
        self.iteration_history.len()
    }

    fn is_empty(&self) -> bool {
        self.iteration_history.is_empty()
    }
}

pub struct AgentStateCoordinator {
    session: Arc<tokio::sync::RwLock<ConversationSession>>,
}

impl AgentStateCoordinator {
    pub fn new(session: Arc<tokio::sync::RwLock<ConversationSession>>) -> Self {
        Self { session }
    }

    pub async fn add_message(&self, message: wf_types::message::Message) {
        self.session.write().await.add_message(message);
    }

    pub async fn messages(&self) -> Vec<wf_types::message::Message> {
        self.session.read().await.messages().to_vec()
    }

    pub async fn snapshot(&self) -> AgentResult<ConversationState> {
        self.session
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(Into::into)
    }

    pub async fn restore(&self, state: ConversationState) -> AgentResult<()> {
        self.session
            .write()
            .await
            .restore_from_snapshot(state)
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_discovery_state_serializes_and_restores() {
        let mut state = AgentLoopState::new();
        state.tool_discovery_mut().activate_tool("write_file");
        state.tool_discovery_mut().activate_tool("edit_file");
        state
            .tool_discovery_mut()
            .record_general_discovery("web_search");

        let json = serde_json::to_string(&state.tool_discovery()).unwrap();
        let restored: ToolDiscoveryState = serde_json::from_str(&json).unwrap();
        assert!(restored.is_activated("write_file"));
        assert!(restored.is_activated("edit_file"));
        assert!(restored.discovered_via_general.contains("web_search"));
        assert!(!restored.is_activated("web_search"));

        // Second activation of the same tool is not a state change.
        let mut discovery = restored.clone();
        assert!(!discovery.activate_tool("write_file"));
        assert!(discovery.activate_tool("shell"));
    }

    #[tokio::test]
    async fn snapshot_roundtrip_preserves_tool_discovery() {
        let mut state = AgentLoopState::new();
        state.start().unwrap();
        state.tool_discovery_mut().activate_tool("write_file");

        let snapshot = state.create_snapshot().await.unwrap();
        let mut restored = AgentLoopState::new();
        restored.restore_from_snapshot(snapshot).await.unwrap();
        assert!(restored.tool_discovery().is_activated("write_file"));
    }

    #[tokio::test]
    async fn pending_and_completed_tool_calls_survive_snapshot_roundtrip() {
        let mut state = AgentLoopState::new();
        state.start().unwrap();
        state.start_iteration();

        state.begin_tool_call("tc-1");
        state.begin_tool_call("tc-2");
        assert_eq!(state.pending_tool_calls().len(), 2);

        state.finish_tool_call("tc-1", Some(serde_json::json!({"ok": true})));
        assert!(!state.pending_tool_calls().contains("tc-1"));
        assert!(state.pending_tool_calls().contains("tc-2"));
        assert!(state.has_completed_tool_call("tc-1"));
        assert_eq!(
            state.completed_tool_result("tc-1"),
            Some(serde_json::json!({"ok": true}))
        );

        let snapshot = state.create_snapshot().await.unwrap();
        let mut restored = AgentLoopState::new();
        restored.restore_from_snapshot(snapshot).await.unwrap();
        assert_eq!(
            restored.pending_tool_calls().len(),
            1,
            "tc-2 still in flight"
        );
        assert!(
            restored.has_completed_tool_call("tc-1"),
            "idempotency table restored"
        );
        assert!(restored.pending_tool_calls().contains("tc-2"));
    }

    #[tokio::test]
    async fn failed_tool_call_is_not_cached_for_replay() {
        let mut state = AgentLoopState::new();
        state.start_iteration();
        state.begin_tool_call("tc-fail");
        state.finish_tool_call("tc-fail", None);
        assert!(!state.has_completed_tool_call("tc-fail"));
        assert!(state.pending_tool_calls().is_empty());
    }

    #[test]
    fn transition_table_enforces_legal_moves() {
        use crate::error::AgentError;

        // Legal transitions.
        let mut state = AgentLoopState::new();
        state.start().unwrap();
        state.pause().unwrap();
        assert!(state.is_paused());
        state.resume().unwrap();
        state.complete().unwrap();
        assert!(state.is_completed());

        // Illegal transitions from a terminal state are rejected.
        let err = state.pause().unwrap_err();
        assert!(matches!(err, AgentError::IllegalStateTransition(_)));
        let err = state.resume().unwrap_err();
        assert!(matches!(err, AgentError::IllegalStateTransition(_)));

        // Failed -> Running is rejected.
        let mut state = AgentLoopState::new();
        state.start().unwrap();
        state.fail("boom".to_string()).unwrap();
        assert!(state.is_failed());
        let err = state.resume().unwrap_err();
        assert!(matches!(err, AgentError::IllegalStateTransition(_)));

        // Completed can be reached only from Running.
        let mut state = AgentLoopState::new();
        let err = state.complete().unwrap_err();
        assert!(matches!(err, AgentError::IllegalStateTransition(_)));
    }

    /// Idempotent re-entry: a checkpoint restore rebuilds the entity in its
    /// snapshotted `Running` state and re-drives the loop through `start`.
    /// The guard must accept `Running -> Running` without resetting the run
    /// clock or the iteration history.
    #[test]
    fn running_to_running_is_an_idempotent_restart() {
        let mut state = AgentLoopState::new();
        let first_start = {
            state.start().unwrap();
            state.start_time()
        };
        // Simulate a mid-run checkpoint: one iteration already completed.
        state.start_iteration();
        state.end_iteration();

        // Restore re-drive: start() from Running succeeds and is a no-op.
        state.start().unwrap();
        assert!(state.is_running());
        assert_eq!(
            state.start_time(),
            first_start,
            "idempotent start must not reset the run clock"
        );
        assert_eq!(state.current_iteration(), 1);
    }

    #[test]
    fn pause_timeout_and_stop_land_on_distinct_terminal_states() {
        let mut state = AgentLoopState::new();
        state.start().unwrap();
        state.timeout().unwrap();
        assert_eq!(state.status(), ExecutionStatus::Timeout);
        assert!(state.end_time().is_some());

        // A paused loop that hits its pause budget transitions to Timeout.
        let mut state = AgentLoopState::new();
        state.start().unwrap();
        state.pause().unwrap();
        state.timeout().unwrap();
        assert_eq!(state.status(), ExecutionStatus::Timeout);

        // Stop lands on Cancelled from any non-terminal state.
        let mut state = AgentLoopState::new();
        state.cancel().unwrap();
        assert_eq!(state.status(), ExecutionStatus::Cancelled);
    }
}
