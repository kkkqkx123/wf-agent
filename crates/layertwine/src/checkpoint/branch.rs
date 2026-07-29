//! Branch Entity - Lightweight Branching
//!
//! Branch essence: only one pointer (CheckpointId), zero data replication.
//! Reference architecture/05-Checkpoint Warehouse and Branch Management.md §5.3

use crate::core::types::CheckpointId;
use serde::{Deserialize, Serialize};

/// Lightweight Branches
///
/// Stores the branch name and a variable pointer to the latest Checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    /// Branch name (e.g. main, task/agent-123)
    pub name: String,
    /// Variable pointer: points to the latest checkpoint
    pub head: CheckpointId,
    /// Creation time (Unix milliseconds)
    pub created_at: i64,
    /// Update time (Unix milliseconds)
    pub updated_at: i64,
}

impl Branch {
    /// Creating a new branch
    pub fn new(name: &str, head: CheckpointId) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Branch {
            name: name.to_string(),
            head,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update branch head pointer
    pub fn set_head(&mut self, checkpoint_id: CheckpointId) {
        self.head = checkpoint_id;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::ContentId;

    fn dummy_checkpoint_id() -> CheckpointId {
        ContentId::from_content(b"test-checkpoint")
    }

    #[test]
    fn test_branch_creation() {
        let head = dummy_checkpoint_id();
        let branch = Branch::new("main", head);
        assert_eq!(branch.name, "main");
        assert_eq!(branch.head, head);
    }

    #[test]
    fn test_branch_set_head() {
        let head1 = dummy_checkpoint_id();
        let head2 = ContentId::from_content(b"new-checkpoint");
        let mut branch = Branch::new("feature", head1);
        branch.set_head(head2);
        assert_eq!(branch.head, head2);
    }
}
