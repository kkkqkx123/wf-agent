use crate::error::CheckpointError;
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use wf_types::storage::CheckpointStorageMetadata;

pub trait ChildCheckpointResolver: Send + Sync {
    fn resolve_children(&self, parent_id: &str) -> Vec<String>;
    fn resolve_parent(&self, child_id: &str) -> Option<String>;
}

pub struct StorageChildResolver {
    parent_to_children: DashMap<String, Vec<String>>,
    child_to_parent: DashMap<String, String>,
}

impl StorageChildResolver {
    pub fn new() -> Self {
        Self {
            parent_to_children: DashMap::new(),
            child_to_parent: DashMap::new(),
        }
    }

    pub fn register_relationship(&self, parent_id: &str, child_id: &str) {
        self.parent_to_children
            .entry(parent_id.to_string())
            .or_default()
            .push(child_id.to_string());
        self.child_to_parent
            .insert(child_id.to_string(), parent_id.to_string());
    }

    pub fn register_batch(&self, relationships: &[(String, String)]) {
        for (parent, child) in relationships {
            self.register_relationship(parent, child);
        }
    }
}

impl Default for StorageChildResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ChildCheckpointResolver for StorageChildResolver {
    fn resolve_children(&self, parent_id: &str) -> Vec<String> {
        self.parent_to_children
            .get(parent_id)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    fn resolve_parent(&self, child_id: &str) -> Option<String> {
        self.child_to_parent.get(child_id).map(|v| v.clone())
    }
}

pub struct CachedChildResolver {
    inner: Arc<dyn ChildCheckpointResolver>,
    cache: DashMap<String, Vec<String>>,
}

impl CachedChildResolver {
    pub fn new(inner: Arc<dyn ChildCheckpointResolver>) -> Self {
        Self {
            inner,
            cache: DashMap::new(),
        }
    }

    pub fn invalidate(&self, parent_id: &str) {
        cache_remove(&self.cache, parent_id);
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

fn cache_remove(cache: &DashMap<String, Vec<String>>, key: &str) {
    cache.remove(key);
}

impl ChildCheckpointResolver for CachedChildResolver {
    fn resolve_children(&self, parent_id: &str) -> Vec<String> {
        if let Some(cached) = self.cache.get(parent_id) {
            return cached.clone();
        }
        let children = self.inner.resolve_children(parent_id);
        self.cache.insert(parent_id.to_string(), children.clone());
        children
    }

    fn resolve_parent(&self, child_id: &str) -> Option<String> {
        self.inner.resolve_parent(child_id)
    }
}

pub struct HierarchyRestorer {
    resolver: Arc<dyn ChildCheckpointResolver>,
}

impl HierarchyRestorer {
    pub fn new(resolver: Arc<dyn ChildCheckpointResolver>) -> Self {
        Self { resolver }
    }

    pub fn restore_children_bfs(
        &self,
        parent_id: &str,
        loader: &dyn CheckpointLoader,
        max_depth: usize,
    ) -> Result<Vec<RestoreResult>, CheckpointError> {
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

                let result = match loader.load_metadata(child_id) {
                    Ok(_) => RestoreResult::Success {
                        checkpoint_id: child_id.clone(),
                        depth: depth + 1,
                    },
                    Err(e) => RestoreResult::Failed {
                        checkpoint_id: child_id.clone(),
                        error: e.to_string(),
                    },
                };

                results.push(result);
                queue.push_back((child_id.clone(), depth + 1));
            }
        }

        Ok(results)
    }

    pub fn summarize_results(results: &[RestoreResult]) -> RestoreSummary {
        let mut success = 0;
        let mut failed = 0;

        for r in results {
            match r {
                RestoreResult::Success { .. } => success += 1,
                RestoreResult::Failed { .. } => failed += 1,
            }
        }

        RestoreSummary {
            total: results.len(),
            success,
            failed,
        }
    }
}

pub trait CheckpointLoader: Send + Sync {
    fn load_metadata(&self, id: &str)
        -> Result<Option<CheckpointStorageMetadata>, CheckpointError>;
}

#[derive(Debug, Clone)]
pub enum RestoreResult {
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
pub struct RestoreSummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

impl RestoreSummary {
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0 && self.total > 0
    }
}

pub struct RecoveryTransaction {
    operations: Vec<RecoveryOperation>,
}

impl RecoveryTransaction {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    pub fn register(&mut self, operation: RecoveryOperation) {
        self.operations.push(operation);
    }

    pub fn operations(&self) -> &[RecoveryOperation] {
        &self.operations
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

impl Default for RecoveryTransaction {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RecoveryOperation {
    pub checkpoint_id: String,
    pub operation_type: RecoveryOperationType,
    pub status: RecoveryOperationStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryOperationType {
    Restore,
    Delete,
    Reconstruct,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryOperationStatus {
    Pending,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum RollbackStrategy {
    AllOrNothing,
    BestEffort,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_child_resolver_basic() {
        let resolver = StorageChildResolver::new();
        resolver.register_relationship("parent-1", "child-1");
        resolver.register_relationship("parent-1", "child-2");
        resolver.register_relationship("child-1", "grandchild-1");

        let children = resolver.resolve_children("parent-1");
        assert_eq!(children.len(), 2);
        assert!(children.contains(&"child-1".to_string()));
        assert!(children.contains(&"child-2".to_string()));

        let parent = resolver.resolve_parent("child-1");
        assert_eq!(parent, Some("parent-1".to_string()));

        let grand_children = resolver.resolve_children("child-1");
        assert_eq!(grand_children.len(), 1);
    }

    #[test]
    fn cached_resolver_caches_results() {
        let inner = Arc::new(StorageChildResolver::new());
        inner.register_relationship("p1", "c1");

        let cached = CachedChildResolver::new(inner);
        let children1 = cached.resolve_children("p1");
        let children2 = cached.resolve_children("p1");

        assert_eq!(children1.len(), 1);
        assert_eq!(children2.len(), 1);
    }

    #[test]
    fn hierarchy_restorer_bfs() {
        let storage_resolver = StorageChildResolver::new();
        storage_resolver.register_relationship("root", "child-a");
        storage_resolver.register_relationship("root", "child-b");
        storage_resolver.register_relationship("child-a", "grandchild-a1");
        let resolver: Arc<dyn ChildCheckpointResolver> = Arc::new(storage_resolver);

        let restorer = HierarchyRestorer::new(resolver);

        struct MockLoader;
        impl CheckpointLoader for MockLoader {
            fn load_metadata(
                &self,
                _id: &str,
            ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
                Ok(Some(CheckpointStorageMetadata {
                    id: _id.to_string(),
                    entity_type: "test".to_string(),
                    entity_id: "test-entity".to_string(),
                    checkpoint_type: wf_types::checkpoint::CheckpointType::Full,
                    timestamp: 0,
                    status: wf_types::checkpoint::CheckpointStatus::Completed,
                }))
            }
        }

        let results = restorer
            .restore_children_bfs("root", &MockLoader, 3)
            .unwrap();

        assert_eq!(results.len(), 3);

        let summary = HierarchyRestorer::summarize_results(&results);
        assert_eq!(summary.total, 3);
        assert!(summary.all_succeeded());
    }

    #[test]
    fn recovery_transaction_tracks_operations() {
        let mut tx = RecoveryTransaction::new();

        tx.register(RecoveryOperation {
            checkpoint_id: "cp-1".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Pending,
        });

        tx.register(RecoveryOperation {
            checkpoint_id: "cp-2".to_string(),
            operation_type: RecoveryOperationType::Reconstruct,
            status: RecoveryOperationStatus::Completed,
        });

        assert_eq!(tx.len(), 2);
    }

    #[test]
    fn rollback_strategy_variants_exist() {
        let _all_or_nothing = RollbackStrategy::AllOrNothing;
        let _best_effort = RollbackStrategy::BestEffort;
    }
}
