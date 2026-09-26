//! Interactive script session subsystem, re-exported from its home in
//! `wf-execution-shared` where it lives as shared execution infrastructure.

pub use wf_execution_shared::interactive_script_session::{
    detect_prompt, InteractiveScriptSessionEntity,
};
pub use wf_execution_shared::interactive_script_session::{
    drive_session, parse_confirmation, replay_inputs, ConfirmationAction, InteractionRecord,
    InteractionSource, InteractiveScriptSessionConfig, InteractiveScriptSessionSnapshot,
    InteractiveScriptSessionState, LlmSuggestionProvider, RoundCapture, SessionDriverContext,
    SessionOutcome, SessionPhase, SessionRegistry, SuggestionProvider,
};
