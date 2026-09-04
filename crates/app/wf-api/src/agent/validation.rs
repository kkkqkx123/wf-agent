//! Agent definition validation using the shared [`ValidationContext`].

use wf_types::{
    validate_hook_type, validate_profile_reference, validate_tool_list, ValidationContext,
    ValidationError, ValidationResult,
};

/// Agent-specific validator that uses the shared [`ValidationContext`].
pub struct AgentValidator<'a> {
    ctx: &'a ValidationContext,
}

impl<'a> AgentValidator<'a> {
    pub fn new(ctx: &'a ValidationContext) -> Self {
        Self { ctx }
    }

    /// Validate an agent definition: shape + profile + tools + hooks.
    pub fn validate(&self, definition: &wf_types::agent::AgentDefinition) -> ValidationResult {
        let mut result = ValidationResult::default();

        // 1. Shape validation.
        if let Err(e) = wf_config::processor::agent_loop::validate_agent_definition(definition) {
            result.push_error(ValidationError::new("definition", e.to_string()));
        }

        // 2. Profile validation (using shared validator).
        if let Some(profile_id) = definition
            .config
            .as_ref()
            .and_then(|c| c.profile_id.as_ref())
        {
            if let Some(e) = validate_profile_reference(profile_id, self.ctx) {
                result.push_error(e);
            }
        }

        // 3. Tool list validation (using shared validator).
        let tool_names = extract_tool_names(definition);
        result.extend_errors(validate_tool_list(&tool_names, self.ctx));

        // 4. Hook type validation (using shared validator).
        if let Some(config) = &definition.config {
            if let Some(hooks) = &config.hooks {
                for hook in hooks {
                    let hook_type_str = serde_json::to_string(&hook.hook_type)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    if let Some(e) = validate_hook_type(&hook_type_str) {
                        result.push_warning(e);
                    }
                }
            }
        }

        // 5. Tool call format protocol compatibility check: agent config
        // format must be compatible with the referenced profile format.
        // Uses the shared engine validator so API-time and runtime rules match.
        if let Some(config) = &definition.config {
            if let Some(tool_call_format) = config.tool_call_format.as_ref() {
                if let Some(format_config) =
                    wf_types::llm::ToolCallFormatConfig::from_format_str(tool_call_format)
                {
                    if let Some(profile_id) = config.profile_id.as_ref() {
                        if let Some(profile_format) = self.ctx.profile_formats.get(profile_id) {
                            let protocol_result =
                                wf_agent::validation::AgentLoopValidator::validate_tool_call_protocol(
                                    Some(&format_config),
                                    Some(profile_format.clone()),
                                );
                            for error in &protocol_result.errors {
                                result.push_error(ValidationError::new(
                                    "tool_call_format",
                                    error.clone(),
                                ));
                            }
                            for warning in &protocol_result.warnings {
                                result.push_warning(ValidationError::new(
                                    "tool_call_format",
                                    warning.clone(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        result
    }
}

/// Extract all tool names from an agent definition's available_tools config.
fn extract_tool_names(definition: &wf_types::agent::AgentDefinition) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(config) = &definition.config {
        if let Some(tools) = &config.available_tools {
            names.extend(tools.available.clone());
            if let Some(initial) = &tools.initial {
                names.extend(initial.clone());
            }
            if let Some(discoverable) = &tools.discoverable {
                names.extend(discoverable.clone());
            }
            if let Some(hidden) = &tools.hidden {
                names.extend(hidden.clone());
            }
        }
    }
    names
}
