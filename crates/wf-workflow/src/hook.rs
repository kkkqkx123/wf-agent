use std::collections::HashMap;

use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::hooks::{dispatch, BaseHookDefinition, HookContext, HookRegistry};

use crate::entity::WorkflowExecutionEntity;

/// Shared no-receiver fallback registry: without an injected registry the
/// dispatch degrades to the audit-only behavior (event publication), so
/// tests and minimal embeddings keep their observable events.
fn registry_or_default(registry: Option<&HookRegistry>) -> &HookRegistry {
    registry.unwrap_or_else(|| {
        static DEFAULT: std::sync::OnceLock<HookRegistry> = std::sync::OnceLock::new();
        DEFAULT.get_or_init(HookRegistry::new)
    })
}

pub struct WorkflowHookHandler;

impl WorkflowHookHandler {
    /// Dispatch the hooks of `hook_type` against the workflow execution
    /// entity: evaluate, notify registered receivers synchronously and
    /// publish the `HOOK_TRIGGERED` audit event.
    pub async fn emit_workflow_hooks(
        entity: &WorkflowExecutionEntity,
        hooks: &[BaseHookDefinition],
        hook_type: &str,
        extra_data: HashMap<String, Value>,
        registry: Option<&HookRegistry>,
        event_bus: Option<&EventBus>,
    ) {
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

        dispatch(
            registry_or_default(registry),
            hooks,
            hook_type,
            &HookContext {
                execution_id: entity.id().clone(),
                hook_type: hook_type.to_string(),
                data,
            },
            event_bus,
        )
        .await;
    }

    /// Dispatch hooks against a caller-built context (e.g. the node
    /// coordinator, which assembles its own payload).
    pub async fn emit_hooks(
        hooks: &[BaseHookDefinition],
        hook_type: &str,
        ctx: &HookContext,
        registry: Option<&HookRegistry>,
        event_bus: Option<&EventBus>,
    ) {
        dispatch(
            registry_or_default(registry),
            hooks,
            hook_type,
            ctx,
            event_bus,
        )
        .await;
    }
}
