use std::collections::HashMap;
use std::sync::Arc;

use wf_execution_shared::hooks::{HookHandler, HookHandlerRegistry};
use wf_types::hook;

/// Errors reported by `register_hook_handler`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookHandlerError {
    /// The hook type is not part of the engine vocabulary (agent /
    /// workflow hook types, or internal signal points).
    UnknownHookType(String),
    /// A handler with the same stable name is already registered.
    AlreadyRegistered(String),
}

/// Register a business `handler` for a validated engine hook type.
///
/// The hook type must belong to the engine vocabulary
/// ([`wf_types::hook::is_known_hook_point`]); the handler is notified
/// synchronously on every fire of that point and can be referenced by
/// static hook configs through `HookDefinition.handler`. A handler
/// whose stable name is already registered is rejected.
pub fn register_hook_handler(
    registry: &HookHandlerRegistry,
    hook_type: &str,
    handler: Arc<dyn HookHandler>,
    weight: i32,
) -> Result<(), HookHandlerError> {
    if !hook::is_known_hook_point(hook_type) {
        return Err(HookHandlerError::UnknownHookType(hook_type.to_string()));
    }
    let name = handler.name().to_string();
    if !registry.register(hook_type, handler, weight) {
        return Err(HookHandlerError::AlreadyRegistered(name));
    }
    Ok(())
}

/// Startup completeness check for request / mutated hook points.
///
/// Observability hooks are usable on demand with zero subscribers and are
/// skipped. Each remaining hook type in `hook_types` must have at least one
/// dynamically registered handler; missing ones are returned so the caller
/// can warn or fail assembly (e.g. the internal compression signal must be
/// wired or compression silently degrades to audit only).
pub fn missing_request_handlers(
    registry: &HookHandlerRegistry,
    hook_types: &[&str],
) -> Vec<String> {
    hook_types
        .iter()
        .filter(|t| hook::hook_requires_handler(t))
        .filter(|t| registry.for_type(t).is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// Register plugin-declared handlers through the plugin hook-type mapping
/// (`plugin hook name -> engine hook type`, declared in plugin manifests).
///
/// Each entry of `handlers` is looked up in `mapping`; unmapped plugin hook
/// names and duplicates are reported per entry, so a partially valid plugin
/// set never fails as a whole. Registered handlers are then referenceable
/// from static hook configs by their stable name.
pub fn register_plugin_hook_handlers(
    registry: &HookHandlerRegistry,
    mapping: &HashMap<String, String>,
    handlers: impl Iterator<Item = (String, Arc<dyn HookHandler>)>,
    weight: i32,
) -> Vec<(String, Result<(), HookHandlerError>)> {
    let mut results = Vec::new();
    for (plugin_name, handler) in handlers {
        let result = match mapping.get(&plugin_name) {
            Some(hook_type) => register_hook_handler(registry, hook_type, handler, weight),
            None => Err(HookHandlerError::UnknownHookType(plugin_name.clone())),
        };
        results.push((plugin_name, result));
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use wf_execution_shared::hooks::{HookContext, HookOutcome};

    struct CountingHandler {
        name: &'static str,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl HookHandler for CountingHandler {
        fn name(&self) -> &str {
            self.name
        }

        async fn on_point(&self, _ctx: &HookContext) -> HookOutcome {
            self.calls.fetch_add(1, Ordering::SeqCst);
            HookOutcome::Continue
        }
    }

    fn ctx() -> HookContext {
        HookContext {
            execution_id: wf_types::Id::from("run-1".to_string()),
            hook_type: "AFTER_AGENT".to_string(),
            data: HashMap::new(),
        }
    }

    #[test]
    fn rejects_unknown_hook_type() {
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let err = register_hook_handler(
            &registry,
            "NO_SUCH_POINT",
            Arc::new(CountingHandler {
                name: "business-a",
                calls,
            }),
            0,
        )
        .unwrap_err();
        assert_eq!(
            err,
            HookHandlerError::UnknownHookType("NO_SUCH_POINT".to_string())
        );
        assert!(registry.for_type("NO_SUCH_POINT").is_empty());
    }

    #[test]
    fn registers_known_hook_type_and_notifies() {
        let registry = HookHandlerRegistry::new();
        let calls = Arc::new(AtomicUsize::new(0));
        register_hook_handler(
            &registry,
            "AFTER_AGENT",
            Arc::new(CountingHandler {
                name: "business-b",
                calls: calls.clone(),
            }),
            10,
        )
        .expect("known hook type must register");
        assert!(registry.contains("business-b"));

        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            let ctx = ctx();
            let handlers = registry.for_type("AFTER_AGENT");
            assert_eq!(handlers.len(), 1);
            registry.notify(&ctx, &handlers[0]).await;
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn rejects_duplicate_handler_name() {
        let registry = HookHandlerRegistry::new();
        register_hook_handler(
            &registry,
            "BEFORE_AGENT",
            Arc::new(CountingHandler {
                name: "dup",
                calls: Arc::new(AtomicUsize::new(0)),
            }),
            0,
        )
        .expect("first registration succeeds");
        assert!(registry.contains("dup"));
        let err = register_hook_handler(
            &registry,
            "BEFORE_AGENT",
            Arc::new(CountingHandler {
                name: "dup",
                calls: Arc::new(AtomicUsize::new(0)),
            }),
            0,
        )
        .unwrap_err();
        assert_eq!(err, HookHandlerError::AlreadyRegistered("dup".to_string()));
    }

    #[test]
    fn plugin_mapping_resolves_hook_types() {
        let registry = HookHandlerRegistry::new();
        let mapping = HashMap::from([
            (
                "plugin.on_agent_done".to_string(),
                "AFTER_AGENT".to_string(),
            ),
            ("plugin.on_tool".to_string(), "AFTER_TOOL_CALL".to_string()),
        ]);
        let handlers = vec![
            (
                "plugin.on_agent_done".to_string(),
                Arc::new(CountingHandler {
                    name: "plugin-agent-done",
                    calls: Arc::new(AtomicUsize::new(0)),
                }) as Arc<dyn HookHandler>,
            ),
            (
                "plugin.on_tool".to_string(),
                Arc::new(CountingHandler {
                    name: "plugin-tool",
                    calls: Arc::new(AtomicUsize::new(0)),
                }) as Arc<dyn HookHandler>,
            ),
            (
                "plugin.unmapped".to_string(),
                Arc::new(CountingHandler {
                    name: "plugin-unmapped",
                    calls: Arc::new(AtomicUsize::new(0)),
                }) as Arc<dyn HookHandler>,
            ),
        ];
        let results = register_plugin_hook_handlers(&registry, &mapping, handlers.into_iter(), 0);
        assert_eq!(results.len(), 3);
        assert!(results[0].1.is_ok());
        assert!(results[1].1.is_ok());
        assert_eq!(
            results[2].1,
            Err(HookHandlerError::UnknownHookType(
                "plugin.unmapped".to_string()
            ))
        );
        assert!(registry.contains("plugin-agent-done"));
        assert!(registry.contains("plugin-tool"));
        assert_eq!(registry.for_type("AFTER_AGENT").len(), 1);
        assert_eq!(registry.for_type("AFTER_TOOL_CALL").len(), 1);
        assert!(!registry.contains("plugin-unmapped"));
    }

    #[test]
    fn observability_hooks_are_skipped_by_startup_check() {
        let registry = HookHandlerRegistry::new();
        let missing = missing_request_handlers(&registry, &["BEFORE_AGENT", "AFTER_TOOL_CALL"]);
        assert!(missing.is_empty());
    }

    #[test]
    fn request_hooks_are_reported_when_unwired() {
        let registry = HookHandlerRegistry::new();
        let missing =
            missing_request_handlers(&registry, &["CONTEXT_COMPRESSION_REQUESTED", "ON_ERROR"]);
        assert_eq!(
            missing,
            vec![
                "CONTEXT_COMPRESSION_REQUESTED".to_string(),
                "ON_ERROR".to_string()
            ]
        );

        register_hook_handler(
            &registry,
            "CONTEXT_COMPRESSION_REQUESTED",
            Arc::new(CountingHandler {
                name: "compression",
                calls: Arc::new(AtomicUsize::new(0)),
            }),
            1000,
        )
        .expect("compression handler registers");
        let missing = missing_request_handlers(&registry, &["CONTEXT_COMPRESSION_REQUESTED"]);
        assert!(missing.is_empty());
    }
}
