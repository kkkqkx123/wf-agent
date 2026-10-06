//! Headless session driver: `wf run "<prompt>"` end to end.
//!
//! Pipeline: preset the execution id → register the headless interaction
//! guard → stream the agent loop through [`wf_api`] → render the agent
//! events (LLM text to the main sink, tool lifecycle to the diagnostics
//! channel) → terminate with a summary line / envelope and an exit code.
//!
//! Output discipline: business output (LLM text,
//! message records, summary envelope) goes through the [`OutputSink`]
//! (stdout); diagnostics (tool lines `▲/✓/✗`, approval rejections,
//! interrupt notices) go to stderr through [`DiagWriter`], so
//! `wf run | jq` keeps working.

use std::sync::{Arc, Mutex};

use crate::output::{OutputFormat, OutputSink};
pub use crate::run_diag::DiagWriter;
use crate::stdio_prompt::DEFAULT_APPROVAL_TIMEOUT_SECS;
use crate::turn::{TurnKind, TurnParams};

pub use crate::run_embedded::run_session;
pub use crate::run_remote::run_session_remote;
pub use crate::run_render::DeltaBuffer;

/// Options for one headless session.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Initial user message (required, non-empty for agent turns).
    pub prompt: String,
    /// Agent definition id (defaults to `cli`).
    pub agent_id: Option<String>,
    /// LLM profile id (defaults to `default`).
    pub model: Option<String>,
    /// Pre-authorization prefixes from `--approve-prefix`.
    pub approve_prefixes: Vec<String>,
    /// Workflow id to execute instead of an agent turn.
    pub workflow: Option<String>,
    /// Workflow input JSON string (requires workflow).
    pub workflow_input: Option<String>,
    /// Read approval/follow-up answers from stdin (`--interactive`).
    pub interactive: bool,
    /// Wait bound for one stdin answer line (`--approval-timeout`).
    pub approval_timeout_secs: u64,
    /// Approve every routed tool call without prompting (`--assume-yes`).
    pub assume_yes: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            agent_id: None,
            model: None,
            approve_prefixes: Vec::new(),
            workflow: None,
            workflow_input: None,
            interactive: false,
            approval_timeout_secs: DEFAULT_APPROVAL_TIMEOUT_SECS,
            assume_yes: false,
        }
    }
}

impl RunOptions {
    /// Single conversion into the shared [`TurnParams`] abstraction so the
    /// headless driver and the interactive forms assemble agent config in
    /// exactly one place (`turn::build_agent_loop_params`).
    pub fn as_turn_params(&self) -> TurnParams {
        let kind = match self.workflow.clone() {
            Some(workflow_id) => {
                let input = crate::turn::parse_workflow_input(self.workflow_input.as_deref())
                    .unwrap_or(None);
                TurnKind::Workflow { workflow_id, input }
            }
            None => TurnKind::Agent {
                prompt: self.prompt.clone(),
            },
        };
        TurnParams {
            agent: self.agent_id.clone(),
            model: self.model.clone(),
            approve_prefixes: self.approve_prefixes.clone(),
            conversation: Vec::new(),
            kind,
        }
    }
}

/// Outcome of a completed headless session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub execution_id: String,
    pub iterations: u32,
    pub duration_ms: u64,
    pub had_output: bool,
}

/// IO bundle for a headless session: main sink, diagnostics channel and
/// the format layer answer.
pub struct RunIo {
    pub sink: Box<dyn OutputSink + Send>,
    pub diag: Arc<Mutex<DiagWriter>>,
    pub format: OutputFormat,
}
