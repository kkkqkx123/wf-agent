//! Agent definition validation using the shared [`ValidationContext`].

use crate::infra::validation::{ValidationContext, ValidationError, ValidationResult};

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
        if let Err(e) =
            wf_config::processor::agent_loop::validate_agent_definition(definition)
        {
            result.push_error(ValidationError::new("definition", e.to_string()));
        }

        // 2. Profile validation (using shared validator).
        if let Some(profile_id) = definition
            .config
            .as_ref()
            .and_then(|c| c.profile_id.as_ref())
        {
            if let Some(e) =
                crate::infra::validation::validate_profile_reference(profile_id, self.ctx)
            {
                result.push_error(e);
            }
        }

        // 3. Tool list validation (using shared validator).
        let tool_names = extract_tool_names(definition);
        result.extend_errors(crate::infra::validation::validate_tool_list(
            &tool_names,
            self.ctx,
        ));

        // 4. Hook type validation (using shared validator).
        if let Some(config) = &definition.config {
            if let Some(hooks) = &config.hooks {
                for hook in hooks {
                    let hook_type_str = serde_json::to_string(&hook.hook_type)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    if let Some(e) =
                        crate::infra::validation::validate_hook_type(&hook_type_str)
                    {
                        result.push_warning(e);
                    }
                }
            }
        }

        // 5. Validate with known profiles (existing logic).
        let known_profiles: std::collections::HashSet<String> =
            self.ctx.profile_ids.iter().cloned().collect();
        if let Err(e) =
            wf_config::processor::agent_loop::validate_agent_definition_with_profiles(
                definition,
                &known_profiles,
            )
        {
            result.push_error(ValidationError::new("definition", e.to_string()));
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
