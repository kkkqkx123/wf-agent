use std::sync::Arc;

use wf_checkpoint::ExecutionEventBus;
use wf_checkpoint::event::CheckpointEventBus;
use wf_storage::backend::StorageBackend;

/// The checkpoint wiring a child execution inherits from its parent.
///
/// Carries only the depth-independent pieces (store and event buses). The
/// node strategy is resolved at spawn time with
/// `NodeCheckpointStrategy::for_depth`, so children always checkpoint more
/// sparsely than the root without copying the parent's cadence counter.
#[derive(Clone)]
pub struct CheckpointScope {
    pub store: Arc<StorageBackend>,
    pub event_bus: Option<CheckpointEventBus>,
    pub execution_events: Option<ExecutionEventBus>,
}

impl CheckpointScope {
    pub fn new(store: Arc<StorageBackend>) -> Self {
        Self {
            store,
            event_bus: None,
            execution_events: None,
        }
    }

    pub fn with_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_execution_events(mut self, bus: ExecutionEventBus) -> Self {
        self.execution_events = Some(bus);
        self
    }
}
