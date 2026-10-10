//! Read-only provenance queries delegated to `ProvenanceReader`.

use crate::provenance::{DeltaSummary, FileDiffView, PartitionView, WorkspaceFile};
use checkpoint_base::error::CheckpointError;

use super::FileCheckpointManager;

impl FileCheckpointManager {
    fn reader(&self) -> Result<crate::provenance::ProvenanceReader<'_>, CheckpointError> {
        Ok(crate::provenance::ProvenanceReader::new(
            self.git_ref()?,
            self.storage_ref()?,
        ))
    }

    /// All partitions of the file-checkpoint store (actor partitions,
    /// approval, mainline features, main).
    pub fn list_partitions(&self) -> Result<Vec<PartitionView>, CheckpointError> {
        self.reader()?.list_partitions()
    }

    /// Changes recorded in an actor partition, in chronological order.
    pub fn list_changes_by_actor(
        &self,
        actor: &str,
        path_filter: Option<&str>,
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<DeltaSummary>, CheckpointError> {
        self.reader()?
            .list_changes_by_actor(actor, path_filter, time_range)
    }

    /// Changes touching a path across every partition. `time_range`
    /// (inclusive `(start, end)` timestamps) narrows the window.
    pub fn list_changes_by_path(
        &self,
        path: &str,
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<DeltaSummary>, CheckpointError> {
        self.reader()?.list_changes_by_path(path, time_range)
    }

    /// Reconstructed file set of an actor partition (current state).
    pub fn get_actor_workspace(&self, actor: &str) -> Result<Vec<WorkspaceFile>, CheckpointError> {
        self.reader()?.get_actor_workspace(actor)
    }

    /// Per-file diff between two actor workspaces.
    pub fn diff_actors(
        &self,
        actor_a: &str,
        actor_b: &str,
    ) -> Result<Vec<FileDiffView>, CheckpointError> {
        self.reader()?.diff_actors(actor_a, actor_b)
    }

    /// Per-file diff between an actor workspace and the main line.
    pub fn diff_against_main(&self, actor: &str) -> Result<Vec<FileDiffView>, CheckpointError> {
        self.reader()?.diff_against_main(actor)
    }

    /// Files with unresolved merge conflicts across the main and feature
    /// partitions, with re-derived conflict regions (see
    /// [`crate::provenance::list_conflicts`]).
    pub fn list_conflicts(&self) -> Result<Vec<crate::provenance::ConflictFile>, CheckpointError> {
        self.reader()?.list_conflicts()
    }
}
