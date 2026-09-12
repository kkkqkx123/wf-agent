use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};
use crate::processor::substitute::substitute_in_struct;
use crate::validator::{validate_min, validate_not_empty, validate_required};

use wf_types::trigger::config::TriggerAction;
use wf_types::trigger::template::TriggerTemplate;

pub fn validate_trigger_template(template: &TriggerTemplate) -> ConfigResult<()> {
    validate_required(&template.name, "name")?;
    if let Some(max_triggers) = template.max_triggers {
        validate_min(max_triggers, 1, "max_triggers")?;
    }
    if let Some(condition) = &template.condition {
        if condition.targets_compression_signal() {
            return Err(ConfigError::Validation(format!(
                "trigger '{}' subscribes to the internal compression signal; register a hook handler for '{}' instead, the event copy is audit-only",
                template.name,
                wf_types::hook::CONTEXT_COMPRESSION_SIGNAL
            )));
        }
        if condition.targets_before_hook() {
            return Err(ConfigError::Validation(format!(
                "trigger '{}' subscribes to a BEFORE_* hook point; trigger actions always run asynchronously after the hook and cannot gate execution. Observe synchronously with a hook handler instead, gate tool calls with approval, or subscribe to the AFTER_* counterpart for post-hoc side effects",
                template.name
            )));
        }
        // A NODE_CUSTOM_EVENT condition is matched by `event_name`: it is
        // required, otherwise the template can never match.
        if condition.event_type == "NODE_CUSTOM_EVENT" && condition.event_name.is_none() {
            return Err(ConfigError::Validation(
                "event_name is required when event_type is NODE_CUSTOM_EVENT".to_string(),
            ));
        }
        // A condition expression is only meaningful together with a concrete
        // event type; standalone expressions against every event are
        // rejected as a likely configuration mistake.
        if condition.condition.is_some() && condition.event_type.is_empty() {
            return Err(ConfigError::Validation(
                "condition expression requires a concrete event_type".to_string(),
            ));
        }
    }
    if let Some(action) = &template.action {
        validate_trigger_action(action, "action")?;
    }
    if template.enabled == Some(true) && template.action.is_none() {
        return Err(ConfigError::Validation(
            "enabled template must have an action".to_string(),
        ));
    }
    Ok(())
}

/// Validate a `TriggerAction` variant's required fields.
///
/// Each variant has different mandatory fields; this function ensures they
/// are present and well-formed.
pub fn validate_trigger_action(action: &TriggerAction, field_prefix: &str) -> ConfigResult<()> {
    match action {
        TriggerAction::SetVariable {
            variable_name,
            value: _,
        } => {
            validate_not_empty(variable_name, &format!("{field_prefix}.variable_name"))?;
        }
        TriggerAction::SendNotification { message } => {
            validate_not_empty(message, &format!("{field_prefix}.message"))?;
        }
        TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id,
            timeout,
            ..
        } => {
            validate_not_empty(
                triggered_workflow_id,
                &format!("{field_prefix}.triggered_workflow_id"),
            )?;
            if let Some(t) = timeout {
                validate_min(*t, 1, &format!("{field_prefix}.timeout"))?;
            }
        }
        TriggerAction::ExecuteScript {
            script_name,
            timeout,
            ..
        } => {
            validate_not_empty(script_name, &format!("{field_prefix}.script_name"))?;
            if let Some(t) = timeout {
                validate_min(*t, 1, &format!("{field_prefix}.timeout"))?;
            }
        }
        TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id, timeout, ..
        } => {
            validate_not_empty(agent_id, &format!("{field_prefix}.agent_id"))?;
            if let Some(t) = timeout {
                validate_min(*t, 1, &format!("{field_prefix}.timeout"))?;
            }
        }
        TriggerAction::SkipNode { node_id } => {
            if let Some(id) = node_id {
                validate_not_empty(id, &format!("{field_prefix}.node_id"))?;
            }
        }
        TriggerAction::SetMessageContext {
            context_id,
            messages,
        } => {
            validate_not_empty(context_id, &format!("{field_prefix}.context_id"))?;
            if messages.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "{field_prefix}.messages cannot be empty"
                )));
            }
        }
        TriggerAction::AppendMessageContext {
            context_id,
            messages,
        } => {
            validate_not_empty(context_id, &format!("{field_prefix}.context_id"))?;
            if messages.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "{field_prefix}.messages cannot be empty"
                )));
            }
        }
        // These variants have no required fields to validate.
        TriggerAction::StopWorkflowExecution {}
        | TriggerAction::PauseWorkflowExecution {}
        | TriggerAction::ResumeWorkflowExecution {} => {}
    }
    Ok(())
}

/// Validate a whole set of trigger templates at load time.
///
/// Runs the single-template validation for every template, then checks the
/// competition scopes shared by the set: the default unique dispatch rejects
/// a second subscriber of one scope, while the explicit best-win dispatch
/// requires every subscriber to declare a distinct priority. Returns the
/// first violation found.
pub fn validate_trigger_set(templates: &[TriggerTemplate]) -> ConfigResult<()> {
    for template in templates {
        validate_trigger_template(template)?;
    }
    if let Some(report) = check_trigger_scopes(&[], templates).into_iter().next() {
        return Err(ConfigError::Validation(report.message));
    }
    Ok(())
}

/// One violated scope: every template in the scope plus the subset that
/// belongs to the incoming batch, plus the message naming the scope, the
/// templates involved, and the way out.
pub struct TriggerScopeReport {
    pub names: Vec<String>,
    pub incoming_names: Vec<String>,
    pub message: String,
}

/// Check the competition scopes of a merged template set and report one
/// entry per violated scope.
///
/// `existing` holds the templates already registered, `incoming` the batch
/// about to be added. Same-name incoming entries replace the existing entry
/// of the same name before grouping. Every violated scope is reported;
/// callers reject the incoming members of a violated scope and warn on
/// violated scopes with no incoming member.
pub fn check_trigger_scopes(
    existing: &[TriggerTemplate],
    incoming: &[TriggerTemplate],
) -> Vec<TriggerScopeReport> {
    use std::collections::HashSet;
    use wf_types::trigger::{check_scope_group, scope_groups, violation_message, TriggerScopeKey};

    let mut merged: Vec<TriggerTemplate> = Vec::new();
    let replaced: HashSet<&str> = incoming.iter().map(|t| t.name.as_str()).collect();
    merged.extend(
        existing
            .iter()
            .filter(|t| !replaced.contains(t.name.as_str()))
            .cloned(),
    );
    merged.extend(incoming.iter().cloned());

    let incoming_names: HashSet<&str> = incoming.iter().map(|t| t.name.as_str()).collect();
    let mut reports = Vec::new();
    for group in scope_groups(&merged) {
        let refs: Vec<&TriggerTemplate> = group.iter().map(|&i| &merged[i]).collect();
        let Some(first) = refs.first() else {
            continue;
        };
        let Some(condition) = first.condition.as_ref() else {
            continue;
        };
        let key = TriggerScopeKey {
            event_type: condition.event_type.clone(),
            event_name: condition.event_name.clone(),
        };
        for violation in check_scope_group(&refs) {
            let names: Vec<String> = refs.iter().map(|t| t.name.clone()).collect();
            let involved: Vec<String> = refs
                .iter()
                .filter(|t| incoming_names.contains(t.name.as_str()))
                .map(|t| t.name.clone())
                .collect();
            reports.push(TriggerScopeReport {
                names,
                incoming_names: involved,
                message: violation_message(&key, &violation),
            });
        }
    }
    reports
}

pub fn transform_trigger_template(
    template: &TriggerTemplate,
    parameters: &HashMap<String, String>,
) -> ConfigResult<TriggerTemplate> {
    let mut cloned = template.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_trigger_template(template: TriggerTemplate) -> TriggerTemplate {
    template
}

#[cfg(test)]
mod tests {
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
            metadata: None,
            created_at: 0,
            updated_at: 0,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }
    }

    #[test]
    fn test_valid_template() {
        let template = make_template();
        assert!(validate_trigger_template(&template).is_ok());
    }

    #[test]
    fn test_empty_name() {
        let mut template = make_template();
        template.name = String::new();
        assert!(validate_trigger_template(&template).is_err());
    }

    #[test]
    fn test_max_triggers_zero_rejected() {
        let mut template = make_template();
        template.max_triggers = Some(0);
        assert!(validate_trigger_template(&template).is_err());
    }

    #[test]
    fn test_max_triggers_positive_accepted() {
        let mut template = make_template();
        template.max_triggers = Some(10);
        assert!(validate_trigger_template(&template).is_ok());
    }

    #[test]
    fn test_enabled_without_action_rejected() {
        let mut template = make_template();
        template.enabled = Some(true);
        template.action = None;
        assert!(validate_trigger_template(&template).is_err());
    }

    #[test]
    fn test_enabled_with_action_accepted() {
        let mut template = make_template();
        template.enabled = Some(true);
        template.action = Some(TriggerAction::StopWorkflowExecution {});
        assert!(validate_trigger_template(&template).is_ok());
    }

    #[test]
    fn test_set_variable_requires_variable_name() {
        let action = TriggerAction::SetVariable {
            variable_name: String::new(),
            value: serde_json::json!(null),
        };
        assert!(validate_trigger_action(&action, "action").is_err());

        let action = TriggerAction::SetVariable {
            variable_name: "x".to_string(),
            value: serde_json::json!(null),
        };
        assert!(validate_trigger_action(&action, "action").is_ok());
    }

    #[test]
    fn test_send_notification_requires_message() {
        let action = TriggerAction::SendNotification {
            message: String::new(),
        };
        assert!(validate_trigger_action(&action, "action").is_err());

        let action = TriggerAction::SendNotification {
            message: "hello".to_string(),
        };
        assert!(validate_trigger_action(&action, "action").is_ok());
    }

    #[test]
    fn test_execute_subworkflow_requires_workflow_id() {
        let action = TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id: String::new(),
            wait_for_completion: None,
            timeout: None,
            input_mapping: None,
            output_mapping: None,
        };
        assert!(validate_trigger_action(&action, "action").is_err());

        let action = TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id: "wf-1".to_string(),
            wait_for_completion: None,
            timeout: None,
            input_mapping: None,
            output_mapping: None,
        };
        assert!(validate_trigger_action(&action, "action").is_ok());
    }

    #[test]
    fn test_execute_subworkflow_timeout_must_be_positive() {
        let action = TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id: "wf-1".to_string(),
            wait_for_completion: None,
            timeout: Some(0),
            input_mapping: None,
            output_mapping: None,
        };
        assert!(validate_trigger_action(&action, "action").is_err());

        let action = TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id: "wf-1".to_string(),
            wait_for_completion: None,
            timeout: Some(1000),
            input_mapping: None,
            output_mapping: None,
        };
        assert!(validate_trigger_action(&action, "action").is_ok());
    }

    #[test]
    fn test_execute_script_requires_script_name() {
        let action = TriggerAction::ExecuteScript {
            script_name: String::new(),
            parameters: None,
            timeout: None,
            ignore_error: None,
        };
        assert!(validate_trigger_action(&action, "action").is_err());

        let action = TriggerAction::ExecuteScript {
            script_name: "my-script".to_string(),
            parameters: None,
            timeout: None,
            ignore_error: None,
        };
        assert!(validate_trigger_action(&action, "action").is_ok());
    }

    #[test]
    fn test_execute_agent_requires_agent_id() {
        let action = TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: String::new(),
            prompt: None,
            model: None,
            result_variable: None,
            wait_for_completion: None,
            timeout: None,
            input_mode: None,
            writeback: None,
        };
        assert!(validate_trigger_action(&action, "action").is_err());

        let action = TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: "child-agent".to_string(),
            prompt: None,
            model: None,
            result_variable: None,
            wait_for_completion: None,
            timeout: None,
            input_mode: None,
            writeback: None,
        };
        assert!(validate_trigger_action(&action, "action").is_ok());
    }

    #[test]
    fn test_stop_pause_resume_need_no_fields() {
        assert!(validate_trigger_action(&TriggerAction::StopWorkflowExecution {}, "a").is_ok());
        assert!(validate_trigger_action(&TriggerAction::PauseWorkflowExecution {}, "a").is_ok());
        assert!(validate_trigger_action(&TriggerAction::ResumeWorkflowExecution {}, "a").is_ok());
    }

    #[test]
    fn test_skip_node_empty_id_rejected() {
        let action = TriggerAction::SkipNode {
            node_id: Some(String::new()),
        };
        assert!(validate_trigger_action(&action, "action").is_err());

        let action = TriggerAction::SkipNode { node_id: None };
        assert!(validate_trigger_action(&action, "action").is_ok());
    }

    #[test]
    fn test_set_message_context_requires_context_id_and_messages() {
        let action = TriggerAction::SetMessageContext {
            context_id: String::new(),
            messages: vec![],
        };
        assert!(validate_trigger_action(&action, "action").is_err());

        let action = TriggerAction::SetMessageContext {
            context_id: "ctx".to_string(),
            messages: vec![],
        };
        assert!(validate_trigger_action(&action, "action").is_err());

        let action = TriggerAction::SetMessageContext {
            context_id: "ctx".to_string(),
            messages: vec![wf_types::message::Message {
                id: wf_types::Id::new(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text("hi".to_string()),
                timestamp: 0,
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
        };
        assert!(validate_trigger_action(&action, "action").is_ok());
    }

    #[test]
    fn test_append_message_context_requires_messages() {
        let action = TriggerAction::AppendMessageContext {
            context_id: "ctx".to_string(),
            messages: vec![],
        };
        assert!(validate_trigger_action(&action, "action").is_err());
    }

    #[test]
    fn test_compression_signal_subscription_rejected() {
        use wf_types::trigger::TriggerCondition;
        let mut template = make_template();
        template.condition = Some(TriggerCondition {
            event_type: wf_types::hook::CONTEXT_COMPRESSION_SIGNAL.to_string(),
            event_name: None,
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        });
        assert!(validate_trigger_template(&template).is_err());

        template.condition = Some(TriggerCondition {
            event_type: "HOOK_TRIGGERED".to_string(),
            event_name: None,
            condition: None,
            metadata: Some(std::collections::HashMap::from([(
                "hook_type".to_string(),
                serde_json::json!(wf_types::hook::CONTEXT_COMPRESSION_SIGNAL),
            )])),
            metadata_exists: None,
            execution_prefix: None,
        });
        assert!(validate_trigger_template(&template).is_err());
    }

    #[test]
    fn test_before_hook_subscription_rejected() {
        use wf_types::trigger::TriggerCondition;
        let mut template = make_template();
        template.condition = Some(TriggerCondition {
            event_type: "HOOK_TRIGGERED".to_string(),
            event_name: None,
            condition: None,
            metadata: Some(std::collections::HashMap::from([(
                "hook_type".to_string(),
                serde_json::json!("BEFORE_TOOL_CALL"),
            )])),
            metadata_exists: None,
            execution_prefix: None,
        });
        let err = validate_trigger_template(&template).expect_err("before hook must be rejected");
        let message = err.to_string();
        assert!(message.contains("BEFORE_*"), "{message}");
        assert!(message.contains("approval"), "{message}");
    }

    #[test]
    fn test_after_hook_subscription_accepted() {
        use wf_types::trigger::TriggerCondition;
        let mut template = make_template();
        template.condition = Some(TriggerCondition {
            event_type: "HOOK_TRIGGERED".to_string(),
            event_name: None,
            condition: None,
            metadata: Some(std::collections::HashMap::from([(
                "hook_type".to_string(),
                serde_json::json!("AFTER_TOOL_CALL"),
            )])),
            metadata_exists: None,
            execution_prefix: None,
        });
        template.action = Some(TriggerAction::StopWorkflowExecution {});
        assert!(validate_trigger_template(&template).is_ok());
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
    fn test_set_single_subscriber_passes() {
        let templates = vec![scoped_template("a", "NODE_COMPLETED", None, None, None)];
        assert!(validate_trigger_set(&templates).is_ok());
    }

    #[test]
    fn test_set_unique_second_subscriber_rejected() {
        let templates = vec![
            scoped_template("a", "NODE_COMPLETED", None, None, None),
            scoped_template("b", "NODE_COMPLETED", None, None, None),
        ];
        let err = validate_trigger_set(&templates).expect_err("unique scope must reject");
        assert!(err.to_string().contains("unique"), "{}", err.to_string());
    }

    #[test]
    fn test_set_best_win_requires_distinct_priorities() {
        use wf_types::trigger::TriggerDispatchMode;
        let missing = vec![
            scoped_template(
                "a",
                "NODE_COMPLETED",
                None,
                None,
                Some(TriggerDispatchMode::BestWin),
            ),
            scoped_template(
                "b",
                "NODE_COMPLETED",
                None,
                Some(1),
                Some(TriggerDispatchMode::BestWin),
            ),
        ];
        assert!(validate_trigger_set(&missing).is_err());

        let duplicate = vec![
            scoped_template(
                "a",
                "NODE_COMPLETED",
                None,
                Some(1),
                Some(TriggerDispatchMode::BestWin),
            ),
            scoped_template(
                "b",
                "NODE_COMPLETED",
                None,
                Some(1),
                Some(TriggerDispatchMode::BestWin),
            ),
        ];
        assert!(validate_trigger_set(&duplicate).is_err());

        let distinct = vec![
            scoped_template(
                "a",
                "NODE_COMPLETED",
                None,
                Some(2),
                Some(TriggerDispatchMode::BestWin),
            ),
            scoped_template(
                "b",
                "NODE_COMPLETED",
                None,
                Some(1),
                Some(TriggerDispatchMode::BestWin),
            ),
        ];
        assert!(validate_trigger_set(&distinct).is_ok());
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
}
