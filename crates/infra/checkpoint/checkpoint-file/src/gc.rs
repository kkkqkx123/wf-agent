//! Object-store cleanup policy and statistics.
//!
//! Collection follows the object store's own reachability: loose objects
//! unreachable from any ref are pruned, and source-index rows for pruned
//! commits go with them. There is no separate mark-sweep over content
//! tables — file bytes live only in the object store.

/// Statistics of one cleanup run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GcStats {
    /// Unreachable commits dropped by the sweep.
    pub removed_checkpoints: u64,
    /// Unreachable trees dropped with the removed commits.
    pub removed_snapshots: u64,
    /// Unreachable blobs dropped by the reclaim sweep.
    pub reclaimed_snapshots: u64,
    /// Reserved: row-level deltas no longer exist.
    pub reclaimed_deltas: u64,
    /// Reserved: file-node rows no longer exist.
    pub reclaimed_file_nodes: u64,
}

impl GcStats {
    pub fn new() -> Self {
        GcStats {
            removed_checkpoints: 0,
            removed_snapshots: 0,
            reclaimed_snapshots: 0,
            reclaimed_deltas: 0,
            reclaimed_file_nodes: 0,
        }
    }
}

impl Default for GcStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Cleanup retention policy: which commits stay protected beyond the
/// built-in protected set (ref heads plus ancestors).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GcRetention {
    /// Keep the N most recently created commits protected (by commit
    /// time, newest first) even when no ref points at them. `0` = only
    /// the built-in protected set.
    pub keep_recent_heads: usize,
}
