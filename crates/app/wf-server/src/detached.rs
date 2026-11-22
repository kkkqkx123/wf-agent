//! Detached-run accounting for断连保活 (keepalive).
//!
//! When a streaming agent run outlives its SSE observer, the execution keeps
//! consuming a capacity permit. Without accounting, a client that repeatedly
//! disconnects and resubmits could pin every concurrent slot — turning
//! "keep the run alive" into "leak the run forever". This module is that
//! accounting layer. It stores no durable state: everything is in memory,
//! keyed by execution id and by caller-derived principal.
//!
//! Three invariants shape the design (see
//! `docs/plan/remote-keepalive-and-shutdown-semantics.md`):
//!
//! * **Quota is per principal.** A principal is derived from the presenting
//!   API key (or an anonymous bucket when auth is off). Admission refuses a
//!   new detach once the principal reaches its cap; existing runs are never
//!   touched by a refusal.
//! * **Idle means "old last event AND no subscriber".** A long tool call with
//!   no events is still progressing, so event silence alone is not idleness.
//!   A live subscriber is a positive signal the run is wanted. Explicitly
//!   persistent runs are exempt from reaping.
//! * **Reaping is advisory.** A sweep returns the ids to cancel; the caller
//!   owns the ordering "settle terminal first, then remove entity" so the
//!   result slot can never outlive or trail the entity.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use sha2::Digest;

/// Default bucket used when the request carries no identifiable API key.
pub const ANONYMOUS_PRINCIPAL: &str = "anonymous";

/// One tracked detached run.
#[derive(Debug, Clone)]
pub struct DetachedRun {
    pub execution_id: String,
    pub principal: String,
    /// Wall-clock ms when the run was detached.
    pub detached_ms: i64,
    /// Wall-clock ms of the last observed event (touch on attach/heartbeat).
    pub last_event_ms: i64,
    /// Number of live SSE subscribers currently attached.
    pub subscribers: usize,
    /// Explicitly marked long-running; exempt from idle reaping.
    pub persistent: bool,
}

/// Bundle of the detached-run registry and its resolved policy, shared by all
/// handlers through `ApiState`. Keeping them together guarantees the quota
/// caps and idle threshold used by admission/reaping match the configured
/// policy that choose `keep` in the first place.
#[derive(Debug, Clone)]
pub struct StreamRuntime {
    pub registry: DetachedRunRegistry,
    pub detach_policy: wf_api::agent::agent_execution::StreamDetachPolicy,
    pub idle_secs: u64,
}

impl StreamRuntime {
    pub fn new(
        policy: wf_api::agent::agent_execution::StreamDetachPolicy,
        max_per_principal: usize,
        idle_secs: u64,
    ) -> Self {
        Self {
            registry: DetachedRunRegistry::new(max_per_principal, idle_secs),
            detach_policy: policy,
            idle_secs,
        }
    }

    /// Whether this runtime detaches disconnects at all.
    pub fn keeps_runs(&self) -> bool {
        self.detach_policy == wf_api::agent::agent_execution::StreamDetachPolicy::Keep
    }
}

impl Default for StreamRuntime {
    fn default() -> Self {
        Self::new(
            wf_api::agent::agent_execution::StreamDetachPolicy::Cancel,
            DEFAULT_MAX_PER_PRINCIPAL,
            DEFAULT_IDLE_SECS,
        )
    }
}

pub const DEFAULT_MAX_PER_PRINCIPAL: usize = 4;
pub const DEFAULT_IDLE_SECS: u64 = 1800;

/// Scan interval for the background reaper. It must be comfortably below the
/// minimum idle threshold (300s) so an orphan is reclaimed promptly without
/// hammering the registry.
pub const REAPER_SCAN_SECS: u64 = 60;

/// Spawn the periodic detached-run reaper. It cancels idle runs on a fixed
/// cadence and stops when `cancel` is triggered (or when host shutdown begins,
/// as a second line of defence for embedders that do not drive the token).
///
/// Returns the task handle so the host can await it during shutdown. Reaping
/// is idempotent and shares the same "settle terminal, then remove" routine
/// used by the opportunistic sweep in the attach handler.
pub fn spawn_reaper(
    registry: DetachedRunRegistry,
    ctx: std::sync::Arc<wf_api::ApiContext>,
    cancel: tokio_util::sync::CancellationToken,
    interval: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // The first tick fires immediately; skip it so a freshly dispatched
        // run is not considered on the very first poll.
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {
                    if wf_common::shutdown::is_active_shutdown() {
                        break;
                    }
                    let reaped = reap(&registry, wf_common::epoch_ms(), |id| {
                        let ctx = std::sync::Arc::clone(&ctx);
                        async move {
                            let _ = ctx.cancel_execution(&id).await;
                        }
                    })
                    .await;
                    if reaped > 0 {
                        emit_reaped(&ctx, reaped);
                        emit_active_gauge(&registry, &ctx);
                        tracing::info!(target: "wf_server", reaped, "detached reaper reclaimed idle runs");
                    }
                }
            }
        }
    })
}

/// In-memory detached-run registry with per-principal quota and idle sweep.
#[derive(Debug, Clone)]
pub struct DetachedRunRegistry {
    runs: Arc<DashMap<String, DetachedRun>>,
    /// principal -> number of live detached runs (derived, kept for O(1) quota).
    counts: Arc<DashMap<String, usize>>,
    max_per_principal: usize,
    idle_secs: u64,
}

impl DetachedRunRegistry {
    pub fn new(max_per_principal: usize, idle_secs: u64) -> Self {
        Self {
            runs: Arc::new(DashMap::new()),
            counts: Arc::new(DashMap::new()),
            max_per_principal: max_per_principal.max(1),
            idle_secs,
        }
    }

    /// Admit a detached run. Returns `Err` with a readable message when the
    /// principal is at quota; an existing id is refreshed, not double-counted.
    pub fn admit(
        &self,
        execution_id: impl Into<String>,
        principal: impl Into<String>,
        persistent: bool,
    ) -> Result<(), String> {
        let execution_id = execution_id.into();
        let principal = principal.into();
        let now = wf_common::epoch_ms();

        if let Some(existing) = self.runs.get(&execution_id) {
            // Re-admitting the same id moves it to the new principal without
            // consuming another slot.
            if existing.principal != principal {
                self.decrement(&existing.principal);
                self.increment(&principal);
            }
            drop(existing);
            self.runs.insert(
                execution_id.clone(),
                DetachedRun {
                    execution_id,
                    principal,
                    detached_ms: now,
                    last_event_ms: now,
                    subscribers: 0,
                    persistent,
                },
            );
            return Ok(());
        }

        let current = self
            .counts
            .get(&principal)
            .map(|count| *count)
            .unwrap_or(0);
        if current >= self.max_per_principal {
            return Err(format!(
                "detached run quota reached for principal '{principal}': {current}/{}",
                self.max_per_principal
            ));
        }

        self.increment(&principal);
        self.runs.insert(
            execution_id.clone(),
            DetachedRun {
                execution_id,
                principal,
                detached_ms: now,
                last_event_ms: now,
                subscribers: 0,
                persistent,
            },
        );
        Ok(())
    }

    /// Refresh the last-event clock for a tracked run.
    pub fn touch(&self, execution_id: &str) {
        if let Some(mut run) = self.runs.get_mut(execution_id) {
            run.last_event_ms = wf_common::epoch_ms();
        }
    }

    /// Add a live subscriber to a run (attach connection opened).
    pub fn add_subscriber(&self, execution_id: &str) {
        if let Some(mut run) = self.runs.get_mut(execution_id) {
            run.subscribers += 1;
            run.last_event_ms = wf_common::epoch_ms();
        }
    }

    /// Remove a live subscriber (attach connection closed).
    pub fn remove_subscriber(&self, execution_id: &str) {
        if let Some(mut run) = self.runs.get_mut(execution_id) {
            run.subscribers = run.subscribers.saturating_sub(1);
        }
    }

    /// Forget a run and release its principal's quota slot. Returns true when
    /// a tracked run was actually removed.
    pub fn remove(&self, execution_id: &str) -> bool {
        if let Some((_, run)) = self.runs.remove(execution_id) {
            self.decrement(&run.principal);
            true
        } else {
            false
        }
    }

    /// Whether a run is currently tracked.
    pub fn contains(&self, execution_id: &str) -> bool {
        self.runs.contains_key(execution_id)
    }

    /// Snapshot of one run for status views.
    pub fn get(&self, execution_id: &str) -> Option<DetachedRun> {
        self.runs.get(execution_id).map(|run| run.clone())
    }

    /// Per-principal live detached count (for metrics/status).
    pub fn principal_count(&self, principal: &str) -> usize {
        self.counts.get(principal).map(|count| *count).unwrap_or(0)
    }

    /// Total number of tracked runs.
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Collect ids eligible for reaping: idle beyond the threshold, with no
    /// live subscriber, and not persistent. This only *selects*; the caller
    /// settles each run then calls [`Self::remove`].
    pub fn reap_candidates(&self, now_ms: i64) -> Vec<String> {
        let idle_ms = (self.idle_secs as i128) * 1000;
        self.runs
            .iter()
            .filter(|entry| {
                let run = entry.value();
                let age = (now_ms as i128) - (run.last_event_ms as i128);
                !run.persistent && run.subscribers == 0 && age >= idle_ms
            })
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Return a deduplicated list of all tracked principals (for reaper stats).
    pub fn principals(&self) -> Vec<String> {
        self.counts.iter().map(|entry| entry.key().clone()).collect()
    }

    fn increment(&self, principal: &str) {
        *self.counts.entry(principal.to_string()).or_insert(0) += 1;
    }

    fn decrement(&self, principal: &str) {
        let mut remove = false;
        if let Some(mut count) = self.counts.get_mut(principal) {
            *count = count.saturating_sub(1);
            remove = *count == 0;
        }
        if remove {
            self.counts.remove(principal);
        }
    }
}

/// Derive a stable principal bucket from a presented API key. Hashing avoids
/// storing or logging the secret while giving each key its own quota. Without
/// a key the anonymous bucket is used (single-user / auth-off deployments).
pub fn principal_from_api_key(key: Option<&str>) -> String {
    match key {
        Some(key) if !key.trim().is_empty() => {
            let digest = sha2::Sha256::digest(key.as_bytes());
            format!("key_{}", hex::encode(&digest[..8]))
        }
        _ => ANONYMOUS_PRINCIPAL.to_string(),
    }
}

/// Publish the current detached-run count as a gauge (no-op when metrics are
/// disabled). Call after any admission/removal that changes the population.
pub fn emit_active_gauge(registry: &DetachedRunRegistry, ctx: &wf_api::ApiContext) {
    if let Some(metrics) = ctx.metrics.as_ref() {
        metrics.resource().record_detached_active(registry.len() as u64);
    }
}

/// Publish rejected-detach and reaped-run counters (no-op without metrics).
pub fn emit_quota_rejection(ctx: &wf_api::ApiContext) {
    if let Some(metrics) = ctx.metrics.as_ref() {
        metrics.resource().record_detached_quota_rejection();
    }
}

pub fn emit_reaped(ctx: &wf_api::ApiContext, count: usize) {
    if count == 0 {
        return;
    }
    if let Some(metrics) = ctx.metrics.as_ref() {
        metrics.resource().record_detached_reaped(count as u64);
    }
}

/// Reap helper that settles candidates then removes them atomically from the
/// registry's point of view. `settle` is called before `remove`, preserving
/// "terminal first, entity removal second". Returns the number reaped.
pub async fn reap<F, Fut>(registry: &DetachedRunRegistry, now_ms: i64, settle: F) -> usize
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let candidates: Vec<String> = registry.reap_candidates(now_ms);
    let mut reaped = 0;
    let mut seen: HashSet<String> = HashSet::new();
    for id in candidates {
        if !seen.insert(id.clone()) {
            continue;
        }
        // Only reap runs still eligible after any concurrent touch/attach.
        let still_idle = registry.get(&id).is_some_and(|run| {
            !run.persistent
                && run.subscribers == 0
                && (now_ms as i128) - (run.last_event_ms as i128)
                    >= (registry.idle_secs as i128) * 1000
        });
        if !still_idle {
            continue;
        }
        settle(id.clone()).await;
        registry.remove(&id);
        reaped += 1;
    }
    reaped
}

/// Convenience for tests and status views: a plain snapshot of runs keyed by id.
pub fn snapshot(registry: &DetachedRunRegistry) -> HashMap<String, DetachedRun> {
    registry
        .runs
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(key: &str) -> String {
        principal_from_api_key(Some(key))
    }

    #[test]
    fn quota_is_per_principal_and_refusals_do_not_disturb_existing() {
        let registry = DetachedRunRegistry::new(2, 300);
        let alice = principal("alice-key");
        let bob = principal("bob-key");

        assert!(registry.admit("a1", &alice, false).is_ok());
        assert!(registry.admit("a2", &alice, false).is_ok());
        assert_eq!(registry.principal_count(&alice), 2);
        // Third run for alice is refused; bob still has room.
        assert!(registry.admit("a3", &alice, false).is_err());
        assert!(registry.admit("b1", &bob, false).is_ok());
        assert_eq!(registry.principal_count(&alice), 2);
        assert_eq!(registry.principal_count(&bob), 1);
        // Refusal never inserted a tracked run.
        assert!(!registry.contains("a3"));
    }

    #[test]
    fn re_admitting_same_id_moves_principal_without_double_count() {
        let registry = DetachedRunRegistry::new(2, 300);
        let alice = principal("alice-key");
        let bob = principal("bob-key");
        registry.admit("run-1", &alice, false).unwrap();
        registry.admit("run-1", &bob, false).unwrap();
        assert_eq!(registry.principal_count(&alice), 0);
        assert_eq!(registry.principal_count(&bob), 1);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn removal_releases_the_quota_slot() {
        let registry = DetachedRunRegistry::new(1, 300);
        let alice = principal("alice-key");
        registry.admit("run-1", &alice, false).unwrap();
        assert!(registry.admit("run-2", &alice, false).is_err());
        assert!(registry.remove("run-1"));
        assert!(registry.admit("run-2", &alice, false).is_ok());
        assert!(!registry.remove("missing"));
    }

    #[test]
    fn idle_requires_no_subscriber() {
        let registry = DetachedRunRegistry::new(4, 300);
        let alice = principal("alice-key");
        registry.admit("watched", &alice, false).unwrap();
        registry.admit("lonely", &alice, false).unwrap();
        registry.add_subscriber("watched");

        let now = wf_common::epoch_ms() + 400_000; // beyond 300s idle
        let candidates = registry.reap_candidates(now);
        assert!(!candidates.iter().any(|id| id == "watched"));
        assert!(candidates.iter().any(|id| id == "lonely"));
    }

    #[test]
    fn idle_uses_last_event_not_detach_time() {
        let registry = DetachedRunRegistry::new(4, 300);
        let alice = principal("alice-key");
        registry.admit("active", &alice, false).unwrap();
        // A long call produced no events at detach time, then a heartbeat
        // arrived recently: it must not be reaped even though detach was old.
        registry.touch("active");
        let now = wf_common::epoch_ms();
        assert!(registry.reap_candidates(now).is_empty());
    }

    #[test]
    fn persistent_runs_are_exempt_from_reaping() {
        let registry = DetachedRunRegistry::new(4, 300);
        let alice = principal("alice-key");
        registry.admit("long", &alice, true).unwrap();
        let now = wf_common::epoch_ms() + 10_000_000;
        assert!(registry.reap_candidates(now).is_empty());
    }

    #[tokio::test]
    async fn reap_settles_before_removing() {
        let registry = DetachedRunRegistry::new(4, 300);
        let alice = principal("alice-key");
        registry.admit("stale", &alice, false).unwrap();
        let now = wf_common::epoch_ms() + 400_000;

        let settled = Arc::new(dashmap::DashMap::<String, bool>::new());
        let settled_ref = settled.clone();
        let count = reap(&registry, now, |id| {
            let settled = settled_ref.clone();
            async move {
                // While settling, the run must still be tracked (terminal
                // first), then removal happens in `reap`.
                settled.insert(id, true);
            }
        })
        .await;
        assert_eq!(count, 1);
        assert!(settled.contains_key("stale"));
        assert!(!registry.contains("stale"));
        assert_eq!(registry.principal_count(&alice), 0);
    }

    #[test]
    fn principal_hash_is_stable_and_secret_free() {
        let a = principal("same-key");
        let b = principal("same-key");
        let c = principal("other-key");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("key_"));
        assert!(!a.contains("same-key"));
        assert_eq!(principal_from_api_key(None), ANONYMOUS_PRINCIPAL);
        assert_eq!(principal_from_api_key(Some("  ")), ANONYMOUS_PRINCIPAL);
    }
}
