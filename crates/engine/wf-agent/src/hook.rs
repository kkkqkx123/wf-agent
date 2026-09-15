use std::collections::HashMap;

use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::hooks::{
    fire, fire::FireSummary, HookContext, HookDefinition, HookHandlerRegistry,
};
use wf_types::checkpoint::CheckpointTiming;

use crate::checkpoint::AgentCheckpointIntegration;
use crate::entity::AgentLoopEntity;

/// Map a fired hook point onto the checkpoint timing it requests. Unknown
/// points fall back to `Manual` so the strategy gate still applies instead
/// of bypassing it.
fn hook_type_to_checkpoint_timing(hook_type: &str) -> CheckpointTiming {
    match hook_type {
        "BEFORE_TOOL_CALL" => CheckpointTiming::ToolBefore,
        "AFTER_TOOL_CALL" => CheckpointTiming::ToolAfter,
        "BEFORE_ITERATION" => CheckpointTiming::BeforeExecute,
        "AFTER_ITERATION" => CheckpointTiming::AfterExecute,
        "BEFORE_LLM_CALL" | "AFTER_LLM_CALL" => CheckpointTiming::AfterExecute,
        "BEFORE_AGENT" => CheckpointTiming::Manual,
        "AFTER_AGENT" => CheckpointTiming::OnComplete,
        "BEFORE_USER_PROMPT" => CheckpointTiming::Manual,
        "SUBAGENT_START" => CheckpointTiming::Manual,
        "SUBAGENT_STOP" => CheckpointTiming::OnComplete,
        _ => CheckpointTiming::Manual,
    }
}

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

    /// Fire a hook point and, when any enabled definition of that type opts
    /// in via `create_checkpoint`, create one strategy-gated checkpoint.
    /// The checkpoint is a direct `create_checkpoint_gated` call (not a
    /// broadcast event) so ordering, strategy gating and error reporting
    /// stay synchronous with the fire. Failures only warn.
    pub async fn fire_agent_point_with_checkpoint(
        entity: &AgentLoopEntity,
        hook_type: &str,
        extra_data: HashMap<String, Value>,
        registry: Option<&HookHandlerRegistry>,
        event_bus: Option<&EventBus>,
        checkpoint: Option<&AgentCheckpointIntegration>,
    ) -> FireSummary {
        let summary =
            Self::fire_agent_point(entity, hook_type, extra_data, registry, event_bus).await;
        let Some(cp) = checkpoint else {
            return summary;
        };
        let opted_in = entity
            .hooks()
            .iter()
            .any(|h| h.hook_type == hook_type && h.enabled && h.create_checkpoint == Some(true));
        if !opted_in {
            return summary;
        }
        let timing = hook_type_to_checkpoint_timing(hook_type);
        if let Err(e) = cp.create_checkpoint_gated(entity, timing.clone()).await {
            tracing::warn!(
                error = %e,
                entity_id = %entity.id(),
                hook_type = %hook_type,
                trigger = ?timing,
                "hook-requested checkpoint failed"
            );
        }
        summary
    }
}
