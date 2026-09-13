use std::collections::HashMap;

use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::hooks::{
    fire, fire::FireSummary, HookContext, HookDefinition, HookHandlerRegistry,
};

use crate::entity::AgentLoopEntity;

/// Shared no-handler fallback registry: without an injected registry the
/// fire degrades to the audit-only behavior (event publication), so
/// tests and minimal embeddings keep their observable events.
fn registry_or_default(registry: Option<&HookHandlerRegistry>) -> &HookHandlerRegistry {
    registry.unwrap_or_else(|| {
        static DEFAULT: std::sync::OnceLock<HookHandlerRegistry> = std::sync::OnceLock::new();
        DEFAULT.get_or_init(HookHandlerRegistry::new)
    })
}

pub struct AgentHookEmitter;

impl AgentHookEmitter {
    /// Fire the hooks of `hook_type` configured on `entity`: evaluate,
    /// notify registered handlers synchronously and publish the
    /// `HOOK_TRIGGERED` audit event. Returns the fire summary so gate
    /// points can act on a veto.
    pub async fn fire_agent_point(
        entity: &AgentLoopEntity,
        hook_type: &str,
        extra_data: HashMap<String, Value>,
        registry: Option<&HookHandlerRegistry>,
        event_bus: Option<&EventBus>,
    ) -> FireSummary {
        let mut data = HashMap::new();
        data.insert(
            "execution_id".to_string(),
            Value::String(entity.id().clone()),
        );
        data.insert(
            "current_iteration".to_string(),
            Value::Number(serde_json::Number::from(
                entity.state.read().await.current_iteration(),
            )),
        );
        data.insert(
            "status".to_string(),
            Value::String(format!("{:?}", entity.state.read().await.status())),
        );
        data.extend(extra_data);

        fire(
            registry_or_default(registry),
            entity.hooks(),
            hook_type,
            &HookContext {
                execution_id: entity.id().clone(),
                hook_type: hook_type.to_string(),
                data,
            },
            event_bus,
        )
        .await
    }

    /// Fire hooks against a caller-built context (e.g. the parallel
    /// tool-call path, where the entity is not available inside the spawned
    /// task). Returns the fire summary so gate points can act on a veto.
    pub async fn fire_point(
        hooks: &[HookDefinition],
        hook_type: &str,
        ctx: &HookContext,
        registry: Option<&HookHandlerRegistry>,
        event_bus: Option<&EventBus>,
    ) -> FireSummary {
        fire(
            registry_or_default(registry),
            hooks,
            hook_type,
            ctx,
            event_bus,
        )
        .await
    }
}
