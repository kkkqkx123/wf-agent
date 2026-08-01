use crate::error::ConfigResult;
use crate::validator::validate_required;

use wf_types::agent::definition::AgentDefinition;
use wf_types::agent_execution::runtime_config::AgentRuntimeConfig;

pub fn validate_agent_definition(definition: &AgentDefinition) -> ConfigResult<()> {
    validate_required(&definition.id, "id")?;
    validate_required(&definition.name, "name")?;
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
        stream: config.and_then(|c| c.stream),
        tool_call_format: None,
        on_failure: None,
        fallback_output: None,
        hooks: config.and_then(|c| c.hooks.clone()),
        triggers: config.and_then(|c| c.triggers.clone()),
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
            triggers: None,
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
}
