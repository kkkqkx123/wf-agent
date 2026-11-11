use std::sync::Arc;

use dashmap::DashMap;

/// Upper bound for the dedup table; overflow evicts the oldest claims by
/// registration time while never evicting the just-claimed key (a late
/// duplicate at worst re-runs one summary whose write-back the version
/// anchor then discards).
pub const COMPRESSION_HANDLED_CAPACITY: usize = 1024;

/// Dedup key for one claimed compression signal.
pub(super) fn claim_key(execution_id: &str, target_context_id: &str, array_version: u64) -> String {
    format!("{execution_id}:{target_context_id}:{array_version}")
}

/// Register the idempotency claim. Returns false when the same version was
/// already claimed (the caller skips without work). The claim releases
/// version-checked when the spawned run settles on any path.
pub(super) fn try_claim(
    handled: &DashMap<String, CompressionAttempt>,
    key: String,
    array_version: u64,
) -> bool {
    if handled.contains_key(&key) {
        return false;
    }
    handled.insert(
        key.clone(),
        CompressionAttempt {
            array_version,
            claimed_at: wf_common::now(),
        },
    );
    evict_overflow(handled, &key);
    true
}

/// Bound the dedup table (never evicts the just-claimed key). Victims are
/// the oldest claims by registration time so an in-flight version keeps
/// its dedup protection while idle leftovers are reclaimed first.
pub(super) fn evict_overflow(handled: &DashMap<String, CompressionAttempt>, keep: &str) {
    if handled.len() <= COMPRESSION_HANDLED_CAPACITY {
        return;
    }
    let mut candidates: Vec<(String, i64)> = handled
        .iter()
        .filter(|entry| entry.key() != keep)
        .map(|entry| (entry.key().clone(), entry.claimed_at))
        .collect();
    candidates.sort_by_key(|(_, claimed_at)| *claimed_at);
    for (victim, _) in candidates.into_iter().take(256) {
        handled.remove(&victim);
    }
}

/// Dedup record for one claimed compression signal.
#[derive(Debug, Clone)]
pub(super) struct CompressionAttempt {
    pub(super) array_version: u64,
    pub(super) claimed_at: i64,
}

/// RAII release of one dedup-table claim when the compression callback
/// leaves scope on any exit path (success, terminal failure, panic or task
/// abort). A leaked entry would permanently swallow re-emissions of the
/// same `(execution, target, version)` until capacity eviction happens to
/// hit it. The key carries the array version, and removal is version-checked
/// so a late claim for a newer version is never deleted by an older task.
pub(super) struct HandledGuard {
    pub(super) handled: Arc<DashMap<String, CompressionAttempt>>,
    pub(super) key: String,
    pub(super) expected_version: u64,
}

impl Drop for HandledGuard {
    fn drop(&mut self) {
        let should_remove = self
            .handled
            .get(&self.key)
            .is_some_and(|entry| entry.array_version == self.expected_version);
        if should_remove {
            self.handled.remove(&self.key);
        }
    }
}
