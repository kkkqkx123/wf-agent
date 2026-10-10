//! Read-only provenance service borrowing the stores.
//!
//! Splits query ownership out of `FileCheckpointManager`: orchestration code
//! builds a reader from the manager's handles, while the manager's own
//! query methods delegate here to keep one implementation.

use crate::git_store::GitStore;
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

use super::conflicts::list_conflicts;
use super::diff::{diff_actors, diff_against_main};
use super::queries::{list_changes_by_actor, list_changes_by_path, list_partitions};
use super::timeline::{file_timeline, FileTimeline};
use super::types::{ConflictFile, DeltaSummary, FileDiffView, PartitionView, WorkspaceFile};
use super::workspace::get_actor_workspace;

pub struct ProvenanceReader<'a> {
    git: &'a GitStore,
    storage: &'a SqliteStorage,
}

impl<'a> ProvenanceReader<'a> {
    pub fn new(git: &'a GitStore, storage: &'a SqliteStorage) -> Self {
        Self { git, storage }
    }

    pub fn list_partitions(&self) -> Result<Vec<PartitionView>, CheckpointError> {
        list_partitions(self.git, self.storage)
    }

    pub fn list_changes_by_actor(
        &self,
        actor: &str,
        path_filter: Option<&str>,
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<DeltaSummary>, CheckpointError> {
        list_changes_by_actor(self.git, self.storage, actor, path_filter, time_range)
    }

    pub fn list_changes_by_path(
        &self,
        path: &str,
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<DeltaSummary>, CheckpointError> {
        list_changes_by_path(self.git, self.storage, path, time_range)
    }

    pub fn get_actor_workspace(&self, actor: &str) -> Result<Vec<WorkspaceFile>, CheckpointError> {
        get_actor_workspace(self.git, actor)
    }

    pub fn diff_actors(
        &self,
        actor_a: &str,
        actor_b: &str,
    ) -> Result<Vec<FileDiffView>, CheckpointError> {
        diff_actors(self.git, actor_a, actor_b)
    }

    pub fn diff_against_main(&self, actor: &str) -> Result<Vec<FileDiffView>, CheckpointError> {
        diff_against_main(self.git, actor)
    }

    pub fn list_conflicts(&self) -> Result<Vec<ConflictFile>, CheckpointError> {
        list_conflicts(self.git)
    }

    pub fn file_timeline(&self, path: &str) -> Result<FileTimeline, CheckpointError> {
        file_timeline(self.git, self.storage, path)
    }
}
