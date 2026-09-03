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
            script_name, timeout, ..
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
