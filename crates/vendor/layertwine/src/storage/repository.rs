use crate::checkpoint::branch::Branch;
use crate::checkpoint::types::Checkpoint;
use crate::core::delta::Delta;
use crate::core::edit_session::EditSession;
use crate::core::file_move::FileMove;
use crate::core::file_node::FileNode;
use crate::core::partition::Partition;
use crate::core::snapshot::Snapshot;
use crate::core::types::{
    CheckpointId, DeltaId, EditSessionId, PartitionId, PartitionType, SnapshotId,
};
use crate::StorageResult;

/// Snapshot storage trait
pub trait SnapshotStore {
    /// Storage Snapshot
    fn store_snapshot(&self, snapshot: &Snapshot, content: &[u8]) -> StorageResult<()>;

    /// Batch store snapshots with atomic guarantee
    ///
    /// Default implementation stores snapshots sequentially.
    /// Implementations can override this for better performance using transactions.
    fn store_snapshots_batch(&self, snapshots: &[(&Snapshot, &[u8])]) -> StorageResult<()> {
        for (snapshot, content) in snapshots {
            self.store_snapshot(snapshot, content)?;
        }
        Ok(())
    }

    /// Getting a Snapshot
    fn get_snapshot(&self, id: &SnapshotId) -> StorageResult<Snapshot>;
    /// Query Snapshots by Path
    fn find_snapshots_by_file(&self, file_path: &str) -> StorageResult<Vec<Snapshot>>;
    /// Query Snapshots by Partition Type
    fn find_snapshots_by_partition(
        &self,
        partition_type: &PartitionType,
    ) -> StorageResult<Vec<Snapshot>>;
    /// Determining if a snapshot exists
    fn snapshot_exists(&self, id: &SnapshotId) -> StorageResult<bool>;

    /// Chain-head delta of each snapshot (lightweight: skips content blobs).
    ///
    /// Returns `(snapshot_id, chain-head delta id)` in input order; snapshots
    /// with an empty delta chain (e.g. full-content snapshots) map to `None`,
    /// missing ids are skipped. Path-scoped provenance queries need this
    /// because a delta-chain snapshot's own file node is its base (see
    /// `Snapshot::from_parent`), so the edited path is only visible through
    /// the chain head — `find_snapshots_by_file` alone would miss those rows.
    fn snapshot_chain_heads(
        &self,
        ids: &[SnapshotId],
    ) -> StorageResult<Vec<(SnapshotId, Option<DeltaId>)>> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let snapshot = self.get_snapshot(id)?;
            out.push((*id, snapshot.deltas.last().copied()));
        }
        Ok(out)
    }

    /// Query snapshots by file path and optional time range.
    /// `time_range` is inclusive `(start, end)` in milliseconds.
    fn find_snapshots_by_file_and_time(
        &self,
        file_path: &str,
        time_range: Option<(i64, i64)>,
    ) -> StorageResult<Vec<Snapshot>> {
        let all = self.find_snapshots_by_file(file_path)?;
        match time_range {
            Some((start, end)) => Ok(all
                .into_iter()
                .filter(|s| s.created_at >= start && s.created_at <= end)
                .collect()),
            None => Ok(all),
        }
    }
}

/// Delta storage trait
pub trait DeltaStore {
    /// Storage Delta
    fn store_delta(&self, delta: &Delta) -> StorageResult<()>;
    /// Get Delta
    fn get_delta(&self, id: &DeltaId) -> StorageResult<Delta>;
    /// Batch acquisition of Delta
    fn get_deltas(&self, ids: &[DeltaId]) -> StorageResult<Vec<Delta>>;
    /// Determine if Delta exists
    fn delta_exists(&self, id: &DeltaId) -> StorageResult<bool>;

    /// Query deltas by file path and optional time range.
    /// `time_range` is inclusive `(start, end)` in milliseconds.
    fn find_deltas_by_file_and_time(
        &self,
        file_path: &str,
        time_range: Option<(i64, i64)>,
    ) -> StorageResult<Vec<Delta>> {
        let _ = (file_path, time_range);
        Ok(Vec::new())
    }
}

/// Edit Session storage trait
pub trait EditSessionStore {
    /// Store an edit session
    fn store_session(&self, session: &EditSession) -> StorageResult<()>;
    /// Get an edit session by ID
    fn get_session(&self, id: &EditSessionId) -> StorageResult<EditSession>;
    /// List all edit sessions (ordered by created_at)
    fn list_sessions(&self) -> StorageResult<Vec<EditSession>>;
    /// Get all delta IDs belonging to a session
    fn get_session_deltas(&self, session_id: &EditSessionId) -> StorageResult<Vec<DeltaId>>;
    /// Get the session that a delta belongs to (if any)
    fn get_delta_session(&self, delta_id: &DeltaId) -> StorageResult<Option<EditSession>>;
    /// Delete an edit session and its associations
    fn delete_session(&self, id: &EditSessionId) -> StorageResult<()>;
    /// Append a single delta to an existing session (assigns the next `seq`).
    fn append_delta_to_session(
        &self,
        session_id: &EditSessionId,
        delta_id: &DeltaId,
    ) -> StorageResult<()> {
        let _ = (session_id, delta_id);
        Ok(())
    }
    /// Associate a snapshot with a session (covers full-content snapshots
    /// that carry no delta). Assigns the next `seq` within the session.
    fn associate_snapshot_with_session(
        &self,
        session_id: &EditSessionId,
        snapshot_id: &SnapshotId,
    ) -> StorageResult<()> {
        let _ = (session_id, snapshot_id);
        Ok(())
    }
    /// Get all snapshot IDs belonging to a session, in creation order.
    fn get_session_snapshots(&self, session_id: &EditSessionId) -> StorageResult<Vec<SnapshotId>> {
        let _ = session_id;
        Ok(Vec::new())
    }
    /// Get the session that a snapshot belongs to (if any).
    fn get_snapshot_session(&self, snapshot_id: &SnapshotId) -> StorageResult<Option<EditSession>> {
        let _ = snapshot_id;
        Ok(None)
    }
}

/// Partition storage trait
pub trait PartitionStore {
    /// Creating Partitions
    fn create_partition(&self, partition: &Partition) -> StorageResult<()>;
    /// Updating the partition pointer
    fn update_pointer(
        &self,
        partition_id: &PartitionId,
        snapshot_id: &SnapshotId,
    ) -> StorageResult<()>;
    /// Get Partition
    fn get_partition(&self, id: &PartitionId) -> StorageResult<Partition>;
    /// Get partitions by name
    fn get_partition_by_name(&self, name: &str) -> StorageResult<Partition>;
    /// List all partitions
    fn list_partitions(&self) -> StorageResult<Vec<Partition>>;

    /// Reset partition pointer to the given snapshot and truncate history.
    ///
    /// Repositions the partition so that `snapshot_id` becomes the sole
    /// baseline (single history entry). Used by branch switching, which must
    /// not pollute history with the previous branch's state.
    ///
    /// The default implementation falls back to `update_pointer` (append);
    /// backends that support history truncation (e.g., Sqlite) override this.
    fn reset_partition_to(
        &self,
        partition_id: &PartitionId,
        snapshot_id: &SnapshotId,
    ) -> StorageResult<()> {
        self.update_pointer(partition_id, snapshot_id)
    }

    /// Reset partition to its baseline (first history entry).
    ///
    /// Sets current_snapshot to history[0] and truncates history to just that entry.
    /// The default implementation uses `update_pointer` which appends to history;
    /// backends that support history truncation (e.g., Sqlite) should override this
    /// for proper cleanup.
    fn reset_partition_to_baseline(&self, partition_id: &PartitionId) -> StorageResult<()> {
        let partition = self.get_partition(partition_id)?;
        let baseline = partition.history.first().ok_or_else(|| {
            crate::StorageError::NotFound(format!("partition {:?} has empty history", partition_id))
        })?;
        self.update_pointer(partition_id, baseline)
    }

    /// Persist the redo stack for a partition.
    ///
    /// Updates the `partition_data` column to reflect the current redo stack
    /// state. Called after `rollback_one_with_redo` or `redo_one` operations.
    fn update_redo_stack(
        &self,
        _partition_id: &PartitionId,
        _redo_stack: &[SnapshotId],
    ) -> StorageResult<()> {
        Ok(())
    }
}

/// File node storage trait
pub trait FileNodeStore {
    /// Storage file node
    fn store_file_node(&self, file_node: &FileNode, content: &[u8]) -> StorageResult<()>;
    /// Get file content by path and base hash
    fn get_file_content(&self, file_path: &str, base_hash: &[u8; 32]) -> StorageResult<Vec<u8>>;
    /// Determine if a file node exists
    fn file_node_exists(&self, file_path: &str, base_hash: &[u8; 32]) -> StorageResult<bool>;
}

/// File move/rename tracking storage trait
pub trait FileMoveStore {
    /// Record a file move/rename operation
    fn store_file_move(&self, file_move: &FileMove) -> StorageResult<()>;
    /// Get all moves originating from a path (file was moved FROM this path)
    fn get_moves_from(&self, from_path: &str) -> StorageResult<Vec<FileMove>>;
    /// Get all moves targeting a path (file was moved TO this path)
    fn get_moves_to(&self, to_path: &str) -> StorageResult<Vec<FileMove>>;
    /// Trace the full rename chain for a path (walks backwards through moves)
    fn trace_rename_chain(&self, path: &str) -> StorageResult<Vec<FileMove>>;
}

/// Atomic operations trait for transactional guarantees
pub trait AtomicOps {
    /// Execute the given closure with atomic (transactional) guarantees.
    ///
    /// Default implementation: no-op wrapping (caller manages atomicity).
    /// Sqlite backend overrides this with SAVEPOINT-based transactions.
    fn with_atomic<F, T>(&self, f: F) -> StorageResult<T>
    where
        F: FnOnce(&Self) -> StorageResult<T>,
    {
        f(self)
    }
}

/// Metadata storage trait
///
/// Used for storing repository-wide metadata such as current branch name.
pub trait MetadataStore {
    /// Store arbitrary metadata key-value pair
    fn store_metadata(&self, key: &str, value: &str) -> StorageResult<()>;
    /// Load metadata value by key
    fn load_metadata(&self, key: &str) -> StorageResult<Option<String>>;
    /// Delete a metadata key. Returns true when the key existed.
    fn delete_metadata(&self, key: &str) -> StorageResult<bool>;
}

/// Combined storage trait (full storage interface)
pub trait Repository:
    SnapshotStore + DeltaStore + PartitionStore + FileNodeStore + CheckpointPersist + AtomicOps
{
}

/// Checkpoint & Branch persistence trait — unified interface for CheckpointRepo
///
/// Combines checkpoint, branch, metadata, and snapshot storage into a single trait.
/// Storage backends (e.g. Sqlite) implement this trait directly.
pub trait CheckpointPersist:
    MetadataStore + SnapshotStore + FileNodeStore + DeltaStore + Send + Sync
{
    /// Store a checkpoint
    fn store_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()>;
    /// Get a checkpoint by ID
    fn get_checkpoint(&self, id: &CheckpointId) -> StorageResult<Checkpoint>;
    /// Check if a checkpoint exists
    fn checkpoint_exists(&self, id: &CheckpointId) -> StorageResult<bool>;
    /// List all checkpoints
    fn list_checkpoints(&self) -> StorageResult<Vec<Checkpoint>>;
    /// Delete a checkpoint
    fn delete_checkpoint(&self, id: &CheckpointId) -> StorageResult<()>;

    /// Store a branch
    fn store_branch(&self, branch: &Branch) -> StorageResult<()>;
    /// Get a branch by name
    fn get_branch(&self, name: &str) -> StorageResult<Branch>;
    /// Update a branch's head pointer
    fn update_branch_head(&self, name: &str, head: &CheckpointId) -> StorageResult<()>;
    /// List all branches
    fn list_branches(&self) -> StorageResult<Vec<Branch>>;
    /// Delete a branch
    fn delete_branch(&self, name: &str) -> StorageResult<()>;
}
