use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wf_core::interruption::InterruptionState;
use wf_core::EventBus;
use wf_execution_shared::types::execution_entity::{ExecutionEntity, ExecutionStatus};
use wf_script::{InteractionMode, RiskEvaluator};
use wf_types::events::{BaseEvent, EventType};
use wf_types::Id;

use crate::error::{WorkflowError, WorkflowResult};
use crate::interaction::InteractionRegistry;

/// Immutable configuration of one interactive script session.
#[derive(Debug, Clone)]
pub struct InteractiveScriptSessionConfig {
    pub script_name: String,
    pub language: String,
    pub command: String,
    pub interaction_mode: InteractionMode,
    pub max_rounds: u32,
    pub round_timeout_ms: u64,
    pub prompt_patterns: Vec<String>,
    pub working_directory: Option<String>,
    pub environment: HashMap<String, String>,
    pub session_timeout_ms: Option<u64>,
    /// Quiet period after a prompt-pattern match before the round fires.
    /// Absorbs output jitter (progress redraws, chunked echoes) so one
    /// stable prompt produces one interaction instead of several.
    pub debounce_ms: u64,
    /// Hybrid mode: use the model suggestion when the confirmation wait
    /// times out instead of failing the round.
    pub hybrid_fallback_to_suggestion: bool,
    /// Cap on model-generated (non-preset) rounds. Defaults to `max_rounds`.
    pub max_autonomous_rounds: Option<u32>,
    /// Cap the final session output at this many tail bytes. The live
    /// prompt detection always works on the tail and is unaffected.
    pub max_output_bytes: Option<u64>,
    /// Model profile used for suggestions (model-assisted and hybrid).
    /// Falls back to the handler default and then to `"default"`.
    pub llm_profile_id: Option<String>,
}

impl InteractiveScriptSessionConfig {
    pub fn mode_from_str(value: Option<&str>) -> InteractionMode {
        match value.map(|v| v.to_lowercase()).as_deref() {
            Some("llm_assisted") => InteractionMode::LlmAssisted,
            Some("hybrid") => InteractionMode::Hybrid,
            _ => InteractionMode::Blocking,
        }
    }

    pub fn autonomous_round_limit(&self) -> u32 {
        self.max_autonomous_rounds.unwrap_or(self.max_rounds)
    }
}

/// Serializable lifecycle phase of the session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Running,
    WaitingInput,
    Completed,
    Failed,
}

/// Where one round's input came from. Recorded per round for audit and for
/// checkpoint replay, which re-feeds the recorded responses in order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum InteractionSource {
    Preset,
    #[default]
    External,
    Model,
    HybridConfirmed,
    HybridEdited,
}

/// One completed interaction round. Prompt and response are stored in full:
/// prompts are short by construction and responses are human or model inputs,
/// so replay reproduces the exact bytes that were sent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractionRecord {
    pub round: u32,
    pub prompt: String,
    pub response: String,
    #[serde(default)]
    pub source: InteractionSource,
}

/// Serializable snapshot of the session state. The live shell process is
/// never serialized; restore re-creates the session shell instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractiveScriptSessionSnapshot {
    pub session_id: String,
    pub script_name: String,
    pub phase: SessionPhase,
    pub current_command: String,
    pub executed_commands: Vec<String>,
    pub accumulated_stdout_len: usize,
    pub accumulated_stderr_len: usize,
    pub interaction_history: Vec<InteractionRecord>,
    pub completed_rounds: u32,
    pub waiting_for_input: bool,
    pub current_prompt: Option<String>,
    pub parent_execution_id: Option<String>,
    pub hierarchy_depth: u32,
    pub ancestors: Vec<String>,
}

/// Runtime state of the session; mirrors the snapshot without the identity.
#[derive(Debug, Clone)]
pub struct InteractiveScriptSessionState {
    pub phase: SessionPhase,
    pub current_command: String,
    pub executed_commands: Vec<String>,
    pub accumulated_stdout_len: usize,
    pub accumulated_stderr_len: usize,
    pub interaction_history: Vec<InteractionRecord>,
    pub completed_rounds: u32,
    pub waiting_for_input: bool,
    pub current_prompt: Option<String>,
}

impl InteractiveScriptSessionState {
    fn new(command: &str) -> Self {
        Self {
            phase: SessionPhase::Running,
            current_command: command.to_string(),
            executed_commands: Vec::new(),
            accumulated_stdout_len: 0,
            accumulated_stderr_len: 0,
            interaction_history: Vec::new(),
            completed_rounds: 0,
            waiting_for_input: false,
            current_prompt: None,
        }
    }
}

/// Stateful session entity for one interactive script run. Configuration is
/// immutable; state is serializable; the shell handle and cancellation token
/// are runtime-only.
pub struct InteractiveScriptSessionEntity {
    id: Id,
    config: InteractiveScriptSessionConfig,
    state: Arc<tokio::sync::RwLock<InteractiveScriptSessionState>>,
    status: Arc<tokio::sync::RwLock<ExecutionStatus>>,
    interruption: InterruptionState,
    cancellation: tokio_util::sync::CancellationToken,
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

    fn attach_shell_session(&self, shell_session_id: &str) {
        if let Ok(mut slot) = self.shell_session_id.write() {
            *slot = Some(shell_session_id.to_string());
        }
    }

    pub(crate) fn shell_session(&self) -> Option<String> {
        self.shell_session_id
            .read()
            .map(|slot| slot.clone())
            .unwrap_or_default()
    }

    async fn set_status(&self, status: ExecutionStatus) {
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

    async fn pause(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.pause().map_err(|e| {
            wf_execution_shared::error::ExecutionSharedError::StateError(e.to_string())
        })?;
        self.set_status(ExecutionStatus::Paused).await;
        Ok(())
    }

    async fn resume(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.resume().map_err(|e| {
            wf_execution_shared::error::ExecutionSharedError::StateError(e.to_string())
        })?;
        self.set_status(ExecutionStatus::Running).await;
        Ok(())
    }

    async fn stop(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        if self.status().is_terminal() {
            return Ok(());
        }
        self.interruption.stop().map_err(|e| {
            wf_execution_shared::error::ExecutionSharedError::StateError(e.to_string())
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

impl wf_execution_shared::types::state_manager::StateManager<InteractiveScriptSessionSnapshot>
    for InteractiveScriptSessionEntity
{
    async fn cleanup(&mut self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.child_execution_ids.write().await.clear();
        if let Ok(mut slot) = self.shell_session_id.write() {
            *slot = None;
        }
        Ok(())
    }

    async fn create_snapshot(
        &self,
    ) -> Result<InteractiveScriptSessionSnapshot, wf_execution_shared::error::ExecutionSharedError>
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
    ) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
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

/// Match the tail of the session output against the configured prompt
/// patterns. Invalid patterns are ignored. Returns the matched pattern.
pub fn detect_prompt(tail: &str, patterns: &[String]) -> Option<String> {
    for pattern in patterns {
        let Ok(re) = regex::Regex::new(pattern) else {
            continue;
        };
        if re.is_match(tail) {
            return Some(pattern.clone());
        }
    }
    None
}

/// Minimal driver context: the workflow coordinates used for interaction
/// events. The driver stays independent of the full node context.
pub struct SessionDriverContext {
    pub execution_id: String,
    pub node_id: String,
    pub event_bus: Option<Arc<EventBus>>,
    /// Interaction registry the driver waits on. `None` selects the
    /// process-wide registry used in production; tests inject an isolated
    /// instance so parallel suites never observe each other's waits.
    pub interaction_registry: Option<Arc<InteractionRegistry>>,
    /// Model suggestion source for model-assisted and hybrid rounds. `None`
    /// keeps the historical behavior (model-assisted without presets fails
    /// explicitly, hybrid behaves like blocking).
    pub suggester: Option<Arc<dyn SuggestionProvider>>,
    /// File-change capture handle. `None` disables workspace capture.
    pub round_capture: Option<RoundCapture>,
}

fn driver_registry(driver: &SessionDriverContext) -> Arc<InteractionRegistry> {
    driver
        .interaction_registry
        .clone()
        .unwrap_or_else(crate::interaction::interaction_registry)
}

/// Model suggestion source for one interaction round. Model-assisted mode
/// adopts the suggestion (subject to the risk gate); hybrid mode presents it
/// for human confirmation. Returning `None` means the model produced nothing
/// usable, which the driver surfaces as an explicit failure.
#[async_trait]
pub trait SuggestionProvider: Send + Sync {
    async fn suggest(
        &self,
        prompt: &str,
        output_tail: &str,
        history: &[InteractionRecord],
    ) -> Option<String>;
}

/// Gateway-backed suggestion provider: one bounded text generation over the
/// current prompt, the recent output tail and the condensed round history.
pub struct LlmSuggestionProvider {
    pub gateway: Arc<wf_llm::LlmGateway>,
    pub profile_id: String,
    pub execution_id: String,
}

#[async_trait]
impl SuggestionProvider for LlmSuggestionProvider {
    async fn suggest(
        &self,
        prompt: &str,
        output_tail: &str,
        history: &[InteractionRecord],
    ) -> Option<String> {
        let mut text = format!(
            "An interactive script is waiting for input.\nPrompt pattern: {prompt}\nRecent output:\n{output_tail}\n"
        );
        if !history.is_empty() {
            text.push_str("Previous rounds:\n");
            for record in history.iter().rev().take(5) {
                text.push_str(&format!(
                    "- round {} ({:?}): prompt '{}' answered '{}'\n",
                    record.round, record.source, record.prompt, record.response
                ));
            }
        }
        text.push_str("Reply with exactly the input text to send (no explanation, no quotes).");
        let request = wf_types::llm::LlmRequest {
            profile_id: self.profile_id.clone(),
            messages: vec![wf_types::message::Message {
                id: Id::new(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text(text),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
            parameters: None,
            generation: None,
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: Some(self.execution_id.clone()),
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        };
        let outcome =
            wf_execution_shared::single_shot::generate_text_once(&self.gateway, &request, None)
                .await
                .ok()?;
        let reply = outcome.content.or(match outcome.message.content {
            wf_types::message::MessageContentValue::Text(text) => Some(text),
            wf_types::message::MessageContentValue::Rich(_) => None,
        })?;
        let trimmed = reply.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_string())
    }
}

/// Workspace capture handle for one driven session. The handler builds it
/// from the file-checkpoint manager; the driver snapshots the workspace
/// before the run and records the diff at every pause and at completion.
/// Best-effort throughout: capture failures never fail the session itself.
#[derive(Clone)]
pub struct RoundCapture {
    pub manager: wf_checkpoint::file::FileCheckpointManager,
    pub entity_id: String,
    pub parent_execution_id: Option<String>,
    pub scope: Vec<String>,
}

impl RoundCapture {
    pub fn snapshot_before(&self) -> HashMap<PathBuf, String> {
        self.manager
            .collector_for(&self.scope)
            .and_then(|collector| collector.capture().ok())
            .unwrap_or_default()
    }

    pub fn capture(&self, before: &mut HashMap<PathBuf, String>) {
        let Some(collector) = self.manager.collector_for(&self.scope) else {
            return;
        };
        let after = match collector.capture() {
            Ok(after) => after,
            Err(err) => {
                tracing::warn!(error = %err, "interactive session: workspace capture failed; changes not recorded");
                return;
            }
        };
        let changes = wf_checkpoint::script_capture::WorkspaceChangeCollector::diff(before, &after);
        if changes.is_empty() {
            *before = after;
            return;
        }
        let actor = self
            .manager
            .resolve_actor(&self.entity_id, self.parent_execution_id.as_deref());
        let Some(base_dir) = self.manager.workspace_root() else {
            return;
        };
        if let Err(err) = self.manager.apply_workspace_changes(
            &actor,
            base_dir,
            &changes,
            self.manager.failure_behavior(),
        ) {
            tracing::warn!(error = %err, "interactive session: failed to apply workspace changes");
        }
        *before = after;
    }
}

/// Live registry of running interactive sessions, held by the handler so a
/// session is observable (and killable by id) while its node executes.
/// Entries are removed when the drive ends; snapshots remain available
/// through the checkpoint path.
#[derive(Default)]
pub struct SessionRegistry {
    sessions: dashmap::DashMap<String, Arc<InteractiveScriptSessionEntity>>,
}

impl SessionRegistry {
    pub fn register(&self, entity: Arc<InteractiveScriptSessionEntity>) {
        self.sessions.insert(entity.id().to_string(), entity);
    }

    pub fn remove(&self, session_id: &str) {
        self.sessions.remove(session_id);
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

/// Human resolution of a hybrid suggestion.
pub enum ConfirmationAction {
    /// Use the model suggestion unchanged.
    Confirm,
    /// Use the supplied value instead.
    Edit(String),
}

/// Interpret an external hybrid response against the suggestion: explicit
/// confirm envelopes and empty input adopt the suggestion, edit envelopes
/// and any other value override it.
pub fn parse_confirmation(value: &Value) -> ConfirmationAction {
    match value {
        Value::Null => ConfirmationAction::Confirm,
        Value::String(text) if text.trim().is_empty() => ConfirmationAction::Confirm,
        Value::String(text) => ConfirmationAction::Edit(text.clone()),
        Value::Object(map) => match map.get("action").and_then(|v| v.as_str()) {
            Some("confirm") => ConfirmationAction::Confirm,
            Some("edit") => match map.get("value") {
                Some(Value::String(text)) => ConfirmationAction::Edit(text.clone()),
                Some(other) => ConfirmationAction::Edit(other.to_string()),
                None => ConfirmationAction::Confirm,
            },
            _ => match map.get("value") {
                Some(Value::String(text)) if !text.trim().is_empty() => {
                    ConfirmationAction::Edit(text.clone())
                }
                Some(Value::String(_)) => ConfirmationAction::Confirm,
                Some(other) => ConfirmationAction::Edit(other.to_string()),
                None => ConfirmationAction::Confirm,
            },
        },
        other => ConfirmationAction::Edit(other.to_string()),
    }
}

/// Outcome of a driven session run.
#[derive(Debug)]
pub struct SessionOutcome {
    pub output: String,
    pub exit_code: Option<i32>,
    pub completed_rounds: u32,
    pub interaction_history: Vec<InteractionRecord>,
    pub output_truncated: bool,
}

/// Recorded responses in round order, ready to re-feed as preset inputs when
/// a checkpoint restore replays the session instead of resuming the lost
/// shell process.
pub fn replay_inputs(snapshot: &InteractiveScriptSessionSnapshot) -> Vec<Value> {
    let mut rounds = snapshot.interaction_history.clone();
    rounds.sort_by_key(|record| record.round);
    rounds
        .into_iter()
        .map(|record| Value::String(record.response))
        .collect()
}

fn emit_session_event(
    bus: Option<&EventBus>,
    event_type: EventType,
    driver: &SessionDriverContext,
    metadata: HashMap<String, Value>,
) {
    let Some(event_bus) = bus else {
        return;
    };
    event_bus
        .publish_logged(
            BaseEvent {
                id: wf_types::Id::new(),
                r#type: event_type,
                timestamp: wf_common::now(),
                workflow_id: None,
                execution_id: None,
                agent_loop_id: None,
                event_name: None,
                metadata: Some(metadata),
            },
            &format!("execution={} node={}", driver.execution_id, driver.node_id),
        )
        .ok();
}

/// Drive one interactive session to completion on a background shell store.
///
/// The command runs on a PTY session so TTY-dependent programs work. Output
/// is polled incrementally; when a prompt pattern matches, input comes from
/// the preset list first (model-assisted runs and tests) and otherwise from
/// the process-wide interaction registry with per-round timeout.
pub async fn drive_session(
    entity: &InteractiveScriptSessionEntity,
    shell: &Arc<wf_shell::engine::BackgroundShellStore>,
    driver: &SessionDriverContext,
    preset_inputs: Vec<Value>,
) -> WorkflowResult<SessionOutcome> {
    let config = entity.config().clone();
    entity.set_status(ExecutionStatus::Running).await;

    let session_id = shell
        .spawn_with_options(wf_shell::store::SpawnOptions {
            command: config.command.clone(),
            cwd: config.working_directory.clone(),
            env: config.environment.clone(),
            interactive: true,
            force_pty: false,
            pty_size: (24, 80),
            task_id: Some(driver.execution_id.clone()),
        })
        .map_err(|e| WorkflowError::Internal(format!("failed to spawn session: {e}")))?;
    entity.attach_shell_session(&session_id);
    {
        let mut state = entity.state.write().await;
        state.executed_commands.push(config.command.clone());
    }

    let mut output = String::new();
    let mut preset = preset_inputs.into_iter();
    let mut rounds = 0u32;
    let mut autonomous_rounds = 0u32;
    let mut consumed_len = 0usize;
    let mut before = driver
        .round_capture
        .as_ref()
        .map(|handle| handle.snapshot_before())
        .unwrap_or_default();
    let session_deadline = std::time::Instant::now()
        + Duration::from_millis(config.session_timeout_ms.unwrap_or(120_000));

    loop {
        if entity.cancellation.is_cancelled() {
            entity.set_status(ExecutionStatus::Cancelled).await;
            return Err(WorkflowError::Internal(
                "interactive session was cancelled".to_string(),
            ));
        }
        if std::time::Instant::now() >= session_deadline {
            entity.set_status(ExecutionStatus::Failed).await;
            return Err(WorkflowError::Internal(
                "interactive session exceeded its total timeout".to_string(),
            ));
        }

        let session = shell.get(&session_id).ok_or_else(|| {
            WorkflowError::Internal(format!("session '{session_id}' disappeared"))
        })?;
        let fresh = session.read_new_output();
        if !fresh.is_empty() {
            output.push_str(&fresh);
            let mut state = entity.state.write().await;
            state.accumulated_stdout_len = state.accumulated_stdout_len.saturating_add(fresh.len());
        }
        let snapshot = session.snapshot();
        let status = snapshot
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("busy");
        let exit_code = snapshot
            .get("exit_code")
            .and_then(|v| v.as_i64())
            .map(|c| c as i32);

        if status != "busy" {
            let drained = session.read_new_output();
            if !drained.is_empty() {
                output.push_str(&drained);
                let mut state = entity.state.write().await;
                state.accumulated_stdout_len =
                    state.accumulated_stdout_len.saturating_add(drained.len());
            }
            let success = exit_code == Some(0);
            {
                let mut state = entity.state.write().await;
                state.phase = if success {
                    SessionPhase::Completed
                } else {
                    SessionPhase::Failed
                };
                state.waiting_for_input = false;
                state.current_prompt = None;
            }
            entity
                .set_status(if success {
                    ExecutionStatus::Completed
                } else {
                    ExecutionStatus::Failed
                })
                .await;
            if !success {
                return Err(WorkflowError::Internal(format!(
                    "interactive session exited with {exit_code:?}: {}",
                    output_trail(&output)
                )));
            }
            let history = entity.state.read().await.interaction_history.clone();
            if let Some(handle) = driver.round_capture.as_ref() {
                handle.capture(&mut before);
            }
            let (output, output_truncated) = match config.max_output_bytes {
                Some(cap) => {
                    let (kept, truncated) = wf_script::truncate_tail(&output, cap as usize);
                    (kept, truncated)
                }
                None => (output, false),
            };
            return Ok(SessionOutcome {
                output,
                exit_code,
                completed_rounds: rounds,
                interaction_history: history,
                output_truncated,
            });
        }

        let search_from = consumed_len.max(output.len().saturating_sub(2048));
        let matched = detect_prompt(&output[search_from..], &config.prompt_patterns);
        if let Some(first_pattern) = matched {
            let pattern = settle_prompt(
                entity,
                &session,
                &mut output,
                consumed_len,
                &config.prompt_patterns,
                first_pattern,
                config.debounce_ms,
            )
            .await;
            if rounds >= config.max_rounds {
                entity.set_status(ExecutionStatus::Failed).await;
                return Err(WorkflowError::Internal(format!(
                    "interactive session exceeded max rounds ({})",
                    config.max_rounds
                )));
            }
            rounds += 1;
            {
                let mut state = entity.state.write().await;
                state.phase = SessionPhase::WaitingInput;
                state.waiting_for_input = true;
                state.current_prompt = Some(pattern.clone());
                state.completed_rounds = rounds;
            }
            entity.set_status(ExecutionStatus::Paused).await;
            if let Some(handle) = driver.round_capture.as_ref() {
                handle.capture(&mut before);
            }

            let history = entity.state.read().await.interaction_history.clone();
            let tail_start = output.len().saturating_sub(2048);
            let (answer, source) = match config.interaction_mode {
                InteractionMode::LlmAssisted => match preset.next() {
                    Some(value) => (value, InteractionSource::Preset),
                    None => {
                        resolve_model_round(
                            entity,
                            driver,
                            &config,
                            &pattern,
                            &output[tail_start..],
                            &history,
                            &mut autonomous_rounds,
                        )
                        .await?
                    }
                },
                InteractionMode::Hybrid => match preset.next() {
                    Some(value) => (value, InteractionSource::Preset),
                    None => {
                        resolve_hybrid_round(
                            entity,
                            driver,
                            &config,
                            &pattern,
                            &output[tail_start..],
                            &history,
                        )
                        .await?
                    }
                },
                InteractionMode::Blocking => match preset.next() {
                    Some(value) => (value, InteractionSource::Preset),
                    None => (
                        wait_for_external_input(entity, driver, &pattern, &config, None).await?,
                        InteractionSource::External,
                    ),
                },
            };

            let answer_text = match &answer {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            shell
                .send_input(&session_id, &answer_text, true)
                .map_err(|e| WorkflowError::Internal(format!("failed to send input: {e}")))?;
            consumed_len = output.len();
            {
                let mut state = entity.state.write().await;
                state.phase = SessionPhase::Running;
                state.waiting_for_input = false;
                state.current_prompt = None;
                state.interaction_history.push(InteractionRecord {
                    round: rounds,
                    prompt: pattern,
                    response: answer_text.clone(),
                    source,
                });
            }
            entity.set_status(ExecutionStatus::Running).await;
            continue;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Wait for a prompt match to go quiet before firing the round: after the
/// first match, fresh output keeps arriving (progress redraws, chunked
/// echoes), so the driver polls until no output grows for `debounce_ms` and
/// re-detects on the latest tail. Starvation is bounded: an ever-growing
/// stream falls back to the latest match after five quiet windows' worth of
/// waiting. A zero `debounce_ms` keeps the historical immediate behavior.
async fn settle_prompt(
    entity: &InteractiveScriptSessionEntity,
    session: &Arc<wf_shell::terminal_session::TerminalSession>,
    output: &mut String,
    consumed_len: usize,
    patterns: &[String],
    first_pattern: String,
    debounce_ms: u64,
) -> String {
    if debounce_ms == 0 {
        return first_pattern;
    }
    let mut stable = first_pattern;
    let settle_start = std::time::Instant::now();
    let max_settle = Duration::from_millis(debounce_ms.saturating_mul(5).max(debounce_ms + 1000));
    let mut last_growth = std::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let fresh = session.read_new_output();
        if !fresh.is_empty() {
            output.push_str(&fresh);
            let mut state = entity.state.write().await;
            state.accumulated_stdout_len = state.accumulated_stdout_len.saturating_add(fresh.len());
            last_growth = std::time::Instant::now();
            let search_from = consumed_len.max(output.len().saturating_sub(2048));
            if let Some(pattern) = detect_prompt(&output[search_from..], patterns) {
                stable = pattern;
            }
        }
        if last_growth.elapsed() >= Duration::from_millis(debounce_ms) {
            break;
        }
        if settle_start.elapsed() >= max_settle {
            break;
        }
    }
    stable
}

/// Raw outcome of one external wait, without status side effects: the caller
/// decides whether a timeout or cancellation ends the session.
enum ExternalWaitOutcome {
    Answered(Value),
    TimedOut,
    Cancelled,
}

async fn await_external_input(
    driver: &SessionDriverContext,
    pattern: &str,
    config: &InteractiveScriptSessionConfig,
    suggestion: Option<&str>,
) -> ExternalWaitOutcome {
    let (interaction_id, waiter) = {
        let registry = driver_registry(driver);
        let interaction_id = wf_common::generate_id();
        let rx = registry.register(interaction_id.clone());
        (
            interaction_id.clone(),
            crate::interaction::InteractionWait::new(interaction_id, rx),
        )
    };
    let mut requested = HashMap::from([
        (
            "interaction_id".to_string(),
            Value::String(interaction_id.clone()),
        ),
        ("prompt".to_string(), Value::String(pattern.to_string())),
        (
            "timeout".to_string(),
            Value::Number(config.round_timeout_ms.into()),
        ),
        ("node_id".to_string(), Value::String(driver.node_id.clone())),
        (
            "operation".to_string(),
            Value::String("script_interaction".to_string()),
        ),
    ]);
    if let Some(text) = suggestion {
        requested.insert("suggestion".to_string(), Value::String(text.to_string()));
    }
    emit_session_event(
        driver.event_bus.as_deref(),
        EventType::FollowupQuestionRequested,
        driver,
        requested,
    );
    emit_session_event(
        driver.event_bus.as_deref(),
        EventType::WorkflowExecutionPaused,
        driver,
        HashMap::from([
            (
                "reason".to_string(),
                Value::String("script_interaction".to_string()),
            ),
            (
                "interaction_id".to_string(),
                Value::String(interaction_id.clone()),
            ),
        ]),
    );

    let outcome =
        tokio::time::timeout(Duration::from_millis(config.round_timeout_ms), waiter).await;

    emit_session_event(
        driver.event_bus.as_deref(),
        EventType::WorkflowExecutionResumed,
        driver,
        HashMap::from([
            (
                "reason".to_string(),
                Value::String("script_interaction_completed".to_string()),
            ),
            (
                "interaction_id".to_string(),
                Value::String(interaction_id.clone()),
            ),
        ]),
    );

    match outcome {
        Ok(Ok(value)) => {
            emit_session_event(
                driver.event_bus.as_deref(),
                EventType::FollowupQuestionResponded,
                driver,
                HashMap::from([(
                    "interaction_id".to_string(),
                    Value::String(interaction_id.clone()),
                )]),
            );
            ExternalWaitOutcome::Answered(value)
        }
        Ok(Err(_)) => ExternalWaitOutcome::Cancelled,
        Err(_) => ExternalWaitOutcome::TimedOut,
    }
}

async fn fail_wait(
    entity: &InteractiveScriptSessionEntity,
    outcome: ExternalWaitOutcome,
    config: &InteractiveScriptSessionConfig,
) -> WorkflowError {
    entity.set_status(ExecutionStatus::Failed).await;
    match outcome {
        ExternalWaitOutcome::Cancelled => {
            WorkflowError::Internal("interaction waiter was cancelled".to_string())
        }
        ExternalWaitOutcome::TimedOut => WorkflowError::Internal(format!(
            "interaction round timed out after {} ms",
            config.round_timeout_ms
        )),
        ExternalWaitOutcome::Answered(_) => {
            WorkflowError::Internal("interaction wait misrouted an answer".to_string())
        }
    }
}

async fn wait_for_external_input(
    entity: &InteractiveScriptSessionEntity,
    driver: &SessionDriverContext,
    pattern: &str,
    config: &InteractiveScriptSessionConfig,
    suggestion: Option<&str>,
) -> WorkflowResult<Value> {
    match await_external_input(driver, pattern, config, suggestion).await {
        ExternalWaitOutcome::Answered(value) => Ok(value),
        outcome => Err(fail_wait(entity, outcome, config).await),
    }
}

/// Model-assisted round: presets win, otherwise the suggestion is adopted
/// unless the risk gate fires, in which case a human decides. High-risk
/// suggestions are never auto-sent.
async fn resolve_model_round(
    entity: &InteractiveScriptSessionEntity,
    driver: &SessionDriverContext,
    config: &InteractiveScriptSessionConfig,
    pattern: &str,
    output_tail: &str,
    history: &[InteractionRecord],
    autonomous_rounds: &mut u32,
) -> WorkflowResult<(Value, InteractionSource)> {
    let Some(suggester) = driver.suggester.clone() else {
        entity.set_status(ExecutionStatus::Failed).await;
        return Err(WorkflowError::Internal(
            "model-assisted session has no preset input for this prompt and no model gateway is attached".to_string(),
        ));
    };
    if *autonomous_rounds >= config.autonomous_round_limit() {
        entity.set_status(ExecutionStatus::Failed).await;
        return Err(WorkflowError::Internal(format!(
            "model-assisted session exceeded max autonomous rounds ({})",
            config.autonomous_round_limit()
        )));
    }
    let Some(suggestion) = suggester.suggest(pattern, output_tail, history).await else {
        entity.set_status(ExecutionStatus::Failed).await;
        return Err(WorkflowError::Internal(
            "model-assisted session received no usable model suggestion".to_string(),
        ));
    };
    if RiskEvaluator::evaluate(&suggestion).rank() >= wf_script::ScriptRiskLevel::High.rank() {
        match await_external_input(driver, pattern, config, Some(&suggestion)).await {
            ExternalWaitOutcome::Answered(value) => match parse_confirmation(&value) {
                ConfirmationAction::Confirm => {
                    Ok((Value::String(suggestion), InteractionSource::Model))
                }
                ConfirmationAction::Edit(text) => {
                    Ok((Value::String(text), InteractionSource::External))
                }
            },
            outcome => Err(fail_wait(entity, outcome, config).await),
        }
    } else {
        *autonomous_rounds += 1;
        Ok((Value::String(suggestion), InteractionSource::Model))
    }
}

/// Hybrid round: presets win, otherwise the suggestion goes to a human for
/// confirmation. A confirmation timeout adopts the suggestion when the
/// fallback is enabled and fails the round otherwise.
async fn resolve_hybrid_round(
    entity: &InteractiveScriptSessionEntity,
    driver: &SessionDriverContext,
    config: &InteractiveScriptSessionConfig,
    pattern: &str,
    output_tail: &str,
    history: &[InteractionRecord],
) -> WorkflowResult<(Value, InteractionSource)> {
    let suggestion = match driver.suggester.clone() {
        Some(suggester) => suggester.suggest(pattern, output_tail, history).await,
        None => None,
    };
    let Some(suggestion) = suggestion else {
        let value = wait_for_external_input(entity, driver, pattern, config, None).await?;
        return Ok((value, InteractionSource::External));
    };
    match await_external_input(driver, pattern, config, Some(&suggestion)).await {
        ExternalWaitOutcome::Answered(value) => match parse_confirmation(&value) {
            ConfirmationAction::Confirm => Ok((
                Value::String(suggestion),
                InteractionSource::HybridConfirmed,
            )),
            ConfirmationAction::Edit(text) => {
                Ok((Value::String(text), InteractionSource::HybridEdited))
            }
        },
        ExternalWaitOutcome::TimedOut if config.hybrid_fallback_to_suggestion => Ok((
            Value::String(suggestion),
            InteractionSource::HybridConfirmed,
        )),
        outcome => Err(fail_wait(entity, outcome, config).await),
    }
}

fn output_trail(output: &str) -> String {
    let start = output.len().saturating_sub(512);
    output[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(command: &str) -> InteractiveScriptSessionConfig {
        InteractiveScriptSessionConfig {
            script_name: "test".to_string(),
            language: "shell".to_string(),
            command: command.to_string(),
            interaction_mode: InteractionMode::Blocking,
            max_rounds: 5,
            round_timeout_ms: 5000,
            prompt_patterns: vec!["name:".to_string()],
            working_directory: None,
            environment: HashMap::new(),
            session_timeout_ms: None,
            debounce_ms: 0,
            hybrid_fallback_to_suggestion: true,
            max_autonomous_rounds: None,
            max_output_bytes: None,
            llm_profile_id: None,
        }
    }

    #[test]
    fn test_detect_prompt_matches() {
        let patterns = vec!["Enter password:".to_string(), "name:".to_string()];
        assert_eq!(
            detect_prompt("please enter name: ", &patterns),
            Some("name:".to_string())
        );
        assert_eq!(detect_prompt("all done", &patterns), None);
    }

    #[test]
    fn test_detect_prompt_ignores_invalid_regex() {
        let patterns = vec!["([invalid".to_string()];
        assert_eq!(detect_prompt("anything", &patterns), None);
    }

    #[tokio::test]
    async fn test_snapshot_roundtrip() {
        use wf_execution_shared::types::state_manager::StateManager;

        let entity = InteractiveScriptSessionEntity::new(
            Id::from("session-1".to_string()),
            test_config("echo hi"),
        );
        assert!(entity.is_empty());
        let snapshot = entity.create_snapshot().await.expect("snapshot works");
        assert_eq!(snapshot.script_name, "test");
        assert_eq!(snapshot.phase, SessionPhase::Running);

        let restored_entity = InteractiveScriptSessionEntity::restore(
            Id::from("session-2".to_string()),
            test_config("echo hi"),
            snapshot.clone(),
        );
        let restored = restored_entity
            .create_snapshot()
            .await
            .expect("snapshot works");
        assert_eq!(restored.script_name, snapshot.script_name);
        assert_eq!(restored.phase, snapshot.phase);
    }

    #[tokio::test]
    async fn test_drive_session_with_preset_input() {
        let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
        let entity = InteractiveScriptSessionEntity::new(
            Id::from("drive-1".to_string()),
            test_config("printf 'name: '; read n; echo \"hi $n\""),
        );
        let driver = SessionDriverContext {
            execution_id: "exec-drive".to_string(),
            node_id: "node-drive".to_string(),
            event_bus: None,
            interaction_registry: None,
            suggester: None,
            round_capture: None,
        };
        let outcome = drive_session(
            &entity,
            &store,
            &driver,
            vec![Value::String("world".to_string())],
        )
        .await
        .expect("preset input completes the session");
        assert!(
            outcome.output.contains("hi world"),
            "output was: {}",
            outcome.output
        );
        assert_eq!(outcome.completed_rounds, 1);
        assert!(entity.is_completed());
    }

    #[tokio::test]
    async fn test_drive_session_round_timeout() {
        let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
        let mut config = test_config("printf 'name: '; read n; echo \"hi $n\"");
        config.round_timeout_ms = 200;
        let entity = InteractiveScriptSessionEntity::new(Id::from("drive-2".to_string()), config);
        let driver = SessionDriverContext {
            execution_id: "exec-timeout".to_string(),
            node_id: "node-timeout".to_string(),
            event_bus: None,
            interaction_registry: Some(Arc::new(InteractionRegistry::new())),
            suggester: None,
            round_capture: None,
        };
        let err = drive_session(&entity, &store, &driver, vec![])
            .await
            .expect_err("no input arrives before the round timeout");
        assert!(err.to_string().contains("timed out"), "error was: {err}");
        assert!(entity.is_failed());
        if let Some(shell_session) = entity.shell_session() {
            let _ = store.kill(&shell_session);
        }
    }

    struct FixedSuggester(String);

    #[async_trait]
    impl SuggestionProvider for FixedSuggester {
        async fn suggest(
            &self,
            _prompt: &str,
            _output_tail: &str,
            _history: &[InteractionRecord],
        ) -> Option<String> {
            Some(self.0.clone())
        }
    }

    #[tokio::test]
    async fn test_drive_session_model_assisted_adopts_suggestion() {
        let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
        let mut config = test_config("printf 'name: '; read n; echo \"hi $n\"");
        config.interaction_mode = InteractionMode::LlmAssisted;
        let entity =
            InteractiveScriptSessionEntity::new(Id::from("drive-model".to_string()), config);
        let driver = SessionDriverContext {
            execution_id: "exec-model".to_string(),
            node_id: "node-model".to_string(),
            event_bus: None,
            interaction_registry: Some(Arc::new(InteractionRegistry::new())),
            suggester: Some(Arc::new(FixedSuggester("modelhi".to_string()))),
            round_capture: None,
        };
        let outcome = drive_session(&entity, &store, &driver, vec![])
            .await
            .expect("model suggestion completes the round");
        assert!(
            outcome.output.contains("hi modelhi"),
            "output was: {}",
            outcome.output
        );
        assert_eq!(
            outcome.interaction_history[0].source,
            InteractionSource::Model
        );
        assert!(entity.is_completed());
    }

    #[tokio::test]
    async fn test_drive_session_hybrid_timeout_falls_back_to_suggestion() {
        let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
        let mut config = test_config("printf 'name: '; read n; echo \"hi $n\"");
        config.interaction_mode = InteractionMode::Hybrid;
        config.round_timeout_ms = 200;
        let entity =
            InteractiveScriptSessionEntity::new(Id::from("drive-hybrid".to_string()), config);
        let driver = SessionDriverContext {
            execution_id: "exec-hybrid".to_string(),
            node_id: "node-hybrid".to_string(),
            event_bus: None,
            interaction_registry: Some(Arc::new(InteractionRegistry::new())),
            suggester: Some(Arc::new(FixedSuggester("fallbackhi".to_string()))),
            round_capture: None,
        };
        let outcome = drive_session(&entity, &store, &driver, vec![])
            .await
            .expect("hybrid timeout adopts the suggestion");
        assert!(
            outcome.output.contains("hi fallbackhi"),
            "output was: {}",
            outcome.output
        );
        assert_eq!(
            outcome.interaction_history[0].source,
            InteractionSource::HybridConfirmed
        );
    }

    #[tokio::test]
    async fn test_drive_session_debounce_still_completes() {
        let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
        let mut config = test_config("printf 'name: '; read n; echo \"hi $n\"");
        config.debounce_ms = 150;
        let entity =
            InteractiveScriptSessionEntity::new(Id::from("drive-debounce".to_string()), config);
        let driver = SessionDriverContext {
            execution_id: "exec-debounce".to_string(),
            node_id: "node-debounce".to_string(),
            event_bus: None,
            interaction_registry: None,
            suggester: None,
            round_capture: None,
        };
        let outcome = drive_session(
            &entity,
            &store,
            &driver,
            vec![Value::String("steady".to_string())],
        )
        .await
        .expect("debounced prompt still fires exactly once");
        assert!(
            outcome.output.contains("hi steady"),
            "output was: {}",
            outcome.output
        );
        assert_eq!(outcome.completed_rounds, 1);
    }

    #[test]
    fn test_parse_confirmation_envelope() {
        use serde_json::json;
        assert!(matches!(
            parse_confirmation(&Value::Null),
            ConfirmationAction::Confirm
        ));
        assert!(matches!(
            parse_confirmation(&json!("")),
            ConfirmationAction::Confirm
        ));
        assert!(matches!(
            parse_confirmation(&json!("typed")),
            ConfirmationAction::Edit(_)
        ));
        assert!(matches!(
            parse_confirmation(&json!({"action": "confirm"})),
            ConfirmationAction::Confirm
        ));
        assert!(matches!(
            parse_confirmation(&json!({"action": "edit", "value": "v"})),
            ConfirmationAction::Edit(_)
        ));
        assert!(matches!(
            parse_confirmation(&json!({"value": "v"})),
            ConfirmationAction::Edit(_)
        ));
    }

    #[test]
    fn test_replay_inputs_follow_round_order() {
        let snapshot = InteractiveScriptSessionSnapshot {
            session_id: "s".to_string(),
            script_name: "test".to_string(),
            phase: SessionPhase::WaitingInput,
            current_command: "cmd".to_string(),
            executed_commands: vec![],
            accumulated_stdout_len: 0,
            accumulated_stderr_len: 0,
            interaction_history: vec![
                InteractionRecord {
                    round: 2,
                    prompt: "second:".to_string(),
                    response: "b".to_string(),
                    source: InteractionSource::External,
                },
                InteractionRecord {
                    round: 1,
                    prompt: "first:".to_string(),
                    response: "a".to_string(),
                    source: InteractionSource::Preset,
                },
            ],
            completed_rounds: 2,
            waiting_for_input: true,
            current_prompt: Some("third:".to_string()),
            parent_execution_id: None,
            hierarchy_depth: 1,
            ancestors: vec![],
        };
        assert_eq!(
            replay_inputs(&snapshot),
            vec![
                Value::String("a".to_string()),
                Value::String("b".to_string())
            ]
        );
    }
}
