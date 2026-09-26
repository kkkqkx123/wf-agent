//! Immutable configuration and serializable state model of one interactive
//! script session.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use wf_script::InteractionMode;

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
    pub(super) fn new(command: &str) -> Self {
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
