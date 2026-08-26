//! Hook audit event publication (record channel).
//!
//! Hook delivery is a synchronous dispatch ([`crate::hooks::dispatch`]); the
//! `HOOK_TRIGGERED` event published here is the audit copy: persistence,
//! external subscriptions and user trigger template matching consume it, but
//! no functional delivery depends on it.

use std::collections::HashMap;

use serde_json::Value;
use wf_core::condition::ConditionEvaluator;
use wf_core::event::EventBus;
use wf_types::events::{BaseEvent, EventType};
use wf_types::Id;

use crate::error::{ExecutionSharedError, ExecutionSharedResult};
use crate::hooks::dispatch::ReceiverResult;
use crate::hooks::types::{BaseHookDefinition, HookContext};

/// Hooks of `hook_type` that are enabled, sorted by weight descending.
pub fn filter_and_sort_hooks(
    hooks: &[BaseHookDefinition],
    hook_type: &str,
) -> Vec<BaseHookDefinition> {
    let mut filtered: Vec<_> = hooks
        .iter()
        .filter(|h| h.hook_type == hook_type && h.enabled)
        .cloned()
        .collect();
    filtered.sort_by_key(|h| std::cmp::Reverse(h.weight));
    filtered
}

/// Evaluate the hook condition against the hook context data.
pub fn evaluate_hook_condition(
    condition: Option<&str>,
    context: &HashMap<String, Value>,
) -> ExecutionSharedResult<bool> {
    match condition {
        None => Ok(true),
        Some(cond) => ConditionEvaluator::evaluate(cond, context)
            .map_err(|e| ExecutionSharedError::ConditionError(e.to_string())),
    }
}

/// Publish the `HOOK_TRIGGERED` audit event for one dispatch.
///
/// The event is routable and matchable by trigger templates:
/// - `execution_id` / `agent_loop_id` come from the hook context;
/// - `workflow_id` is picked up from the context data when the caller
///   injected it (workflow hooks do; agent hooks have no workflow);
/// - metadata carries `hook_type` (the dispatched type), `hook_count`,
///   per-hook `weights` and `payloads` (template-resolved), plus the
///   dispatch summary: `receivers` (name / outcome / duration_ms / error per
///   notified receiver) and the total `duration_ms`.
///
/// Returns the number of events published (0 when nothing was dispatched or
/// no bus is attached).
pub fn publish_hook_audit_event(
    event_bus: Option<&EventBus>,
    ctx: &HookContext,
    payloads: &[Value],
    weights: &[i32],
    results: &[ReceiverResult],
    duration_ms: i64,
) -> usize {
    if payloads.is_empty() && results.is_empty() {
        return 0;
    }
    let Some(bus) = event_bus else {
        return 0;
    };

    let receivers: Vec<Value> = results
        .iter()
        .map(|r| {
            let mut entry = serde_json::Map::new();
            entry.insert("name".to_string(), Value::String(r.name.clone()));
            entry.insert(
                "outcome".to_string(),
                match &r.outcome {
                    crate::hooks::types::HookOutcome::Continue => {
                        Value::String("continue".to_string())
                    }
                    crate::hooks::types::HookOutcome::Intercept { reason } => {
                        serde_json::json!({"intercept": reason})
                    }
                },
            );
            entry.insert(
                "duration_ms".to_string(),
                Value::Number(serde_json::Number::from(r.duration_ms)),
            );
            if let Some(error) = &r.error {
                entry.insert("error".to_string(), Value::String(error.clone()));
            }
            Value::Object(entry)
        })
        .collect();

    let metadata_value = serde_json::json!({
        "hook_type": [ctx.hook_type],
        "hook_count": payloads.len(),
        "weights": weights,
        "payloads": payloads,
        "receivers": receivers,
        "duration_ms": duration_ms,
    });
    let metadata = metadata_value
        .as_object()
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let workflow_id = ctx
        .data
        .get("workflow_id")
        .and_then(|v| v.as_str())
        .map(|s| Id::from(s.to_string()));
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: EventType::HookTriggered,
        timestamp: wf_common::now(),
        event_name: None,
        workflow_id,
        execution_id: Some(ctx.execution_id.clone()),
        agent_loop_id: Some(ctx.execution_id.clone()),
        metadata: Some(metadata),
    };
    let _ = bus.publish(event);
    1
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::hooks::types::HookOutcome;
    use wf_types::Id;

    fn make_hook(id: &str, hook_type: &str, weight: i32, enabled: bool) -> BaseHookDefinition {
        BaseHookDefinition {
            id: id.to_string(),
            hook_type: hook_type.to_string(),
            weight,
            condition: None,
            enabled,
            payload: None,
            receiver: None,
        }
    }

    #[test]
    fn test_filter_by_type() {
        let hooks = vec![
            make_hook("1", "before_iteration", 10, true),
            make_hook("2", "after_iteration", 5, true),
            make_hook("3", "before_iteration", 1, true),
        ];
        let filtered = filter_and_sort_hooks(&hooks, "before_iteration");
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].id, "1");
        assert_eq!(filtered[1].id, "3");
    }

    #[test]
    fn test_filter_disabled() {
        let hooks = vec![
            make_hook("1", "before_iteration", 10, false),
            make_hook("2", "before_iteration", 5, true),
        ];
        let filtered = filter_and_sort_hooks(&hooks, "before_iteration");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "2");
    }

    #[test]
    fn test_sort_by_weight_desc() {
        let hooks = vec![
            make_hook("1", "before_iteration", 1, true),
            make_hook("2", "before_iteration", 100, true),
            make_hook("3", "before_iteration", 50, true),
        ];
        let filtered = filter_and_sort_hooks(&hooks, "before_iteration");
        assert_eq!(filtered[0].id, "2");
        assert_eq!(filtered[1].id, "3");
        assert_eq!(filtered[2].id, "1");
    }

    #[test]
    fn test_evaluate_condition_none() {
        let ctx = HashMap::new();
        assert!(evaluate_hook_condition(None, &ctx).unwrap());
    }

    #[test]
    fn test_evaluate_condition_true() {
        let mut ctx = HashMap::new();
        ctx.insert("flag".to_string(), serde_json::Value::Bool(true));
        assert!(evaluate_hook_condition(Some("flag"), &ctx).unwrap());
    }

    #[test]
    fn test_evaluate_condition_false() {
        let ctx = HashMap::new();
        assert!(!evaluate_hook_condition(Some("missing"), &ctx).unwrap());
    }

    fn hook_ctx(execution_id: &str, data: HashMap<String, Value>) -> HookContext {
        HookContext {
            execution_id: Id::from(execution_id.to_string()),
            hook_type: "test".to_string(),
            data,
        }
    }

    #[test]
    fn test_publish_without_bus_is_noop() {
        let ctx = hook_ctx("exec-1", HashMap::new());
        assert_eq!(publish_hook_audit_event(None, &ctx, &[], &[], &[], 0), 0);
    }

    #[test]
    fn test_publish_empty_dispatch_is_noop() {
        let bus = Arc::new(EventBus::new(16));
        let ctx = hook_ctx("exec-1", HashMap::new());
        assert_eq!(
            publish_hook_audit_event(Some(&bus), &ctx, &[], &[], &[], 0),
            0
        );
    }

    #[test]
    fn test_event_bus_receives_audit_event() {
        let bus = Arc::new(EventBus::new(16));
        let ctx = hook_ctx("exec-1", HashMap::new());

        let mut sub = bus.subscribe();

        let results = vec![ReceiverResult {
            name: "r1".to_string(),
            outcome: HookOutcome::Continue,
            duration_ms: 3,
            error: None,
        }];
        assert_eq!(
            publish_hook_audit_event(Some(&bus), &ctx, &[Value::Null], &[1], &results, 3),
            1
        );

        let event = sub.try_recv().unwrap();
        assert_eq!(event.r#type, EventType::HookTriggered);
        assert_eq!(event.execution_id.as_deref(), Some("exec-1"));
        assert_eq!(event.agent_loop_id.as_deref(), Some("exec-1"));
        let metadata = event.metadata.as_ref().unwrap();
        assert_eq!(metadata["hook_type"], serde_json::json!(["test"]));
        assert_eq!(metadata["hook_count"], serde_json::json!(1));
        assert_eq!(metadata["receivers"][0]["name"], serde_json::json!("r1"));
        assert_eq!(
            metadata["receivers"][0]["outcome"],
            serde_json::json!("continue")
        );
        assert_eq!(
            metadata["receivers"][0]["duration_ms"],
            serde_json::json!(3)
        );
    }

    #[test]
    fn test_event_picks_up_workflow_id_from_context_data() {
        let bus = Arc::new(EventBus::new(16));
        let mut data = HashMap::new();
        data.insert("workflow_id".to_string(), serde_json::json!("wf-1"));
        let ctx = hook_ctx("exec-1", data);

        let mut sub = bus.subscribe();

        publish_hook_audit_event(Some(&bus), &ctx, &[Value::Null], &[1], &[], 1);

        let event = sub.try_recv().unwrap();
        assert_eq!(event.workflow_id.as_deref(), Some("wf-1"));
    }
}
