use std::collections::{HashMap, HashSet};
use std::time::Instant;

use wf_types::checkpoint::CheckpointCleanupMetrics;
use wf_types::storage::CheckpointStorageMetadata;

use crate::checkpoint_graph::CheckpointDependencyGraph;
use crate::metrics::CheckpointMetricsCollector;

const DAY_MS: i64 = 86_400_000;

/// Age tier for the tiered cleanup strategy, aligned with the TS
/// `RetentionTier` (`{minAgeDays, maxAgeDays?, retentionIntervalDays}`).
#[derive(Debug, Clone)]
pub struct RetentionTier {
    pub min_age_days: u64,
    pub max_age_days: Option<u64>,
    /// Keep at most one checkpoint per window of this many days.
    /// `0` keeps all checkpoints in the tier.
    pub retention_interval_days: u64,
}

impl RetentionTier {
    pub fn new(min_age_days: u64, max_age_days: Option<u64>, retention_interval_days: u64) -> Self {
        Self {
            min_age_days,
            max_age_days,
            retention_interval_days,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CleanupStrategy {
    TimeBased {
        max_age_seconds: u64,
        min_retention: u64,
    },
    CountBased {
        max_checkpoints: u64,
        min_retention: u64,
    },
    SizeBased {
        max_total_bytes: u64,
        min_retention: u64,
    },
    Tiered {
        tiers: Vec<RetentionTier>,
        min_retention: u64,
    },
}

impl CleanupStrategy {
    pub fn time_based(max_age_seconds: u64) -> Self {
        Self::TimeBased {
            max_age_seconds,
            min_retention: 1,
        }
    }

    pub fn count_based(max_checkpoints: u64) -> Self {
        Self::CountBased {
            max_checkpoints,
            min_retention: 1,
        }
    }

    pub fn size_based(max_total_bytes: u64) -> Self {
        Self::SizeBased {
            max_total_bytes,
            min_retention: 1,
        }
    }
}

/// Result of a cleanup run, aligned with the TS `CleanupResult`
/// (`deletedCheckpointIds`, `deletedCount`, `freedSpaceBytes`,
/// `remainingCount`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CleanupResult {
    pub deleted_checkpoint_ids: Vec<String>,
    pub deleted_count: u64,
    pub freed_bytes: u64,
    pub remaining_count: u64,
}

pub struct CleanupExecutor;

impl CleanupExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Evaluate which checkpoints to remove, optionally recording cleanup
    /// metrics. Freed bytes are unavailable at evaluation time and reported
    /// as 0 (use `evaluate_protected_with_result` for accurate accounting).
    pub fn evaluate_with_metrics(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        strategy: &CleanupStrategy,
        metrics: Option<&CheckpointMetricsCollector>,
    ) -> Vec<String> {
        let start = Instant::now();
        let to_remove = self.evaluate(checkpoints, strategy);
        if let Some(metrics) = metrics {
            metrics.record_cleanup(&CheckpointCleanupMetrics {
                deleted_count: to_remove.len() as u32,
                freed_bytes: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }
        to_remove
    }

    pub fn evaluate(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        strategy: &CleanupStrategy,
    ) -> Vec<String> {
        match strategy {
            CleanupStrategy::TimeBased {
                max_age_seconds,
                min_retention,
            } => self.evaluate_time(checkpoints, *max_age_seconds, *min_retention),
            CleanupStrategy::CountBased {
                max_checkpoints,
                min_retention,
            } => self.evaluate_count(checkpoints, *max_checkpoints, *min_retention),
            CleanupStrategy::SizeBased {
                max_total_bytes,
                min_retention,
            } => self.evaluate_size(checkpoints, *max_total_bytes, *min_retention),
            CleanupStrategy::Tiered {
                tiers,
                min_retention,
            } => self.evaluate_tiered(checkpoints, tiers, *min_retention),
        }
    }

    /// Evaluate candidates with dependency protection, aligned with the TS
    /// cleanup flow: the latest checkpoint is always kept, any candidate
    /// referenced by a surviving checkpoint through the previous-id chain is
    /// protected, and chain-group members whose FULL baseline survives are
    /// protected as well.
    pub fn evaluate_protected(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        strategy: &CleanupStrategy,
    ) -> Vec<String> {
        self.evaluate_protected_with_result(checkpoints, strategy)
            .deleted_checkpoint_ids
    }

    /// Same as `evaluate_protected` but also accounts freed bytes from the
    /// real `blob_size` and the remaining count.
    pub fn evaluate_protected_with_result(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        strategy: &CleanupStrategy,
    ) -> CleanupResult {
        let candidates = self.evaluate(checkpoints, strategy);
        let candidate_set: HashSet<String> = candidates.iter().cloned().collect();

        let mut to_remove: HashSet<String> = candidate_set.clone();
        if !to_remove.is_empty() {
            let all_ids: HashSet<String> = checkpoints.iter().map(|c| c.id.clone()).collect();
            let graph = CheckpointDependencyGraph::build(checkpoints);
            let protected = graph.compute_protected(&candidate_set, &all_ids);
            let chain_protected = graph.chain_group_protected(&candidate_set);
            to_remove.retain(|id| !protected.contains(id) && !chain_protected.contains(id));

            let latest_id = checkpoints
                .iter()
                .max_by_key(|c| c.timestamp)
                .map(|c| c.id.clone());
            if let Some(latest) = latest_id {
                to_remove.remove(&latest);
            }
        }

        let size_by_id: HashMap<&str, u64> = checkpoints
            .iter()
            .map(|c| (c.id.as_str(), c.blob_size.unwrap_or(0)))
            .collect();
        let freed_bytes = to_remove.iter().map(|id| size_by_id.get(id.as_str()).copied().unwrap_or(0)).sum();

        let mut deleted_checkpoint_ids: Vec<String> = to_remove.into_iter().collect();
        deleted_checkpoint_ids.sort_unstable();
        let remaining_count =
            checkpoints.len().saturating_sub(deleted_checkpoint_ids.len()) as u64;

        CleanupResult {
            deleted_count: deleted_checkpoint_ids.len() as u64,
            deleted_checkpoint_ids,
            freed_bytes,
            remaining_count,
        }
    }

    fn evaluate_time(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        max_age_seconds: u64,
        min_retention: u64,
    ) -> Vec<String> {
        let now = chrono::Utc::now().timestamp_millis();
        let max_age_ms = (max_age_seconds * 1000) as i64;
        let candidates: HashSet<String> = checkpoints
            .iter()
            .filter(|c| now - c.timestamp > max_age_ms)
            .map(|c| c.id.clone())
            .collect();
        self.apply_min_retention(checkpoints, candidates, min_retention)
    }

    fn evaluate_count(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        max_count: u64,
        min_retention: u64,
    ) -> Vec<String> {
        if checkpoints.len() as u64 <= max_count {
            return Vec::new();
        }
        let mut sorted: Vec<_> = checkpoints.to_vec();
        sorted.sort_by_key(|c| c.timestamp);
        let to_remove = sorted.len() as u64 - max_count;
        let candidates: HashSet<String> = sorted[..to_remove as usize]
            .iter()
            .map(|c| c.id.clone())
            .collect();
        self.apply_min_retention(checkpoints, candidates, min_retention)
    }

    /// Size-based cleanup using the real `blob_size` per checkpoint
    /// (missing sizes count as 0 and are warned about, aligned with TS).
    fn evaluate_size(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        max_total_bytes: u64,
        min_retention: u64,
    ) -> Vec<String> {
        let mut missing = 0usize;
        let total_size: u64 = checkpoints
            .iter()
            .map(|c| c.blob_size.unwrap_or_else(|| {
                missing += 1;
                0
            }))
            .sum();
        if missing > 0 {
            tracing::warn!(
                missing_blob_sizes = missing,
                "checkpoints without blob_size treated as 0 bytes in size-based cleanup"
            );
        }
        if total_size <= max_total_bytes {
            return Vec::new();
        }
        let mut sorted: Vec<_> = checkpoints.to_vec();
        sorted.sort_by_key(|c| c.timestamp);
        let mut accumulated: u64 = 0;
        let mut candidates: HashSet<String> = HashSet::new();
        for cp in &sorted {
            if total_size.saturating_sub(accumulated) <= max_total_bytes {
                break;
            }
            accumulated += cp.blob_size.unwrap_or(0);
            candidates.insert(cp.id.clone());
        }
        self.apply_min_retention(checkpoints, candidates, min_retention)
    }

    /// Tiered cleanup with TS age-window semantics: for each tier (evaluated
    /// oldest-first), keep at most one checkpoint per
    /// `retention_interval_days` window; `interval == 0` keeps all
    /// checkpoints in that tier. `min_retention` newest checkpoints are
    /// always kept.
    fn evaluate_tiered(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        tiers: &[RetentionTier],
        min_retention: u64,
    ) -> Vec<String> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tiers: Vec<&RetentionTier> = tiers.iter().collect();
        tiers.sort_by_key(|t| t.min_age_days);

        let mut to_remove: HashSet<String> = HashSet::new();

        for tier in tiers {
            if tier.retention_interval_days == 0 {
                continue;
            }
            let in_tier: Vec<&CheckpointStorageMetadata> = checkpoints
                .iter()
                .filter(|c| {
                    let age_days = (now - c.timestamp) / DAY_MS;
                    age_days >= tier.min_age_days as i64
                        && tier
                            .max_age_days
                            .is_none_or(|max| age_days < max as i64)
                })
                .collect();

            let mut by_window: HashMap<i64, Vec<&CheckpointStorageMetadata>> = HashMap::new();
            for cp in in_tier {
                let window = (now - cp.timestamp) / DAY_MS / tier.retention_interval_days as i64;
                by_window.entry(window).or_default().push(cp);
            }
            for group in by_window.into_values() {
                if let Some(keep) = group.iter().max_by_key(|c| c.timestamp) {
                    let keep_id = keep.id.clone();
                    for cp in group {
                        if cp.id != keep_id {
                            to_remove.insert(cp.id.clone());
                        }
                    }
                }
            }
        }

        self.apply_min_retention(checkpoints, to_remove, min_retention)
    }

    fn apply_min_retention(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        candidates: HashSet<String>,
        min_retention: u64,
    ) -> Vec<String> {
        if min_retention == 0 || candidates.is_empty() {
            return candidates.into_iter().collect();
        }
        let mut sorted = checkpoints.to_vec();
        sorted.sort_by_key(|c| c.timestamp);
        let keep: HashSet<String> = sorted
            .iter()
            .rev()
            .take(min_retention as usize)
            .map(|c| c.id.clone())
            .collect();
        candidates
            .into_iter()
            .filter(|id| !keep.contains(id))
            .collect()
    }
}

impl Default for CleanupExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_checkpoint(
        id: &str,
        timestamp_ms: i64,
        blob_size: Option<u64>,
    ) -> CheckpointStorageMetadata {
        CheckpointStorageMetadata {
            id: id.to_string(),
            entity_type: "test".to_string(),
            entity_id: "entity-1".to_string(),
            checkpoint_type: wf_types::checkpoint::CheckpointType::Full,
            timestamp: timestamp_ms,
            status: wf_types::checkpoint::CheckpointStatus::Completed,
            previous_checkpoint_id: None,
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: None,
            blob_size,
            tags: None,
            custom_fields: None,
        }
    }

    fn make_delta_checkpoint(
        id: &str,
        timestamp_ms: i64,
        previous: Option<&str>,
        chain_root: Option<&str>,
    ) -> CheckpointStorageMetadata {
        let mut cp = make_checkpoint(id, timestamp_ms, Some(100));
        cp.checkpoint_type = wf_types::checkpoint::CheckpointType::Delta;
        cp.previous_checkpoint_id = previous.map(String::from);
        cp.chain_root_id = chain_root.map(String::from);
        cp
    }

    #[test]
    fn time_based_cleanup() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("old", now - 3_600_000, None),
            make_checkpoint("new", now - 1_000, None),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::TimeBased {
                max_age_seconds: 1800,
                min_retention: 1,
            },
        );
        assert_eq!(to_remove, vec!["old"]);
    }

    #[test]
    fn time_based_min_retention_keeps_newest() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("old-1", now - 7_200_000, None),
            make_checkpoint("old-2", now - 7_000_000, None),
            make_checkpoint("old-3", now - 6_800_000, None),
            make_checkpoint("new", now - 1_000, None),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::TimeBased {
                max_age_seconds: 3600,
                min_retention: 2,
            },
        );
        assert_eq!(to_remove.len(), 2);
        assert!(!to_remove.contains(&"old-3".to_string()));
        assert!(!to_remove.contains(&"new".to_string()));
    }

    #[test]
    fn count_based_cleanup() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("c1", now - 5000, None),
            make_checkpoint("c2", now - 4000, None),
            make_checkpoint("c3", now - 3000, None),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::CountBased {
                max_checkpoints: 2,
                min_retention: 1,
            },
        );
        assert_eq!(to_remove, vec!["c1"]);
    }

    #[test]
    fn count_based_under_limit() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![make_checkpoint("c1", now - 5000, None)];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::CountBased {
                max_checkpoints: 10,
                min_retention: 1,
            },
        );
        assert!(to_remove.is_empty());
    }

    #[test]
    fn size_based_uses_real_blob_size() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("big", now - 5000, Some(8000)),
            make_checkpoint("small", now - 4000, Some(1000)),
            make_checkpoint("tiny", now - 3000, Some(500)),
        ];
        let executor = CleanupExecutor::new();
        // Total = 9500; removing "big" (8000) gets us under 2000.
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::SizeBased {
                max_total_bytes: 2000,
                min_retention: 1,
            },
        );
        assert_eq!(to_remove, vec!["big"]);
    }

    #[test]
    fn size_based_result_reports_freed_bytes() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("c1", now - 5000, Some(1000)),
            make_checkpoint("c2", now - 4000, Some(2000)),
            make_checkpoint("c3", now - 3000, Some(500)),
        ];
        let executor = CleanupExecutor::new();
        let result = executor.evaluate_protected_with_result(
            &checkpoints,
            &CleanupStrategy::CountBased {
                max_checkpoints: 1,
                min_retention: 0,
            },
        );
        assert_eq!(result.deleted_count, 2);
        assert_eq!(result.freed_bytes, 3000);
        assert_eq!(result.remaining_count, 1);
        assert_eq!(result.deleted_checkpoint_ids, vec!["c1", "c2"]);
    }

    #[test]
    fn tiered_cleanup_keeps_one_per_window() {
        let now = chrono::Utc::now().timestamp_millis();
        let day = DAY_MS;
        let checkpoints = vec![
            make_checkpoint("d10-1", now - 10 * day, None),
            make_checkpoint("d10-2", now - 10 * day - 1, None),
            make_checkpoint("d9", now - 9 * day, None),
            make_checkpoint("recent", now - 1_000, None),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::Tiered {
                tiers: vec![RetentionTier::new(7, None, 1)],
                min_retention: 1,
            },
        );
        // Only the two checkpoints sharing a 1-day window are candidates;
        // the newest (recent) is protected by min_retention.
        assert_eq!(to_remove.len(), 1);
        assert!(!to_remove.contains(&"recent".to_string()));
    }

    #[test]
    fn tiered_zero_interval_keeps_all() {
        let now = chrono::Utc::now().timestamp_millis();
        let day = DAY_MS;
        let checkpoints = vec![
            make_checkpoint("a", now - 30 * day, None),
            make_checkpoint("b", now - 20 * day, None),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::Tiered {
                tiers: vec![RetentionTier::new(7, None, 0)],
                min_retention: 0,
            },
        );
        assert!(to_remove.is_empty());
    }

    #[test]
    fn tiered_max_age_excludes_newer_checkpoints() {
        let now = chrono::Utc::now().timestamp_millis();
        let day = DAY_MS;
        let checkpoints = vec![
            make_checkpoint("d30", now - 30 * day, None),
            make_checkpoint("d5", now - 5 * day, None),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::Tiered {
                tiers: vec![RetentionTier::new(7, Some(14), 0)],
                min_retention: 0,
            },
        );
        assert!(to_remove.is_empty(), "d5 is outside the tier age window");
    }

    #[test]
    fn protection_keeps_chain_members_of_survivors() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("full-1", now - 4000, Some(100)),
            make_delta_checkpoint("delta-1", now - 3000, Some("full-1"), Some("full-1")),
            make_delta_checkpoint("delta-2", now - 2000, Some("delta-1"), Some("full-1")),
            make_delta_checkpoint("delta-3", now - 1000, Some("delta-2"), Some("full-1")),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate_protected(
            &checkpoints,
            &CleanupStrategy::CountBased {
                max_checkpoints: 2,
                min_retention: 0,
            },
        );

        assert!(to_remove.is_empty(), "whole chain protected");
    }

    #[test]
    fn protection_allows_removing_unreferenced_full() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("full-1", now - 4000, Some(100)),
            make_checkpoint("full-2", now - 3000, Some(100)),
            make_delta_checkpoint("delta-1", now - 2000, Some("full-2"), Some("full-2")),
            make_delta_checkpoint("delta-2", now - 1000, Some("delta-1"), Some("full-2")),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate_protected(
            &checkpoints,
            &CleanupStrategy::CountBased {
                max_checkpoints: 2,
                min_retention: 0,
            },
        );

        assert_eq!(to_remove, vec!["full-1"]);
    }

    #[test]
    fn chain_group_protection_keeps_deltas_of_surviving_baseline() {
        let now = chrono::Utc::now().timestamp_millis();
        let day = 86_400_000i64;
        let checkpoints = vec![
            // Baseline and the other baseline survive (recent).
            make_checkpoint("full-1", now - 1_000, Some(100)),
            make_checkpoint("full-2", now - 2_000, Some(100)),
            // Old deltas are candidates, but their baseline full-1 survives.
            make_delta_checkpoint("delta-1", now - 3 * day, Some("full-1"), Some("full-1")),
            make_delta_checkpoint("delta-2", now - 2 * day, Some("delta-1"), Some("full-1")),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate_protected(
            &checkpoints,
            &CleanupStrategy::TimeBased {
                max_age_seconds: 86_400,
                min_retention: 0,
            },
        );

        assert!(
            to_remove.is_empty(),
            "deltas protected because their baseline full-1 survives"
        );
    }
}
