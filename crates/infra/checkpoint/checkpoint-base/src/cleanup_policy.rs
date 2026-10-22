use std::collections::{HashMap, HashSet};
use std::time::Instant;

use wf_types::checkpoint::CheckpointRetentionConfig;
use wf_types::storage::CheckpointStorageMetadata;

use crate::checkpoint_graph::CheckpointDependencyGraph;
use crate::clock::CheckpointClock;
use wf_metrics::CheckpointMetricsCollector;

const DAY_MS: i64 = 86_400_000;

// Division of labor: this policy decides *which* execution-state checkpoints
// may go by age, count, size, or tier, then applies chain protection. File
// history reclamation lives in `checkpoint-file` (`gc`) and only
// reclaims commit rows outside the branch-head ancestor closure. The two
// collectors share result shapes but must not share implementation.
/// Age tier for the tiered cleanup strategy
/// (`{minAgeDays, maxAgeDays?, retentionIntervalDays}`).
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

/// Bridge a retention config onto executable cleanup strategies. This is the
/// single milliseconds-to-seconds conversion point: `max_age` is
/// milliseconds, `CleanupStrategy::TimeBased` is seconds (rounded up, at
/// least one second). A config carrying both limits yields both strategies,
/// count first; an empty config yields no strategy.
pub fn cleanup_strategies_from_retention(
    config: &CheckpointRetentionConfig,
) -> Vec<CleanupStrategy> {
    let mut strategies = Vec::new();
    if let Some(max) = config.max_checkpoints {
        strategies.push(CleanupStrategy::CountBased {
            max_checkpoints: max as u64,
            min_retention: 1,
        });
    }
    if let Some(max_age_ms) = config.max_age.filter(|ms| *ms > 0) {
        let seconds = (max_age_ms as u64).div_ceil(1000).max(1);
        strategies.push(CleanupStrategy::TimeBased {
            max_age_seconds: seconds,
            min_retention: 1,
        });
    }
    strategies
}

/// Result of a cleanup run
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
    /// Time based decisions read the injected clock and fail closed when the
    /// clock is unavailable.
    pub fn evaluate_with_metrics(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        strategy: &CleanupStrategy,
        metrics: Option<&CheckpointMetricsCollector>,
        clock: &CheckpointClock,
    ) -> Vec<String> {
        let start = Instant::now();
        let to_remove = self.evaluate(checkpoints, strategy, clock);
        if let Some(metrics) = metrics {
            metrics.record_cleanup(
                to_remove.len() as u64,
                0,
                start.elapsed().as_millis() as f64,
            );
        }
        to_remove
    }

    pub fn evaluate(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        strategy: &CleanupStrategy,
        clock: &CheckpointClock,
    ) -> Vec<String> {
        match strategy {
            CleanupStrategy::TimeBased {
                max_age_seconds,
                min_retention,
            } => self.evaluate_time(checkpoints, *max_age_seconds, *min_retention, clock),
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
            } => self.evaluate_tiered(checkpoints, tiers, *min_retention, clock),
        }
    }

    /// Evaluate candidates with dependency protection: the latest checkpoint
    /// is always kept, any candidate on a surviving delta's restore chain
    /// (previous links up to and including its FULL head) is protected, and
    /// chain-group members whose FULL baseline survives are protected as well.
    pub fn evaluate_protected(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        strategy: &CleanupStrategy,
        clock: &CheckpointClock,
    ) -> Vec<String> {
        self.evaluate_protected_with_result(checkpoints, strategy, clock)
            .deleted_checkpoint_ids
    }

    /// Same as `evaluate_protected` but also accounts freed bytes from the
    /// real `blob_size` and the remaining count.
    pub fn evaluate_protected_with_result(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        strategy: &CleanupStrategy,
        clock: &CheckpointClock,
    ) -> CleanupResult {
        let candidates = self.evaluate(checkpoints, strategy, clock);
        let candidate_set: HashSet<String> = candidates.iter().cloned().collect();

        let mut to_remove: HashSet<String> = candidate_set.clone();
        if !to_remove.is_empty() {
            let all_ids: HashSet<String> = checkpoints.iter().map(|c| c.id.clone()).collect();
            let graph = CheckpointDependencyGraph::build(checkpoints);
            let protected = graph.compute_protected(&candidate_set, &all_ids);
            let chain_protected = graph.chain_group_protected(&candidate_set);
            to_remove.retain(|id| !protected.contains(id) && !chain_protected.contains(id));

            // Base-priority rule: a base checkpoint may only retire together
            // with the whole generation that depends on it. If any delta that
            // transitively references a candidate base survives (is not itself
            // scheduled for removal), the base is protected — a surviving
            // delta must never lose its base.
            let bases_to_keep = graph.bases_with_surviving_dependents(&to_remove);
            to_remove.retain(|id| !bases_to_keep.contains(id));

            let latest_id = checkpoints
                .iter()
                .max_by(|a, b| (a.timestamp, &a.id).cmp(&(b.timestamp, &b.id)))
                .map(|c| c.id.clone());
            if let Some(latest) = latest_id {
                to_remove.remove(&latest);
            }
        }

        let size_by_id: HashMap<&str, u64> = checkpoints
            .iter()
            .map(|c| (c.id.as_str(), c.blob_size.unwrap_or(0)))
            .collect();
        let freed_bytes = to_remove
            .iter()
            .map(|id| size_by_id.get(id.as_str()).copied().unwrap_or(0))
            .sum();

        let mut deleted_checkpoint_ids: Vec<String> = to_remove.into_iter().collect();
        deleted_checkpoint_ids.sort_unstable();
        let remaining_count = checkpoints
            .len()
            .saturating_sub(deleted_checkpoint_ids.len()) as u64;

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
        clock: &CheckpointClock,
    ) -> Vec<String> {
        let Some(now) = clock.now_ms().filter(|ms| *ms >= 0) else {
            return Vec::new();
        };
        let max_age_ms = (max_age_seconds * 1000) as i64;
        // Future timestamps clamp to zero age so they read as newest and
        // are retained, consistent with the tiered path.
        let candidates: HashSet<String> = checkpoints
            .iter()
            .filter(|c| (now - c.timestamp).max(0) > max_age_ms)
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
        sorted.sort_by(|a, b| (a.timestamp, &a.id).cmp(&(b.timestamp, &b.id)));
        let to_remove = sorted.len() as u64 - max_count;
        let candidates: HashSet<String> = sorted[..to_remove as usize]
            .iter()
            .map(|c| c.id.clone())
            .collect();
        self.apply_min_retention(checkpoints, candidates, min_retention)
    }

    /// Size-based cleanup using the real `blob_size` per checkpoint
    /// (missing sizes count as 0 and are warned about).
    fn evaluate_size(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        max_total_bytes: u64,
        min_retention: u64,
    ) -> Vec<String> {
        let mut missing = 0usize;
        let total_size: u64 = checkpoints
            .iter()
            .map(|c| {
                c.blob_size.unwrap_or_else(|| {
                    missing += 1;
                    0
                })
            })
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
        sorted.sort_by(|a, b| (a.timestamp, &a.id).cmp(&(b.timestamp, &b.id)));
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

    /// Tiered cleanup with age-window semantics: for each tier (evaluated
    /// oldest-first), keep at most one checkpoint per
    /// `retention_interval_days` window; `interval == 0` keeps all
    /// checkpoints in that tier. `min_retention` newest checkpoints are
    /// always kept.
    fn evaluate_tiered(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        tiers: &[RetentionTier],
        min_retention: u64,
        clock: &CheckpointClock,
    ) -> Vec<String> {
        let Some(now) = clock.now_ms().filter(|ms| *ms >= 0) else {
            return Vec::new();
        };
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
                    let age_days = (now - c.timestamp).max(0) / DAY_MS;
                    age_days >= tier.min_age_days as i64
                        && tier.max_age_days.is_none_or(|max| age_days < max as i64)
                })
                .collect();

            let mut by_window: HashMap<i64, Vec<&CheckpointStorageMetadata>> = HashMap::new();
            for cp in in_tier {
                let age_ms = (now - cp.timestamp).max(0);
                let window = age_ms / DAY_MS / tier.retention_interval_days as i64;
                by_window.entry(window).or_default().push(cp);
            }
            for group in by_window.into_values() {
                if let Some(keep) = group
                    .iter()
                    .max_by(|a, b| (a.timestamp, &a.id).cmp(&(b.timestamp, &b.id)))
                {
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
            let mut out: Vec<String> = candidates.into_iter().collect();
            out.sort_unstable();
            return out;
        }
        let mut sorted = checkpoints.to_vec();
        sorted.sort_by(|a, b| (a.timestamp, &a.id).cmp(&(b.timestamp, &b.id)));
        let keep: HashSet<String> = sorted
            .iter()
            .rev()
            .take(min_retention as usize)
            .map(|c| c.id.clone())
            .collect();
        let mut out: Vec<String> = candidates
            .into_iter()
            .filter(|id| !keep.contains(id))
            .collect();
        out.sort_unstable();
        out
    }
}

impl Default for CleanupExecutor {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
#[path = "cleanup_policy/tests.rs"]
mod tests;
