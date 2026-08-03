use crate::error::CheckpointError;
use crate::metrics::CheckpointMetricsCollector;
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;
use wf_types::checkpoint::CheckpointLoadMetrics;
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

    /// Restore children breadth-first, optionally recording load metrics per
    /// child. Size bytes are unavailable at metadata load and reported as 0;
    /// `None` keeps the path zero-overhead.
    pub fn restore_children_bfs(
        &self,
        parent_id: &str,
        loader: &dyn CheckpointLoader,
        max_depth: usize,
        metrics: Option<&CheckpointMetricsCollector>,
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

                let start = Instant::now();
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
                if let Some(metrics) = metrics {
                    metrics.record_load(
                        &CheckpointLoadMetrics {
                            duration_ms: start.elapsed().as_millis() as u64,
                            size_bytes: 0,
                            compressed: false,
                        },
                        matches!(result, RestoreResult::Success { .. }),
                    );
                }

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
    rollback_strategy: RollbackStrategy,
    /// Compensating actions executed LIFO on rollback (TS
    /// `addCompensatingAction`).
    compensating_actions: Vec<Box<dyn Fn() -> Result<(), String> + Send + Sync>>,
    status: RecoveryTransactionStatus,
}

impl RecoveryTransaction {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            rollback_strategy: RollbackStrategy::AllOrNothing,
            compensating_actions: Vec::new(),
            status: RecoveryTransactionStatus::Pending,
        }
    }

    pub fn with_rollback_strategy(strategy: RollbackStrategy) -> Self {
        Self {
            operations: Vec::new(),
            rollback_strategy: strategy,
            compensating_actions: Vec::new(),
            status: RecoveryTransactionStatus::Pending,
        }
    }

    /// Transition the transaction into the in-progress state.
    pub fn begin(&mut self) {
        self.status = RecoveryTransactionStatus::InProgress;
    }

    pub fn register(&mut self, operation: RecoveryOperation) {
        self.operations.push(operation);
    }

    /// Register a compensating action executed (LIFO) during rollback.
    pub fn add_compensating_action(
        &mut self,
        action: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
    ) {
        self.compensating_actions.push(action);
    }

    /// Execute all pending operations through the provided executor.
    /// Successful operations are marked `Completed`; failed ones are marked
    /// `Failed` and, under `AllOrNothing`, previously completed operations are
    /// rolled back.
    pub async fn execute<F, Fut>(&mut self, mut executor: F) -> Result<(), CheckpointError>
    where
        F: FnMut(&RecoveryOperation) -> Fut,
        Fut: std::future::Future<Output = Result<(), CheckpointError>>,
    {
        self.begin();
        for operation in self.operations.iter_mut() {
            if operation.status != RecoveryOperationStatus::Pending {
                continue;
            }
            match executor(operation).await {
                Ok(()) => {
                    operation.status = RecoveryOperationStatus::Completed;
                }
                Err(e) => {
                    operation.status = RecoveryOperationStatus::Failed(e.to_string());
                    if self.rollback_strategy == RollbackStrategy::AllOrNothing {
                        self.rollback();
                        return Err(CheckpointError::Coordinator(format!(
                            "recovery transaction failed: {}",
                            e
                        )));
                    }
                }
            }
        }
        if self.status == RecoveryTransactionStatus::InProgress {
            self.status = RecoveryTransactionStatus::Completed;
        }
        Ok(())
    }

    /// Mark the operation at `index` as completed.
    pub fn complete(&mut self, index: usize) {
        if let Some(operation) = self.operations.get_mut(index) {
            operation.status = RecoveryOperationStatus::Completed;
        }
    }

    /// Mark the operation at `index` as failed with the given error message.
    pub fn fail(&mut self, index: usize, error: impl Into<String>) {
        if let Some(operation) = self.operations.get_mut(index) {
            operation.status = RecoveryOperationStatus::Failed(error.into());
        }
    }

    /// Commit the transaction: with `AllOrNothing`, any failed operation
    /// triggers a rollback; otherwise the transaction commits with partial
    /// success (TS `commit` semantics).
    pub fn commit(&mut self) -> RecoveryTransactionResult {
        let has_failed = self
            .operations
            .iter()
            .any(|op| matches!(op.status, RecoveryOperationStatus::Failed(_)));
        match (self.rollback_strategy, has_failed) {
            (RollbackStrategy::AllOrNothing, true) => self.rollback(),
            _ => {
                self.status = RecoveryTransactionStatus::Completed;
                RecoveryTransactionResult {
                    status: RecoveryTransactionStatus::Completed,
                    errors: Vec::new(),
                }
            }
        }
    }

    /// Rollback the transaction: every operation is marked `Failed("rolled
    /// back")` — including previously completed ones — and the registered
    /// compensating actions run LIFO. Errors from the compensating actions
    /// are collected and reported.
    pub fn rollback(&mut self) -> RecoveryTransactionResult {
        for operation in self.operations.iter_mut() {
            operation.status = RecoveryOperationStatus::Failed("rolled back".to_string());
        }
        let mut errors = Vec::new();
        for action in self.compensating_actions.drain(..).rev() {
            if let Err(err) = action() {
                errors.push(err);
            }
        }
        self.status = if errors.is_empty() {
            RecoveryTransactionStatus::RolledBack
        } else {
            RecoveryTransactionStatus::RolledBackWithErrors
        };
        RecoveryTransactionResult {
            status: self.status.clone(),
            errors,
        }
    }

    pub fn operations(&self) -> &[RecoveryOperation] {
        &self.operations
    }

    pub fn status(&self) -> &RecoveryTransactionStatus {
        &self.status
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Number of operations that completed successfully.
    pub fn completed_count(&self) -> usize {
        self.operations
            .iter()
            .filter(|op| op.status == RecoveryOperationStatus::Completed)
            .count()
    }

    /// Number of operations that failed.
    pub fn failed_count(&self) -> usize {
        self.operations
            .iter()
            .filter(|op| matches!(op.status, RecoveryOperationStatus::Failed(_)))
            .count()
    }

    pub fn rollback_strategy(&self) -> &RollbackStrategy {
        &self.rollback_strategy
    }
}

/// Result of a commit/rollback, aligned with the TS `RecoveryTransactionResult`.
#[derive(Debug, Clone)]
pub struct RecoveryTransactionResult {
    pub status: RecoveryTransactionStatus,
    pub errors: Vec<String>,
}

/// Transaction lifecycle aligned with the TS `RecoveryTransactionStatus`.
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryTransactionStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    RolledBack,
    RolledBackWithErrors,
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

#[derive(Debug, Clone, Copy, PartialEq)]
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
                    previous_checkpoint_id: None,
                    base_checkpoint_id: None,
                    chain_root_id: None,
                    chain_position: None,
                    blob_size: None,
                    tags: None,
                    custom_fields: None,
                }))
            }
        }

        let results = restorer
            .restore_children_bfs("root", &MockLoader, 3, None)
            .unwrap();

        assert_eq!(results.len(), 3);

        let summary = HierarchyRestorer::summarize_results(&results);
        assert_eq!(summary.total, 3);
        assert!(summary.all_succeeded());
    }

    #[test]
    fn restore_records_load_metrics() {
        let storage_resolver = StorageChildResolver::new();
        storage_resolver.register_relationship("root", "child-a");
        storage_resolver.register_relationship("root", "child-b");
        let resolver: Arc<dyn ChildCheckpointResolver> = Arc::new(storage_resolver);

        let restorer = HierarchyRestorer::new(resolver);
        let metrics = CheckpointMetricsCollector::new();

        struct FailingLoader;
        impl CheckpointLoader for FailingLoader {
            fn load_metadata(
                &self,
                _id: &str,
            ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
                Err(CheckpointError::NotFound {
                    id: _id.to_string(),
                })
            }
        }

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
                    previous_checkpoint_id: None,
                    base_checkpoint_id: None,
                    chain_root_id: None,
                    chain_position: None,
                    blob_size: None,
                    tags: None,
                    custom_fields: None,
                }))
            }
        }

        let _ = restorer
            .restore_children_bfs("root", &FailingLoader, 3, Some(&metrics))
            .unwrap();
        let _ = restorer
            .restore_children_bfs("root", &MockLoader, 3, Some(&metrics))
            .unwrap();

        let agg = metrics.aggregate();
        assert_eq!(agg.load_count, 4);
        assert_eq!(agg.load_failed, 2);
        assert_eq!(agg.load_success, 2);
        assert!(agg.avg_load_duration_ms >= 0.0);
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

    #[tokio::test]
    async fn recovery_transaction_executes_all_pending() {
        let mut tx = RecoveryTransaction::new();
        tx.register(RecoveryOperation {
            checkpoint_id: "cp-1".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Pending,
        });
        tx.register(RecoveryOperation {
            checkpoint_id: "cp-2".to_string(),
            operation_type: RecoveryOperationType::Delete,
            status: RecoveryOperationStatus::Pending,
        });

        let _ = tx.execute(|_op| Box::pin(async { Ok(()) })).await.unwrap();

        assert_eq!(tx.completed_count(), 2);
        assert_eq!(tx.failed_count(), 0);
    }

    #[tokio::test]
    async fn recovery_transaction_best_effort_marks_failures() {
        let mut tx = RecoveryTransaction::with_rollback_strategy(RollbackStrategy::BestEffort);
        tx.register(RecoveryOperation {
            checkpoint_id: "ok".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Pending,
        });
        tx.register(RecoveryOperation {
            checkpoint_id: "bad".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Pending,
        });

        tx.execute(|op| {
            let op = op.clone();
            Box::pin(async move {
                if op.checkpoint_id == "bad" {
                    Err(CheckpointError::NotFound {
                        id: op.checkpoint_id.clone(),
                    })
                } else {
                    Ok(())
                }
            })
        })
        .await
        .unwrap();

        assert_eq!(tx.completed_count(), 1);
        assert_eq!(tx.failed_count(), 1);
    }

    #[tokio::test]
    async fn recovery_transaction_all_or_nothing_rolls_back_on_failure() {
        let mut tx = RecoveryTransaction::with_rollback_strategy(RollbackStrategy::AllOrNothing);
        tx.register(RecoveryOperation {
            checkpoint_id: "ok".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Pending,
        });
        tx.register(RecoveryOperation {
            checkpoint_id: "bad".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Pending,
        });

        let err = tx
            .execute(|op| {
                let op = op.clone();
                Box::pin(async move {
                    if op.checkpoint_id == "bad" {
                        Err(CheckpointError::NotFound {
                            id: op.checkpoint_id.clone(),
                        })
                    } else {
                        Ok(())
                    }
                })
            })
            .await
            .unwrap_err();

        assert!(err.to_string().contains("recovery transaction failed"));
        assert_eq!(tx.completed_count(), 0, "completed ops must be rolled back");
        assert_eq!(tx.failed_count(), 2);
    }

    #[test]
    fn recovery_transaction_manual_complete_fail_rollback() {
        let mut tx = RecoveryTransaction::new();
        tx.register(RecoveryOperation {
            checkpoint_id: "cp-1".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Pending,
        });
        tx.complete(0);
        assert_eq!(tx.completed_count(), 1);

        tx.register(RecoveryOperation {
            checkpoint_id: "cp-2".to_string(),
            operation_type: RecoveryOperationType::Reconstruct,
            status: RecoveryOperationStatus::Pending,
        });
        tx.fail(1, "boom");
        assert_eq!(tx.failed_count(), 1);

        tx.rollback();
        assert_eq!(tx.completed_count(), 0, "completed ops are rolled back too");
        assert_eq!(tx.failed_count(), 2);
    }

    #[test]
    fn rollback_strategy_variants_exist() {
        let _all_or_nothing = RollbackStrategy::AllOrNothing;
        let _best_effort = RollbackStrategy::BestEffort;
    }

    #[test]
    fn transaction_lifecycle_begin_commit() {
        let mut tx = RecoveryTransaction::new();
        assert_eq!(tx.status(), &RecoveryTransactionStatus::Pending);
        tx.begin();
        assert_eq!(tx.status(), &RecoveryTransactionStatus::InProgress);

        tx.register(RecoveryOperation {
            checkpoint_id: "cp-1".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Pending,
        });
        tx.complete(0);

        let result = tx.commit();
        assert_eq!(result.status, RecoveryTransactionStatus::Completed);
        assert_eq!(tx.status(), &RecoveryTransactionStatus::Completed);
    }

    #[test]
    fn all_or_nothing_commit_rolls_back_on_failure() {
        let mut tx = RecoveryTransaction::with_rollback_strategy(RollbackStrategy::AllOrNothing);
        tx.begin();
        tx.register(RecoveryOperation {
            checkpoint_id: "ok".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Completed,
        });
        tx.register(RecoveryOperation {
            checkpoint_id: "bad".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Failed("boom".to_string()),
        });

        let result = tx.commit();
        assert_eq!(result.status, RecoveryTransactionStatus::RolledBack);
        assert_eq!(tx.completed_count(), 0, "completed ops rolled back");
    }

    #[test]
    fn best_effort_commit_keeps_partial_success() {
        let mut tx = RecoveryTransaction::with_rollback_strategy(RollbackStrategy::BestEffort);
        tx.begin();
        tx.register(RecoveryOperation {
            checkpoint_id: "ok".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Completed,
        });
        tx.register(RecoveryOperation {
            checkpoint_id: "bad".to_string(),
            operation_type: RecoveryOperationType::Restore,
            status: RecoveryOperationStatus::Failed("boom".to_string()),
        });

        let result = tx.commit();
        assert_eq!(result.status, RecoveryTransactionStatus::Completed);
        assert_eq!(tx.completed_count(), 1);
    }

    #[test]
    fn rollback_runs_compensating_actions_lifo() {
        let mut tx = RecoveryTransaction::new();
        tx.add_compensating_action(Box::new(|| {
            order_push(1);
            Ok(())
        }));
        tx.add_compensating_action(Box::new(|| {
            order_push(2);
            Ok(())
        }));

        let result = tx.rollback();
        assert_eq!(result.status, RecoveryTransactionStatus::RolledBack);
        assert!(result.errors.is_empty());
        assert_eq!(order_take(), vec![2, 1], "compensating actions run LIFO");
    }

    #[test]
    fn rollback_reports_compensating_errors() {
        let mut tx = RecoveryTransaction::new();
        tx.add_compensating_action(Box::new(|| Err("undo failed".to_string())));
        let result = tx.rollback();
        assert_eq!(
            result.status,
            RecoveryTransactionStatus::RolledBackWithErrors
        );
        assert_eq!(result.errors, vec!["undo failed".to_string()]);
    }

    thread_local! {
        static ORDER: std::cell::RefCell<Vec<i32>> = std::cell::RefCell::new(Vec::new());
    }

    fn order_push(value: i32) {
        ORDER.with(|o| o.borrow_mut().push(value));
    }

    fn order_take() -> Vec<i32> {
        ORDER.with(|o| std::mem::take(&mut *o.borrow_mut()))
    }
}
