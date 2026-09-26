//! Process-wide hook handler registry.
//!
//! Handlers register under a stable name, optionally bound to a hook type
//! (dynamic registration) and prioritized. Fire ([`fire`]) resolves
//! static `HookDefinition.handler` names through this registry and
//! notifies type-bound handlers; each notification is panic-guarded so a
//! panicking handler never takes down the engine. The registry imposes no
//! time budget: pacing is the handler's own policy, bounded at least by
//! the execution cancellation carried on every [`HookContext`].

use std::sync::Arc;

use dashmap::DashMap;
use tracing::warn;

use crate::hooks::handler::HookHandler;
use crate::hooks::types::{HookContext, HookOutcome};

/// A handler registered for a hook type, with its registration priority and
/// an optional condition evaluated against the hook context data at fire
/// time (same expression language and skip-on-error policy as static hook
/// definitions: a failing condition skips the handler, never the engine).
#[derive(Clone)]
pub struct RegisteredHandler {
    pub name: String,
    pub priority: i32,
    pub handler: Arc<dyn HookHandler>,
    pub condition: Option<String>,
}

/// Per-hook-type, priority-descending list of registered handlers.
#[derive(Clone)]
pub struct HookHandlerRegistry {
    /// name -> handler (dedup / unregister / `handler` field resolution).
    named: Arc<DashMap<String, Arc<dyn HookHandler>>>,
    /// hook_type -> handlers sorted by priority descending.
    per_type: Arc<DashMap<String, Vec<RegisteredHandler>>>,
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
        }
    }

    /// Shared no-handler fallback: without an injected registry the fire
    /// degrades to audit-only event publication, so tests and minimal
    /// embeddings keep their observable events.
    pub fn fallback() -> &'static Self {
        static DEFAULT: std::sync::OnceLock<HookHandlerRegistry> = std::sync::OnceLock::new();
        DEFAULT.get_or_init(HookHandlerRegistry::new)
    }

    /// Register `handler` for `hook_type` with `priority`. Registration is
    /// deduplicated by the handler's stable name: a second registration with
    /// the same name is ignored (returns `false`). The handler carries no
    /// condition and runs on every fire of the type.
    pub fn register(&self, hook_type: &str, handler: Arc<dyn HookHandler>, priority: i32) -> bool {
        self.register_with_condition(hook_type, handler, priority, None)
    }

    /// Register `handler` for `hook_type` with `priority` and an optional
    /// condition: at fire time the condition is evaluated against the hook
    /// context data with the same evaluator and skip policy as static hook
    /// definitions, so dynamic and static handlers share one evaluation
    /// semantic per hook point.
    pub fn register_with_condition(
        &self,
        hook_type: &str,
        handler: Arc<dyn HookHandler>,
        priority: i32,
        condition: Option<String>,
    ) -> bool {
        let name = handler.name().to_string();
        if self.named.contains_key(&name) {
            return false;
        }
        self.named.insert(name.clone(), handler.clone());
        let mut list = self.per_type.entry(hook_type.to_string()).or_default();
        list.push(RegisteredHandler {
            name,
            priority,
            handler,
            condition,
        });
        list.sort_by_key(|r| std::cmp::Reverse(r.priority));
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

    /// Handlers dynamically registered for `hook_type` (priority descending).
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

    /// Names from `hook_defs` handler fields that are not registered.
    /// Assembly-time precheck: warn on the returned names (a plugin may
    /// register the handler later, so this stays a warning, never a
    /// rejection); the fire path still skips unresolvable handlers loudly
    /// per fire.
    pub fn missing_handlers(
        &self,
        hook_defs: &[crate::hooks::types::HookDefinition],
    ) -> Vec<String> {
        let mut missing = Vec::new();
        for def in hook_defs {
            if let Some(name) = def.handler.as_deref() {
                if !name.trim().is_empty()
                    && !self.contains(name)
                    && !missing.contains(&name.to_string())
                {
                    missing.push(name.to_string());
                }
            }
        }
        missing
    }

    /// Notify one handler. The call is panic-guarded: a panicking handler
    /// is reported as an error result and treated as `Continue`, never
    /// taking down the engine task. The pipeline applies no timeout — a
    /// handler bounds its own work and must honor `ctx.cancellation` so it
    /// can never outlive the owning execution.
    pub async fn notify(
        &self,
        ctx: &HookContext,
        handler: &RegisteredHandler,
    ) -> crate::hooks::fire::HandlerResult {
        use futures::FutureExt;
        let started = wf_common::now();
        let outcome = std::panic::AssertUnwindSafe(handler.handler.on_point(ctx))
            .catch_unwind()
            .await;
        let duration_ms = wf_common::now() - started;
        match outcome {
            Ok(outcome) => crate::hooks::fire::HandlerResult {
                name: handler.name.clone(),
                outcome,
                duration_ms,
                error: None,
            },
            Err(payload) => {
                let detail = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "unknown panic".to_string());
                warn!(
                    handler = %handler.name,
                    panic = %detail,
                    "hook handler panicked, treated as continue"
                );
                crate::hooks::fire::HandlerResult {
                    name: handler.name.clone(),
                    outcome: HookOutcome::Continue,
                    duration_ms,
                    error: Some(format!("handler panicked: {detail}")),
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
            cancellation: tokio_util::sync::CancellationToken::new(),
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
    async fn register_sorts_by_priority_descending() {
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
        let handler = Arc::new(RecordingHandler::new("r1", calls.clone()));
        registry.register("A", handler.clone(), 1);

        let registered = registry.for_type("A").remove(0);
        let result: HandlerResult = registry.notify(&ctx(), &registered).await;
        assert_eq!(result.name, "r1");
        assert_eq!(result.outcome, HookOutcome::Continue);
        assert!(result.error.is_none());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn panicking_handler_is_reported_as_continue_with_error() {
        let registry = HookHandlerRegistry::new();

        struct PanickingHandler;
        #[async_trait::async_trait]
        impl HookHandler for PanickingHandler {
            fn name(&self) -> &str {
                "boom"
            }
            async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
                panic!("handler blew up");
            }
        }

        registry.register("A", Arc::new(PanickingHandler), 1);
        let registered = registry.for_type("A").remove(0);
        let result = registry.notify(&ctx(), &registered).await;
        assert_eq!(result.name, "boom");
        assert_eq!(result.outcome, HookOutcome::Continue);
        assert!(result.error.is_some());
    }

    #[test]
    fn missing_handlers_reports_unregistered_names_once() {
        use crate::hooks::types::HookDefinition;
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        registry.register(
            "A",
            Arc::new(RecordingHandler::new("ready", calls.clone())),
            1,
        );
        let defs = vec![
            HookDefinition {
                id: "1".to_string(),
                hook_type: "A".to_string(),
                priority: 0,
                condition: None,
                enabled: true,
                payload: None,
                handler: Some("ready".to_string()),
                create_checkpoint: None,
                checkpoint_description: None,
            },
            HookDefinition {
                id: "2".to_string(),
                hook_type: "A".to_string(),
                priority: 0,
                condition: None,
                enabled: true,
                payload: None,
                handler: Some("typo-handler".to_string()),
                create_checkpoint: None,
                checkpoint_description: None,
            },
            HookDefinition {
                id: "3".to_string(),
                hook_type: "A".to_string(),
                priority: 0,
                condition: None,
                enabled: true,
                payload: None,
                handler: Some("typo-handler".to_string()),
                create_checkpoint: None,
                checkpoint_description: None,
            },
        ];
        assert_eq!(
            registry.missing_handlers(&defs),
            vec!["typo-handler".to_string()]
        );
    }
}
