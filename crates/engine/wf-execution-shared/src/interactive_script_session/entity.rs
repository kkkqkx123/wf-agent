//! Stateful session entity for one interactive script run, including its
//! `ExecutionEntity` and `StateManager` implementations.

use std::sync::Arc;

use async_trait::async_trait;
use wf_core::interruption::InterruptionState;
use crate::types::execution_entity::{ExecutionEntity, ExecutionStatus};
use wf_types::Id;

use super::config::{
    InteractiveScriptSessionConfig, InteractiveScriptSessionSnapshot, InteractiveScriptSessionState,
};

/// Stateful session entity for one interactive script run. Configuration is
/// immutable; state is serializable; the shell handle and cancellation token
/// are runtime-only.
pub struct InteractiveScriptSessionEntity {
    id: Id,
    config: InteractiveScriptSessionConfig,
    pub(super) state: Arc<tokio::sync::RwLock<InteractiveScriptSessionState>>,
    status: Arc<tokio::sync::RwLock<ExecutionStatus>>,
    interruption: InterruptionState,
    pub(super) cancellation: tokio_util::sync::CancellationToken,
    shell_session_id: std::sync::RwLock<Option<String>>,
    parent_execution_id: Option<Id>,
    child_execution_ids: Arc<tokio::sync::RwLock<Vec<Id>>>,
    hierarchy_depth: u32,
    root_execution_id: Option<Id>,
    ancestors: Vec<Id>,
}

impl InteractiveScriptSessionEntity {
    pub fn new(id: Id, config: InteractiveScriptSessionConfig) -> Self {
        let command = config.command.clone();
        Self {
            id,
            config,
            state: Arc::new(tokio::sync::RwLock::new(
                InteractiveScriptSessionState::new(&command),
            )),
            status: Arc::new(tokio::sync::RwLock::new(ExecutionStatus::Created)),
            interruption: InterruptionState::new(),
            cancellation: tokio_util::sync::CancellationToken::new(),
            shell_session_id: std::sync::RwLock::new(None),
            parent_execution_id: None,
            child_execution_ids: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            hierarchy_depth: 0,
            root_execution_id: None,
            ancestors: Vec::new(),
        }
    }

    pub fn with_parent_execution_id(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }

    pub fn with_hierarchy_depth(mut self, depth: u32) -> Self {
        self.hierarchy_depth = depth;
        self
    }

    pub fn with_root_execution_id(mut self, root_id: Id) -> Self {
        self.root_execution_id = Some(root_id);
        self
    }

    pub fn with_ancestors(mut self, ancestors: Vec<Id>) -> Self {
        self.ancestors = ancestors;
        self
    }

    pub fn config(&self) -> &InteractiveScriptSessionConfig {
        &self.config
    }

    pub async fn snapshot_state(&self) -> InteractiveScriptSessionState {
        self.state.read().await.clone()
    }

    pub(super) fn attach_shell_session(&self, shell_session_id: &str) {
        if let Ok(mut slot) = self.shell_session_id.write() {
            *slot = Some(shell_session_id.to_string());
        }
    }

    pub fn shell_session(&self) -> Option<String> {
        self.shell_session_id
            .read()
            .map(|slot| slot.clone())
            .unwrap_or_default()
    }

    pub(super) async fn set_status(&self, status: ExecutionStatus) {
        *self.status.write().await = status;
    }

    /// Re-create a runnable entity from a snapshot. The shell process itself
    /// is not restored; the caller re-spawns it and records replayed output
    /// through the state accessors.
    pub fn restore(
        id: Id,
        config: InteractiveScriptSessionConfig,
        snapshot: InteractiveScriptSessionSnapshot,
    ) -> Self {
        let entity = Self::new(id, config);
        if let Ok(state) = entity.state.try_write() {
            let mut state = state;
            state.phase = snapshot.phase;
            state.current_command = snapshot.current_command;
            state.executed_commands = snapshot.executed_commands;
            state.accumulated_stdout_len = snapshot.accumulated_stdout_len;
            state.accumulated_stderr_len = snapshot.accumulated_stderr_len;
            state.interaction_history = snapshot.interaction_history;
            state.completed_rounds = snapshot.completed_rounds;
            state.waiting_for_input = snapshot.waiting_for_input;
            state.current_prompt = snapshot.current_prompt;
        }
        entity
    }
}

#[async_trait]
impl ExecutionEntity for InteractiveScriptSessionEntity {
    fn id(&self) -> &Id {
        &self.id
    }

    fn status(&self) -> ExecutionStatus {
        self.status
            .try_read()
            .map(|s| s.clone())
            .unwrap_or(ExecutionStatus::Running)
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

    async fn pause(&self) -> Result<(), crate::error::ExecutionSharedError> {
        self.interruption.pause().map_err(|e| {
            crate::error::ExecutionSharedError::StateError(e.to_string())
        })?;
        self.set_status(ExecutionStatus::Paused).await;
        Ok(())
    }

    async fn resume(&self) -> Result<(), crate::error::ExecutionSharedError> {
        self.interruption.resume().map_err(|e| {
            crate::error::ExecutionSharedError::StateError(e.to_string())
        })?;
        self.set_status(ExecutionStatus::Running).await;
        Ok(())
    }

    async fn stop(&self) -> Result<(), crate::error::ExecutionSharedError> {
        if self.status().is_terminal() {
            return Ok(());
        }
        self.interruption.stop().map_err(|e| {
            crate::error::ExecutionSharedError::StateError(e.to_string())
        })?;
        self.cancellation.cancel();
        self.set_status(ExecutionStatus::Stopped).await;
        Ok(())
    }

    async fn abort(&self) {
        self.cancellation.cancel();
        self.set_status(ExecutionStatus::Cancelled).await;
    }

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken {
        self.cancellation.clone()
    }

    fn get_hierarchy_depth(&self) -> u32 {
        self.hierarchy_depth
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        self.root_execution_id.clone()
    }

    fn get_ancestors(&self) -> Vec<Id> {
        self.ancestors.clone()
    }
}

impl crate::types::state_manager::StateManager<InteractiveScriptSessionSnapshot>
    for InteractiveScriptSessionEntity
{
    async fn cleanup(&mut self) -> Result<(), crate::error::ExecutionSharedError> {
        self.child_execution_ids.write().await.clear();
        if let Ok(mut slot) = self.shell_session_id.write() {
            *slot = None;
        }
        Ok(())
    }

    async fn create_snapshot(
        &self,
    ) -> Result<InteractiveScriptSessionSnapshot, crate::error::ExecutionSharedError>
    {
        let state = self.state.read().await;
        Ok(InteractiveScriptSessionSnapshot {
            session_id: self.id.to_string(),
            script_name: self.config.script_name.clone(),
            phase: state.phase.clone(),
            current_command: state.current_command.clone(),
            executed_commands: state.executed_commands.clone(),
            accumulated_stdout_len: state.accumulated_stdout_len,
            accumulated_stderr_len: state.accumulated_stderr_len,
            interaction_history: state.interaction_history.clone(),
            completed_rounds: state.completed_rounds,
            waiting_for_input: state.waiting_for_input,
            current_prompt: state.current_prompt.clone(),
            parent_execution_id: self.parent_execution_id.as_ref().map(|id| id.to_string()),
            hierarchy_depth: self.hierarchy_depth,
            ancestors: self.ancestors.iter().map(|id| id.to_string()).collect(),
        })
    }

    async fn restore_from_snapshot(
        &mut self,
        snapshot: InteractiveScriptSessionSnapshot,
    ) -> Result<(), crate::error::ExecutionSharedError> {
        let mut state = self.state.write().await;
        state.phase = snapshot.phase;
        state.current_command = snapshot.current_command;
        state.executed_commands = snapshot.executed_commands;
        state.accumulated_stdout_len = snapshot.accumulated_stdout_len;
        state.accumulated_stderr_len = snapshot.accumulated_stderr_len;
        state.interaction_history = snapshot.interaction_history;
        state.completed_rounds = snapshot.completed_rounds;
        state.waiting_for_input = snapshot.waiting_for_input;
        state.current_prompt = snapshot.current_prompt;
        Ok(())
    }

    fn size(&self) -> usize {
        self.state
            .try_read()
            .map(|state| {
                state.accumulated_stdout_len
                    + state.accumulated_stderr_len
                    + state.interaction_history.len() * 64
            })
            .unwrap_or(0)
    }

    fn is_empty(&self) -> bool {
        self.state
            .try_read()
            .map(|state| {
                state.executed_commands.is_empty()
                    && state.interaction_history.is_empty()
                    && state.accumulated_stdout_len == 0
            })
            .unwrap_or(true)
    }
}
