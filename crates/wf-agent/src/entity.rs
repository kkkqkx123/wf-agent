use std::sync::Arc;

use wf_common::gate::GatePermit;
use wf_core::interruption::{InterruptionSignal, InterruptionState};
use wf_execution_shared::error::ExecutionSharedError;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_execution_shared::types::execution_entity::{ExecutionStatus, IExecutionEntity};
use wf_llm::messaging::conversation_session::ConversationSession;
use wf_types::llm::ToolCallFormatConfig;
use wf_types::Id;

use crate::coordinator::state_transitor::AgentLoopStateTransitor;
use crate::state::AgentLoopState;
use crate::timeout::{AgentTimeoutManager, TimeoutHandle};

pub struct AgentLoopEntity {
    id: Id,
    /// Id of the agent definition this loop runs against (the `agent_id` of
    /// the loop config). Distinct from `id`, which is the per-run loop id.
    definition_id: Id,
    pub state: Arc<tokio::sync::RwLock<AgentLoopState>>,
    interruption: InterruptionState,
    conversation: Arc<tokio::sync::RwLock<ConversationSession>>,
    cancellation: tokio_util::sync::CancellationToken,
    parent_execution_id: Option<Id>,
    child_execution_ids: Arc<tokio::sync::RwLock<Vec<Id>>>,
    hooks: Vec<BaseHookDefinition>,
    model: String,
    tool_call_format: Option<ToolCallFormatConfig>,
    available_tool_names: Vec<String>,
    initial_tool_names: Vec<String>,
    discoverable_tool_names: Vec<String>,
    enable_general_tool: Option<bool>,
    hidden_tool_names: Vec<String>,
    exposure_overrides: Vec<(String, wf_types::tool::ToolExposure)>,
    timeout_manager: AgentTimeoutManager,
    max_pause_duration: Option<u64>,
    pause_timeout_handle: std::sync::RwLock<Option<TimeoutHandle>>,
    /// Depth of this execution in the agent hierarchy (0 = root). Populated
    /// when the run is linked to a parent execution.
    hierarchy_depth: u32,
    /// Root execution id of the hierarchy (own id for a root run). Resolved
    /// when the run is linked to a parent execution.
    root_execution_id: Option<Id>,
    /// Root-to-parent execution id chain (oldest first, excluding self).
    /// Resolved from the parent entity when the run is linked, so deep
    /// hierarchies keep full ancestry across checkpoint restore.
    ancestors: Vec<Id>,
    /// Permit held against the registry's concurrency gate for the duration
    /// of this execution. Released when the execution reaches a terminal
    /// state or when the entity is removed from the registry.
    gate_permit: std::sync::RwLock<Option<GatePermit>>,
}

impl AgentLoopEntity {
    pub fn new(id: Id) -> Self {
        let definition_id = id.clone();
        Self {
            id,
            definition_id,
            state: Arc::new(tokio::sync::RwLock::new(AgentLoopState::new())),
            interruption: InterruptionState::new(),
            conversation: Arc::new(tokio::sync::RwLock::new(ConversationSession::new())),
            cancellation: tokio_util::sync::CancellationToken::new(),
            parent_execution_id: None,
            child_execution_ids: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            hooks: Vec::new(),
            model: String::new(),
            tool_call_format: None,
            available_tool_names: Vec::new(),
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            hidden_tool_names: Vec::new(),
            exposure_overrides: Vec::new(),
            timeout_manager: AgentTimeoutManager::new(),
            max_pause_duration: None,
            pause_timeout_handle: std::sync::RwLock::new(None),
            hierarchy_depth: 0,
            root_execution_id: None,
            ancestors: Vec::new(),
            gate_permit: std::sync::RwLock::new(None),
        }
    }

    pub fn with_parent_execution_id(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }

    /// Set the agent definition id (the `agent_id` of the loop config). The
    /// definition id identifies the agent definition; `id` is the per-run
    /// loop id.
    pub fn with_definition_id(mut self, definition_id: Id) -> Self {
        self.definition_id = definition_id;
        self
    }

    pub fn with_hooks(mut self, hooks: Vec<BaseHookDefinition>) -> Self {
        self.hooks = hooks;
        self
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn with_tool_call_format(mut self, format: ToolCallFormatConfig) -> Self {
        self.tool_call_format = Some(format);
        self
    }

    pub fn with_available_tool_names(mut self, names: Vec<String>) -> Self {
        self.available_tool_names = names;
        self
    }

    /// Tools visible in the initial schema; when absent all available tools
    /// are initially visible.
    pub fn with_initial_tool_names(mut self, names: Vec<String>) -> Self {
        self.initial_tool_names = names;
        self
    }

    /// Discoverable tools: metadata-only injection, invoked via `general`.
    pub fn with_discoverable_tool_names(mut self, names: Vec<String>) -> Self {
        self.discoverable_tool_names = names;
        self
    }

    /// Escape hatch controlling `general` tool exposure (default: auto).
    pub fn with_enable_general_tool(mut self, enabled: Option<bool>) -> Self {
        self.enable_general_tool = enabled;
        self
    }

    pub fn with_hidden_tool_names(mut self, names: Vec<String>) -> Self {
        self.hidden_tool_names = names;
        self
    }

    /// Override the declared exposure of specific tools for this run
    /// (e.g. guardian/reviewer forms switch tool sets by changing inputs).
    pub fn with_exposure_overrides(
        mut self,
        overrides: Vec<(String, wf_types::tool::ToolExposure)>,
    ) -> Self {
        self.exposure_overrides = overrides;
        self
    }

    pub fn with_max_pause_duration(mut self, duration_ms: u64) -> Self {
        self.max_pause_duration = Some(duration_ms);
        self
    }

    /// Record this execution's depth in the agent hierarchy (parent depth + 1).
    pub fn with_hierarchy_depth(mut self, depth: u32) -> Self {
        self.hierarchy_depth = depth;
        self
    }

    /// Record the root execution id of the hierarchy this run belongs to.
    pub fn with_root_execution_id(mut self, root: Id) -> Self {
        self.root_execution_id = Some(root);
        self
    }

    /// Record the full ancestor chain (oldest first, excluding self),
    /// resolved from the parent execution at build time.
    pub fn with_ancestors(mut self, ancestors: Vec<Id>) -> Self {
        self.ancestors = ancestors;
        self
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn definition_id(&self) -> &Id {
        &self.definition_id
    }

    pub fn conversation(&self) -> &Arc<tokio::sync::RwLock<ConversationSession>> {
        &self.conversation
    }

    pub fn interruption(&self) -> &InterruptionState {
        &self.interruption
    }

    /// The event bus the interruption publishes lifecycle events to (wired
    /// through `set_event_bus` at entity build time).
    pub fn event_bus(&self) -> Option<Arc<wf_core::EventBus>> {
        self.interruption.event_bus()
    }

    /// Wait until the interruption leaves the `Pause` state. Returns as soon
    /// as the loop is `Active` (resumed) or `Stop`ped (timeout / explicit
    /// stop), so a paused loop waiting on this never blocks a forced stop.
    pub async fn wait_until_active(&self) {
        let mut rx = self.interruption.subscribe();
        loop {
            let signal = rx.borrow().clone();
            match signal {
                InterruptionSignal::Active | InterruptionSignal::Stop => return,
                InterruptionSignal::Pause => {
                    if rx.changed().await.is_err() {
                        return;
                    }
                }
            }
        }
    }

    pub fn hooks(&self) -> &[BaseHookDefinition] {
        &self.hooks
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn tool_call_format(&self) -> Option<&ToolCallFormatConfig> {
        self.tool_call_format.as_ref()
    }

    pub fn available_tool_names(&self) -> &[String] {
        &self.available_tool_names
    }

    pub fn initial_tool_names(&self) -> &[String] {
        &self.initial_tool_names
    }

    pub fn discoverable_tool_names(&self) -> &[String] {
        &self.discoverable_tool_names
    }

    pub fn enable_general_tool(&self) -> Option<bool> {
        self.enable_general_tool
    }

    pub fn hidden_tool_names(&self) -> &[String] {
        &self.hidden_tool_names
    }

    pub fn exposure_overrides(&self) -> &[(String, wf_types::tool::ToolExposure)] {
        &self.exposure_overrides
    }

    pub fn timeout_manager(&self) -> &AgentTimeoutManager {
        &self.timeout_manager
    }

    pub fn max_pause_duration(&self) -> Option<u64> {
        self.max_pause_duration
    }

    pub fn parent_execution_id(&self) -> Option<&Id> {
        self.parent_execution_id.as_ref()
    }

    pub fn ancestors(&self) -> &[Id] {
        &self.ancestors
    }

    pub fn child_execution_ids(&self) -> &Arc<tokio::sync::RwLock<Vec<Id>>> {
        &self.child_execution_ids
    }

    pub async fn register_child(&self, child_id: Id) {
        self.child_execution_ids.write().await.push(child_id);
    }

    pub async fn unregister_child(&self, child_id: &Id) {
        self.child_execution_ids
            .write()
            .await
            .retain(|id| id != child_id);
    }

    /// Read the shared status from a synchronous context. Tries a non-blocking
    /// `try_read` first; when the lock is contended it blocks on the tokio
    /// runtime (multi-thread only, where `block_in_place` is safe). When no
    /// suitable runtime context exists — `block_in_place` would panic on a
    /// current-thread runtime and blocking outside tokio would deadlock — it
    /// infers a coherent status from the sync-visible signals (cancellation /
    /// interruption) instead of fabricating contradictory values.
    fn sync_status(&self) -> ExecutionStatus {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
                return tokio::task::block_in_place(|| {
                    handle.block_on(async { self.state.read().await.status() })
                });
            }
        }
        if self.cancellation.is_cancelled() {
            return ExecutionStatus::Cancelled;
        }
        match self.interruption.check() {
            Some(InterruptionSignal::Stop) => return ExecutionStatus::Cancelled,
            Some(InterruptionSignal::Pause) => return ExecutionStatus::Paused,
            _ => {}
        }
        ExecutionStatus::Running
    }
}

#[async_trait::async_trait]
impl IExecutionEntity for AgentLoopEntity {
    fn id(&self) -> &Id {
        &self.id
    }

    fn status(&self) -> ExecutionStatus {
        if let Ok(state) = self.state.try_read() {
            return state.status();
        }
        self.sync_status()
    }

    fn is_running(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Running)
    }

    fn is_paused(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Paused)
    }

    fn is_completed(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Completed)
    }

    fn is_failed(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Failed)
    }

    fn is_cancelled(&self) -> bool {
        matches!(
            self.status(),
            ExecutionStatus::Cancelled | ExecutionStatus::Stopped
        )
    }

    async fn pause(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        AgentLoopStateTransitor::pause_agent_loop(self, self.event_bus().as_deref())
            .await
            .map_err(|e| ExecutionSharedError::StateError(e.to_string()))?;
        self.interruption.pause()?;
        self.start_pause_timeout();
        Ok(())
    }

    async fn resume(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.cancel_pause_timeout();
        AgentLoopStateTransitor::resume_agent_loop(self, self.event_bus().as_deref())
            .await
            .map_err(|e| ExecutionSharedError::StateError(e.to_string()))?;
        self.interruption.resume()?;
        Ok(())
    }

    async fn stop(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        AgentLoopStateTransitor::cancel_agent_loop(self, self.event_bus().as_deref())
            .await
            .map_err(|e| ExecutionSharedError::StateError(e.to_string()))?;
        self.interruption.stop()?;
        self.cancellation.cancel();
        Ok(())
    }

    async fn abort(&self) {
        self.cancellation.cancel();
    }

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken {
        self.cancellation.clone()
    }

    fn get_hierarchy_depth(&self) -> u32 {
        self.hierarchy_depth
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        self.root_execution_id
            .clone()
            .or_else(|| Some(self.id.clone()))
    }

    fn get_ancestors(&self) -> Vec<Id> {
        self.ancestors.clone()
    }
}

impl AgentLoopEntity {
    /// Attach the capacity-gate permit to this entity (registration path).
    pub fn set_gate_permit(&self, permit: Option<GatePermit>) {
        *wf_common::lock::write_ok(self.gate_permit.write()) = permit;
    }

    /// Detach and return the capacity-gate permit; dropping it releases the
    /// concurrency slot.
    pub fn take_gate_permit(&self) -> Option<GatePermit> {
        wf_common::lock::write_ok(self.gate_permit.write()).take()
    }

    /// Release the capacity-gate permit immediately (terminal transition or
    /// failed placeholder registration).
    pub fn release_gate_permit(&self) {
        drop(self.take_gate_permit());
    }

    /// Register a pause timeout: if the loop stays paused beyond
    /// `max_pause_duration` the interruption state is stopped.
    fn start_pause_timeout(&self) {
        let Some(max_pause) = self.max_pause_duration else {
            return;
        };
        if max_pause == 0 {
            return;
        }
        self.cancel_pause_timeout();
        let interruption = self.interruption.clone();
        let agent_loop_id = self.id.clone();
        let handle = self.timeout_manager.register(
            format!("pause-{}", self.id),
            std::time::Duration::from_millis(max_pause),
            move || {
                tracing::warn!(
                    agent_loop_id = %agent_loop_id,
                    max_pause_duration = max_pause,
                    "Agent loop pause timeout exceeded, stopping execution"
                );
                let _ = interruption.stop();
            },
        );
        *wf_common::lock::write_ok(self.pause_timeout_handle.write()) = Some(handle);
    }

    fn cancel_pause_timeout(&self) {
        if let Some(handle) = wf_common::lock::write_ok(self.pause_timeout_handle.write()).take() {
            handle.cancel();
        }
        self.timeout_manager.cancel(&format!("pause-{}", self.id));
    }
}

impl wf_core::execution_loop::HasInterruption for AgentLoopEntity {
    fn interruption(&self) -> &InterruptionState {
        &self.interruption
    }
}
