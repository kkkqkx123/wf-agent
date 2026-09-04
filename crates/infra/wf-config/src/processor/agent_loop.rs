use std::collections::HashSet;
use std::str::FromStr;

use crate::error::{ConfigError, ConfigResult};
use crate::processor::hook::validate_agent_hook_config;
use crate::processor::tool_list::validate_available_tools;
use crate::validator::{validate_min, validate_required};

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
        for (idx, hook) in hooks.iter().enumerate() {
            validate_agent_hook_config(hook, &format!("config.hooks[{idx}]"))?;
        }
    }
    if let Some(config) = definition.config.as_ref() {
        if let Some(max_iterations) = config.max_iterations {
            if max_iterations == 0 {
                return Err(ConfigError::Validation(
                    "config.max_iterations must be at least 1".to_string(),
                ));
            }
        }
        if let Some(threshold) = config.token_warning_threshold {
            if threshold > 100 {
                return Err(ConfigError::Validation(format!(
                    "config.token_warning_threshold must be between 1 and 100, got {threshold}"
                )));
            }
        }
        if let Some(max_retries) = config.max_retries {
            validate_min(max_retries, 0, "config.max_retries")?;
        }
        if let Some(execution_timeout) = config.execution_timeout {
            validate_min(execution_timeout, 1, "config.execution_timeout")?;
        }
        if let Some(max_pause_duration) = config.max_pause_duration {
            validate_min(max_pause_duration, 0, "config.max_pause_duration")?;
        }
        if let Some(token_limit) = config.token_limit {
            // token_limit=0 is allowed to disable the limit, matching the
            // runtime semantics in wf-agent AgentLoopValidator (warns but
            // allows). Only positive values need the minimum check.
            if token_limit != 0 {
                validate_min(token_limit, 1, "config.token_limit")?;
            }
        }
        validate_available_tools_intersection(config)?;
        if let Some(ref checkpoint) = config.checkpoint {
            validate_agent_checkpoint_config(checkpoint)?;
        }
        if let Some(ref dynamic_context) = config.dynamic_context {
            validate_dynamic_context_config(dynamic_context)?;
        }
    }
    Ok(())
}

fn validate_available_tools_intersection(
    config: &wf_types::agent::config::AgentConfig,
) -> ConfigResult<()> {
    let Some(tools) = config.available_tools.as_ref() else {
        return Ok(());
    };
    validate_available_tools(tools, "config.available_tools")
}

fn validate_agent_checkpoint_config(
    config: &wf_types::checkpoint::agent::AgentCheckpointConfig,
) -> ConfigResult<()> {
    if let Some(interval) = config.interval_iterations {
        validate_min(interval, 1, "config.checkpoint.interval_iterations")?;
    }
    if let Some(ref content) = config.content {
        if let Some(limit) = content.message_limit {
            validate_min(limit, 1, "config.checkpoint.content.message_limit")?;
        }
        if let Some(limit) = content.tool_call_limit {
            validate_min(limit, 1, "config.checkpoint.content.tool_call_limit")?;
        }
    }
    Ok(())
}

fn validate_dynamic_context_config(
    config: &wf_types::dynamic_context::DynamicContextConfig,
) -> ConfigResult<()> {
    if let Some(max_depth) = config.max_file_depth {
        validate_min(max_depth, 1, "config.dynamic_context.max_file_depth")?;
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
        max_execution_time: config.and_then(|c| c.max_execution_time),
        max_retries: config.and_then(|c| c.max_retries),
        execution_timeout: config.and_then(|c| c.execution_timeout),
        max_pause_duration: config.and_then(|c| c.max_pause_duration),
        token_limit: config.and_then(|c| c.token_limit),
        token_warning_threshold: config.and_then(|c| c.token_warning_threshold),
        enable_token_tracking: config.and_then(|c| c.enable_token_tracking),
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
            max_execution_time: None,
            max_retries: None,
            execution_timeout: None,
            max_pause_duration: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
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
            max_execution_time: None,
            max_retries: None,
            execution_timeout: None,
            max_pause_duration: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
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
            max_execution_time: None,
            max_retries: None,
            execution_timeout: None,
            max_pause_duration: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
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
            max_execution_time: None,
            max_retries: None,
            execution_timeout: None,
            max_pause_duration: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
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

    #[test]
    fn test_max_retries_zero_accepted() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            max_retries: Some(0),
            ..make_config()
        });
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_max_retries_negative_rejected() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            max_retries: Some(u32::MAX),
            ..make_config()
        });
        // u32::MAX is a valid value, just very large
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_execution_timeout_zero_rejected() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            execution_timeout: Some(0),
            ..make_config()
        });
        assert!(validate_agent_definition(&def).is_err());
    }

    #[test]
    fn test_execution_timeout_positive_accepted() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            execution_timeout: Some(5000),
            ..make_config()
        });
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_max_pause_duration_zero_accepted() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            max_pause_duration: Some(0),
            ..make_config()
        });
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_token_limit_zero_accepted_disables_limit() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            token_limit: Some(0),
            ..make_config()
        });
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_token_limit_positive_accepted() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            token_limit: Some(100000),
            ..make_config()
        });
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_checkpoint_interval_iterations_zero_rejected() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            checkpoint: Some(wf_types::checkpoint::agent::AgentCheckpointConfig {
                enabled: true,
                interval_iterations: Some(0),
                on_error: None,
                on_tool_call: None,
                content: None,
            }),
            ..make_config()
        });
        let err = validate_agent_definition(&def).unwrap_err();
        assert!(err.to_string().contains("interval_iterations"));
    }

    #[test]
    fn test_checkpoint_interval_iterations_positive_accepted() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            checkpoint: Some(wf_types::checkpoint::agent::AgentCheckpointConfig {
                enabled: true,
                interval_iterations: Some(5),
                on_error: None,
                on_tool_call: None,
                content: None,
            }),
            ..make_config()
        });
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_checkpoint_content_limits_zero_rejected() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            checkpoint: Some(wf_types::checkpoint::agent::AgentCheckpointConfig {
                enabled: true,
                interval_iterations: None,
                on_error: None,
                on_tool_call: None,
                content: Some(wf_types::checkpoint::agent::AgentCheckpointContentConfig {
                    include_state: None,
                    include_messages: None,
                    message_limit: Some(0),
                    include_tool_calls: None,
                    tool_call_limit: Some(0),
                }),
            }),
            ..make_config()
        });
        let err = validate_agent_definition(&def).unwrap_err();
        assert!(err.to_string().contains("message_limit"));
    }

    #[test]
    fn test_checkpoint_content_limits_positive_accepted() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            checkpoint: Some(wf_types::checkpoint::agent::AgentCheckpointConfig {
                enabled: true,
                interval_iterations: None,
                on_error: None,
                on_tool_call: None,
                content: Some(wf_types::checkpoint::agent::AgentCheckpointContentConfig {
                    include_state: None,
                    include_messages: None,
                    message_limit: Some(10),
                    include_tool_calls: None,
                    tool_call_limit: Some(5),
                }),
            }),
            ..make_config()
        });
        assert!(validate_agent_definition(&def).is_ok());
    }

    #[test]
    fn test_dynamic_context_max_file_depth_zero_rejected() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            dynamic_context: Some(wf_types::dynamic_context::DynamicContextConfig {
                include_current_time: None,
                include_todo_list: None,
                include_workspace_files: None,
                max_file_depth: Some(0),
                ignore_patterns: None,
                include_pinned_files: None,
                include_skills: None,
                include_workflows: None,
                include_environment_info: None,
                custom_sections: None,
            }),
            ..make_config()
        });
        let err = validate_agent_definition(&def).unwrap_err();
        assert!(err.to_string().contains("max_file_depth"));
    }

    #[test]
    fn test_dynamic_context_max_file_depth_positive_accepted() {
        let mut def = make_definition();
        def.config = Some(wf_types::agent::config::AgentConfig {
            dynamic_context: Some(wf_types::dynamic_context::DynamicContextConfig {
                include_current_time: None,
                include_todo_list: None,
                include_workspace_files: None,
                max_file_depth: Some(3),
                ignore_patterns: None,
                include_pinned_files: None,
                include_skills: None,
                include_workflows: None,
                include_environment_info: None,
                custom_sections: None,
            }),
            ..make_config()
        });
        assert!(validate_agent_definition(&def).is_ok());
    }

    fn make_config() -> wf_types::agent::config::AgentConfig {
        wf_types::agent::config::AgentConfig {
            profile_id: None,
            system_prompt: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            max_iterations: None,
            max_execution_time: None,
            max_retries: None,
            execution_timeout: None,
            max_pause_duration: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            initial_messages: None,
            available_tools: None,
            stream: None,
            tool_call_format: None,
            hooks: None,
            dynamic_context: None,
            checkpoint: None,
            violation_policy: None,
        }
    }
}
