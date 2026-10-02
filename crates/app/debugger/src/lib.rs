pub mod cli;
pub mod dimensions;
pub mod engine;
pub mod input;
pub mod model;
pub mod probes;

pub use engine::{replay_trace, run_check, CheckOutcome, ReplayOutcome};
pub use model::{SectionReport, Trace, TraceKind, UnifiedReport};
