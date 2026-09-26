//! Interactive script session subsystem, re-exported from its home in
//! `wf-execution-shared` where it lives as shared execution infrastructure.

pub use wf_execution_shared::interactive_script_session::{
    drive_session, parse_confirmation, replay_inputs, ConfirmationAction, InteractionRecord,
    InteractiveScriptSessionConfig, InteractiveScriptSessionSnapshot,
    InteractiveScriptSessionState, InteractionSource, LlmSuggestionProvider, RoundCapture,
    SessionDriverContext, SessionOutcome, SessionPhase, SessionRegistry, SuggestionProvider,
};
pub use wf_execution_shared::interactive_script_session::{
    detect_prompt, InteractiveScriptSessionEntity,
};
