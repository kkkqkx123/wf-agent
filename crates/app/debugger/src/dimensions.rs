pub mod checkpoints;
pub mod compression;
pub mod cost;
pub mod interactions;
pub mod interruptions;
pub mod loops;
pub mod merges;
pub mod spec;
pub mod subexec;

use crate::model::{SectionReport, Trace};

pub fn analyze_all(trace: &Trace) -> Vec<SectionReport> {
    vec![
        loops::analyze(trace),
        merges::analyze(trace),
        interruptions::analyze(trace),
        checkpoints::analyze(trace),
        interactions::analyze(trace),
        subexec::analyze(trace),
        cost::analyze(trace),
        compression::analyze(trace),
        spec::analyze(trace),
    ]
}
