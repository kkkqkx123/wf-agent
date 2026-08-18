//! Checkpoint Restore Module
//!
//! Restore operations: full restore, selective restore by source, time-based restore.
//! Supports recovering Agent/Graph execution state alongside file content.

use crate::checkpoint::repo::CheckpointRepo;
use crate::checkpoint::types::{Checkpoint, CheckpointDiff};
use crate::core::snapshot::SnapshotContent;
use crate::core::types::{source, CheckpointId, SnapshotId};
use crate::error::{LayertwineError, Result};
use std::collections::HashSet;

/// Restore request parameters
///
/// Dispatcher logic in `restore()`:
/// - `time_range` set → time-based restore (range query, nearest to `end`)
/// - `checkpoint_id` set → ID-based restore (full or selective)
/// - Neither set → error
pub struct RestoreRequest {
    /// Target checkpoint ID (used only when time_range is None)
    pub checkpoint_id: Option<CheckpointId>,
    /// Optional source filter (supports glob patterns)
    /// e.g., ["agent://", "file://src/**"]
    pub source_filter: Option<Vec<String>>,
    /// Optional time range (start, end) in Unix milliseconds
    /// Restores from the nearest checkpoint within this range (closest to `end`)
    pub time_range: Option<(i64, i64)>,
}

/// Restore response containing checkpoint, snapshots, and ancestry
pub struct RestoreResponse {
    /// Checkpoint information
    pub checkpoint: Checkpoint,
    /// Snapshot list with content: (snapshot_id, content, source)
    pub snapshots: Vec<(SnapshotId, SnapshotContent, String)>,
    /// Ancestry chain of checkpoint IDs from root to target
    pub ancestry: Vec<CheckpointId>,
}

impl CheckpointRepo {
    /// Dispatch a RestoreRequest to the appropriate restore method.
    ///
    /// Routes:
    /// - time_range set → time-based: queries range, picks nearest to `end`, applies optional filter
    /// - checkpoint_id set → ID-based: full or selective by source_filter
    /// - neither set → Err(InvalidArgument)
    pub fn restore(&self, request: &RestoreRequest) -> Result<RestoreResponse> {
        if let Some((start, end)) = request.time_range {
            let source_filters: Option<Vec<&str>> = request
                .source_filter
                .as_ref()
                .map(|v| v.iter().map(|s| s.as_str()).collect());
            return self.restore_by_time_range(start, end, source_filters.as_deref());
        }

        match &request.checkpoint_id {
            Some(cp_id) => {
                if let Some(ref filters) = request.source_filter {
                    let fil: Vec<&str> = filters.iter().map(|s| s.as_str()).collect();
                    return self.restore_selective(cp_id, fil);
                }
                self.restore_full(cp_id)
            }
            None => Err(LayertwineError::General(
                "RestoreRequest must specify either checkpoint_id or time_range".to_string(),
            )),
        }
    }

    /// Full restore: return all snapshots and their content for a checkpoint.
    ///
    /// Loads all baseline snapshots associated with the checkpoint,
    /// including both file content and JSON metadata snapshots.
    /// Also returns the ancestry chain for delta reconstruction.
    pub fn restore_full(&self, cp_id: &CheckpointId) -> Result<RestoreResponse> {
        let cp = self.get_checkpoint(cp_id)?;
        let ancestry = self.get_ancestry_chain(cp_id)?;
        let snapshots = self.load_all_snapshot_contents(&cp.baseline_snapshots)?;

        Ok(RestoreResponse {
            checkpoint: cp.clone(),
            snapshots,
            ancestry,
        })
    }

    /// Selective restore: filter snapshots by source pattern.
    ///
    /// Examples:
    ///   restore_selective(cp_id, vec!["agent://"])  // only Agent state
    ///   restore_selective(cp_id, vec!["file://src/**"])  // only source files
    ///   restore_selective(cp_id, vec!["agent://", "graph://"])  // Agent + Graph state
    pub fn restore_selective(
        &self,
        cp_id: &CheckpointId,
        source_filters: Vec<&str>,
    ) -> Result<RestoreResponse> {
        let cp = self.get_checkpoint(cp_id)?;
        let ancestry = self.get_ancestry_chain(cp_id)?;

        let filtered_snapshots: Vec<SnapshotId> = cp
            .baseline_snapshots
            .iter()
            .filter(|snap_id| {
                let source_str = cp
                    .snapshot_sources
                    .get(snap_id)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                source_filters
                    .iter()
                    .any(|filter| source::matches_glob(source_str, filter))
            })
            .copied()
            .collect();

        let snapshots = self.load_all_snapshot_contents(&filtered_snapshots)?;

        Ok(RestoreResponse {
            checkpoint: cp.clone(),
            snapshots,
            ancestry,
        })
    }

    /// Time-based restore within a time range.
    ///
    /// Finds the checkpoint nearest to `end` within the [start, end] range.
    /// Optionally filters by source patterns.
    fn restore_by_time_range(
        &self,
        start: i64,
        end: i64,
        source_filters: Option<&[&str]>,
    ) -> Result<RestoreResponse> {
        let candidates = self.time_index.query_range(start, end);
        if candidates.is_empty() {
            return Err(LayertwineError::NotFound(format!(
                "No checkpoint found in time range [{}, {}]",
                start, end
            )));
        }

        let (_, cp_id) = candidates
            .into_iter()
            .max_by_key(|(t, _)| *t)
            .expect("candidates non-empty");

        match source_filters {
            Some(filters) if !filters.is_empty() => {
                self.restore_selective(&cp_id, filters.to_vec())
            }
            _ => self.restore_full(&cp_id),
        }
    }

    /// Point-in-time restore: find the checkpoint nearest to the target time.
    ///
    /// Uses the TimeIndex for O(log n) nearest-neighbor lookup.
    /// Optionally filters by source patterns.
    pub fn restore_by_time(
        &self,
        target_time: i64,
        source_filters: Option<&[&str]>,
    ) -> Result<RestoreResponse> {
        let (_, cp_id) = self.time_index.find_nearest(target_time).ok_or_else(|| {
            LayertwineError::NotFound("No checkpoint near target time".to_string())
        })?;

        match source_filters {
            Some(filters) if !filters.is_empty() => {
                self.restore_selective(&cp_id, filters.to_vec())
            }
            _ => self.restore_full(&cp_id),
        }
    }

    /// List snapshots for a checkpoint with their type, source, and size info
    pub fn list_snapshots(
        &self,
        cp_id: &CheckpointId,
    ) -> Result<Vec<(SnapshotId, String, String, usize)>> {
        let cp = self.get_checkpoint(cp_id)?;

        cp.baseline_snapshots
            .iter()
            .map(|snap_id| {
                let source_str = cp
                    .snapshot_sources
                    .get(snap_id)
                    .cloned()
                    .unwrap_or_default();
                let (content_type, size) = self
                    .get_snapshot_by_id(snap_id)
                    .map(|s| {
                        let ct = s
                            .content
                            .as_ref()
                            .map(|c| c.content_type().to_string())
                            .unwrap_or_else(|| "unknown".to_string());
                        let sz = s.content.as_ref().map(|c| c.to_bytes().len()).unwrap_or(0);
                        (ct, sz)
                    })
                    .unwrap_or_else(|_| ("unknown".to_string(), 0));
                Ok((*snap_id, source_str, content_type, size))
            })
            .collect()
    }

    /// Compute diff between two checkpoints
    pub fn diff_checkpoints(
        &self,
        from_id: &CheckpointId,
        to_id: &CheckpointId,
    ) -> Result<CheckpointDiff> {
        let from_cp = self.get_checkpoint(from_id)?;
        let to_cp = self.get_checkpoint(to_id)?;

        let from_set: HashSet<&SnapshotId> = from_cp.baseline_snapshots.iter().collect();
        let to_set: HashSet<&SnapshotId> = to_cp.baseline_snapshots.iter().collect();

        let removed: Vec<SnapshotId> = from_set
            .iter()
            .filter(|id| !to_set.contains(*id))
            .map(|&&id| id)
            .collect();

        let added: Vec<SnapshotId> = to_set
            .iter()
            .filter(|id| !from_set.contains(*id))
            .map(|&&id| id)
            .collect();

        let common: Vec<&&SnapshotId> = from_set.intersection(&to_set).collect();
        let modified: Vec<SnapshotId> = common
            .iter()
            .filter_map(|&&&snap_id| {
                let from_snap = self.get_snapshot_by_id(&snap_id).ok();
                let to_snap = self.get_snapshot_by_id(&snap_id).ok();
                let from_content = from_snap.as_ref().and_then(|s| s.content.as_ref());
                let to_content = to_snap.as_ref().and_then(|s| s.content.as_ref());
                if from_content != to_content {
                    Some(snap_id)
                } else {
                    None
                }
            })
            .collect();

        Ok(CheckpointDiff {
            from_id: *from_id,
            to_id: *to_id,
            added,
            removed,
            modified,
        })
    }

    /// Validate checkpoint data integrity.
    ///
    /// Checks that all referenced snapshots exist and their IDs match.
    /// Returns list of issues found (empty = valid).
    pub fn validate_integrity(&self, cp_id: &CheckpointId) -> Result<Vec<String>> {
        let mut issues = Vec::new();
        let cp = self.get_checkpoint(cp_id)?;

        for snap_id in &cp.baseline_snapshots {
            match self.get_snapshot_by_id(snap_id) {
                Ok(snap) => {
                    let computed = snap.compute_id();
                    if computed != *snap_id {
                        issues.push(format!(
                            "Snapshot {} has mismatched ID (expected {})",
                            snap_id, computed
                        ));
                    }
                    if !cp.snapshot_sources.contains_key(snap_id) {
                        issues.push(format!(
                            "Snapshot {} is missing source mapping in checkpoint",
                            snap_id
                        ));
                    }
                }
                Err(_) => {
                    issues.push(format!("Snapshot {} referenced but not found", snap_id));
                }
            }
        }

        Ok(issues)
    }

    // Internal helpers

    /// Load all snapshot contents for a list of snapshot IDs
    ///
    /// Snapshots store deltas (not inline content), and a manually/staged
    /// snapshot's `source`/`file` fields are inherited from its parent (the
    /// init marker) rather than the real file path. To make
    /// `RestoredSnapshotInfo` usable for file-based checkpoints we therefore:
    ///   - reconstruct the full file content from the delta chain via
    ///     `reconstruct_text` (when a storage backend is attached), and
    ///   - report the real file path taken from the snapshot's most recent
    ///     delta (`delta.file.path_str()`), falling back to the snapshot's
    ///     own (inherited) `file.path_str()`.
    fn load_all_snapshot_contents(
        &self,
        snap_ids: &[SnapshotId],
    ) -> Result<Vec<(SnapshotId, SnapshotContent, String)>> {
        snap_ids
            .iter()
            .map(|id| {
                let snap = self.get_snapshot_by_id(id)?;
                // Materialize the full file content from the delta chain.
                // Fall back to the (typically empty) stored snapshot content
                // only when no storage backend is attached (e.g. in-memory
                // test repos that never persisted deltas).
                let content = match &self.storage {
                    Some(storage) => {
                        let text = crate::layered::transition::reconstruct_text(&**storage, &snap)?;
                        SnapshotContent::FileContent(text.into_bytes())
                    }
                    None => snap
                        .content
                        .clone()
                        .unwrap_or_else(|| SnapshotContent::FileContent(vec![])),
                };
                // Derive the real file path from the snapshot's most recent
                // delta (the snapshot/file fields are parent-inherited and
                // would otherwise report the init marker). When no storage is
                // attached, fall back to the inherited source for parity.
                let source = match &self.storage {
                    Some(storage) => {
                        let deltas = storage
                            .get_deltas(&snap.deltas)
                            .map_err(LayertwineError::Storage)?;
                        deltas
                            .last()
                            .map(|d| d.file.path_str().to_string())
                            .unwrap_or_else(|| snap.file.path_str().to_string())
                    }
                    None => snap.source.clone(),
                };
                Ok((*id, content, source))
            })
            .collect()
    }
}

/// Result of applying a restore operation (writing snapshot content to disk)
pub struct RestoreApplyResult {
    /// Checkpoint that was restored
    pub checkpoint_id: CheckpointId,
    /// File paths written to disk
    pub files_written: Vec<String>,
}

impl CheckpointRepo {
    /// Apply a restore: write `file://` snapshot contents back to the filesystem.
    ///
    /// Only snapshots with `file://` source URIs are written to disk.
    /// Other sources (`agent://`, `graph://`, etc.) are skipped since they
    /// are managed externally (e.g., by an AI agent application).
    ///
    /// `source_filter` can be used to limit which snapshots are written
    /// (e.g., `["file://src/**"]` to only restore specific directories).
    ///
    /// Returns the list of file paths that were written.
    pub fn apply_restore(
        &self,
        response: &RestoreResponse,
        source_filter: Option<&[&str]>,
    ) -> Result<RestoreApplyResult> {
        let mut files_written = Vec::new();

        for (_snap_id, content, source) in &response.snapshots {
            // Apply optional source filter
            if let Some(filters) = source_filter {
                let matched = filters.iter().any(|f| source::matches_glob(source, f));
                if !matched {
                    continue;
                }
            }

            // Only write file:// snapshots to disk
            if let Some(file_path) = source.strip_prefix("file://") {
                let bytes = content.to_bytes();
                // Ensure parent directory exists
                if let Some(parent) = std::path::Path::new(file_path).parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            LayertwineError::General(format!(
                                "failed to create directory '{}': {}",
                                parent.display(),
                                e
                            ))
                        })?;
                    }
                }
                std::fs::write(file_path, &bytes).map_err(|e| {
                    LayertwineError::General(format!("failed to write file '{}': {}", file_path, e))
                })?;
                files_written.push(file_path.to_string());
            }
        }

        Ok(RestoreApplyResult {
            checkpoint_id: response.checkpoint.id,
            files_written,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::file_node::FileNode;
    use crate::core::snapshot::Snapshot;
    use crate::core::types::ContentId;

    // --- helpers ---

    fn dummy_cp_id(n: u8) -> CheckpointId {
        ContentId::from_content(&[n; 16])
    }

    /// Build a Snapshot with content-addressed ID, auto-caching its content.
    fn make_snapshot(seed: u8, source: &str) -> Snapshot {
        let file = FileNode::new("dummy".into(), &[seed]);
        Snapshot::new_with_content(
            file,
            SnapshotContent::FileContent(vec![seed]),
            source.to_string(),
            String::new(),
            vec![],
            vec![],
        )
    }

    /// Create a repo whose root checkpoint has multiple snapshots.
    /// All snapshots are cached so restore can find them.
    fn multi_snapshot_repo(specs: Vec<(u8, &str)>) -> CheckpointRepo {
        let snaps: Vec<Snapshot> = specs
            .iter()
            .map(|&(seed, src)| make_snapshot(seed, src))
            .collect();
        let ids: Vec<SnapshotId> = snaps.iter().map(|s| s.id).collect();

        let mut repo = CheckpointRepo::new(ids);
        // Set source mapping on the root checkpoint
        if let Some(root_cp) = repo.checkpoints.get_mut(&repo.current_branch_head()) {
            for snap in &snaps {
                root_cp
                    .snapshot_sources
                    .insert(snap.id, snap.source.clone());
            }
        }
        // Cache snapshots
        for snap in snaps {
            repo.cache_snapshot(snap);
        }
        repo
    }

    /// Create a linear repo: root has one snapshot, then N sequential commits.
    /// Each commit adds a single new-snapshot checkpoint.
    /// Returns (repo, checkpoint_ids in order from root to head).
    fn linear_repo(
        specs: Vec<(u8, &str)>,
        stamps: Option<Vec<i64>>,
    ) -> (CheckpointRepo, Vec<CheckpointId>) {
        let first_snap = make_snapshot(specs[0].0, specs[0].1);
        let first_id = first_snap.id;
        let mut repo = CheckpointRepo::new_single(first_id);
        repo.cache_snapshot(first_snap);

        // Set source on root
        if let Some(root_cp) = repo.checkpoints.get_mut(&repo.current_branch_head()) {
            root_cp
                .snapshot_sources
                .insert(first_id, specs[0].1.to_string());
        }

        let mut cp_ids = vec![repo.current_branch_head()];

        for (i, &(seed, source)) in specs.iter().enumerate().skip(1) {
            let snap = make_snapshot(seed, source);
            let snap_id = snap.id;
            repo.cache_snapshot(snap);

            let cp_id = repo
                .commit_single(snap_id, &format!("c{}", i), "test")
                .unwrap();

            // Set source on the new checkpoint
            if let Some(cp) = repo.checkpoints.get_mut(&cp_id) {
                cp.snapshot_sources.insert(snap_id, source.to_string());
            }

            // Override timestamp if requested
            if let Some(ref stamps) = stamps {
                if let Some(cp) = repo.checkpoints.get_mut(&cp_id) {
                    cp.created_at = stamps.get(i).copied().unwrap_or(cp.created_at);
                }
                // Timestamp change doesn't affect checkpoint ID (created_at excluded from hash),
                // but we must re-insert into TimeIndex
                repo.time_index
                    .insert(repo.checkpoints.get(&cp_id).unwrap());
            }

            cp_ids.push(cp_id);
        }

        (repo, cp_ids)
    }

    // =========================================================================
    // Full restore
    // =========================================================================

    #[test]
    fn test_restore_full_returns_all_snapshots_with_content() {
        let repo = multi_snapshot_repo(vec![(1, "file://src/main.rs"), (2, "agent://state")]);

        let head = repo.current_branch_head();
        let resp = repo.restore_full(&head).unwrap();

        assert_eq!(resp.snapshots.len(), 2);
        let sources: Vec<&str> = resp.snapshots.iter().map(|(_, _, s)| s.as_str()).collect();
        assert!(sources.contains(&"file://src/main.rs"));
        assert!(sources.contains(&"agent://state"));
        assert_eq!(resp.ancestry.len(), 1);
        assert_eq!(*resp.ancestry.last().unwrap(), head);
    }

    #[test]
    fn test_restore_full_nonexistent_checkpoint_fails() {
        let repo = multi_snapshot_repo(vec![(1, "file://src/main.rs")]);
        let result = repo.restore_full(&dummy_cp_id(99));
        assert!(result.is_err());
    }

    // =========================================================================
    // Selective restore
    // =========================================================================

    #[test]
    fn test_restore_selective_matches_agent_source() {
        let repo = multi_snapshot_repo(vec![
            (1, "file://src/main.rs"),
            (2, "agent://state"),
            (3, "agent://graph"),
        ]);

        let head = repo.current_branch_head();
        let resp = repo.restore_selective(&head, vec!["agent://"]).unwrap();

        assert_eq!(resp.snapshots.len(), 2);
        for (_, _, source) in &resp.snapshots {
            assert!(source.starts_with("agent://"));
        }
    }

    #[test]
    fn test_restore_selective_no_match_returns_empty_snapshots() {
        let repo = multi_snapshot_repo(vec![(1, "file://src/main.rs"), (2, "file://src/lib.rs")]);

        let head = repo.current_branch_head();
        let resp = repo.restore_selective(&head, vec!["agent://"]).unwrap();

        assert!(resp.snapshots.is_empty());
        assert!(!resp.ancestry.is_empty());
    }

    #[test]
    fn test_restore_selective_multiple_filters() {
        let repo = multi_snapshot_repo(vec![
            (1, "agent://state"),
            (2, "graph://exec"),
            (3, "file://src/main.rs"),
        ]);

        let head = repo.current_branch_head();
        let resp = repo
            .restore_selective(&head, vec!["agent://", "graph://"])
            .unwrap();

        assert_eq!(resp.snapshots.len(), 2);
        let sources: Vec<&str> = resp.snapshots.iter().map(|(_, _, s)| s.as_str()).collect();
        assert!(sources.contains(&"agent://state"));
        assert!(sources.contains(&"graph://exec"));
    }

    #[test]
    fn test_restore_selective_nonexistent_checkpoint_fails() {
        let repo = multi_snapshot_repo(vec![(1, "file://src/main.rs")]);
        let result = repo.restore_selective(&dummy_cp_id(99), vec!["agent://"]);
        assert!(result.is_err());
    }

    // =========================================================================
    // Point-in-time restore (restore_by_time)
    // =========================================================================

    #[test]
    fn test_restore_by_time_finds_nearest_checkpoint() {
        let stamps = Some(vec![0, 0, 1000, 3000]);
        let (repo, _cp_ids) = linear_repo(
            vec![
                (1, "file://v1"),
                (2, "file://v2"),
                (3, "file://v3"),
                (4, "file://v4"),
            ],
            stamps,
        );

        // Nearest to 1500 → cp_ids[2] at t=1000 (has snapshot for seed=3)
        let resp = repo.restore_by_time(1500, None).unwrap();
        assert_eq!(resp.snapshots.len(), 1);
        assert_eq!(resp.ancestry.len(), 3);

        // Nearest to 2500 → cp_ids[3] at t=3000 (has snapshot for seed=4)
        let resp = repo.restore_by_time(2500, None).unwrap();
        assert_eq!(resp.snapshots.len(), 1);
        assert_eq!(resp.ancestry.len(), 4);
    }

    #[test]
    fn test_restore_by_time_with_source_filter() {
        // stamps: t=0, 0, 800 (agent), 2000 (file), 3000 (agent)
        let stamps = Some(vec![0, 0, 800, 2000, 3000]);
        let (repo, _cp_ids) = linear_repo(
            vec![
                (1, "file://v1"),
                (2, "file://v2"),
                (3, "agent://state"),
                (4, "file://v3"),
                (5, "agent://state2"),
            ],
            stamps,
        );

        // nearest to 1000 → checkpoint at t=800 with snapshot "agent://state"
        let resp = repo.restore_by_time(1000, Some(&["agent://"])).unwrap();
        assert_eq!(resp.snapshots.len(), 1);
        assert_eq!(resp.snapshots[0].2, "agent://state");
    }

    #[test]
    fn test_restore_by_time_finds_root_when_only_one() {
        let (repo, _) = linear_repo(vec![(1, "file://v1")], None);
        let result = repo.restore_by_time(100, None);
        assert!(result.is_ok());
    }

    // =========================================================================
    // Time-range restore (dispatcher: time_range)
    // =========================================================================

    #[test]
    fn test_restore_dispatcher_with_time_range() {
        let stamps = Some(vec![0, 0, 500, 1500, 3000]);
        let (repo, _cp_ids) = linear_repo(
            vec![
                (1, "file://v1"),
                (2, "file://v2"),
                (3, "file://v3"),
                (4, "file://v4"),
                (5, "file://v5"),
            ],
            stamps,
        );

        let req = RestoreRequest {
            checkpoint_id: None,
            source_filter: None,
            time_range: Some((400, 1600)),
        };
        let resp = repo.restore(&req).unwrap();

        // Latest within [400,1600] → t=1500 checkpoint (snapshot for seed=4)
        assert_eq!(resp.snapshots.len(), 1);
    }

    #[test]
    fn test_restore_dispatcher_with_time_range_and_filter() {
        // stamps: t=0, 0, 500, 2000 (agent), 1500 (file)
        // time_range (600, 2500) → candidates: t=1500 (file), t=2000 (agent)
        // latest in range: t=2000 → agent://state
        let stamps = Some(vec![0, 0, 500, 2000, 1500]);
        let (repo, _cp_ids) = linear_repo(
            vec![
                (1, "file://v1"),
                (2, "file://v2"),
                (3, "file://v3"),
                (4, "agent://state"),
                (5, "file://v5"),
            ],
            stamps,
        );

        let req = RestoreRequest {
            checkpoint_id: None,
            source_filter: Some(vec!["agent://".to_string()]),
            time_range: Some((600, 2500)),
        };
        let resp = repo.restore(&req).unwrap();

        assert_eq!(resp.snapshots.len(), 1);
        assert!(resp.snapshots[0].2.starts_with("agent://"));
    }

    #[test]
    fn test_restore_dispatcher_time_range_empty_fails() {
        let stamps = Some(vec![0, 0, 500, 1000]);
        let (repo, _) = linear_repo(vec![(1, "v1"), (2, "v2"), (3, "v3"), (4, "v4")], stamps);

        let req = RestoreRequest {
            checkpoint_id: None,
            source_filter: None,
            time_range: Some((5000, 6000)),
        };
        let result = repo.restore(&req);
        assert!(result.is_err());
    }

    // =========================================================================
    // Full + selective restore via dispatcher: checkpoint_id
    // =========================================================================

    #[test]
    fn test_restore_dispatcher_with_checkpoint_id_only() {
        let repo = multi_snapshot_repo(vec![(1, "file://src/main.rs"), (2, "agent://state")]);

        let head = repo.current_branch_head();
        let req = RestoreRequest {
            checkpoint_id: Some(head),
            source_filter: None,
            time_range: None,
        };
        let resp = repo.restore(&req).unwrap();
        assert_eq!(resp.snapshots.len(), 2);
    }

    #[test]
    fn test_restore_dispatcher_with_checkpoint_id_and_filter() {
        let repo = multi_snapshot_repo(vec![(1, "file://src/main.rs"), (2, "agent://state")]);

        let head = repo.current_branch_head();
        let req = RestoreRequest {
            checkpoint_id: Some(head),
            source_filter: Some(vec!["agent://".to_string()]),
            time_range: None,
        };
        let resp = repo.restore(&req).unwrap();
        assert_eq!(resp.snapshots.len(), 1);
        assert_eq!(resp.snapshots[0].2, "agent://state");
    }

    #[test]
    fn test_restore_dispatcher_no_id_no_time_fails() {
        let repo = multi_snapshot_repo(vec![(1, "v1")]);

        let req = RestoreRequest {
            checkpoint_id: None,
            source_filter: None,
            time_range: None,
        };
        let result = repo.restore(&req);
        assert!(result.is_err());
    }

    // =========================================================================
    // List snapshots
    // =========================================================================

    #[test]
    fn test_list_snapshots_multi_snapshot() {
        let repo = multi_snapshot_repo(vec![
            (1, "file://src/a.rs"),
            (2, "agent://state"),
            (3, "graph://exec"),
        ]);

        let head = repo.current_branch_head();
        let snapshots = repo.list_snapshots(&head).unwrap();
        assert_eq!(snapshots.len(), 3);

        let sources: Vec<&str> = snapshots.iter().map(|s| s.1.as_str()).collect();
        assert!(sources.contains(&"file://src/a.rs"));
        assert!(sources.contains(&"agent://state"));
        assert!(sources.contains(&"graph://exec"));
    }

    // =========================================================================
    // Diff checkpoints
    // =========================================================================

    #[test]
    fn test_diff_checkpoints_added() {
        // Root has [snap1]; after commit, head has [snap2] (commit_single replaces baseline)
        let (repo, cp_ids) = linear_repo(vec![(1, "file://a.rs"), (2, "file://b.rs")], None);
        let root_id = cp_ids[0];
        let head = cp_ids[1];

        let diff = repo.diff_checkpoints(&root_id, &head).unwrap();
        // snap1 removed from root, snap2 added in head
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 1);
    }

    #[test]
    fn test_diff_checkpoints_removed() {
        let (repo, cp_ids) = linear_repo(vec![(1, "file://a.rs"), (2, "file://b.rs")], None);
        let root_id = cp_ids[0];
        let head = cp_ids[1];

        // Reverse: head→root (snap2 removed going back to root, snap1 "added"=restored)
        let diff = repo.diff_checkpoints(&head, &root_id).unwrap();
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.added.len(), 1);
    }

    #[test]
    fn test_diff_checkpoints_same_is_empty() {
        let repo = multi_snapshot_repo(vec![(1, "v1"), (2, "v2")]);
        let head = repo.current_branch_head();
        let diff = repo.diff_checkpoints(&head, &head).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_diff_checkpoints_nonexistent_from_fails() {
        let repo = multi_snapshot_repo(vec![(1, "v1")]);
        let head = repo.current_branch_head();
        let result = repo.diff_checkpoints(&dummy_cp_id(255), &head);
        assert!(result.is_err());
    }

    // =========================================================================
    // Validate integrity
    // =========================================================================

    #[test]
    fn test_validate_integrity_all_snapshots_valid() {
        let repo = multi_snapshot_repo(vec![(1, "file://src/main.rs"), (2, "agent://state")]);

        let head = repo.current_branch_head();
        let issues = repo.validate_integrity(&head).unwrap();
        assert!(issues.is_empty(), "expected no issues, got: {:?}", issues);
    }

    #[test]
    fn test_validate_integrity_missing_snapshot_reported() {
        let snap = make_snapshot(1, "file://src/main.rs");
        let snap_id = snap.id;
        let mut repo = CheckpointRepo::new_single(snap_id);
        if let Some(cp) = repo.checkpoints.get_mut(&repo.current_branch_head()) {
            cp.snapshot_sources
                .insert(snap_id, "file://src/main.rs".to_string());
        }
        // snapshot is NOT cached

        let head = repo.current_branch_head();
        let issues = repo.validate_integrity(&head).unwrap();
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("not found")));
    }

    // =========================================================================
    // Ancestry chain
    // =========================================================================

    #[test]
    fn test_ancestry_chain_linear() {
        let (repo, cp_ids) = linear_repo(vec![(1, "v1"), (2, "v2"), (3, "v3")], None);

        let head = cp_ids.last().copied().unwrap();
        let ancestry = repo.get_ancestry_chain(&head).unwrap();
        assert_eq!(ancestry.len(), 3);
        assert_eq!(*ancestry.last().unwrap(), head);
    }

    #[test]
    fn test_ancestry_chain_single_checkpoint() {
        let repo = multi_snapshot_repo(vec![(1, "v1")]);
        let head = repo.current_branch_head();
        let ancestry = repo.get_ancestry_chain(&head).unwrap();
        assert_eq!(ancestry.len(), 1);
        assert_eq!(ancestry[0], head);
    }
}
