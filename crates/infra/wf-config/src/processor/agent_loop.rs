use std::collections::HashSet;
use std::str::FromStr;

use crate::error::{ConfigError, ConfigResult};
use crate::validator::{validate_hook_type, validate_required};

use wf_types::agent::definition::AgentDefinition;
use wf_types::agent_execution::runtime_config::AgentRuntimeConfig;
use wf_types::llm::ToolCallFormat;

pub fn validate_agent_definition(definition: &AgentDefinition) -> ConfigResult<()> {
    validate_required(&definition.id, "id")?;
    validate_required(&definition.name, "name")?;
    if let Some(format) = definition
        .config
        .as_ref()
        .and_then(|c| c.tool_call_format.as_ref())
    {
        ToolCallFormat::from_str(format).map_err(ConfigError::Validation)?;
    }
    if let Some(hooks) = definition.config.as_ref().and_then(|c| c.hooks.as_ref()) {
        for hook in hooks {
            let hook_type = serde_json::to_value(&hook.hook_type)
                .ok()
                .and_then(|v| v.as_str().map(ToString::to_string))
                .unwrap_or_default();
            validate_hook_type(&hook_type, "config.hooks")?;
        }
    }
    Ok(())
}

/// Profile-aware variant: the referenced `profile_id` must exist in the
/// registered profile set (assembly-time reference closure).
pub fn validate_agent_definition_with_profiles(
    definition: &AgentDefinition,
    known_profile_ids: &HashSet<String>,
) -> ConfigResult<()> {
    validate_agent_definition(definition)?;
    if let Some(profile_id) = definition
        .config
        .as_ref()
        .and_then(|c| c.profile_id.as_ref())
    {
        if !known_profile_ids.contains(profile_id) {
            return Err(ConfigError::Validation(format!(
                "agent '{}' references profile '{}' which is not registered",
                definition.id, profile_id
            )));
        }
    }
    Ok(())
}

pub fn transform_to_agent_loop_config(definition: &AgentDefinition) -> AgentRuntimeConfig {
    let config = definition.config.as_ref();

    AgentRuntimeConfig {
        profile_id: config.and_then(|c| c.profile_id.clone()),
        system_prompt: config.and_then(|c| c.system_prompt.clone()),
        max_iterations: config.and_then(|c| c.max_iterations),
        max_execution_time: None,
        max_retries: None,
        execution_timeout: None,
        initial_messages: config.and_then(|c| c.initial_messages.clone()),
        available_tools: config
            .and_then(|c| c.available_tools.as_ref().map(|t| t.available.clone())),
        discoverable_tool_names: config.and_then(|c| {
            c.available_tools
                .as_ref()
                .and_then(|t| t.discoverable.clone())
        }),
        hidden_tool_names: config
            .and_then(|c| c.available_tools.as_ref().and_then(|t| t.hidden.clone())),
        stream: config.and_then(|c| c.stream),
        tool_call_format: None,
        on_failure: None,
        fallback_output: None,
        hooks: config.and_then(|c| c.hooks.clone()),
        dynamic_context_config: config.and_then(|c| {
            c.dynamic_context.as_ref().map(|d| {
                serde_json::to_value(d)
                    .ok()
                    .and_then(|v| {
                        serde_json::from_value::<
                                std::collections::HashMap<String, serde_json::Value>,
                            >(v)
                            .ok()
                    })
                    .unwrap_or_default()
            })
        }),
        checkpoint_config: config.and_then(|c| {
            c.checkpoint.as_ref().map(|cp| {
                serde_json::to_value(cp)
                    .ok()
                    .and_then(|v| {
                        serde_json::from_value::<
                                std::collections::HashMap<String, serde_json::Value>,
                            >(v)
                            .ok()
                    })
                    .unwrap_or_default()
            })
        }),
    }
}

pub fn export_agent_loop_config(definition: AgentDefinition) -> AgentDefinition {
    definition
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_definition() -> AgentDefinition {
        AgentDefinition {
            id: "agent-1".to_string(),
            name: "Code Agent".to_string(),
            description: None,
            version: None,
            config: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn test_valid_definition() {
        let def = make_definition();
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_empty_id() {
        let mut def = make_definition();
        def.id = String::new();
        assert!(validate_agent_definition(&def).is_err());
    }

    #[test]
    fn test_invalid_tool_call_format_rejected() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            profile_id: None,
            system_prompt: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            max_iterations: None,
            initial_messages: None,
            available_tools: None,
            stream: None,
            tool_call_format: Some("yaml".to_string()),
            hooks: None,
            dynamic_context: None,
            checkpoint: None,
            violation_policy: None,
        });
        assert!(validate_agent_definition(&def).is_err());
    }

    #[test]
    fn test_valid_tool_call_format_accepted() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            profile_id: None,
            system_prompt: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            max_iterations: None,
            initial_messages: None,
            available_tools: None,
            stream: None,
            tool_call_format: Some("json_wrapped".to_string()),
            hooks: None,
            dynamic_context: None,
            checkpoint: None,
            violation_policy: None,
        });
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_transform_to_agent_loop_config() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            profile_id: Some("profile-1".to_string()),
            system_prompt: Some("You are a code assistant".to_string()),
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            max_iterations: Some(10),
            initial_messages: None,
            available_tools: None,
            stream: Some(true),
            tool_call_format: None,
            hooks: None,
            dynamic_context: None,
            checkpoint: None,
            violation_policy: None,
        });

        let runtime = transform_to_agent_loop_config(&def);
        assert_eq!(runtime.profile_id, Some("profile-1".to_string()));
        assert_eq!(runtime.max_iterations, Some(10));
        assert_eq!(runtime.stream, Some(true));
    }

    #[test]
    fn test_export_agent_loop_config() {
        let def = make_definition();
        let exported = export_agent_loop_config(def.clone());
        assert_eq!(exported.id, def.id);
        assert_eq!(exported.name, def.name);
    }

    #[test]
    fn test_profile_aware_definition_validation() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            profile_id: Some("profile-1".to_string()),
            system_prompt: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            max_iterations: None,
            initial_messages: None,
            available_tools: None,
            stream: None,
            tool_call_format: None,
            hooks: None,
            dynamic_context: None,
            checkpoint: None,
            violation_policy: None,
        });

        let mut known = HashSet::new();
        known.insert("profile-1".to_string());
        assert!(validate_agent_definition_with_profiles(&def, &known).is_ok());

        assert!(validate_agent_definition_with_profiles(&def, &HashSet::new()).is_err());

        // Without a registry the plain validator stays lenient about the
        // profile reference (checked at assembly time instead).
        assert!(validate_agent_definition(&def).is_ok());
    }
}
