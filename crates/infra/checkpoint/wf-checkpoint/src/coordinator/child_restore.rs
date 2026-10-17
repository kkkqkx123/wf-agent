use crate::coordinator::base::ChildDiscoveryIndex;
use crate::coordinator::events::publish_best_effort_failed;
use checkpoint_base::error::CheckpointError;
use checkpoint_file::event::CheckpointEventBus;
use checkpoint_state::restore::hierarchy::{
    ChildDiscovery, ChildDiscoverySummary, ChildRestoreFailure, InMemoryChildResolver,
};
use checkpoint_state::restore::registry::RestoreStrategyRegistry;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use wf_common::gate::ConcurrencyGate;
use wf_types::storage::CheckpointStorageMetadata;

/// Bounded concurrency for the per-child restore phase.
const CHILD_RESTORE_CONCURRENCY: usize = 5;

/// Depth limit of the post-restore child discovery traversal.
const CHILD_DISCOVERY_DEPTH: usize = 8;

/// Total restore attempts per child (one initial attempt plus retries).
const CHILD_RESTORE_MAX_ATTEMPTS: u32 = 3;

/// Initial retry backoff; doubled after every failed attempt.
const CHILD_RESTORE_INITIAL_BACKOFF_MS: u64 = 500;

/// How the parent restore reacts to child restore failures.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChildRestorePolicy {
    /// Restore what can be restored, report every failure, and keep the
    /// parent restore successful. The report carries the details.
    #[default]
    BestEffort,
    /// Fail the whole child restore phase when any child fails. Use when
    /// partial recovery would be more dangerous than no recovery.
    Strict,
}

/// Restore one child through the restore strategy registry when one is
/// registered for the child's entity type. Raw bytes are loaded through
/// `load_bytes`, so each coordinator supplies the loader matching its own
/// checkpoint type. Returns whether the child was restored; `false` means
/// no strategy or no stored bytes.
async fn restore_child_once<F, Fut>(
    meta: &CheckpointStorageMetadata,
    restore_registry: Option<&RestoreStrategyRegistry>,
    load_bytes: &F,
) -> Result<bool, CheckpointError>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Option<Vec<u8>>, CheckpointError>>,
{
    let Some(reg) = restore_registry else {
        return Ok(false);
    };
    let Some(data) = load_bytes(meta.id.clone()).await? else {
        return Ok(false);
    };
    reg.restore(&meta.entity_type, &meta.id, &data).await?;
    Ok(true)
}

/// Retry wrapper around `restore_child_once`: transient storage or strategy
/// failures get bounded retries with exponential backoff so a single hiccup
/// does not silently drop a child from the restored hierarchy.
async fn restore_child_with_retry<F, Fut>(
    meta: &CheckpointStorageMetadata,
    restore_registry: Option<&RestoreStrategyRegistry>,
    load_bytes: &F,
) -> Result<bool, CheckpointError>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Option<Vec<u8>>, CheckpointError>>,
{
    let mut attempt = 1;
    let mut backoff_ms = CHILD_RESTORE_INITIAL_BACKOFF_MS;
    loop {
        match restore_child_once(meta, restore_registry, load_bytes).await {
            Ok(restored) => return Ok(restored),
            Err(err) if attempt < CHILD_RESTORE_MAX_ATTEMPTS => {
                tracing::warn!(
                    child_id = %meta.entity_id,
                    checkpoint_id = %meta.id,
                    attempt,
                    max_attempts = CHILD_RESTORE_MAX_ATTEMPTS,
                    error = %err,
                    "child restore failed; retrying"
                );
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                attempt += 1;
                backoff_ms *= 2;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Post-restore child pass shared by both coordinators: restore every child
/// with bounded concurrency, index the resolved metadata, then walk the
/// parent-child relationships breadth-first.
///
/// Children are passed in pre-resolved as the caller's latest-per-child
/// query result, so the discovery covers every child that ever
/// checkpointed, including ones spawned after the parent's own last
/// persist. `load_bytes` resolves a checkpoint id to its raw bytes.
///
/// Every failure is recorded in the returned summary and published on the
/// checkpoint event bus. `ChildRestorePolicy::Strict` turns any failure into
/// an error; the default keeps the parent restore successful.
#[allow(clippy::too_many_arguments)]
pub async fn collect_child_restore<F, Fut>(
    latest_by_child: Vec<CheckpointStorageMetadata>,
    checkpoint_id: &str,
    parent_entity_id: &str,
    restore_registry: Option<RestoreStrategyRegistry>,
    policy: ChildRestorePolicy,
    bus: Option<&CheckpointEventBus>,
    load_bytes: F,
) -> Result<ChildDiscoverySummary, CheckpointError>
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Option<Vec<u8>>, CheckpointError>> + Send + 'static,
{
    if latest_by_child.is_empty() {
        return Ok(ChildDiscoverySummary {
            total: 0,
            success: 0,
            failed: 0,
            restored: 0,
            skipped: 0,
            failures: Vec::new(),
        });
    }

    let gate = Arc::new(ConcurrencyGate::new(CHILD_RESTORE_CONCURRENCY));
    let load_bytes = Arc::new(load_bytes);
    let mut handles = Vec::with_capacity(latest_by_child.len());
    for meta in latest_by_child {
        let gate = gate.clone();
        let restore_registry = restore_registry.clone();
        let load_bytes = Arc::clone(&load_bytes);
        handles.push(tokio::spawn(async move {
            let result = match gate.acquire_wait().await {
                Ok(_permit) => {
                    restore_child_with_retry(
                        &meta,
                        restore_registry.as_ref(),
                        &|id: String| load_bytes(id),
                    )
                    .await
                }
                Err(e) => Err(CheckpointError::Internal(format!(
                    "child restore gate acquire failed: {e}"
                ))),
            };
            (meta, result)
        }));
    }

    let resolver = InMemoryChildResolver::new();
    let mut index: HashMap<String, CheckpointStorageMetadata> = HashMap::new();
    let mut restore_failures: Vec<ChildRestoreFailure> = Vec::new();
    let mut restored = 0usize;

    for handle in handles {
        match handle.await {
            Ok((meta, Ok(did_restore))) => {
                if did_restore {
                    restored += 1;
                }
                index.insert(meta.id.clone(), meta.clone());
                resolver.register_relationship(checkpoint_id, &meta.id);
            }
            Ok((meta, Err(err))) => {
                tracing::warn!(
                    parent = %parent_entity_id,
                    child_id = %meta.entity_id,
                    checkpoint_id = %meta.id,
                    error = %err,
                    "child restore failed"
                );
                publish_best_effort_failed(
                    bus,
                    Some(meta.id.clone()),
                    &meta.entity_id,
                    "child_restore",
                    &err.to_string(),
                );
                restore_failures.push(ChildRestoreFailure {
                    checkpoint_id: meta.id.clone(),
                    entity_id: meta.entity_id.clone(),
                    error: err.to_string(),
                });
                // Keep the node reachable so descendants are still
                // discovered even when this child's state could not load.
                index.insert(meta.id.clone(), meta.clone());
                resolver.register_relationship(checkpoint_id, &meta.id);
            }
            Err(join_err) => {
                tracing::warn!(
                    parent = %parent_entity_id,
                    error = %join_err,
                    "child restore task panicked"
                );
                publish_best_effort_failed(
                    bus,
                    None,
                    parent_entity_id,
                    "child_restore",
                    &format!("child restore task panicked: {join_err}"),
                );
                restore_failures.push(ChildRestoreFailure {
                    checkpoint_id: String::new(),
                    entity_id: String::new(),
                    error: format!("child restore task panicked: {join_err}"),
                });
            }
        }
    }

    let loader = ChildDiscoveryIndex::new(index);
    let discovery = ChildDiscovery::new(Arc::new(resolver));
    let results =
        discovery.discover_children_bfs(checkpoint_id, &loader, CHILD_DISCOVERY_DEPTH, None)?;
    let mut summary = ChildDiscovery::summarize_results(&results);

    for failure in restore_failures {
        if failure.checkpoint_id.is_empty()
            || !summary
                .failures
                .iter()
                .any(|known| known.checkpoint_id == failure.checkpoint_id)
        {
            summary.failures.push(failure);
        }
    }
    summary.failed = summary.failures.len();
    // A restore failure can belong to a child that BFS did not count (for
    // example when the parent relationship was not indexed). Keep the
    // summary internally consistent instead of reporting success > total.
    summary.total = summary.total.max(summary.failed);
    summary.success = summary.total.saturating_sub(summary.failed);
    summary.restored = restored;
    summary.skipped = summary.success.saturating_sub(restored);

    if policy == ChildRestorePolicy::Strict && summary.failed > 0 {
        let mut reasons: Vec<String> = summary
            .failures
            .iter()
            .map(|failure| {
                format!(
                    "{} ({}): {}",
                    failure.entity_id, failure.checkpoint_id, failure.error
                )
            })
            .collect();
        reasons.sort();
        return Err(CheckpointError::Coordinator(format!(
            "child restore failed for {} of {} children: {}",
            summary.failed,
            summary.total,
            reasons.join("; ")
        )));
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use checkpoint_state::restore::registry::RestoreFn;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wf_types::checkpoint::{CheckpointStatus, CheckpointType};
    use wf_types::storage::checkpoint::CheckpointStorageMetadata;

    fn child_meta(checkpoint_id: &str, entity_id: &str, parent: &str) -> CheckpointStorageMetadata {
        CheckpointStorageMetadata {
            id: checkpoint_id.to_string(),
            entity_type: "child".to_string(),
            entity_id: entity_id.to_string(),
            parent_entity_id: Some(parent.to_string()),
            checkpoint_type: CheckpointType::Full,
            timestamp: 0,
            status: CheckpointStatus::Completed,
            previous_checkpoint_id: None,
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: None,
            blob_size: None,
            tags: None,
            custom_fields: None,
        }
    }

    fn counting_handler(calls: Arc<AtomicUsize>, fail_first: usize) -> RestoreFn {
        Arc::new(move |_id, _data| {
            let calls = Arc::clone(&calls);
            Box::pin(async move {
                let attempt = calls.fetch_add(1, Ordering::SeqCst);
                if attempt < fail_first {
                    Err(CheckpointError::Internal("transient".to_string()))
                } else {
                    Ok(serde_json::json!({"ok": true}))
                }
            })
        })
    }

    #[tokio::test]
    async fn child_restore_retries_transient_failure() {
        let calls = Arc::new(AtomicUsize::new(0));
        let registry = RestoreStrategyRegistry::new();
        registry.register("child", counting_handler(calls.clone(), 1));

        let summary = collect_child_restore(
            vec![child_meta("cp-child", "child-1", "parent-1")],
            "cp-parent",
            "parent-1",
            Some(registry),
            ChildRestorePolicy::BestEffort,
            None,
            |_id: String| async move { Ok(Some(vec![1u8, 2, 3])) },
        )
        .await
        .unwrap();

        assert_eq!(summary.restored, 1, "retry recovers the transient failure");
        assert_eq!(summary.failed, 0);
        assert!(summary.failures.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn best_effort_reports_persistent_failure() {
        let calls = Arc::new(AtomicUsize::new(0));
        let registry = RestoreStrategyRegistry::new();
        registry.register("child", counting_handler(calls.clone(), usize::MAX));

        let summary = collect_child_restore(
            vec![child_meta("cp-child", "child-1", "parent-1")],
            "cp-parent",
            "parent-1",
            Some(registry),
            ChildRestorePolicy::BestEffort,
            None,
            |_id: String| async move { Ok(Some(vec![1u8, 2, 3])) },
        )
        .await
        .unwrap();

        assert_eq!(summary.failed, 1);
        assert_eq!(summary.failures.len(), 1);
        assert_eq!(summary.failures[0].entity_id, "child-1");
        assert_eq!(summary.failures[0].checkpoint_id, "cp-child");
        assert_eq!(calls.load(Ordering::SeqCst), CHILD_RESTORE_MAX_ATTEMPTS as usize);
    }

    #[tokio::test]
    async fn strict_policy_surfaces_child_failure() {
        let registry = RestoreStrategyRegistry::new();
        registry.register("child", counting_handler(Arc::new(AtomicUsize::new(0)), usize::MAX));

        let err = collect_child_restore(
            vec![child_meta("cp-child", "child-1", "parent-1")],
            "cp-parent",
            "parent-1",
            Some(registry),
            ChildRestorePolicy::Strict,
            None,
            |_id: String| async move { Ok(Some(vec![1u8, 2, 3])) },
        )
        .await
        .unwrap_err();

        assert!(matches!(err, CheckpointError::Coordinator(_)));
    }
}
