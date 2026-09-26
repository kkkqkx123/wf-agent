//! Interactive script session subsystem: configuration/state model, session
//! entity, prompt detection, external-input waiting and the session driver
//! that runs one session to completion on a background shell store.

pub mod config;
pub mod detect;
pub mod driver;
pub mod entity;
pub mod input;

#[cfg(test)]
mod tests;

pub use config::{
    InteractionRecord, InteractionSource, InteractiveScriptSessionConfig,
    InteractiveScriptSessionSnapshot, InteractiveScriptSessionState, SessionPhase,
};
pub use detect::detect_prompt;
pub use driver::{
    drive_session, replay_inputs, LlmSuggestionProvider, RoundCapture, SessionDriverContext,
    SessionOutcome, SessionRegistry, SuggestionProvider,
};
pub use entity::InteractiveScriptSessionEntity;
pub use input::{parse_confirmation, ConfirmationAction};
