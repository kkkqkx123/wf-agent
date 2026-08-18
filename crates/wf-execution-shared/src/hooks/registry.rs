//! Process-wide hook receiver registry.
//!
//! Receivers register under a stable name, optionally bound to a hook type
//! (dynamic registration) and weighted. Dispatch ([`dispatch`]) resolves
//! static `BaseHookDefinition.receiver` names through this registry and
//! notifies type-bound receivers; every notification is guarded by a timeout
//! so a slow receiver never blocks the engine.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tracing::warn;

use crate::hooks::receiver::HookReceiver;
use crate::hooks::types::{HookContext, HookOutcome};

/// A receiver registered for a hook type, with its registration weight.
#[derive(Clone)]
pub struct RegisteredReceiver {
    pub name: String,
    pub weight: i32,
    pub receiver: Arc<dyn HookReceiver>,
}

/// Per-hook-type, weight-descending list of registered receivers.
#[derive(Clone)]
pub struct HookRegistry {
    /// name -> receiver (dedup / unregister / `receiver` field resolution).
    named: Arc<DashMap<String, Arc<dyn HookReceiver>>>,
    /// hook_type -> receivers sorted by weight descending.
    per_type: Arc<DashMap<String, Vec<RegisteredReceiver>>>,
    /// Per-receiver notification timeout; a timeout skips the receiver.
    timeout: Duration,
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            named: Arc::new(DashMap::new()),
            per_type: Arc::new(DashMap::new()),
            timeout: Duration::from_secs(3),
        }
    }

    /// Override the per-receiver notification timeout (default 3s).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Register `receiver` for `hook_type` with `weight`. Registration is
    /// deduplicated by the receiver's stable name: a second registration with
    /// the same name is ignored (returns `false`).
    pub fn register(&self, hook_type: &str, receiver: Arc<dyn HookReceiver>, weight: i32) -> bool {
        let name = receiver.name().to_string();
        if self.named.contains_key(&name) {
            return false;
        }
        self.named.insert(name.clone(), receiver.clone());
        let mut list = self.per_type.entry(hook_type.to_string()).or_default();
        list.push(RegisteredReceiver {
            name,
            weight,
            receiver,
        });
        list.sort_by_key(|r| std::cmp::Reverse(r.weight));
        true
    }

    /// Remove the receiver with `name` from the registry. Returns `false`
    /// when no such receiver is registered.
    pub fn unregister(&self, name: &str) -> bool {
        let Some(receiver) = self.named.remove(name).map(|(_, r)| r) else {
            return false;
        };
        let name = receiver.name().to_string();
        for mut list in self.per_type.iter_mut() {
            list.retain(|r| r.name != name);
        }
        true
    }

    /// Resolve a receiver by its stable name (`BaseHookDefinition.receiver`).
    pub fn get(&self, name: &str) -> Option<Arc<dyn HookReceiver>> {
        self.named.get(name).map(|r| r.clone())
    }

    /// Receivers dynamically registered for `hook_type` (weight descending).
    pub fn for_type(&self, hook_type: &str) -> Vec<RegisteredReceiver> {
        self.per_type
            .get(hook_type)
            .map(|list| list.clone())
            .unwrap_or_default()
    }

    /// Whether a receiver with `name` is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.named.contains_key(name)
    }

    /// Per-receiver notification timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Notify one receiver with the timeout guard. A timeout or missing
    /// cancellation is reported as an error result and never blocks the
    /// engine; the outcome is treated as `Continue`.
    pub async fn notify(
        &self,
        ctx: &HookContext,
        receiver: &RegisteredReceiver,
    ) -> crate::hooks::dispatch::ReceiverResult {
        let started = wf_common::now();
        match tokio::time::timeout(self.timeout, receiver.receiver.on_hook(ctx)).await {
            Ok(outcome) => crate::hooks::dispatch::ReceiverResult {
                name: receiver.name.clone(),
                outcome,
                duration_ms: wf_common::now() - started,
                error: None,
            },
            Err(_) => {
                warn!(
                    receiver = %receiver.name,
                    timeout_ms = self.timeout.as_millis(),
                    "hook receiver timed out, skipping"
                );
                crate::hooks::dispatch::ReceiverResult {
                    name: receiver.name.clone(),
                    outcome: HookOutcome::Continue,
                    duration_ms: wf_common::now() - started,
                    error: Some("receiver timed out".to_string()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::hooks::dispatch::ReceiverResult;
    use crate::hooks::types::HookContext;
    use wf_types::Id;

    struct RecordingReceiver {
        name: &'static str,
        calls: Arc<std::sync::atomic::AtomicU32>,
        outcome: HookOutcome,
    }

    impl RecordingReceiver {
        fn new(name: &'static str, calls: Arc<std::sync::atomic::AtomicU32>) -> Self {
            Self {
                name,
                calls,
                outcome: HookOutcome::Continue,
            }
        }
        fn intercepting(name: &'static str, calls: Arc<std::sync::atomic::AtomicU32>) -> Self {
            Self {
                name,
                calls,
                outcome: HookOutcome::Intercept {
                    reason: "blocked".to_string(),
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl HookReceiver for RecordingReceiver {
        fn name(&self) -> &str {
            self.name
        }
        async fn on_hook(&self, _ctx: &HookContext) -> HookOutcome {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.outcome.clone()
        }
    }

    fn ctx() -> HookContext {
        HookContext {
            execution_id: Id::from("exec-1".to_string()),
            hook_type: "TEST".to_string(),
            data: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn register_deduplicates_by_name() {
        let registry = HookRegistry::new();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        assert!(registry.register(
            "A",
            Arc::new(RecordingReceiver::new("r1", calls.clone())),
            1
        ));
        assert!(!registry.register(
            "A",
            Arc::new(RecordingReceiver::new("r1", calls.clone())),
            1
        ));
        assert_eq!(registry.for_type("A").len(), 1);
    }

    #[tokio::test]
    async fn register_sorts_by_weight_descending() {
        let registry = HookRegistry::new();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        registry.register(
            "A",
            Arc::new(RecordingReceiver::new("low", calls.clone())),
            1,
        );
        registry.register(
            "A",
            Arc::new(RecordingReceiver::new("high", calls.clone())),
            10,
        );
        let list = registry.for_type("A");
        assert_eq!(list[0].name, "high");
        assert_eq!(list[1].name, "low");
    }

    #[tokio::test]
    async fn unregister_removes_from_named_and_type_lists() {
        let registry = HookRegistry::new();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        registry.register(
            "A",
            Arc::new(RecordingReceiver::new("r1", calls.clone())),
            1,
        );
        registry.register(
            "B",
            Arc::new(RecordingReceiver::new("r1", calls.clone())),
            1,
        );
        assert!(registry.unregister("r1"));
        assert!(!registry.unregister("r1"));
        assert!(registry.for_type("A").is_empty());
        assert!(registry.for_type("B").is_empty());
    }

    #[tokio::test]
    async fn notify_calls_receiver_and_reports_outcome() {
        let registry = HookRegistry::new();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let receiver = Arc::new(RecordingReceiver::intercepting("r1", calls.clone()));
        registry.register("A", receiver.clone(), 1);

        let registered = registry.for_type("A").remove(0);
        let result: ReceiverResult = registry.notify(&ctx(), &registered).await;
        assert_eq!(result.name, "r1");
        assert_eq!(
            result.outcome,
            HookOutcome::Intercept {
                reason: "blocked".to_string()
            }
        );
        assert!(result.error.is_none());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn slow_receiver_times_out_and_is_reported() {
        let registry = HookRegistry::new().with_timeout(Duration::from_millis(20));

        struct SlowReceiver;
        #[async_trait::async_trait]
        impl HookReceiver for SlowReceiver {
            fn name(&self) -> &str {
                "slow"
            }
            async fn on_hook(&self, _ctx: &HookContext) -> HookOutcome {
                tokio::time::sleep(Duration::from_millis(200)).await;
                HookOutcome::Continue
            }
        }

        registry.register("A", Arc::new(SlowReceiver), 1);
        let registered = registry.for_type("A").remove(0);
        let result = registry.notify(&ctx(), &registered).await;
        assert_eq!(result.name, "slow");
        assert_eq!(result.outcome, HookOutcome::Continue);
        assert!(result.error.is_some());
    }
}
