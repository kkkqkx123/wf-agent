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
    /// Edit refs removed because their history is already contained in the
    /// mainline (post-merge dead lines).
    #[serde(default)]
    pub removed_edit_refs: u64,
    /// Loose objects folded into a packfile (reduces inode pressure on
    /// long-running stores).
    #[serde(default)]
    pub packed_objects: u64,
}

impl GcStats {
    pub fn new() -> Self {
        GcStats {
            removed_checkpoints: 0,
            removed_snapshots: 0,
            reclaimed_snapshots: 0,
            removed_edit_refs: 0,
            packed_objects: 0,
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

/// Metadata key holding the timestamp (Unix milliseconds) of the last
/// completed GC run, persisted so the storage dashboard can report it.
pub const LAST_GC_KEY: &str = "file_checkpoint.last_gc_ms";

/// Read-only object-store statistics for the storage dashboard. Counts are
/// honest measurements of the store as it is; reachability-based
/// reclaimable estimates are deliberately not derived here because that
/// requires a full graph walk. For the walked variant see
/// [`StoreWatermark`], which splits the loose-object count into
/// reachable and reclaimable tiers for retention tuning.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoreStats {
    /// Commits reachable from any ref.
    pub commits: u64,
    /// Refs under the `refs/wf/` namespace.
    pub refs: u64,
    /// Loose objects currently in the object store.
    pub loose_objects: u64,
    /// Total bytes occupied by the bare repository directory.
    pub store_bytes: u64,
    /// Time of the last completed GC run (Unix milliseconds), when recorded.
    #[serde(default)]
    pub last_gc_ms: Option<i64>,
}

/// Point-in-time storage watermark from a full reachability walk.
/// Unlike [`StoreStats`] (cheap counters), this splits loose commits into
/// tiers so retention can be tuned and dashboards can show water levels:
/// ref-reachable plus baseline-anchored commits are protected, the rest
/// is reclaimable by the next sweep (modulo the in-flight grace window).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoreWatermark {
    /// Loose objects currently in the object store.
    pub loose_objects: u64,
    /// Loose commits reachable from a ref or the baseline anchor.
    pub reachable_commits: u64,
    /// Loose commits unreachable from every ref and the baseline anchor
    /// (reclaimable by the next sweep outside the grace window).
    pub reclaimable_commits: u64,
    /// Time of the last completed GC run (Unix milliseconds), when recorded.
    #[serde(default)]
    pub last_gc_ms: Option<i64>,
}

/// Report of one cleanup run: the reclaimed counts plus the watermark
/// observed after the sweep. Returned by the `run_gc_report` entry so
/// callers can observe storage pressure without a second walk; the
/// `GcCompleted` event continues to carry the plain [`GcStats`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GcReport {
    /// What this sweep reclaimed.
    pub stats: GcStats,
    /// Watermark observed after the sweep.
    pub watermark_after: StoreWatermark,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gc_stats_default_counts_are_zero() {
        let stats = GcStats::new();
        assert_eq!(stats.removed_checkpoints, 0);
        assert_eq!(stats.removed_snapshots, 0);
        assert_eq!(stats.reclaimed_snapshots, 0);
        assert_eq!(stats.removed_edit_refs, 0);
        assert_eq!(stats.packed_objects, 0);
        assert_eq!(stats, GcStats::default());
    }

    #[test]
    fn gc_retention_default_protects_only_reachable() {
        assert_eq!(GcRetention::default().keep_recent_heads, 0);
    }

    #[test]
    fn store_stats_serde_roundtrip_preserves_last_gc() {
        let stats = StoreStats {
            commits: 3,
            refs: 2,
            loose_objects: 9,
            store_bytes: 1024,
            last_gc_ms: Some(1_700_000_000_000),
        };
        let back: StoreStats =
            serde_json::from_slice(&serde_json::to_vec(&stats).unwrap()).unwrap();
        assert_eq!(back, stats);
    }

    #[test]
    fn gc_report_carries_stats_and_watermark() {
        let report = GcReport {
            stats: GcStats {
                removed_checkpoints: 1,
                ..GcStats::new()
            },
            watermark_after: StoreWatermark {
                loose_objects: 4,
                reachable_commits: 3,
                reclaimable_commits: 1,
                last_gc_ms: None,
            },
        };
        assert_eq!(report.stats.removed_checkpoints, 1);
        assert_eq!(report.watermark_after.reclaimable_commits, 1);
        let back: GcReport = serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
        assert_eq!(back.stats, report.stats);
        assert_eq!(back.watermark_after, report.watermark_after);
    }
}
