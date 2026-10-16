//! TUI debug utilities: diff recording, scenario replay, key receiver and
//! animation helpers.

pub mod scenario;

#[cfg(feature = "diff-record")]
pub mod diff_recorder;

#[cfg(feature = "diff-record")]
pub use diff_recorder::DiffRecorderBackend;
pub use scenario::{
    replay_scenario_json, ScenarioRecorder, TuiScenario,
};
