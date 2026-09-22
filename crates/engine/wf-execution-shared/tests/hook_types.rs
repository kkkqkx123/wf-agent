use std::collections::HashMap;

use serde_json::json;
use wf_execution_shared::hooks::{HookContext, HookDefinition, HookOutcome};
use wf_types::hook::{CanonicalHookSpec, HookPointStaticConfig};

#[test]
fn outcome_continue_is_not_veto() {
    let outcome = HookOutcome::Continue;
    assert!(!outcome.is_veto());
    assert_eq!(outcome.as_str(), "continue");
}

#[test]
fn outcome_veto_carries_reason() {
    let outcome = HookOutcome::Veto {
        reason: "deny deploy".to_string(),
    };
    assert!(outcome.is_veto());
    assert_eq!(outcome.as_str(), "vetoed");
}

#[test]
fn definition_from_canonical_parts_carries_fields() {
    let spec = CanonicalHookSpec::from_parts(
        "BEFORE_EXECUTE".to_string(),
        Some("input.ok".to_string()),
        true,
        10,
        Some(json!({"key": "value"})),
        Some("audit-handler".to_string()),
    );
    let definition = HookDefinition::from(&spec);
    assert_eq!(definition.hook_type, "BEFORE_EXECUTE");
    assert_eq!(definition.priority, 10);
    assert_eq!(definition.condition.as_deref(), Some("input.ok"));
    assert!(definition.enabled);
    assert_eq!(definition.payload, Some(json!({"key": "value"})));
    assert_eq!(definition.handler.as_deref(), Some("audit-handler"));
    assert!(definition.create_checkpoint.is_none());
}

#[test]
fn definition_from_static_config_applies_defaults() {
    let config = HookPointStaticConfig {
        hook_type: "AFTER_EXECUTE".to_string(),
        condition: None,
        event_payload: None,
        enabled: None,
        priority: None,
        create_checkpoint: None,
        checkpoint_description: None,
        handler: None,
    };
    let definition = HookDefinition::from(&config);
    assert_eq!(definition.hook_type, "AFTER_EXECUTE");
    assert_eq!(definition.priority, 0);
    assert!(definition.enabled);
    assert!(definition.handler.is_none());
}

#[test]
fn hook_context_carries_execution_data() {
    let mut data = HashMap::new();
    data.insert("node_id".to_string(), json!("n1"));
    let ctx = HookContext {
        execution_id: "exec-1".to_string(),
        hook_type: "BEFORE_EXECUTE".to_string(),
        data,
    };
    assert_eq!(ctx.execution_id, "exec-1");
    assert_eq!(ctx.hook_type, "BEFORE_EXECUTE");
    assert_eq!(ctx.data["node_id"], json!("n1"));
}
