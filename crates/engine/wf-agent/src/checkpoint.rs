pub mod coordinator;
pub mod strategy;

pub use coordinator::{AgentCheckpointIntegration, RestoreMode, RestoredAgentLoop};

/// One checkpoint anchor on an execution timeline, ordered by sequence end.
#[derive(Debug, Clone)]
pub struct TimelineEntry {
    pub checkpoint_id: String,
    pub seq_start: Option<u64>,
    pub seq_end: Option<u64>,
    pub trigger: Option<String>,
    pub timestamp: Option<i64>,
}

pub use strategy::{AgentCheckpointStrategy, AgentCheckpointTiming};
