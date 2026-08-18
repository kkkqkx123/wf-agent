use std::collections::HashMap;
use std::sync::Arc;

use wf_execution_shared::hooks::{HookReceiver, HookRegistry};
use wf_types::hook;

/// Errors reported by `register_hook_receiver`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookReceiverError {
    /// The hook type is not part of the engine vocabulary (agent /
    /// workflow hook types, or internal signal points).
    UnknownHookType(String),
    /// A receiver with the same stable name is already registered.
    AlreadyRegistered(String),
}

/// Register a business `receiver` for a validated engine hook type.
///
/// The hook type must belong to the engine vocabulary
/// ([`wf_types::hook::is_known_hook_type`]); the receiver is notified
/// synchronously on every dispatch of that point and can be referenced by
/// static hook configs through `BaseHookDefinition.receiver`. A receiver
/// whose stable name is already registered is rejected.
pub fn register_hook_receiver(
    registry: &HookRegistry,
    hook_type: &str,
    receiver: Arc<dyn HookReceiver>,
    weight: i32,
) -> Result<(), HookReceiverError> {
    if !hook::is_known_hook_type(hook_type) {
        return Err(HookReceiverError::UnknownHookType(hook_type.to_string()));
    }
    let name = receiver.name().to_string();
    if !registry.register(hook_type, receiver, weight) {
        return Err(HookReceiverError::AlreadyRegistered(name));
    }
    Ok(())
}

/// Register plugin-declared receivers through the plugin hook-type mapping
/// (`plugin hook name -> engine hook type`, declared in plugin manifests).
///
/// Each entry of `receivers` is looked up in `mapping`; unmapped plugin hook
/// names and duplicates are reported per entry, so a partially valid plugin
/// set never fails as a whole. Registered receivers are then referenceable
/// from static hook configs by their stable name.
pub fn register_plugin_hook_receivers(
    registry: &HookRegistry,
    mapping: &HashMap<String, String>,
    receivers: impl Iterator<Item = (String, Arc<dyn HookReceiver>)>,
    weight: i32,
) -> Vec<(String, Result<(), HookReceiverError>)> {
    let mut results = Vec::new();
    for (plugin_name, receiver) in receivers {
        let result = match mapping.get(&plugin_name) {
            Some(hook_type) => register_hook_receiver(registry, hook_type, receiver, weight),
            None => Err(HookReceiverError::UnknownHookType(plugin_name.clone())),
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

    struct CountingReceiver {
        name: &'static str,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl HookReceiver for CountingReceiver {
        fn name(&self) -> &str {
            self.name
        }

        async fn on_hook(&self, _ctx: &HookContext) -> HookOutcome {
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
        let registry = HookRegistry::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let err = register_hook_receiver(
            &registry,
            "NO_SUCH_POINT",
            Arc::new(CountingReceiver {
                name: "business-a",
                calls,
            }),
            0,
        )
        .unwrap_err();
        assert_eq!(
            err,
            HookReceiverError::UnknownHookType("NO_SUCH_POINT".to_string())
        );
        assert!(registry.for_type("NO_SUCH_POINT").is_empty());
    }

    #[test]
    fn registers_known_hook_type_and_notifies() {
        let registry = HookRegistry::new();
        let calls = Arc::new(AtomicUsize::new(0));
        register_hook_receiver(
            &registry,
            "AFTER_AGENT",
            Arc::new(CountingReceiver {
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
            let receivers = registry.for_type("AFTER_AGENT");
            assert_eq!(receivers.len(), 1);
            registry.notify(&ctx, &receivers[0]).await;
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn rejects_duplicate_receiver_name() {
        let registry = HookRegistry::new();
        register_hook_receiver(
            &registry,
            "BEFORE_AGENT",
            Arc::new(CountingReceiver {
                name: "dup",
                calls: Arc::new(AtomicUsize::new(0)),
            }),
            0,
        )
        .expect("first registration succeeds");
        assert!(registry.contains("dup"));
        let err = register_hook_receiver(
            &registry,
            "BEFORE_AGENT",
            Arc::new(CountingReceiver {
                name: "dup",
                calls: Arc::new(AtomicUsize::new(0)),
            }),
            0,
        )
        .unwrap_err();
        assert_eq!(err, HookReceiverError::AlreadyRegistered("dup".to_string()));
    }

    #[test]
    fn plugin_mapping_resolves_hook_types() {
        let registry = HookRegistry::new();
        let mapping = HashMap::from([
            (
                "plugin.on_agent_done".to_string(),
                "AFTER_AGENT".to_string(),
            ),
            ("plugin.on_tool".to_string(), "AFTER_TOOL_CALL".to_string()),
        ]);
        let receivers = vec![
            (
                "plugin.on_agent_done".to_string(),
                Arc::new(CountingReceiver {
                    name: "plugin-agent-done",
                    calls: Arc::new(AtomicUsize::new(0)),
                }) as Arc<dyn HookReceiver>,
            ),
            (
                "plugin.on_tool".to_string(),
                Arc::new(CountingReceiver {
                    name: "plugin-tool",
                    calls: Arc::new(AtomicUsize::new(0)),
                }) as Arc<dyn HookReceiver>,
            ),
            (
                "plugin.unmapped".to_string(),
                Arc::new(CountingReceiver {
                    name: "plugin-unmapped",
                    calls: Arc::new(AtomicUsize::new(0)),
                }) as Arc<dyn HookReceiver>,
            ),
        ];
        let results = register_plugin_hook_receivers(&registry, &mapping, receivers.into_iter(), 0);
        assert_eq!(results.len(), 3);
        assert!(results[0].1.is_ok());
        assert!(results[1].1.is_ok());
        assert_eq!(
            results[2].1,
            Err(HookReceiverError::UnknownHookType(
                "plugin.unmapped".to_string()
            ))
        );
        assert!(registry.contains("plugin-agent-done"));
        assert!(registry.contains("plugin-tool"));
        assert_eq!(registry.for_type("AFTER_AGENT").len(), 1);
        assert_eq!(registry.for_type("AFTER_TOOL_CALL").len(), 1);
        assert!(!registry.contains("plugin-unmapped"));
    }
}
