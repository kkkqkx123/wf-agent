//! Session driver: the main loop that runs one interactive script session to
//! completion on a background shell store, plus the driver context, the model
//! suggestion provider, the workspace capture handle and the live session
//! registry.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::types::execution_entity::{ExecutionEntity, ExecutionStatus};
use async_trait::async_trait;
use serde_json::Value;
use wf_core::EventBus;
use wf_script::{InteractionMode, RiskEvaluator};
use wf_types::events::{BaseEvent, EventType};
use wf_types::Id;

use crate::error::{ExecutionSharedError, ExecutionSharedResult};
use crate::interaction::InteractionRegistry;

use super::config::{
    InteractionRecord, InteractionSource, InteractiveScriptSessionConfig,
    InteractiveScriptSessionSnapshot, SessionPhase,
};
use super::detect::detect_prompt;
use super::entity::InteractiveScriptSessionEntity;
use super::input::{
    await_external_input, fail_wait, parse_confirmation, wait_for_external_input,
    ConfirmationAction,
};

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

pub(super) fn driver_registry(driver: &SessionDriverContext) -> Arc<InteractionRegistry> {
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
            timeout_ms: None,
        };
        let outcome = crate::single_shot::generate_text_once(&self.gateway, &request, None)
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

pub(super) fn emit_session_event(
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
) -> ExecutionSharedResult<SessionOutcome> {
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
        .map_err(|e| ExecutionSharedError::Internal(format!("failed to spawn session: {e}")))?;
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
            return Err(ExecutionSharedError::NodeFailure {
                node_id: driver.node_id.clone(),
                category: wf_types::workflow::error_branch::NodeErrorCategory::CancelledInterrupted,
                detail: "interactive session was cancelled".to_string(),
            });
        }
        if std::time::Instant::now() >= session_deadline {
            entity.set_status(ExecutionStatus::Failed).await;
            return Err(ExecutionSharedError::NodeFailure {
                node_id: driver.node_id.clone(),
                category: wf_types::workflow::error_branch::NodeErrorCategory::TransportTimeout,
                detail: "interactive session exceeded its total timeout".to_string(),
            });
        }

        let session = shell.get(&session_id).ok_or_else(|| {
            ExecutionSharedError::Internal(format!("session '{session_id}' disappeared"))
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
                return Err(ExecutionSharedError::Internal(format!(
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
                return Err(ExecutionSharedError::Internal(format!(
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
                .map_err(|e| {
                    ExecutionSharedError::Internal(format!("failed to send input: {e}"))
                })?;
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
) -> ExecutionSharedResult<(Value, InteractionSource)> {
    let Some(suggester) = driver.suggester.clone() else {
        entity.set_status(ExecutionStatus::Failed).await;
        return Err(ExecutionSharedError::Internal(
            "model-assisted session has no preset input for this prompt and no model gateway is attached".to_string(),
        ));
    };
    if *autonomous_rounds >= config.autonomous_round_limit() {
        entity.set_status(ExecutionStatus::Failed).await;
        return Err(ExecutionSharedError::Internal(format!(
            "model-assisted session exceeded max autonomous rounds ({})",
            config.autonomous_round_limit()
        )));
    }
    let Some(suggestion) = suggester.suggest(pattern, output_tail, history).await else {
        entity.set_status(ExecutionStatus::Failed).await;
        return Err(ExecutionSharedError::Internal(
            "model-assisted session received no usable model suggestion".to_string(),
        ));
    };
    if RiskEvaluator::evaluate(&suggestion).rank() >= wf_script::ScriptRiskLevel::High.rank() {
        match await_external_input(driver, pattern, config, Some(&suggestion)).await {
            super::input::ExternalWaitOutcome::Answered(value) => {
                match parse_confirmation(&value) {
                    ConfirmationAction::Confirm => {
                        Ok((Value::String(suggestion), InteractionSource::Model))
                    }
                    ConfirmationAction::Edit(text) => {
                        Ok((Value::String(text), InteractionSource::External))
                    }
                }
            }
            outcome => Err(fail_wait(entity, &driver.node_id, outcome, config).await),
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
) -> ExecutionSharedResult<(Value, InteractionSource)> {
    let suggestion = match driver.suggester.clone() {
        Some(suggester) => suggester.suggest(pattern, output_tail, history).await,
        None => None,
    };
    let Some(suggestion) = suggestion else {
        let value = wait_for_external_input(entity, driver, pattern, config, None).await?;
        return Ok((value, InteractionSource::External));
    };
    match await_external_input(driver, pattern, config, Some(&suggestion)).await {
        super::input::ExternalWaitOutcome::Answered(value) => match parse_confirmation(&value) {
            ConfirmationAction::Confirm => Ok((
                Value::String(suggestion),
                InteractionSource::HybridConfirmed,
            )),
            ConfirmationAction::Edit(text) => {
                Ok((Value::String(text), InteractionSource::HybridEdited))
            }
        },
        super::input::ExternalWaitOutcome::TimedOut if config.hybrid_fallback_to_suggestion => {
            Ok((
                Value::String(suggestion),
                InteractionSource::HybridConfirmed,
            ))
        }
        outcome => Err(fail_wait(entity, &driver.node_id, outcome, config).await),
    }
}

fn output_trail(output: &str) -> String {
    let start = output.len().saturating_sub(512);
    output[start..].to_string()
}
