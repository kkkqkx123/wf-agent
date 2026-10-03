use std::collections::HashMap;

use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::hooks::{
    fire, fire::FireSummary, hook_checkpoint_description_fired, hook_opted_in_fired, HookContext,
    HookDefinition, HookHandlerRegistry,
};
use wf_execution_shared::types::execution_entity::ExecutionEntity;

use crate::checkpoint::AgentCheckpointIntegration;
use crate::entity::AgentLoopEntity;

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
        let ctx = HookContext::agent_base(
            entity.id().clone(),
            hook_type.to_string(),
            format!("{:?}", entity.state.read().await.status()),
            entity.state.read().await.current_iteration(),
            extra_data,
            entity.get_abort_signal(),
        );

        fire(
            registry.unwrap_or_else(|| HookHandlerRegistry::fallback()),
            entity.hooks(),
            hook_type,
            &ctx,
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
            registry.unwrap_or_else(|| HookHandlerRegistry::fallback()),
            hooks,
            hook_type,
            ctx,
            event_bus,
        )
        .await
    }

    /// Fire a hook point and, when an opted-in definition of that type
    /// actually fired (condition passed, see `FireSummary::matched_hook_ids`),
    /// create one hook-requested checkpoint.
    /// The checkpoint honors the master switch but bypasses per-trigger
    /// cadence (one hook forces a checkpoint independently of policy),
    /// mirroring the workflow hook contract. Failures only warn.
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
        Self::maybe_hook_checkpoint(
            entity.hooks(),
            hook_type,
            &summary.matched_hook_ids,
            checkpoint,
            entity,
        )
        .await;
        summary
    }

    /// Hook opt-in checkpoint for a hook point that was fired without the
    /// entity (e.g. the parallel tool-call path fires via `fire_point` inside
    /// spawned tasks, then settles one batch-level checkpoint per hook type
    /// here where the entity is available). Only definitions that passed
    /// evaluation in the associated fire count (`fired_hook_ids`): a
    /// condition-filtered opt-in requests no snapshot. No fired opt-in or
    /// no handle means no checkpoint; failures only warn so the fire outcome
    /// never changes.
    pub async fn maybe_hook_checkpoint(
        hooks: &[HookDefinition],
        hook_type: &str,
        fired_hook_ids: &[String],
        checkpoint: Option<&AgentCheckpointIntegration>,
        entity: &AgentLoopEntity,
    ) {
        let Some(cp) = checkpoint else {
            return;
        };
        if !hook_opted_in_fired(hooks, hook_type, fired_hook_ids) {
            return;
        }
        let timing = wf_types::hook::hook_checkpoint_timing(hook_type);
        let description = hook_checkpoint_description_fired(hooks, hook_type, fired_hook_ids);
        cp.create_hook_checkpoint(entity, timing, description).await;
    }
}
