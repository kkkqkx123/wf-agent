use wf_types::storage::CheckpointStorageMetadata;

#[derive(Debug, Clone)]
pub enum CleanupStrategy {
    TimeBased { max_age_seconds: u64 },
    CountBased { max_checkpoints: u64 },
    SizeBased { max_total_bytes: u64 },
    Tiered(Vec<CleanupStrategy>),
}

pub struct CleanupExecutor;

impl CleanupExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        strategy: &CleanupStrategy,
    ) -> Vec<String> {
        match strategy {
            CleanupStrategy::TimeBased { max_age_seconds } => {
                self.evaluate_time(checkpoints, *max_age_seconds)
            }
            CleanupStrategy::CountBased { max_checkpoints } => {
                self.evaluate_count(checkpoints, *max_checkpoints)
            }
            CleanupStrategy::SizeBased { max_total_bytes } => {
                self.evaluate_size(checkpoints, *max_total_bytes)
            }
            CleanupStrategy::Tiered(strategies) => {
                let mut result = Vec::new();
                for s in strategies {
                    result.extend(self.evaluate(checkpoints, s));
                }
                result.sort_unstable();
                result.dedup();
                result
            }
        }
    }

    fn evaluate_time(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        max_age_seconds: u64,
    ) -> Vec<String> {
        let now = chrono::Utc::now().timestamp_millis();
        let max_age_ms = (max_age_seconds * 1000) as i64;
        checkpoints
            .iter()
            .filter(|c| now - c.timestamp > max_age_ms)
            .map(|c| c.id.clone())
            .collect()
    }

    fn evaluate_count(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        max_count: u64,
    ) -> Vec<String> {
        if checkpoints.len() as u64 <= max_count {
            return Vec::new();
        }
        let mut sorted: Vec<_> = checkpoints.to_vec();
        sorted.sort_by_key(|c| c.timestamp);
        let to_remove = sorted.len() as u64 - max_count;
        sorted[..to_remove as usize]
            .iter()
            .map(|c| c.id.clone())
            .collect()
    }

    fn evaluate_size(
        &self,
        checkpoints: &[CheckpointStorageMetadata],
        max_total_bytes: u64,
    ) -> Vec<String> {
        let total_size: u64 = checkpoints.len() as u64 * 1024;
        if total_size <= max_total_bytes {
            return Vec::new();
        }
        let mut sorted: Vec<_> = checkpoints.to_vec();
        sorted.sort_by_key(|c| c.timestamp);
        let mut accumulated: u64 = 0;
        let mut to_remove = Vec::new();
        for cp in &sorted {
            if total_size - accumulated <= max_total_bytes {
                break;
            }
            accumulated += 1024;
            to_remove.push(cp.id.clone());
        }
        to_remove
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

    fn make_checkpoint(id: &str, timestamp_ms: i64) -> CheckpointStorageMetadata {
        CheckpointStorageMetadata {
            id: id.to_string(),
            entity_type: "test".to_string(),
            entity_id: "entity-1".to_string(),
            checkpoint_type: wf_types::checkpoint::CheckpointType::Full,
            timestamp: timestamp_ms,
            status: wf_types::checkpoint::CheckpointStatus::Completed,
        }
    }

    #[test]
    fn time_based_cleanup() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("old", now - 3_600_000),
            make_checkpoint("new", now - 1_000),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::TimeBased {
                max_age_seconds: 1800,
            },
        );
        assert_eq!(to_remove, vec!["old"]);
    }

    #[test]
    fn count_based_cleanup() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("c1", now - 5000),
            make_checkpoint("c2", now - 4000),
            make_checkpoint("c3", now - 3000),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::CountBased { max_checkpoints: 2 },
        );
        assert_eq!(to_remove, vec!["c1"]);
    }

    #[test]
    fn count_based_under_limit() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![make_checkpoint("c1", now - 5000)];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::CountBased {
                max_checkpoints: 10,
            },
        );
        assert!(to_remove.is_empty());
    }

    #[test]
    fn tiered_cleanup() {
        let now = chrono::Utc::now().timestamp_millis();
        let checkpoints = vec![
            make_checkpoint("old", now - 3_600_000),
            make_checkpoint("mid", now - 1_800_000),
            make_checkpoint("new", now - 1_000),
        ];
        let executor = CleanupExecutor::new();
        let to_remove = executor.evaluate(
            &checkpoints,
            &CleanupStrategy::Tiered(vec![
                CleanupStrategy::CountBased { max_checkpoints: 2 },
                CleanupStrategy::TimeBased {
                    max_age_seconds: 7200,
                },
            ]),
        );
        assert!(to_remove.contains(&"old".to_string()));
    }
}
