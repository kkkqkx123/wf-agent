//! Process-wide hook handler registry.
//!
//! Handlers register under a stable name, optionally bound to a hook type
//! (dynamic registration) and weighted. Fire ([`fire`]) resolves
//! static `HookDefinition.handler` names through this registry and
//! notifies type-bound handlers; every notification is guarded by a timeout
//! so a slow handler never blocks the engine.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tracing::warn;

use crate::hooks::handler::HookHandler;
use crate::hooks::types::{HookContext, HookOutcome};

/// A handler registered for a hook type, with its registration weight.
#[derive(Clone)]
pub struct RegisteredHandler {
    pub name: String,
    pub weight: i32,
    pub handler: Arc<dyn HookHandler>,
}

/// Per-hook-type, weight-descending list of registered handlers.
#[derive(Clone)]
pub struct HookHandlerRegistry {
    /// name -> handler (dedup / unregister / `handler` field resolution).
    named: Arc<DashMap<String, Arc<dyn HookHandler>>>,
    /// hook_type -> handlers sorted by weight descending.
    per_type: Arc<DashMap<String, Vec<RegisteredHandler>>>,
    /// Per-handler notification timeout; a timeout skips the handler.
    timeout: Duration,
}

impl Default for HookHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HookHandlerRegistry {
    pub fn new() -> Self {
        Self {
            named: Arc::new(DashMap::new()),
            per_type: Arc::new(DashMap::new()),
            timeout: Duration::from_secs(3),
        }
    }

    /// Override the per-handler notification timeout (default 3s).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Register `handler` for `hook_type` with `weight`. Registration is
    /// deduplicated by the handler's stable name: a second registration with
    /// the same name is ignored (returns `false`).
    pub fn register(&self, hook_type: &str, handler: Arc<dyn HookHandler>, weight: i32) -> bool {
        let name = handler.name().to_string();
        if self.named.contains_key(&name) {
            return false;
        }
        self.named.insert(name.clone(), handler.clone());
        let mut list = self.per_type.entry(hook_type.to_string()).or_default();
        list.push(RegisteredHandler {
            name,
            weight,
            handler,
        });
        list.sort_by_key(|r| std::cmp::Reverse(r.weight));
        true
    }

    /// Remove the handler with `name` from the registry. Returns `false`
    /// when no such handler is registered.
    pub fn unregister(&self, name: &str) -> bool {
        let Some(handler) = self.named.remove(name).map(|(_, r)| r) else {
            return false;
        };
        let name = handler.name().to_string();
        for mut list in self.per_type.iter_mut() {
            list.retain(|r| r.name != name);
        }
        true
    }

    /// Resolve a handler by its stable name (`HookDefinition.handler`).
    pub fn get(&self, name: &str) -> Option<Arc<dyn HookHandler>> {
        self.named.get(name).map(|r| r.clone())
    }

    /// Handlers dynamically registered for `hook_type` (weight descending).
    pub fn for_type(&self, hook_type: &str) -> Vec<RegisteredHandler> {
        self.per_type
            .get(hook_type)
            .map(|list| list.clone())
            .unwrap_or_default()
    }

    /// Whether a handler with `name` is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.named.contains_key(name)
    }

    /// Per-handler notification timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Notify one handler with the timeout guard. A timeout or missing
    /// cancellation is reported as an error result and never blocks the
    /// engine; the outcome is treated as `Continue`.
    pub async fn notify(
        &self,
        ctx: &HookContext,
        handler: &RegisteredHandler,
    ) -> crate::hooks::fire::HandlerResult {
        let started = wf_common::now();
        match tokio::time::timeout(self.timeout, handler.handler.on_point(ctx)).await {
            Ok(outcome) => crate::hooks::fire::HandlerResult {
                name: handler.name.clone(),
                outcome,
                duration_ms: wf_common::now() - started,
                error: None,
            },
            Err(_) => {
                warn!(
                    handler = %handler.name,
                    timeout_ms = self.timeout.as_millis(),
                    "hook handler timed out, skipping"
                );
                crate::hooks::fire::HandlerResult {
                    name: handler.name.clone(),
                    outcome: HookOutcome::Continue,
                    duration_ms: wf_common::now() - started,
                    error: Some("handler timed out".to_string()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::hooks::fire::HandlerResult;
    use crate::hooks::types::HookContext;
    use wf_types::Id;

    struct RecordingHandler {
        name: &'static str,
        calls: Arc<std::sync::atomic::AtomicU32>,
        outcome: HookOutcome,
    }

    impl RecordingHandler {
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
    impl HookHandler for RecordingHandler {
        fn name(&self) -> &str {
            self.name
        }
        async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
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
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        assert!(registry.register("A", Arc::new(RecordingHandler::new("r1", calls.clone())), 1));
        assert!(!registry.register("A", Arc::new(RecordingHandler::new("r1", calls.clone())), 1));
        assert_eq!(registry.for_type("A").len(), 1);
    }

    #[tokio::test]
    async fn register_sorts_by_weight_descending() {
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        registry.register(
            "A",
            Arc::new(RecordingHandler::new("low", calls.clone())),
            1,
        );
        registry.register(
            "A",
            Arc::new(RecordingHandler::new("high", calls.clone())),
            10,
        );
        let list = registry.for_type("A");
        assert_eq!(list[0].name, "high");
        assert_eq!(list[1].name, "low");
    }

    #[tokio::test]
    async fn unregister_removes_from_named_and_type_lists() {
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        registry.register("A", Arc::new(RecordingHandler::new("r1", calls.clone())), 1);
        registry.register("B", Arc::new(RecordingHandler::new("r1", calls.clone())), 1);
        assert!(registry.unregister("r1"));
        assert!(!registry.unregister("r1"));
        assert!(registry.for_type("A").is_empty());
        assert!(registry.for_type("B").is_empty());
    }

    #[tokio::test]
    async fn notify_calls_handler_and_reports_outcome() {
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let handler = Arc::new(RecordingHandler::intercepting("r1", calls.clone()));
        registry.register("A", handler.clone(), 1);

        let registered = registry.for_type("A").remove(0);
        let result: HandlerResult = registry.notify(&ctx(), &registered).await;
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
    async fn slow_handler_times_out_and_is_reported() {
        let registry = HookHandlerRegistry::new().with_timeout(Duration::from_millis(20));

        struct SlowHandler;
        #[async_trait::async_trait]
        impl HookHandler for SlowHandler {
            fn name(&self) -> &str {
                "slow"
            }
            async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
                tokio::time::sleep(Duration::from_millis(200)).await;
                HookOutcome::Continue
            }
        }

        registry.register("A", Arc::new(SlowHandler), 1);
        let registered = registry.for_type("A").remove(0);
        let result = registry.notify(&ctx(), &registered).await;
        assert_eq!(result.name, "slow");
        assert_eq!(result.outcome, HookOutcome::Continue);
        assert!(result.error.is_some());
    }
}
