//! Trigger template validation using the shared [`ValidationContext`].

use crate::infra::validation::{ValidationContext, ValidationError, ValidationResult};

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
            TriggerAction::ExecuteScript {
                script_name, ..
            } => {
                if !self.ctx.script_names.contains(script_name) {
                    errors.push(ValidationError::new(
                        "script_name",
                        format!("Script '{}' not registered", script_name),
                    ));
                }
            }
            TriggerAction::ExecuteTriggeredAgentExecution {
                model: Some(profile),
                ..
            } => {
                if let Some(e) = crate::infra::validation::validate_profile_reference(
                    profile,
                    self.ctx,
                ) {
                    errors.push(e);
                }
            }
            _ => {}
        }

        errors
    }
}
