use super::*;
use wf_types::trigger::config::TriggerAction;

fn make_template() -> TriggerTemplate {
    TriggerTemplate {
        name: "on-file-change".to_string(),
        description: None,
        condition: None,
        action: None,
        enabled: None,
        max_triggers: None,
        priority: None,
        dispatch_mode: None,
        allow_multi_effect: None,
        effect_order: None,
        metadata: None,
        created_at: 0,
        updated_at: 0,
        create_checkpoint: None,
        checkpoint_description_template: None,
    }
}

fn scoped_template(
    name: &str,
    event_type: &str,
    prefix: Option<&str>,
    priority: Option<i32>,
    mode: Option<wf_types::trigger::TriggerDispatchMode>,
) -> TriggerTemplate {
    use wf_types::trigger::TriggerCondition;
    let mut template = make_template();
    template.name = name.to_string();
    template.condition = Some(TriggerCondition {
        event_type: event_type.to_string(),
        event_name: None,
        condition: None,
        metadata: None,
        metadata_exists: None,
        execution_prefix: prefix.map(str::to_string),
    });
    template.action = Some(TriggerAction::StopWorkflowExecution {});
    template.priority = priority;
    template.dispatch_mode = mode;
    template
}

#[test]
fn test_set_disjoint_prefixes_do_not_compete() {
    let templates = vec![
        scoped_template("a", "NODE_COMPLETED", Some("exec-a-"), None, None),
        scoped_template("b", "NODE_COMPLETED", Some("exec-b-"), None, None),
    ];
    assert!(validate_trigger_set(&templates).is_ok());
}

#[test]
fn test_set_disabled_templates_are_excluded() {
    let mut templates = vec![
        scoped_template("a", "NODE_COMPLETED", None, None, None),
        scoped_template("b", "NODE_COMPLETED", None, None, None),
    ];
    templates[1].enabled = Some(false);
    assert!(validate_trigger_set(&templates).is_ok());
}

#[test]
fn test_check_scopes_reports_every_violation() {
    let existing = vec![scoped_template("old", "NODE_COMPLETED", None, None, None)];
    let incoming = vec![scoped_template("new", "NODE_COMPLETED", None, None, None)];
    let reports = check_trigger_scopes(&existing, &incoming);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].incoming_names, vec!["new".to_string()]);
    assert!(reports[0].names.contains(&"old".to_string()));
    assert!(reports[0].names.contains(&"new".to_string()));
    assert!(reports[0].message.contains("new"));

    // A single existing subscriber alone is valid.
    let reports = check_trigger_scopes(&existing, &[]);
    assert!(reports.is_empty());

    // A violated scope with no incoming member is still reported so
    // callers can warn on the pre-existing conflict.
    let conflicting = vec![
        scoped_template("old", "NODE_COMPLETED", None, None, None),
        scoped_template("older", "NODE_COMPLETED", None, None, None),
    ];
    let reports = check_trigger_scopes(&conflicting, &[]);
    assert_eq!(reports.len(), 1);
    assert!(reports[0].incoming_names.is_empty());
    assert_eq!(reports[0].names.len(), 2);
}

#[test]
fn test_transform_trigger_template() {
    let template = make_template();
    let mut params = HashMap::new();
    params.insert("target".to_string(), "main".to_string());

    let result = transform_trigger_template(&template, &params).unwrap();
    assert_eq!(result.name, "on-file-change");
}

#[test]
fn test_export_trigger_template() {
    let template = make_template();
    let exported = export_trigger_template(template.clone());
    assert_eq!(exported.name, template.name);
}

#[test]
fn test_multi_effect_defaults_to_single_execution() {
    let template = make_template();
    assert!(validate_multi_effect(&template).is_ok());
    assert!(validate_trigger_template(&template).is_ok());
}

#[test]
fn test_multi_effect_opt_in_rejected_until_implemented() {
    let mut template = make_template();
    template.allow_multi_effect = Some(true);
    template.effect_order = None;
    assert!(validate_multi_effect(&template).is_err());
    assert!(validate_trigger_template(&template).is_err());

    template.effect_order = Some(vec![]);
    assert!(validate_multi_effect(&template).is_err());

    template.effect_order = Some(vec!["audit".to_string(), "notify".to_string()]);
    assert!(
        validate_multi_effect(&template).is_err(),
        "opt-in stays rejected even with an order until the runtime implements it"
    );
    assert!(validate_trigger_template(&template).is_err());
}

#[test]
fn test_effect_order_without_opt_in_rejected() {
    let mut template = make_template();
    template.effect_order = Some(vec!["audit".to_string()]);
    assert!(validate_multi_effect(&template).is_err());
}

#[test]
fn test_runtime_limits_validation() {
    let limits = TriggerRuntimeLimits::default();
    assert!(validate_trigger_runtime_limits(&limits).is_ok());

    let limits = TriggerRuntimeLimits {
        max_concurrent_actions: Some(8),
        dispatch_burst_limit: Some(1),
        ..TriggerRuntimeLimits::default()
    };
    assert!(validate_trigger_runtime_limits(&limits).is_ok());

    let limits = TriggerRuntimeLimits {
        max_concurrent_actions: Some(0),
        dispatch_burst_limit: None,
        ..TriggerRuntimeLimits::default()
    };
    assert!(validate_trigger_runtime_limits(&limits).is_err());

    let limits = TriggerRuntimeLimits {
        max_concurrent_actions: None,
        dispatch_burst_limit: Some(0),
        ..TriggerRuntimeLimits::default()
    };
    assert!(validate_trigger_runtime_limits(&limits).is_err());
}

#[test]
fn test_action_context_matrix_listener_accepts_all() {
    use wf_types::trigger::TriggerExecutionContext::EventListener;
    let actions = vec![
        TriggerAction::StopWorkflowExecution {},
        TriggerAction::SetVariable {
            variable_name: "x".to_string(),
            value: serde_json::json!(1),
        },
        TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: "child".to_string(),
            prompt: None,
            model: None,
            result_variable: None,
            wait_for_completion: None,
            timeout: None,
            input_mode: None,
            writeback: None,
            checkpoint_message_interval: None,
        },
        TriggerAction::ExecuteWorkflow {
            workflow_id: "wf".to_string(),
            input: None,
            timeout: None,
        },
        TriggerAction::ExecuteAgent {
            agent_id: "child".to_string(),
            prompt: None,
            model: None,
            input: None,
            timeout: None,
            checkpoint_message_interval: None,
        },
    ];
    for action in &actions {
        assert!(
            validate_trigger_action_for_context(action, EventListener, "action").is_ok(),
            "{} must validate in the event listener",
            action.action_name()
        );
    }
}

#[test]
fn test_action_context_matrix_message_node_rejects_nested_agent() {
    use wf_types::trigger::TriggerExecutionContext::MessageNode;
    let nested = TriggerAction::ExecuteTriggeredAgentExecution {
        agent_id: "child".to_string(),
        prompt: None,
        model: None,
        result_variable: None,
        wait_for_completion: None,
        timeout: None,
        input_mode: None,
        writeback: None,
        checkpoint_message_interval: None,
    };
    let err = validate_trigger_action_for_context(&nested, MessageNode, "action")
        .expect_err("nested agent must be rejected in message nodes");
    let message = err.to_string();
    assert!(
        message.contains("execute_triggered_agent_execution"),
        "{message}"
    );
    assert!(
        message.contains("execute_triggered_subworkflow"),
        "{message}"
    );

    let supported = TriggerAction::SetVariable {
        variable_name: "x".to_string(),
        value: serde_json::json!(1),
    };
    assert!(validate_trigger_action_for_context(&supported, MessageNode, "action").is_ok());

    for cold in [
        TriggerAction::ExecuteWorkflow {
            workflow_id: "wf".to_string(),
            input: None,
            timeout: None,
        },
        TriggerAction::ExecuteAgent {
            agent_id: "child".to_string(),
            prompt: None,
            model: None,
            input: None,
            timeout: None,
            checkpoint_message_interval: None,
        },
    ] {
        let err = validate_trigger_action_for_context(&cold, MessageNode, "action")
            .expect_err("cold-start actions must be rejected in message nodes");
        let message = err.to_string();
        assert!(message.contains(cold.action_name()), "{message}");
        assert!(
            message.contains("event-driven trigger listener"),
            "{message}"
        );
    }

    let err = validate_trigger_action(
        &TriggerAction::ExecuteWorkflow {
            workflow_id: String::new(),
            input: None,
            timeout: None,
        },
        "action",
    )
    .expect_err("empty workflow_id must be rejected");
    assert!(err.to_string().contains("workflow_id"));
}

#[test]
fn test_burst_cap_truncates_winners() {
    let limits = TriggerRuntimeLimits {
        max_concurrent_actions: None,
        dispatch_burst_limit: Some(1),
        ..TriggerRuntimeLimits::default()
    };
    let mut winners = vec!["a", "b", "c"];
    assert_eq!(limits.apply_burst_cap(&mut winners), 2);
    assert_eq!(winners, vec!["a"]);

    let limits = TriggerRuntimeLimits::default();
    let mut winners = vec!["a", "b"];
    assert_eq!(limits.apply_burst_cap(&mut winners), 0);
    assert_eq!(winners.len(), 2);
}

#[test]
fn test_retry_event_subscription_accepted() {
    use wf_types::trigger::TriggerCondition;
    let mut template = make_template();
    template.condition = Some(TriggerCondition {
        event_type: "LLM_RETRY_SCHEDULED".to_string(),
        event_name: None,
        condition: None,
        metadata: None,
        metadata_exists: Some(vec!["attempt".to_string()]),
        execution_prefix: None,
    });
    template.action = Some(TriggerAction::SendNotification {
        message: "retry scheduled".to_string(),
    });
    assert!(validate_trigger_template(&template).is_ok());
    assert!(validate_trigger_set(std::slice::from_ref(&template)).is_ok());
}

#[test]
fn test_empty_event_name_rejected() {
    use wf_types::trigger::TriggerCondition;
    let mut template = make_template();
    template.condition = Some(TriggerCondition {
        event_type: "NODE_COMPLETED".to_string(),
        event_name: Some("   ".to_string()),
        condition: None,
        metadata: None,
        metadata_exists: None,
        execution_prefix: None,
    });
    template.action = Some(TriggerAction::StopWorkflowExecution {});
    assert!(validate_trigger_template(&template).is_err());
}

#[test]
fn test_empty_execution_prefix_rejected() {
    use wf_types::trigger::TriggerCondition;
    let mut template = make_template();
    template.condition = Some(TriggerCondition {
        event_type: "NODE_COMPLETED".to_string(),
        event_name: None,
        condition: None,
        metadata: None,
        metadata_exists: None,
        execution_prefix: Some("  ".to_string()),
    });
    template.action = Some(TriggerAction::StopWorkflowExecution {});
    assert!(validate_trigger_template(&template).is_err());
}

#[test]
fn test_non_string_hook_type_rejected() {
    use wf_types::trigger::TriggerCondition;
    let mut template = make_template();
    template.condition = Some(TriggerCondition {
        event_type: "HOOK_TRIGGERED".to_string(),
        event_name: None,
        condition: None,
        metadata: Some(std::collections::HashMap::from([(
            "hook_type".to_string(),
            serde_json::json!(42),
        )])),
        metadata_exists: None,
        execution_prefix: None,
    });
    template.action = Some(TriggerAction::StopWorkflowExecution {});
    assert!(validate_trigger_template(&template).is_err());
}

#[test]
fn test_duplicate_metadata_exists_rejected() {
    use wf_types::trigger::TriggerCondition;
    let mut template = make_template();
    template.condition = Some(TriggerCondition {
        event_type: "NODE_COMPLETED".to_string(),
        event_name: None,
        condition: None,
        metadata: None,
        metadata_exists: Some(vec!["attempt".to_string(), "attempt".to_string()]),
        execution_prefix: None,
    });
    template.action = Some(TriggerAction::StopWorkflowExecution {});
    assert!(validate_trigger_template(&template).is_err());
}

#[test]
fn test_invalid_variable_identifier_rejected() {
    let action = TriggerAction::SetVariable {
        variable_name: "9bad".to_string(),
        value: serde_json::json!(1),
    };
    assert!(validate_trigger_action(&action, "action").is_err());
}

#[test]
fn test_invalid_context_identifier_rejected() {
    let action = TriggerAction::AppendMessageContext {
        context_id: "bad-name".to_string(),
        messages: vec![wf_types::message::Message {
            id: wf_types::Id::new(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("hi".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }],
    };
    assert!(validate_trigger_action(&action, "action").is_err());
}
