use crate::core::types::{PartitionId, PartitionType, SnapshotId};
use serde::{Deserialize, Serialize};

/// Partition - partition (variable pointer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partition {
    /// Partition ID
    pub id: PartitionId,
    /// partition name
    pub name: String,
    /// Current active snapshot ID (pointer)
    pub current_snapshot: SnapshotId,
    /// List of historical snapshot IDs (full retention)
    pub history: Vec<SnapshotId>,
    /// Partition type
    pub partition_type: PartitionType,
}

impl Partition {
    pub fn new(name: String, partition_type: PartitionType, initial_snapshot: SnapshotId) -> Self {
        Partition {
            id: uuid::Uuid::now_v7(),
            name,
            current_snapshot: initial_snapshot,
            history: vec![initial_snapshot],
            partition_type,
        }
    }

    /// Updating the current snapshot pointer (preserving history)
    pub fn advance(&mut self, new_snapshot: SnapshotId) {
        self.current_snapshot = new_snapshot;
        self.history.push(new_snapshot);
    }

    /// Fall back to the specified ID in the history (only the pointer is switched, no data is moved)
    pub fn rollback_to(&mut self, target_snapshot: &SnapshotId) -> bool {
        if let Some(pos) = self.history.iter().position(|s| s == target_snapshot) {
            self.current_snapshot = *target_snapshot;
            self.history.truncate(pos + 1);
            true
        } else {
            false
        }
    }

    /// take a step back
    pub fn rollback_one(&mut self) -> Option<SnapshotId> {
        if self.history.len() > 1 {
            let prev = self.history[self.history.len() - 2];
            self.current_snapshot = prev;
            self.history.pop();
            Some(prev)
        } else {
            None
        }
    }
}
