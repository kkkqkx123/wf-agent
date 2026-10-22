use super::*;

/// Validate a `TriggerAction` variant's required fields.
///
/// Shared field validation used by both execution contexts; context support
/// itself is checked by [`validate_trigger_action_for_context`].
pub fn validate_trigger_action(action: &TriggerAction, field_prefix: &str) -> ConfigResult<()> {
    match action {
        TriggerAction::SetVariable {
            variable_name,
            value: _,
        } => {
            validate_not_empty(variable_name, &format!("{field_prefix}.variable_name"))?;
            if !crate::processor::node_config::is_valid_identifier(variable_name) {
                return Err(ConfigError::Validation(format!(
                    "{field_prefix}.variable_name '{}' must start with a letter or '_' and contain only letters, digits or '_'",
                    variable_name
                )));
            }
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
            agent_id,
            timeout,
            checkpoint_message_interval,
            result_variable,
            ..
        } => {
            validate_not_empty(agent_id, &format!("{field_prefix}.agent_id"))?;
            if let Some(t) = timeout {
                validate_min(*t, 1, &format!("{field_prefix}.timeout"))?;
            }
            if let Some(n) = checkpoint_message_interval {
                validate_min(
                    *n,
                    1,
                    &format!("{field_prefix}.checkpoint_message_interval"),
                )?;
            }
            if let Some(name) = result_variable {
                if !crate::processor::node_config::is_valid_identifier(name) {
                    return Err(ConfigError::Validation(format!(
                        "{field_prefix}.result_variable '{}' must start with a letter or '_' and contain only letters, digits or '_'",
                        name
                    )));
                }
            }
        }
        TriggerAction::ExecuteWorkflow {
            workflow_id,
            timeout,
            ..
        } => {
            validate_not_empty(workflow_id, &format!("{field_prefix}.workflow_id"))?;
            if let Some(t) = timeout {
                validate_min(*t, 1, &format!("{field_prefix}.timeout"))?;
            }
        }
        TriggerAction::ExecuteAgent {
            agent_id,
            timeout,
            checkpoint_message_interval,
            ..
        } => {
            validate_not_empty(agent_id, &format!("{field_prefix}.agent_id"))?;
            if let Some(t) = timeout {
                validate_min(*t, 1, &format!("{field_prefix}.timeout"))?;
            }
            if let Some(n) = checkpoint_message_interval {
                validate_min(
                    *n,
                    1,
                    &format!("{field_prefix}.checkpoint_message_interval"),
                )?;
            }
        }
        TriggerAction::SkipNode { node_id } => {
            if let Some(id) = node_id {
                validate_not_empty(id, &format!("{field_prefix}.node_id"))?;
                if !crate::processor::node_config::is_valid_identifier(id) {
                    return Err(ConfigError::Validation(format!(
                        "{field_prefix}.node_id '{}' must start with a letter or '_' and contain only letters, digits or '_'",
                        id
                    )));
                }
            }
        }
        TriggerAction::SetMessageContext {
            context_id,
            messages,
        } => {
            validate_not_empty(context_id, &format!("{field_prefix}.context_id"))?;
            if !crate::processor::node_config::is_valid_identifier(context_id) {
                return Err(ConfigError::Validation(format!(
                    "{field_prefix}.context_id '{}' must start with a letter or '_' and contain only letters, digits or '_'",
                    context_id
                )));
            }
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
            if !crate::processor::node_config::is_valid_identifier(context_id) {
                return Err(ConfigError::Validation(format!(
                    "{field_prefix}.context_id '{}' must start with a letter or '_' and contain only letters, digits or '_'",
                    context_id
                )));
            }
            if messages.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "{field_prefix}.messages cannot be empty"
                )));
            }
        }
        TriggerAction::FilterMessageContext {
            context_id,
            role,
            custom_filter,
            ..
        } => {
            validate_not_empty(context_id, &format!("{field_prefix}.context_id"))?;
            if !crate::processor::node_config::is_valid_identifier(context_id) {
                return Err(ConfigError::Validation(format!(
                    "{field_prefix}.context_id '{}' must start with a letter or '_' and contain only letters, digits or '_'",
                    context_id
                )));
            }
            if role.is_none() && custom_filter.as_ref().is_none_or(|s| s.trim().is_empty()) {
                return Err(ConfigError::Validation(format!(
                    "{field_prefix} needs at least one of role or custom_filter"
                )));
            }
        }
        // These variants have no required fields to validate. The builtin
        // compression action never reaches here from user configuration
        // (`validate_trigger_template` rejects it as reserved); the arm
        // exists so the match stays exhaustive.
        TriggerAction::StopWorkflowExecution {}
        | TriggerAction::PauseWorkflowExecution {}
        | TriggerAction::ResumeWorkflowExecution {}
        | TriggerAction::ExecuteContextCompression {} => {}
    }
    Ok(())
}

/// Validate a `TriggerAction` for one execution context.
///
/// Shared field validation runs first; then the authoritative support
/// matrix (`TriggerAction::supported_in`) decides. The event-listener
/// context (trigger templates) supports every action; the message-node
/// context keeps rejecting the nested-agent action with the unified error
/// naming the alternative path.
pub fn validate_trigger_action_for_context(
    action: &TriggerAction,
    context: wf_types::trigger::TriggerExecutionContext,
    field_prefix: &str,
) -> ConfigResult<()> {
    validate_trigger_action(action, field_prefix)?;
    if let Some(message) = action.rejection_message(context) {
        return Err(ConfigError::Validation(format!(
            "{field_prefix}: {message}"
        )));
    }
    Ok(())
}
