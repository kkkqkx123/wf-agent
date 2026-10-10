use crate::coordinator::base::ChildDiscoveryIndex;
use checkpoint_base::error::CheckpointError;
use checkpoint_state::restore::hierarchy::{
    ChildDiscovery, ChildDiscoverySummary, InMemoryChildResolver,
};
use checkpoint_state::restore::registry::RestoreStrategyRegistry;
use std::collections::HashMap;
use std::sync::Arc;
use wf_common::gate::ConcurrencyGate;
use wf_types::storage::CheckpointStorageMetadata;

/// Bounded concurrency for the per-child restore phase.
const CHILD_RESTORE_CONCURRENCY: usize = 5;

/// Depth limit of the post-restore child discovery traversal.
const CHILD_DISCOVERY_DEPTH: usize = 8;

/// Outcome of resolving one child during a post-restore discovery pass.
struct ChildRestoreOutcome {
    metadata: CheckpointStorageMetadata,
    restored: bool,
}

/// Restore one child through the restore strategy registry when one is
/// registered for the child's entity type. Raw bytes are loaded through
/// `load_bytes`, so each coordinator supplies the loader matching its own
/// checkpoint type.
async fn restore_child<F, Fut>(
    meta: CheckpointStorageMetadata,
    restore_registry: Option<&RestoreStrategyRegistry>,
    load_bytes: F,
) -> Result<ChildRestoreOutcome, CheckpointError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<Option<Vec<u8>>, CheckpointError>>,
{
    let mut outcome = ChildRestoreOutcome {
        metadata: meta.clone(),
        restored: false,
    };

    if let Some(reg) = restore_registry {
        if let Some(data) = load_bytes(meta.id.clone()).await? {
            match reg.restore(&meta.entity_type, &meta.id, &data).await {
                Ok(_) => outcome.restored = true,
                Err(err) => {
                    tracing::warn!(
                        child_id = %meta.entity_id,
                        checkpoint_id = %meta.id,
                        error = %err,
                        "child restore strategy failed"
                    );
                }
            }
        }
    }
    Ok(outcome)
}

/// Post-restore child pass shared by both coordinators: restore every child
/// with bounded concurrency, index the resolved metadata, then walk the
/// parent-child relationships breadth-first.
///
/// Children are passed in pre-resolved as the caller's latest-per-child
/// query result, so the discovery covers every child that ever
/// checkpointed, including ones spawned after the parent's own last
/// persist. `load_bytes` resolves a checkpoint id to its raw bytes.
pub async fn collect_child_restore<F, Fut>(
    latest_by_child: Vec<CheckpointStorageMetadata>,
    checkpoint_id: &str,
    parent_entity_id: &str,
    restore_registry: Option<RestoreStrategyRegistry>,
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
            let _permit = match gate.acquire_wait().await {
                Ok(permit) => permit,
                Err(e) => {
                    return Err(CheckpointError::Internal(format!(
                        "child restore gate acquire failed: {e}"
                    )))
                }
            };
            restore_child(meta, restore_registry.as_ref(), |id| load_bytes(id)).await
        }));
    }

    let resolver = InMemoryChildResolver::new();
    let mut index: HashMap<String, CheckpointStorageMetadata> = HashMap::new();
    let mut restored = 0u32;

    for handle in handles {
        match handle.await {
            Ok(Ok(outcome)) => {
                index.insert(outcome.metadata.id.clone(), outcome.metadata.clone());
                resolver.register_relationship(checkpoint_id, &outcome.metadata.id);
                if outcome.restored {
                    restored += 1;
                }
            }
            Ok(Err(err)) => {
                tracing::warn!(
                    parent = %parent_entity_id,
                    error = %err,
                    "child restore failed"
                );
            }
            Err(join_err) => {
                tracing::warn!(
                    parent = %parent_entity_id,
                    error = %join_err,
                    "child restore task panicked"
                );
            }
        }
    }

    let loader = ChildDiscoveryIndex::new(index);
    let discovery = ChildDiscovery::new(Arc::new(resolver));
    let results =
        discovery.discover_children_bfs(checkpoint_id, &loader, CHILD_DISCOVERY_DEPTH, None)?;
    let mut summary = ChildDiscovery::summarize_results(&results);
    summary.success += restored as usize;
    Ok(summary)
}
