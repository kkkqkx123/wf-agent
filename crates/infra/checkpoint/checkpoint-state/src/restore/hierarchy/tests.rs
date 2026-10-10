use super::*;
use checkpoint_base::error::CheckpointError;
use std::sync::Arc;
use wf_metrics::CheckpointMetricsCollector;
use wf_types::storage::CheckpointStorageMetadata;

#[test]
fn storage_child_resolver_basic() {
    let resolver = InMemoryChildResolver::new();
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
    let inner = Arc::new(InMemoryChildResolver::new());
    inner.register_relationship("p1", "c1");

    let cached = CachedChildResolver::new(inner);
    let children1 = cached.resolve_children("p1");
    let children2 = cached.resolve_children("p1");

    assert_eq!(children1.len(), 1);
    assert_eq!(children2.len(), 1);
}

#[test]
fn hierarchy_restorer_bfs() {
    let storage_resolver = InMemoryChildResolver::new();
    storage_resolver.register_relationship("root", "child-a");
    storage_resolver.register_relationship("root", "child-b");
    storage_resolver.register_relationship("child-a", "grandchild-a1");
    let resolver: Arc<dyn ChildCheckpointResolver> = Arc::new(storage_resolver);

    let restorer = ChildDiscovery::new(resolver);

    struct MockLoader;
    impl ChildDiscoveryLoader for MockLoader {
        fn load_child_metadata(
            &self,
            _id: &str,
        ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
            Ok(Some(CheckpointStorageMetadata {
                id: _id.to_string(),
                entity_type: "test".to_string(),
                entity_id: "test-entity".to_string(),
                parent_entity_id: None,
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
        .discover_children_bfs("root", &MockLoader, 3, None)
        .unwrap();

    assert_eq!(results.len(), 3);

    let summary = ChildDiscovery::summarize_results(&results);
    assert_eq!(summary.total, 3);
    assert!(summary.all_succeeded());
}

#[test]
fn restore_records_load_metrics() {
    let storage_resolver = InMemoryChildResolver::new();
    storage_resolver.register_relationship("root", "child-a");
    storage_resolver.register_relationship("root", "child-b");
    let resolver: Arc<dyn ChildCheckpointResolver> = Arc::new(storage_resolver);

    let restorer = ChildDiscovery::new(resolver);
    let metrics = CheckpointMetricsCollector::new(wf_metrics::CollectorConfig::default());

    struct FailingLoader;
    impl ChildDiscoveryLoader for FailingLoader {
        fn load_child_metadata(
            &self,
            _id: &str,
        ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
            Err(CheckpointError::NotFound {
                id: _id.to_string(),
            })
        }
    }

    struct MockLoader;
    impl ChildDiscoveryLoader for MockLoader {
        fn load_child_metadata(
            &self,
            _id: &str,
        ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
            Ok(Some(CheckpointStorageMetadata {
                id: _id.to_string(),
                entity_type: "test".to_string(),
                entity_id: "test-entity".to_string(),
                parent_entity_id: None,
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
        .discover_children_bfs("root", &FailingLoader, 3, Some(&metrics))
        .unwrap();
    let _ = restorer
        .discover_children_bfs("root", &MockLoader, 3, Some(&metrics))
        .unwrap();

    let stats = metrics.usage_stats();
    assert_eq!(stats.load_count, 4);
    assert_eq!(stats.load_failures, 2);
    assert!(stats.avg_load_duration_ms >= 0.0);
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

    tx.execute(|_op| Box::pin(async { Ok(()) })).await.unwrap();

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

#[test]
fn rollback_keeps_pending_and_rejects_second_rollback() {
    let mut tx = RecoveryTransaction::new();
    tx.register(RecoveryOperation {
        checkpoint_id: "done".to_string(),
        operation_type: RecoveryOperationType::Restore,
        status: RecoveryOperationStatus::Completed,
    });
    tx.register(RecoveryOperation {
        checkpoint_id: "waiting".to_string(),
        operation_type: RecoveryOperationType::Restore,
        status: RecoveryOperationStatus::Pending,
    });

    let first = tx.rollback();
    assert_eq!(first.status, RecoveryTransactionStatus::RolledBack);
    assert!(first.errors.is_empty());
    assert_eq!(tx.completed_count(), 0);
    assert_eq!(tx.failed_count(), 1);
    assert!(matches!(
        tx.operations()[1].status,
        RecoveryOperationStatus::Pending
    ));

    let second = tx.rollback();
    assert_eq!(
        second.errors,
        vec!["transaction already rolled back".to_string()]
    );
    assert_eq!(tx.failed_count(), 1);
}

thread_local! {
    static ORDER: std::cell::RefCell<Vec<i32>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn order_push(value: i32) {
    ORDER.with(|o| o.borrow_mut().push(value));
}

fn order_take() -> Vec<i32> {
    ORDER.with(|o| std::mem::take(&mut *o.borrow_mut()))
}
