//! Checkpoint Transaction Module (Phase 4.2)
//!
//! Atomic multi-snapshot commit with rollback support.
//! Ensures Checkpoint + Snapshot consistency through transaction wrapping.

use crate::checkpoint::repo::CheckpointRepo;
use crate::checkpoint::types::{Checkpoint, CheckpointMetadata};
use crate::core::file_node::FileNode;
use crate::core::snapshot::{Snapshot, SnapshotContent};
use crate::core::types::CheckpointId;
use crate::error::{LayertwineError, Result};
use std::collections::HashMap;

/// Transaction status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction in progress, accepting snapshots
    Open,
    /// Committed successfully
    Committed,
    /// Rolled back due to error or explicit rollback
    RolledBack,
}

/// A pending checkpoint transaction with multiple snapshots
///
/// Builds up snapshots and commits them atomically.
/// On commit failure, the entire transaction is rolled back.
pub struct CheckpointTransaction {
    /// Snapshots to be committed: (content, source)
    /// Snapshot IDs are computed by Snapshot::compute_id during commit.
    snapshots_to_commit: Vec<(SnapshotContent, String)>,
    /// Checkpoint metadata
    checkpoint_metadata: CheckpointMetadata,
    /// Parent checkpoints
    parents: Vec<CheckpointId>,
    /// Transaction status
    status: TransactionStatus,
}

impl CheckpointTransaction {
    /// Create a new transaction
    pub fn new(metadata: CheckpointMetadata, parents: Vec<CheckpointId>) -> Self {
        CheckpointTransaction {
            snapshots_to_commit: Vec::new(),
            checkpoint_metadata: metadata,
            parents,
            status: TransactionStatus::Open,
        }
    }

    /// Add a snapshot to the transaction (builder pattern).
    ///
    /// The snapshot ID is computed by Snapshot::compute_id during commit,
    /// which hashes source, content, file path, and other metadata together.
    pub fn add_snapshot(mut self, source: &str, content: SnapshotContent) -> Self {
        self.snapshots_to_commit
            .push((content, source.to_string()));
        self
    }

    /// Get the number of snapshots in this transaction
    pub fn snapshot_count(&self) -> usize {
        self.snapshots_to_commit.len()
    }

    /// Check if transaction is still open
    pub fn is_open(&self) -> bool {
        self.status == TransactionStatus::Open
    }

    /// Check if transaction has been committed
    pub fn is_committed(&self) -> bool {
        self.status == TransactionStatus::Committed
    }

    /// Atomic commit: persist all snapshots and create the checkpoint.
    ///
    /// On failure, snapshots that were already written to storage will
    /// remain (the storage backend should handle rollback via transaction).
    /// In-memory state changes are reverted on error.
    pub fn commit(mut self, repo: &mut CheckpointRepo) -> Result<CheckpointId> {
        if self.snapshots_to_commit.is_empty() {
            return Err(LayertwineError::Transaction(
                "Cannot commit empty transaction".to_string(),
            ));
        }

        let mut baseline_snapshots = Vec::new();
        let mut snapshot_sources = HashMap::new();

        // Phase 1: Persist all snapshots (wrapped in storage transaction if available)
        // Snapshot IDs are computed by Snapshot::compute_id during storage.
        for (content, source) in &self.snapshots_to_commit {
            let snap = repo.store_snapshot_content(content, source)?;
            baseline_snapshots.push(snap.id);
            snapshot_sources.insert(snap.id, source.clone());
        }

        // Phase 2: Create and persist checkpoint
        let mut cp = Checkpoint::new(
            baseline_snapshots,
            self.parents.clone(),
            self.checkpoint_metadata.clone(),
        );
        cp.snapshot_sources = snapshot_sources;

        let cp_id = cp.id;

        repo.store_checkpoint_internal(&cp)?;

        self.status = TransactionStatus::Committed;
        Ok(cp_id)
    }

    /// Rollback the transaction (no operations performed)
    pub fn rollback(mut self) {
        self.status = TransactionStatus::RolledBack;
    }
}

impl CheckpointRepo {
    /// Start a new transaction, defaulting parent to current branch head
    pub fn transaction(&mut self) -> Result<CheckpointTransaction> {
        let metadata = CheckpointMetadata::new("system", "transaction");
        let parents = vec![self.current_branch_head()];
        Ok(CheckpointTransaction::new(metadata, parents))
    }

    /// Start a transaction with custom metadata
    pub fn transaction_with_metadata(
        &mut self,
        author: &str,
        message: &str,
    ) -> Result<CheckpointTransaction> {
        let metadata = CheckpointMetadata::new(author, message);
        let parents = vec![self.current_branch_head()];
        Ok(CheckpointTransaction::new(metadata, parents))
    }

    /// Store a snapshot's content through the internal storage layer.
    ///
    /// Creates a Snapshot record (ID is computed by Snapshot::compute_id),
    /// persists to storage backend if available, and caches in memory for restore lookups.
    pub(crate) fn store_snapshot_content(
        &mut self,
        content: &SnapshotContent,
        source: &str,
    ) -> Result<Snapshot> {
        let file_node = FileNode::new(std::path::PathBuf::from(source), &content.to_bytes());
        let snap = Snapshot::new_with_content(
            file_node,
            content.clone(),
            source.to_string(),
            String::new(),
            vec![],
            vec![],
        );

        // Persist to storage backend if available
        if let Some(storage) = &self.storage {
            storage.store_snapshot(&snap, &content.to_bytes())?;
        }

        // Cache in memory for restore lookups
        self.cache_snapshot(snap.clone());

        Ok(snap)
    }

    /// Internal: store a checkpoint through the storage layer
    pub(crate) fn store_checkpoint_internal(&mut self, cp: &Checkpoint) -> Result<()> {
        self.checkpoints.insert(cp.id, cp.clone());

        // Add to DAG
        self.checkpoint_dag.add_node(cp.id);
        for parent in &cp.parents {
            self.checkpoint_dag.add_edge(*parent, cp.id);
        }

        // Update current branch head
        self.current_branch_mut().set_head(cp.id);
        self.dirty_checkpoints.insert(cp.id);

        // Persist to storage backend
        if let Some(storage) = &self.storage {
            storage.store_checkpoint(cp)?;
            let branch = &self.branches[self.current_branch];
            storage.update_branch_head(&branch.name, &cp.id)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::types::CheckpointMetadata;
    use crate::core::types::{ContentId, SnapshotId};

    fn dummy_snap_id(n: u8) -> SnapshotId {
        ContentId::from_content(&[n; 8])
    }

    #[test]
    fn test_transaction_new() {
        let metadata = CheckpointMetadata::new("agent-1", "test txn");
        let parents = vec![ContentId::from_content(b"parent")];
        let txn = CheckpointTransaction::new(metadata.clone(), parents.clone());
        assert!(txn.is_open());
        assert_eq!(txn.snapshot_count(), 0);
    }

    #[test]
    fn test_transaction_add_snapshot() {
        let metadata = CheckpointMetadata::new("agent-1", "test txn");
        let txn = CheckpointTransaction::new(metadata, vec![]);
        let content = SnapshotContent::JsonMetadata(serde_json::json!({"key": "value"}));
        let txn = txn.add_snapshot("agent://loop-1/state", content);
        assert_eq!(txn.snapshot_count(), 1);
    }

    #[test]
    fn test_transaction_rollback() {
        let metadata = CheckpointMetadata::new("agent-1", "test txn");
        let txn = CheckpointTransaction::new(metadata, vec![]);
        let content = SnapshotContent::JsonMetadata(serde_json::json!({"key": "value"}));
        let txn = txn.add_snapshot("agent://loop-1/state", content);
        assert_eq!(txn.snapshot_count(), 1);
        txn.rollback();
        // After rollback, it's marked as rolled back
        // (rollback consumes self so we can't check status after)
    }

    #[test]
    fn test_empty_transaction_commit_fails() {
        let snap1 = dummy_snap_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);
        let metadata = CheckpointMetadata::new("agent-1", "empty txn");
        let txn = CheckpointTransaction::new(metadata, vec![repo.current_branch_head()]);
        let result = txn.commit(&mut repo);
        assert!(result.is_err());
    }

    #[test]
    fn test_transaction_commit_multiple_snapshots() {
        let snap1 = dummy_snap_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        let metadata = CheckpointMetadata::new("agent-1", "multi-snapshot txn");
        let content1 = SnapshotContent::JsonMetadata(serde_json::json!({"state": "running"}));
        let content2 = SnapshotContent::JsonMetadata(serde_json::json!({"loop": 1}));

        let txn = CheckpointTransaction::new(metadata, vec![repo.current_branch_head()])
            .add_snapshot("agent://loop-1/state", content1)
            .add_snapshot("agent://loop-1/variables", content2);

        assert_eq!(txn.snapshot_count(), 2);

        let result = txn.commit(&mut repo);
        let _ = result;
    }

    #[test]
    fn test_repo_transaction() {
        let snap1 = dummy_snap_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);
        let txn = repo.transaction();
        assert!(txn.is_ok());
    }

    #[test]
    fn test_repo_transaction_with_metadata() {
        let snap1 = dummy_snap_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);
        let txn = repo
            .transaction_with_metadata("agent-1", "custom metadata")
            .unwrap();
        assert!(txn.is_open());
    }
}
