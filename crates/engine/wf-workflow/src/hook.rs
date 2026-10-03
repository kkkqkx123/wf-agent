use std::collections::HashMap;

use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::hooks::{
    fire, fire::FireSummary, hook_checkpoint_description_fired, hook_opted_in_fired, HookContext,
    HookDefinition, HookHandlerRegistry,
};
use wf_execution_shared::types::execution_entity::ExecutionEntity;

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
        let ctx = HookContext::workflow_base(
            entity.id().clone(),
            entity.workflow_id().clone(),
            hook_type.to_string(),
            format!("{:?}", entity.state.read().await.status()),
            extra_data,
            entity.get_abort_signal(),
        );

        fire(
            registry.unwrap_or_else(|| HookHandlerRegistry::fallback()),
            hooks,
            hook_type,
            &ctx,
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

    /// Hook opt-in checkpoint for a fired workflow hook point: only
    /// definitions that passed evaluation in the associated fire count
    /// (`fired_hook_ids`), so a condition-filtered opt-in requests no
    /// snapshot. No fired opt-in or no handle means no checkpoint.
    /// The checkpoint honors the master switch but not the instance trigger
    /// list, so one hook can force a checkpoint independently of the policy.
    /// Failures only warn; a veto at a gate point still takes effect because
    /// the fire summary is untouched.
    pub async fn maybe_hook_checkpoint(
        hooks: &[HookDefinition],
        hook_type: &str,
        fired_hook_ids: &[String],
        checkpoint: Option<&WorkflowCheckpointIntegration>,
        entity: &WorkflowExecutionEntity,
    ) {
        let Some(cp) = checkpoint else {
            return;
        };
        if !hook_opted_in_fired(hooks, hook_type, fired_hook_ids) {
            return;
        }
        let timing = wf_types::hook::hook_checkpoint_timing(hook_type);
        cp.create_hook_checkpoint(
            entity,
            timing,
            hook_checkpoint_description_fired(hooks, hook_type, fired_hook_ids),
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
    fn hook_opt_in_requires_enabled_fired_and_true() {
        let hooks = vec![
            definition("BEFORE_EXECUTE", true, Some(true)),
            definition("AFTER_EXECUTE", true, None),
            definition("ON_ERROR", false, Some(true)),
        ];
        let fired = vec!["h-BEFORE_EXECUTE".to_string()];
        assert!(hook_opted_in_fired(&hooks, "BEFORE_EXECUTE", &fired));
        assert!(!hook_opted_in_fired(&hooks, "BEFORE_EXECUTE", &[]));
        assert!(!hook_opted_in_fired(&hooks, "AFTER_EXECUTE", &fired));
        assert!(!hook_opted_in_fired(
            &hooks,
            "ON_ERROR",
            &["h-ON_ERROR".to_string()]
        ));
        assert!(!hook_opted_in_fired(&hooks, "WORKFLOW_BEFORE", &fired));
    }

    #[test]
    fn hook_checkpoint_description_comes_from_first_fired_opt_in() {
        let hooks = vec![definition("AFTER_EXECUTE", true, Some(true))];
        assert_eq!(
            hook_checkpoint_description_fired(
                &hooks,
                "AFTER_EXECUTE",
                &["h-AFTER_EXECUTE".to_string()]
            ),
            Some("AFTER_EXECUTE snapshot".to_string())
        );
        assert_eq!(
            hook_checkpoint_description_fired(&hooks, "AFTER_EXECUTE", &[]),
            None
        );
        assert_eq!(
            hook_checkpoint_description_fired(
                &hooks,
                "BEFORE_EXECUTE",
                &["h-AFTER_EXECUTE".to_string()]
            ),
            None
        );
    }

    #[test]
    fn hook_timing_mapping_covers_workflow_points() {
        use wf_types::checkpoint::CheckpointTiming;
        use wf_types::hook::hook_checkpoint_timing;
        assert_eq!(
            hook_checkpoint_timing("BEFORE_EXECUTE"),
            CheckpointTiming::BeforeExecute
        );
        assert_eq!(
            hook_checkpoint_timing("AFTER_EXECUTE"),
            CheckpointTiming::AfterExecute
        );
        assert_eq!(
            hook_checkpoint_timing("ON_ERROR"),
            CheckpointTiming::OnError
        );
        assert_eq!(
            hook_checkpoint_timing("WORKFLOW_BEFORE"),
            CheckpointTiming::Manual
        );
        assert_eq!(
            hook_checkpoint_timing("WORKFLOW_AFTER"),
            CheckpointTiming::OnComplete
        );
    }
}
