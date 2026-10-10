//! Breadth-first child discovery over the resolver, reporting per-child
//! load outcomes.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use checkpoint_base::error::CheckpointError;
use wf_metrics::CheckpointMetricsCollector;
use wf_types::storage::CheckpointStorageMetadata;

use super::resolver::ChildCheckpointResolver;

/// Single-recovery child discovery over an in-memory parent to children
/// index. The index is a per-recovery deduplication and pruning cache, not
/// cross-call truth: persistent truth lives in the storage parent index.
pub struct ChildDiscovery {
    resolver: Arc<dyn ChildCheckpointResolver>,
}

impl ChildDiscovery {
    pub fn new(resolver: Arc<dyn ChildCheckpointResolver>) -> Self {
        Self { resolver }
    }

    /// Discover reachable children breadth-first, optionally recording load
    /// metrics per child. This checks metadata reachability only; restoring a
    /// discovered entity happens through the restore registry. Size bytes are
    /// unavailable at metadata load and reported as 0; `None` keeps the path
    /// zero-overhead.
    pub fn discover_children_bfs(
        &self,
        parent_id: &str,
        loader: &dyn ChildDiscoveryLoader,
        max_depth: usize,
        metrics: Option<&CheckpointMetricsCollector>,
    ) -> Result<Vec<ChildDiscoveryResult>, CheckpointError> {
        let mut results = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = VecDeque::new();

        visited.insert(parent_id.to_string());
        queue.push_back((parent_id.to_string(), 0));

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let children = self.resolver.resolve_children(&current_id);

            for child_id in &children {
                if visited.contains(child_id) {
                    continue;
                }
                visited.insert(child_id.clone());

                let start = Instant::now();
                let result = match loader.load_child_metadata(child_id) {
                    Ok(_) => ChildDiscoveryResult::Success {
                        checkpoint_id: child_id.clone(),
                        depth: depth + 1,
                    },
                    Err(e) => ChildDiscoveryResult::Failed {
                        checkpoint_id: child_id.clone(),
                        error: e.to_string(),
                    },
                };
                if let Some(metrics) = metrics {
                    metrics.record_load(
                        child_id,
                        start.elapsed().as_millis() as f64,
                        matches!(result, ChildDiscoveryResult::Success { .. }),
                    );
                }

                results.push(result);
                queue.push_back((child_id.clone(), depth + 1));
            }
        }

        Ok(results)
    }

    pub fn summarize_results(results: &[ChildDiscoveryResult]) -> ChildDiscoverySummary {
        let mut success = 0;
        let mut failed = 0;

        for r in results {
            match r {
                ChildDiscoveryResult::Success { .. } => success += 1,
                ChildDiscoveryResult::Failed { .. } => failed += 1,
            }
        }

        ChildDiscoverySummary {
            total: results.len(),
            success,
            failed,
        }
    }
}

pub trait ChildDiscoveryLoader: Send + Sync {
    fn load_child_metadata(
        &self,
        id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError>;
}

#[derive(Debug, Clone)]
pub enum ChildDiscoveryResult {
    Success {
        checkpoint_id: String,
        depth: usize,
    },
    Failed {
        checkpoint_id: String,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub struct ChildDiscoverySummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

impl ChildDiscoverySummary {
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0 && self.total > 0
    }
}
