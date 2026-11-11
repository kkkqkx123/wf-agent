use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::hooks::{HookContext, HookHandler, HookHandlerRegistry, HookOutcome};
use wf_plugin::{
    ContributionManager, PluginError, PluginHookHandler, PluginHookOutcome, PluginManifest,
    PluginPermission, PluginResult, PluginType,
};
use wf_types::message::Message;

use crate::hook_handler::{HookHandlerError, register_hook_handler};

/// Stable engine-side name for one bridged plugin hook. Names live in the
/// registry's single global namespace, so the plugin id prefixes the plugin
/// hook name: a second plugin claiming the same engine point fails loudly
/// under the duplicate rejection, and teardown removes exactly this name.
fn stable_name(plugin_id: &str, plugin_hook: &str) -> String {
    format!("plugin-hook:{plugin_id}:{plugin_hook}")
}

/// Capability table for hook points: gate points require a permission,
/// pure observation points need only the manifest mapping. New rows extend
/// the table; the mechanism never changes.
pub(crate) fn required_permission_for_hook(hook_type: &str) -> Option<PluginPermission> {
    if hook_type == wf_types::hook::CONTEXT_COMPRESSION_SIGNAL {
        Some(PluginPermission::Compaction)
    } else if hook_type == "BEFORE_TOOL_CALL" || hook_type == "BEFORE_EXECUTE" {
        Some(PluginPermission::Tool)
    } else {
        None
    }
}

/// Parse one plugin hook response into an engine outcome: veto and valid
/// replacements pass through, everything else (backend failures, unknown
/// shapes, empty or malformed summaries) resolves to continue with a
/// warning, so a misbehaving plugin never blocks the engine. A veto from
/// any handler wins upstream; denial stays the safer direction.
fn parse_bridge_outcome(
    name: &str,
    response: PluginResult<serde_json::Value>,
) -> HookOutcome {
    let value = match response {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(handler = %name, error = %error, "plugin hook call failed; treated as continue");
            return HookOutcome::Continue;
        }
    };
    match wf_plugin::parse_hook_outcome(&value) {
        PluginHookOutcome::Continue => HookOutcome::Continue,
        PluginHookOutcome::Veto { reason } => HookOutcome::Veto { reason },
        PluginHookOutcome::Replace { summaries } => {
            match serde_json::from_value::<Vec<Message>>(summaries) {
                Ok(summaries) if !summaries.is_empty() => HookOutcome::Replace { summaries },
                _ => {
                    tracing::warn!(handler = %name, "plugin hook replacement is empty or malformed; treated as continue");
                    HookOutcome::Continue
                }
            }
        }
    }
}

/// Snapshot for one hook call as JSON: the compression attachment carries
/// the live messages, every other point carries none. The JSON is built
/// per call and never enters the audit data map.
fn snapshot_json(ctx: &HookContext) -> Option<serde_json::Value> {
    ctx.attachment.as_ref().and_then(|attachment| {
        serde_json::to_value(attachment.messages.as_ref()).ok()
    })
}

/// Hook data map as JSON for one hook call.
fn data_json(ctx: &HookContext) -> serde_json::Value {
    serde_json::Value::Object(ctx.data.clone().into_iter().collect())
}

/// Native hook bridge: the handler runs the synchronous FFI dispatch, so
/// the call rides the blocking pool and never pins an async worker.
/// Timeout and cancellation stay on the registry notify path.
struct NativeHookBridge {
    name: String,
    inner: Arc<dyn PluginHookHandler>,
}

#[async_trait]
impl HookHandler for NativeHookBridge {
    fn name(&self) -> &str {
        &self.name
    }

    async fn on_point(&self, ctx: &HookContext) -> HookOutcome {
        let inner = self.inner.clone();
        let data = data_json(ctx);
        let snapshot = snapshot_json(ctx);
        let response =
            tokio::task::spawn_blocking(move || inner.handle(data, snapshot)).await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(handler = %self.name, error = %error, "plugin hook task panicked; treated as continue");
                return HookOutcome::Continue;
            }
        };
        parse_bridge_outcome(&self.name, response)
    }
}

/// Lua hook bridge: the handler self-offloads through the blocking pool
/// under its execution limits (registry key plus guarded call precedent),
/// so the bridge calls it directly.
struct LuaHookBridge {
    name: String,
    inner: Arc<dyn PluginHookHandler>,
}

#[async_trait]
impl HookHandler for LuaHookBridge {
    fn name(&self) -> &str {
        &self.name
    }

    async fn on_point(&self, ctx: &HookContext) -> HookOutcome {
        let response = self.inner.handle(data_json(ctx), snapshot_json(ctx));
        parse_bridge_outcome(&self.name, response)
    }
}

/// Register one plugin's hook handlers: resolve each contributed plugin
/// hook name through the manifest mapping, enforce the gate permission,
/// and register through the shared entry (unknown engine types and
/// duplicate names fail loudly instead of overriding).
fn register_one(
    registry: &HookHandlerRegistry,
    manifest: &PluginManifest,
    plugin_id: &str,
    plugin_hook: &str,
    engine_type: &str,
    inner: Arc<dyn PluginHookHandler>,
) -> PluginResult<()> {
    if let Some(permission) = required_permission_for_hook(engine_type) {
        if !manifest.permissions.contains(&permission) {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' hook '{plugin_hook}' targets gate '{engine_type}' without {permission:?} permission"
            )));
        }
    }
    let name = stable_name(plugin_id, plugin_hook);
    let handler: Arc<dyn HookHandler> = match manifest.plugin_type {
        Some(PluginType::Wasm) => {
            tracing::debug!("  hook (wasm skipped, no channel): {}", plugin_hook);
            return Ok(());
        }
        Some(PluginType::Lua) => Arc::new(LuaHookBridge {
            name: name.clone(),
            inner,
        }),
        Some(PluginType::Native) | None => Arc::new(NativeHookBridge {
            name: name.clone(),
            inner,
        }),
    };
    if let Err(error) = register_hook_handler(registry, engine_type, handler, 0) {
        let detail = match &error {
            HookHandlerError::UnknownHookType(t) => format!("unknown engine hook type '{t}'"),
            HookHandlerError::AlreadyRegistered(n) => {
                format!("handler name '{n}' is already registered")
            }
        };
        return Err(PluginError::LoadFailed(format!(
            "plugin '{plugin_id}' hook '{plugin_hook}': {detail}"
        )));
    }
    tracing::debug!("  hook (engine-registered): {} -> {}", plugin_hook, engine_type);
    Ok(())
}

pub(crate) fn sync_hooks(
    plugin_id: &str,
    manager: &ContributionManager,
    manifest: &PluginManifest,
    registry: &HookHandlerRegistry,
) -> PluginResult<()> {
    for (plugin_hook, owner) in manager.all_hook_handlers() {
        if owner != plugin_id {
            continue;
        }
        let Some(engine_type) = manifest
            .hooks
            .as_ref()
            .and_then(|mapping| mapping.get(&plugin_hook))
        else {
            tracing::warn!(
                "plugin '{plugin_id}' hook '{plugin_hook}' has no manifest mapping; skipping"
            );
            continue;
        };
        let Some(inner) = manager.get_hook_handler(&plugin_hook) else {
            continue;
        };
        register_one(
            registry,
            manifest,
            plugin_id,
            &plugin_hook,
            engine_type,
            inner,
        )?;
    }
    Ok(())
}

pub(crate) fn unsync_hooks(
    plugin_id: &str,
    manager: &ContributionManager,
    registry: &HookHandlerRegistry,
) {
    for (plugin_hook, owner) in manager.all_hook_handlers() {
        if owner == plugin_id {
            registry.unregister(&stable_name(plugin_id, &plugin_hook));
            tracing::debug!("  hook (engine-removed): {}", plugin_hook);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_execution_shared::hooks::{HookContext, HookOutcome};
    use wf_plugin::PluginResult;

    struct ScriptedHook {
        value: serde_json::Value,
        error: Option<String>,
    }

    impl PluginHookHandler for ScriptedHook {
        fn handle(
            &self,
            _data: serde_json::Value,
            _snapshot: Option<serde_json::Value>,
        ) -> PluginResult<serde_json::Value> {
            match &self.error {
                Some(error) => Err(PluginError::Internal(error.clone())),
                None => Ok(self.value.clone()),
            }
        }
    }

    fn scripted(response: serde_json::Value) -> Arc<dyn PluginHookHandler> {
        Arc::new(ScriptedHook {
            value: response,
            error: None,
        })
    }

    fn failing(error: &str) -> Arc<dyn PluginHookHandler> {
        Arc::new(ScriptedHook {
            value: serde_json::Value::Null,
            error: Some(error.to_string()),
        })
    }

    fn hook_ctx() -> HookContext {
        HookContext::new(
            wf_types::Id::from("exec-1".to_string()),
            "AFTER_TOOL_CALL".to_string(),
            HashMap::new(),
            tokio_util::sync::CancellationToken::new(),
        )
    }

    fn test_manifest(
        hooks: HashMap<String, String>,
        permissions: Vec<PluginPermission>,
        plugin_type: Option<PluginType>,
    ) -> PluginManifest {
        PluginManifest {
            id: "p1".to_string(),
            version: "1.0.0".to_string(),
            name: None,
            description: None,
            plugin_type,
            entry_point: String::new(),
            versioned_entry_points: Default::default(),
            dependencies: Default::default(),
            optional_dependencies: Default::default(),
            contributions: Vec::new(),
            permissions,
            config_schema: None,
            config: None,
            hooks: Some(hooks),
            llm_providers: Vec::new(),
            tools: Vec::new(),
            tool_descriptions: Vec::new(),
            cli_commands: Vec::new(),
            completion_sources: Vec::new(),
            setting_fields: Vec::new(),
            tool_summaries: Vec::new(),
            markdown_rules: Vec::new(),
            wasm: None,
            lua: None,
        }
    }

    #[tokio::test]
    async fn native_observe_veto_replace_roundtrip() {
        let ctx = hook_ctx();
        let observe = NativeHookBridge {
            name: "observe".to_string(),
            inner: scripted(serde_json::json!({"action": "continue"})),
        };
        assert_eq!(observe.on_point(&ctx).await, HookOutcome::Continue);
        let veto = NativeHookBridge {
            name: "veto".to_string(),
            inner: scripted(serde_json::json!({"action": "veto", "reason": "no"})),
        };
        assert_eq!(
            veto.on_point(&ctx).await,
            HookOutcome::Veto {
                reason: "no".to_string()
            }
        );
        let summary = serde_json::to_value(Message::user_text("done".to_string())).unwrap();
        let replace = NativeHookBridge {
            name: "replace".to_string(),
            inner: scripted(serde_json::json!({
                "action": "replace",
                "summaries": [summary],
            })),
        };
        let outcome = replace.on_point(&ctx).await;
        assert!(matches!(outcome, HookOutcome::Replace { .. }));
        let broken = NativeHookBridge {
            name: "broken".to_string(),
            inner: scripted(serde_json::json!({"action": "replace", "summaries": []})),
        };
        assert_eq!(broken.on_point(&ctx).await, HookOutcome::Continue);
        let failed = NativeHookBridge {
            name: "failed".to_string(),
            inner: failing("boom"),
        };
        assert_eq!(failed.on_point(&ctx).await, HookOutcome::Continue);
    }

    #[tokio::test]
    async fn lua_observe_veto_replace_roundtrip() {
        let ctx = hook_ctx();
        let veto = LuaHookBridge {
            name: "veto".to_string(),
            inner: scripted(serde_json::json!({"action": "veto", "reason": "no"})),
        };
        assert_eq!(
            veto.on_point(&ctx).await,
            HookOutcome::Veto {
                reason: "no".to_string()
            }
        );
        let summary = serde_json::to_value(Message::user_text("done".to_string())).unwrap();
        let replace = LuaHookBridge {
            name: "replace".to_string(),
            inner: scripted(serde_json::json!({
                "action": "replace",
                "summaries": [summary],
            })),
        };
        assert!(matches!(
            replace.on_point(&ctx).await,
            HookOutcome::Replace { .. }
        ));
        let observe = LuaHookBridge {
            name: "observe".to_string(),
            inner: scripted(serde_json::json!(true)),
        };
        assert_eq!(observe.on_point(&ctx).await, HookOutcome::Continue);
    }

    #[test]
    fn capability_table_gates_only_gate_points() {
        assert_eq!(
            required_permission_for_hook(wf_types::hook::CONTEXT_COMPRESSION_SIGNAL),
            Some(PluginPermission::Compaction)
        );
        assert_eq!(
            required_permission_for_hook("BEFORE_TOOL_CALL"),
            Some(PluginPermission::Tool)
        );
        assert_eq!(
            required_permission_for_hook("BEFORE_EXECUTE"),
            Some(PluginPermission::Tool)
        );
        assert_eq!(required_permission_for_hook("AFTER_TOOL_CALL"), None);
        assert_eq!(required_permission_for_hook("SOME_FUTURE_POINT"), None);
    }

    #[test]
    fn sync_registers_unmapped_warns_and_missing_permission_fails() {
        use wf_plugin::ContributionRegistrar;
        let registry = HookHandlerRegistry::new();
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_hook_handler("on_compress", scripted(serde_json::json!(true)))
            .unwrap();
        manager
            .as_registrar()
            .register_hook_handler("unmapped", scripted(serde_json::json!(true)))
            .unwrap();

        // Gate without the permission fails loudly.
        let manifest = test_manifest(
            HashMap::from([(
                "on_compress".to_string(),
                wf_types::hook::CONTEXT_COMPRESSION_SIGNAL.to_string(),
            )]),
            vec![],
            Some(PluginType::Native),
        );
        assert!(sync_hooks("p1", &manager, &manifest, &registry).is_err());
        assert!(!registry.contains("plugin-hook:p1:on_compress"));

        // With the permission the gate registers; unmapped handlers warn.
        let manifest = test_manifest(
            HashMap::from([(
                "on_compress".to_string(),
                wf_types::hook::CONTEXT_COMPRESSION_SIGNAL.to_string(),
            )]),
            vec![PluginPermission::Compaction],
            Some(PluginType::Native),
        );
        sync_hooks("p1", &manager, &manifest, &registry).unwrap();
        assert!(registry.contains("plugin-hook:p1:on_compress"));
        assert!(!registry.contains("plugin-hook:p1:unmapped"));

        // Unknown engine types fail loudly too.
        let bad = test_manifest(
            HashMap::from([("on_compress".to_string(), "NO_SUCH_POINT".to_string())]),
            vec![PluginPermission::Compaction],
            Some(PluginType::Native),
        );
        let fresh = HookHandlerRegistry::new();
        assert!(sync_hooks("p1", &manager, &bad, &fresh).is_err());

        // Teardown removes by name.
        unsync_hooks("p1", &manager, &registry);
        assert!(!registry.contains("plugin-hook:p1:on_compress"));
    }

    #[test]
    fn sync_rejects_duplicate_stable_names() {
        use wf_plugin::ContributionRegistrar;
        let registry = HookHandlerRegistry::new();
        let first = ContributionManager::new();
        first.start_registration("p1");
        first
            .as_registrar()
            .register_hook_handler("hook", scripted(serde_json::json!(true)))
            .unwrap();
        let manifest = test_manifest(
            HashMap::from([("hook".to_string(), "AFTER_TOOL_CALL".to_string())]),
            vec![],
            Some(PluginType::Lua),
        );
        sync_hooks("p1", &first, &manifest, &registry).unwrap();

        // A second plugin claiming the same stable name fails; reload of the
        // same plugin after unsync succeeds (no residue).
        let second = ContributionManager::new();
        second.start_registration("p1");
        second
            .as_registrar()
            .register_hook_handler("hook", scripted(serde_json::json!(true)))
            .unwrap();
        assert!(sync_hooks("p1", &second, &manifest, &registry).is_err());
        unsync_hooks("p1", &first, &registry);
        sync_hooks("p1", &second, &manifest, &registry).unwrap();
    }
}
