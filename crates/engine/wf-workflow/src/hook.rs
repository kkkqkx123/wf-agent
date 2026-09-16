use std::collections::HashMap;

use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::hooks::{
    fire, fire::FireSummary, hook_checkpoint_description, hook_opted_in, HookContext,
    HookDefinition, HookHandlerRegistry,
};
use wf_types::checkpoint::CheckpointTiming;

use crate::checkpoint::WorkflowCheckpointIntegration;
use crate::entity::WorkflowExecutionEntity;

pub struct WorkflowHookEmitter;

impl WorkflowHookEmitter {
    /// Fire the hooks of `hook_type` against the workflow execution
    /// entity: evaluate, notify registered handlers synchronously and
    /// publish the `HOOK_TRIGGERED` audit event. Returns the fire summary
    /// so callers can inspect veto/observed outcomes uniformly with the
    /// agent emitter; non-gate callers ignore it.
    pub async fn fire_workflow_point(
        entity: &WorkflowExecutionEntity,
        hooks: &[HookDefinition],
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
            "workflow_id".to_string(),
            Value::String(entity.workflow_id().clone()),
        );
        data.insert(
            "status".to_string(),
            Value::String(format!("{:?}", entity.state.read().await.status())),
        );
        data.extend(extra_data);

        fire(
            registry.unwrap_or_else(|| HookHandlerRegistry::fallback()),
            hooks,
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

    /// Fire hooks against a caller-built context (e.g. the node
    /// coordinator, which assembles its own payload). Returns the fire
    /// summary so gate points can act on a veto.
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

    /// Map a workflow hook point onto the checkpoint timing of its
    /// hook-requested checkpoint. Node points map to node timings;
    /// workflow-scope points map to the lifecycle timings.
    pub fn hook_type_to_checkpoint_timing(hook_type: &str) -> CheckpointTiming {
        match hook_type {
            "BEFORE_EXECUTE" => CheckpointTiming::BeforeExecute,
            "AFTER_EXECUTE" => CheckpointTiming::AfterExecute,
            "ON_ERROR" => CheckpointTiming::OnError,
            "WORKFLOW_BEFORE" => CheckpointTiming::Manual,
            "WORKFLOW_AFTER" => CheckpointTiming::OnComplete,
            _ => CheckpointTiming::Manual,
        }
    }

    /// Hook opt-in checkpoint for a fired workflow hook point: no opt-in
    /// or no handle means no checkpoint. The checkpoint honors the master
    /// switch but not the instance trigger list, so one hook can force a
    /// checkpoint independently of the policy. Failures only warn; a veto at a
    /// gate point still takes effect because the fire summary is untouched.
    pub async fn maybe_hook_checkpoint(
        hooks: &[HookDefinition],
        hook_type: &str,
        checkpoint: Option<&WorkflowCheckpointIntegration>,
        entity: &WorkflowExecutionEntity,
    ) {
        let Some(cp) = checkpoint else {
            return;
        };
        if !hook_opted_in(hooks, hook_type) {
            return;
        }
        let timing = Self::hook_type_to_checkpoint_timing(hook_type);
        cp.create_hook_checkpoint(
            entity,
            timing,
            hook_checkpoint_description(hooks, hook_type),
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_execution_shared::hooks::HookDefinition;

    fn definition(hook_type: &str, enabled: bool, opt_in: Option<bool>) -> HookDefinition {
        HookDefinition {
            id: wf_types::Id::from(format!("h-{hook_type}")),
            hook_type: hook_type.to_string(),
            priority: 0,
            condition: None,
            enabled,
            payload: None,
            handler: None,
            create_checkpoint: opt_in,
            checkpoint_description: Some(format!("{hook_type} snapshot")),
        }
    }

    #[test]
    fn hook_opt_in_requires_enabled_and_true() {
        let hooks = vec![
            definition("BEFORE_EXECUTE", true, Some(true)),
            definition("AFTER_EXECUTE", true, None),
            definition("ON_ERROR", false, Some(true)),
        ];
        assert!(hook_opted_in(&hooks, "BEFORE_EXECUTE"));
        assert!(!hook_opted_in(&hooks, "AFTER_EXECUTE"));
        assert!(!hook_opted_in(&hooks, "ON_ERROR"));
        assert!(!hook_opted_in(&hooks, "WORKFLOW_BEFORE"));
    }

    #[test]
    fn hook_checkpoint_description_comes_from_first_opt_in() {
        let hooks = vec![definition("AFTER_EXECUTE", true, Some(true))];
        assert_eq!(
            hook_checkpoint_description(&hooks, "AFTER_EXECUTE"),
            Some("AFTER_EXECUTE snapshot".to_string())
        );
        assert_eq!(hook_checkpoint_description(&hooks, "BEFORE_EXECUTE"), None);
    }

    #[test]
    fn hook_timing_mapping_covers_workflow_points() {
        assert_eq!(
            WorkflowHookEmitter::hook_type_to_checkpoint_timing("BEFORE_EXECUTE"),
            CheckpointTiming::BeforeExecute
        );
        assert_eq!(
            WorkflowHookEmitter::hook_type_to_checkpoint_timing("AFTER_EXECUTE"),
            CheckpointTiming::AfterExecute
        );
        assert_eq!(
            WorkflowHookEmitter::hook_type_to_checkpoint_timing("ON_ERROR"),
            CheckpointTiming::OnError
        );
        assert_eq!(
            WorkflowHookEmitter::hook_type_to_checkpoint_timing("WORKFLOW_BEFORE"),
            CheckpointTiming::Manual
        );
        assert_eq!(
            WorkflowHookEmitter::hook_type_to_checkpoint_timing("WORKFLOW_AFTER"),
            CheckpointTiming::OnComplete
        );
    }
}
