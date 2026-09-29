use std::sync::Arc;

use wf_common::gate::GatePermit;
use wf_core::interruption::{InterruptionSignal, InterruptionState};
use wf_execution_shared::conversation_session::ConversationSession;
use wf_execution_shared::error::ExecutionSharedError;
use wf_execution_shared::hooks::types::HookDefinition;
use wf_execution_shared::types::execution_entity::{ExecutionEntity, ExecutionStatus};
use wf_metrics::TimeoutMetricsCollector;
use wf_types::llm::ToolCallProtocolConfig;
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
    hierarchy: Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>,
    effective_config: Option<wf_types::agent_execution::AgentLoopConfig>,
    hooks: Vec<HookDefinition>,
    model: String,
    tool_call_protocol: Option<ToolCallProtocolConfig>,
    available_tool_names: Vec<String>,
    initial_tool_names: Vec<String>,
    discoverable_tool_names: Vec<String>,
    enable_general_tool: Option<bool>,
    hidden_tool_names: Vec<String>,
    exposure_overrides: Vec<(String, wf_types::tool::ToolExposure)>,
    /// Opt-in per-turn history projection (`build_agent_request` rewrites the
    /// projected history to the current turn's exposure). Default `false`;
    /// see `AgentLoopConfig::history_normalization` for the tradeoff.
    history_normalization: bool,
    timeout_manager: AgentTimeoutManager,
    timeout_expired: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Entity-scoped monotonic counter for `general`-tool fallback outer
    /// ids. Combined with the execution id it yields collision-free replay
    /// keys without hashing.
    general_outer_seq: std::sync::Arc<std::sync::atomic::AtomicU64>,
    max_pause_duration: Option<u64>,
    pause_timeout_handle: std::sync::RwLock<Option<TimeoutHandle>>,
    timeout_metrics: Option<Arc<TimeoutMetricsCollector>>,
    /// Permit held against the registry's concurrency gate for the duration
    /// of this execution. Released when the execution reaches a terminal
    /// state or when the entity is removed from the registry.
    gate_permit: std::sync::RwLock<Option<GatePermit>>,
}

impl AgentLoopEntity {
    pub fn new(id: Id) -> Self {
        let definition_id = id.clone();
        let hierarchy = Arc::new(wf_core::hierarchy::manager::ExecutionHierarchyManager::new(
            id.clone(),
            wf_types::execution::ExecutionType::AgentLoop,
        ));
        Self {
            id,
            definition_id,
            state: Arc::new(tokio::sync::RwLock::new(AgentLoopState::new())),
            interruption: InterruptionState::new(),
            conversation: Arc::new(tokio::sync::RwLock::new(ConversationSession::new())),
            cancellation: tokio_util::sync::CancellationToken::new(),
            hierarchy,
            effective_config: None,
            hooks: Vec::new(),
            model: String::new(),
            tool_call_protocol: None,
            available_tool_names: Vec::new(),
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            hidden_tool_names: Vec::new(),
            exposure_overrides: Vec::new(),
            history_normalization: false,
            timeout_manager: AgentTimeoutManager::new(),
            timeout_expired: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            general_outer_seq: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            max_pause_duration: None,
            pause_timeout_handle: std::sync::RwLock::new(None),
            timeout_metrics: None,
            gate_permit: std::sync::RwLock::new(None),
        }
    }

    pub fn hierarchy_manager(&self) -> Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager> {
        self.hierarchy.clone()
    }

    pub fn with_hierarchy_manager(
        mut self,
        manager: Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>,
    ) -> Self {
        self.hierarchy = manager;
        self
    }

    pub fn with_effective_config(
        mut self,
        config: wf_types::agent_execution::AgentLoopConfig,
    ) -> Self {
        self.effective_config = Some(config);
        self
    }

    pub fn effective_config(&self) -> Option<&wf_types::agent_execution::AgentLoopConfig> {
        self.effective_config.as_ref()
    }

    /// Set the agent definition id (the `agent_id` of the loop config). The
    /// definition id identifies the agent definition; `id` is the per-run
    /// loop id.
    pub fn with_definition_id(mut self, definition_id: Id) -> Self {
        self.definition_id = definition_id;
        self
    }

    pub fn with_hooks(mut self, hooks: Vec<HookDefinition>) -> Self {
        self.hooks = hooks;
        self
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn with_tool_call_protocol(mut self, format: ToolCallProtocolConfig) -> Self {
        self.tool_call_protocol = Some(format);
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

    /// Opt into per-turn history projection for this run. Off by default.
    pub fn with_history_normalization(mut self, enabled: bool) -> Self {
        self.history_normalization = enabled;
        self
    }

    pub fn with_max_pause_duration(mut self, duration_ms: u64) -> Self {
        self.max_pause_duration = Some(duration_ms);
        self
    }

    pub fn with_timeout_metrics(mut self, metrics: Arc<TimeoutMetricsCollector>) -> Self {
        self.timeout_metrics = Some(metrics);
        self
    }

    pub fn timeout_metrics(&self) -> Option<Arc<TimeoutMetricsCollector>> {
        self.timeout_metrics.clone()
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

    pub fn hooks(&self) -> &[HookDefinition] {
        &self.hooks
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn tool_call_protocol(&self) -> Option<&ToolCallProtocolConfig> {
        self.tool_call_protocol.as_ref()
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

    pub fn history_normalization_enabled(&self) -> bool {
        self.history_normalization
    }

    pub fn timeout_manager(&self) -> &AgentTimeoutManager {
        &self.timeout_manager
    }

    pub fn timeout_expired(&self) -> bool {
        self.timeout_expired
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn timeout_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.timeout_expired.clone()
    }

    /// Collision-free fallback outer id for `general`-tool invocations that
    /// carry no stamped outer call id. Monotonic within the execution and
    /// scoped by the execution id, so two inner calls in one run never share
    /// a replay key.
    pub fn next_general_outer_id(&self) -> String {
        let seq = self
            .general_outer_seq
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("general-{}-{seq}", self.id)
    }

    pub fn max_pause_duration(&self) -> Option<u64> {
        self.max_pause_duration
    }

    pub fn parent_execution_id(&self) -> Option<Id> {
        self.hierarchy.parent_id()
    }

    pub fn ancestors(&self) -> Vec<Id> {
        self.hierarchy.ancestors()
    }

    pub fn child_ids(&self) -> Vec<Id> {
        let mut ids: Vec<Id> = self
            .hierarchy
            .children()
            .into_iter()
            .map(|c| c.child_id)
            .collect();
        ids.sort();
        ids
    }

    pub async fn register_child(&self, child_id: Id) {
        let already = self
            .hierarchy
            .children()
            .iter()
            .any(|c| c.child_id == child_id);
        if already {
            return;
        }
        self.hierarchy
            .register_child_ref(wf_types::execution::ChildExecutionReference {
                child_type: wf_types::execution::ExecutionType::AgentLoop,
                child_id,
                created_at: wf_common::now(),
                fork_path: None,
            });
    }

    pub async fn register_child_ref(
        &self,
        child_ref: wf_types::execution::ChildExecutionReference,
    ) {
        self.hierarchy.register_child_ref(child_ref);
    }

    pub async fn unregister_child(&self, child_id: &Id) {
        for child_type in [
            wf_types::execution::ExecutionType::Workflow,
            wf_types::execution::ExecutionType::AgentLoop,
        ] {
            if self.hierarchy.remove_child(child_id, &child_type) {
                break;
            }
        }
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
impl ExecutionEntity for AgentLoopEntity {
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
        if self.status().is_terminal() {
            return Ok(());
        }
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
        self.hierarchy.depth()
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        Some(self.hierarchy.root_execution_id())
    }

    fn get_ancestors(&self) -> Vec<Id> {
        self.hierarchy.ancestors()
    }

    fn hierarchy_manager(
        &self,
    ) -> Option<Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>> {
        Some(self.hierarchy.clone())
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
        self.clear_pause_timeout();
        let interruption = self.interruption.clone();
        let timeout_expired = self.timeout_expired.clone();
        let agent_loop_id = self.id.clone();
        let execution_id = self.id.to_string();
        let metrics = self.timeout_metrics.clone();
        if let Some(ref metrics) = metrics {
            metrics.record_registration("agent_pause", max_pause as f64, &execution_id);
        }
        let registered_at = std::time::Instant::now();
        let handle = self.timeout_manager.register(
            format!("pause-{}", self.id),
            std::time::Duration::from_millis(max_pause),
            move || {
                tracing::warn!(
                    agent_loop_id = %agent_loop_id,
                    max_pause_duration = max_pause,
                    "Agent loop pause timeout exceeded, stopping execution"
                );
                if let Some(ref metrics) = metrics {
                    metrics.record_expiration(
                        "agent_pause",
                        registered_at.elapsed().as_millis() as f64,
                        &execution_id,
                    );
                }
                timeout_expired.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = interruption.stop();
            },
        );
        *wf_common::lock::write_ok(self.pause_timeout_handle.write()) = Some(handle);
    }

    fn cancel_pause_timeout(&self) {
        let had_handle = self.clear_pause_timeout();
        if had_handle {
            if let Some(ref metrics) = self.timeout_metrics {
                metrics.record_cancellation("agent_pause", "resume", &self.id.to_string());
            }
        }
    }

    fn clear_pause_timeout(&self) -> bool {
        let had_handle = wf_common::lock::write_ok(self.pause_timeout_handle.write())
            .take()
            .is_some();
        self.timeout_manager.cancel(&format!("pause-{}", self.id));
        had_handle
    }
}

impl wf_execution_shared::execution_loop::HasInterruption for AgentLoopEntity {
    fn interruption(&self) -> &InterruptionState {
        &self.interruption
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_outer_ids_are_unique_within_a_run() {
        let entity = AgentLoopEntity::new(Id::from("exec-1".to_string()));
        let first = entity.next_general_outer_id();
        let second = entity.next_general_outer_id();
        assert_ne!(first, second);
        assert!(first.starts_with("general-exec-1-"));
        assert!(second.starts_with("general-exec-1-"));
    }
}
