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
    /// Redo stack: snapshots that were rolled back via `rollback_one_with_redo`
    /// and can be restored via `redo_one`. Items are ordered from oldest (bottom)
    /// to most recent (top).
    #[serde(default)]
    pub redo_stack: Vec<SnapshotId>,
}

impl Partition {
    pub fn new(name: String, partition_type: PartitionType, initial_snapshot: SnapshotId) -> Self {
        Partition {
            id: uuid::Uuid::now_v7(),
            name,
            current_snapshot: initial_snapshot,
            history: vec![initial_snapshot],
            partition_type,
            redo_stack: Vec::new(),
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

    /// Roll back one step and push the undone snapshot onto the redo stack.
    ///
    /// Unlike `rollback_one` which truncates history, this preserves the
    /// undone snapshot so it can be restored later via `redo_one`. The
    /// partition pointer moves back one step, and the snapshot that was
    /// removed from the history tail is pushed onto `redo_stack`.
    pub fn rollback_one_with_redo(&mut self) -> Option<SnapshotId> {
        if self.history.len() > 1 {
            let undone = self
                .history
                .pop()
                .expect("history non-empty after len check");
            self.current_snapshot = self.history[self.history.len() - 1];
            self.redo_stack.push(undone);
            Some(self.current_snapshot)
        } else {
            None
        }
    }

    /// Redo: restore the most recently undone snapshot from the redo stack.
    ///
    /// Pops the top of `redo_stack`, appends it to history, and moves the
    /// pointer forward. Returns the restored snapshot ID, or `None` if the
    /// redo stack is empty.
    pub fn redo_one(&mut self) -> Option<SnapshotId> {
        let restored = self.redo_stack.pop()?;
        self.current_snapshot = restored;
        self.history.push(restored);
        Some(restored)
    }

    /// Whether a redo operation is possible.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}
