pub mod assert;
pub mod branches;
pub mod format;
pub mod replay;
pub mod runner;
pub mod timeline;

pub use assert::run_assertions;
pub use branches::{
    evaluate_decision, summarize_verdicts, BranchVerdict, CoverageSummary, DecisionVerdict,
};
pub use format::{format_json, format_text};
pub use replay::{replay_trace, ReplayOutcome, ReplaySummary, StepOutcome};
pub use runner::{
    render_check, render_check_json, render_check_text, render_replay, run_check, CheckOutcome,
};
pub use timeline::{build_timeline, TimelineEntry};
