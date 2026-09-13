//! Trigger template validation using the shared [`ValidationContext`].

use wf_types::{validate_profile_reference, ValidationContext, ValidationError, ValidationResult};

/// Trigger-specific validator that uses the shared [`ValidationContext`].
pub struct TriggerValidator<'a> {
    ctx: &'a ValidationContext,
}

impl<'a> TriggerValidator<'a> {
    pub fn new(ctx: &'a ValidationContext) -> Self {
        Self { ctx }
    }

    /// Validate a trigger template: shape + action references.
    pub fn validate(&self, template: &wf_types::trigger::TriggerTemplate) -> ValidationResult {
        let mut result = ValidationResult::default();

        // 1. Template structure validation.
        if let Err(e) = wf_config::processor::trigger::validate_trigger_template(template) {
            result.push_error(ValidationError::new("template", e.to_string()));
        }

        // 2. Action reference validation.
        if let Some(action) = &template.action {
            result.extend_errors(self.validate_action_references(action));
        }

        // 3. Competition scope against the already-known set: the incoming
        // template must not collide with an existing subscriber unless the
        // scope is an explicit best-win with distinct priorities.
        let reports = wf_config::processor::trigger::check_trigger_scopes(
            &self.ctx.trigger_templates,
            std::slice::from_ref(template),
        );
        for report in reports {
            if report.incoming_names.iter().any(|n| n == &template.name) {
                result.push_error(ValidationError::new("competition", report.message));
            }
        }

        result
    }

    /// Validate a whole incoming set against the context set through the
    /// unified registration entry (single shape + merged scopes). Production
    /// registration paths should prefer this over looping [`Self::validate`].
    pub fn validate_set(
        &self,
        incoming: &[wf_types::trigger::TriggerTemplate],
    ) -> ValidationResult {
        let mut result = ValidationResult::default();
        let (validated, reports) = wf_config::processor::trigger::validate_trigger_registration(
            &self.ctx.trigger_templates,
            incoming,
        );
        if let Err(e) = validated {
            result.push_error(ValidationError::new("template", e.to_string()));
            return result;
        }
        for report in reports {
            if report.incoming_names.is_empty() {
                result.push_warning(ValidationError::new("competition", report.message));
            } else {
                result.push_error(ValidationError::new("competition", report.message));
            }
        }
        for template in incoming {
            if let Some(action) = &template.action {
                result.extend_errors(self.validate_action_references(action));
            }
        }
        result
    }

    fn validate_action_references(
        &self,
        action: &wf_types::trigger::TriggerAction,
    ) -> Vec<ValidationError> {
        use wf_types::trigger::TriggerAction;
        let mut errors = Vec::new();

        match action {
            TriggerAction::ExecuteTriggeredSubworkflow {
                triggered_workflow_id,
                ..
            } => {
                if !self.ctx.workflow_ids.contains(triggered_workflow_id) {
                    errors.push(ValidationError::new(
                        "triggered_workflow_id",
                        format!("Workflow '{}' not registered", triggered_workflow_id),
                    ));
                }
            }
            TriggerAction::ExecuteScript { script_name, .. } => {
                if !self.ctx.script_names.contains(script_name) {
                    errors.push(ValidationError::new(
                        "script_name",
                        format!("Script '{}' not registered", script_name),
                    ));
                }
            }
            TriggerAction::ExecuteTriggeredAgentExecution {
                agent_id, model, ..
            } => {
                if agent_id.trim().is_empty() {
                    errors.push(ValidationError::new(
                        "agent_id",
                        "Agent id must not be empty for ExecuteTriggeredAgentExecution",
                    ));
                }
                // When an agent registry is added to ValidationContext,
                // validate agent_id existence here:
                // if !self.ctx.agent_ids.contains(agent_id) {
                //     errors.push(ValidationError::new(
                //         "agent_id",
                //         format!("Agent '{}' not registered", agent_id),
                //     ));
                // }
                if let Some(profile) = model {
                    if let Some(e) = validate_profile_reference(profile, self.ctx) {
                        errors.push(e);
                    }
                }
            }
            TriggerAction::ExecuteWorkflow { workflow_id, .. } => {
                if workflow_id.trim().is_empty() {
                    errors.push(ValidationError::new(
                        "workflow_id",
                        "Workflow id must not be empty for ExecuteWorkflow",
                    ));
                } else if !self.ctx.workflow_ids.contains(workflow_id) {
                    errors.push(ValidationError::new(
                        "workflow_id",
                        format!("Workflow '{}' not registered", workflow_id),
                    ));
                }
            }
            TriggerAction::ExecuteAgent {
                agent_id, model, ..
            } => {
                if agent_id.trim().is_empty() {
                    errors.push(ValidationError::new(
                        "agent_id",
                        "Agent id must not be empty for ExecuteAgent",
                    ));
                }
                if let Some(profile) = model {
                    if let Some(e) = validate_profile_reference(profile, self.ctx) {
                        errors.push(e);
                    }
                }
            }
            _ => {}
        }

        errors
    }
}
