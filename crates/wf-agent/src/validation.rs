use wf_tools::callback::HookConfig;
use wf_tools::registry::ToolRegistry;
use wf_types::llm::{ToolCallFormat, ToolCallFormatConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationSeverity {
    Error,
    Warning,
}

/// A single validation issue with a structured field reference, matching the
/// TS validation error shape (field + message).
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub field: String,
    pub message: String,
    pub severity: ValidationSeverity,
}

impl ValidationIssue {
    pub fn error(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            severity: ValidationSeverity::Error,
        }
    }

    pub fn warning(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            severity: ValidationSeverity::Warning,
        }
    }
}

/// Result of the tool call protocol compatibility check (TS
/// ProtocolValidationResult).
#[derive(Debug, Clone, Default)]
pub struct ProtocolValidationResult {
    pub valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Agent loop configuration validator, aligned with TS agent-loop-validator.
pub struct AgentLoopValidator;

impl AgentLoopValidator {
    /// Validate a full agent loop config against the tool registry.
    pub fn validate_config(
        config: &wf_tools::callback::AgentLoopConfig,
        registry: &ToolRegistry,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        if config.agent_id.is_empty() {
            issues.push(ValidationIssue::error(
                "agent_id",
                "agent_id must not be empty",
            ));
        }

        if config.model.trim().is_empty() {
            issues.push(ValidationIssue::error(
                "model",
                "model (profile_id) must not be empty",
            ));
        }

        if let Some(max_iterations) = config.max_iterations {
            if max_iterations == 0 {
                issues.push(ValidationIssue::error(
                    "max_iterations",
                    "max_iterations must be >= 1",
                ));
            } else if max_iterations > 100 {
                issues.push(ValidationIssue::warning(
                    "max_iterations",
                    "high max_iterations (100+) may cause long-running loops",
                ));
            }
        }

        if let Some(max_execution_time) = config.max_execution_time {
            if max_execution_time == 0 {
                issues.push(ValidationIssue::warning(
                    "max_execution_time",
                    "max_execution_time of 0 disables the wall-clock limit",
                ));
            }
        }

        for hook in &config.hooks {
            validate_hook(hook, &mut issues);
        }

        if !config.available_tool_names.is_empty() {
            let known: Vec<String> = registry.list_tools().into_iter().map(|t| t.name).collect();
            for name in &config.available_tool_names {
                if !known.contains(name) {
                    issues.push(ValidationIssue::error(
                        "available_tool_names",
                        format!("tool '{}' is not registered in the tool registry", name),
                    ));
                }
            }
        }

        issues
    }

    /// Validate tool call format protocol locking between the agent config
    /// and the LLM profile (TS validateAgentToolCallProtocol). Uses the same
    /// compatibility rule the gateway applies at runtime.
    pub fn validate_tool_call_protocol(
        config_format: Option<&ToolCallFormatConfig>,
        profile_format: Option<ToolCallFormat>,
    ) -> ProtocolValidationResult {
        let mut result = ProtocolValidationResult::default();

        match (config_format, profile_format) {
            (None, None) => result.valid = true,
            (Some(cfg), Some(profile)) => {
                if cfg.format == profile {
                    result.valid = true;
                } else if cfg.format.is_compatible_with(&profile) {
                    result.warnings.push(format!(
                        "Tool call format mismatch: agent specifies \"{:?}\" but profile is configured for \"{:?}\". Both are JSON-based and may work, but markers may differ.",
                        cfg.format, profile
                    ));
                    result.valid = true;
                } else {
                    result.errors.push(format!(
                        "Tool call format mismatch: agent specifies \"{:?}\" but profile is configured for \"{:?}\"",
                        cfg.format, profile
                    ));
                    result.valid = false;
                }
            }
            (Some(cfg), None) => {
                result.warnings.push(format!(
                    "Agent specifies tool call format \"{:?}\" but profile has no explicit format; the agent's format will be used at runtime",
                    cfg.format
                ));
                result.valid = true;
            }
            (None, Some(_)) => {
                result.valid = true;
            }
        }

        result
    }

    /// Convenience entry point used before agent execution: returns Err with
    /// the first error-level issue or Ok(()) when only warnings exist.
    pub fn validate_or_fail(
        config: &wf_tools::callback::AgentLoopConfig,
        registry: &ToolRegistry,
    ) -> Result<Vec<ValidationIssue>, Vec<ValidationIssue>> {
        let issues = Self::validate_config(config, registry);
        let errors: Vec<ValidationIssue> = issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Error)
            .cloned()
            .collect();
        if errors.is_empty() {
            Ok(issues)
        } else {
            Err(errors)
        }
    }
}

fn validate_hook(hook: &HookConfig, issues: &mut Vec<ValidationIssue>) {
    const KNOWN_HOOK_TYPES: &[&str] = &[
        "BEFORE_ITERATION",
        "AFTER_ITERATION",
        "BEFORE_LLM_CALL",
        "AFTER_LLM_CALL",
        "BEFORE_TOOL_CALL",
        "AFTER_TOOL_CALL",
        "BEFORE_AGENT",
        "AFTER_AGENT",
    ];
    if !KNOWN_HOOK_TYPES.contains(&hook.hook_type.as_str()) {
        issues.push(ValidationIssue::warning(
            "hooks",
            format!("unknown hook type '{}' will never fire", hook.hook_type),
        ));
    }
}

/// Validate that a tool call format config is structurally sound (markers
/// present for non-native formats).
pub fn validate_tool_call_format_config(config: &ToolCallFormatConfig) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    match config.format {
        ToolCallFormat::Native => {}
        ToolCallFormat::Xml => {
            if config.xml_tags.is_none() {
                issues.push(ValidationIssue::warning(
                    "tool_call_format.xml_tags",
                    "XML format without custom xml_tags falls back to defaults",
                ));
            }
        }
        ToolCallFormat::JsonWrapped | ToolCallFormat::JsonRaw => {}
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_tools::callback::AgentLoopConfig;
    use wf_types::Id;

    fn base_config() -> AgentLoopConfig {
        AgentLoopConfig {
            agent_id: Id::from("agent-1".to_string()),
            model: "mock".to_string(),
            max_iterations: Some(10),
            max_execution_time: Some(1000),
            hooks: Vec::new(),
            available_tool_names: Vec::new(),
            tool_call_format: None,
            token_limit: None,
            token_warning_threshold: None,
        }
    }

    #[test]
    fn test_valid_config_passes() {
        let registry = ToolRegistry::new();
        let issues = AgentLoopValidator::validate_config(&base_config(), &registry);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_empty_agent_id_fails() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            agent_id: Id::from("".to_string()),
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(issues
            .iter()
            .any(|i| i.field == "agent_id" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn test_empty_model_fails() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            model: "".to_string(),
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(issues
            .iter()
            .any(|i| i.field == "model" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn test_unknown_tool_rejected() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            available_tool_names: vec!["nope_tool".to_string()],
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(issues
            .iter()
            .any(|i| i.field == "available_tool_names" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn test_zero_iterations_rejected() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            max_iterations: Some(0),
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(issues
            .iter()
            .any(|i| i.field == "max_iterations" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn test_unknown_hook_warns() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            hooks: vec![HookConfig {
                hook_type: "NOPE_HOOK".to_string(),
                condition: None,
                enabled: true,
                parallel: None,
                continue_on_error: None,
            }],
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(issues
            .iter()
            .any(|i| i.field == "hooks" && i.severity == ValidationSeverity::Warning));
    }

    #[test]
    fn test_protocol_mismatch() {
        let cfg = ToolCallFormatConfig {
            format: ToolCallFormat::Xml,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        };
        let result = AgentLoopValidator::validate_tool_call_protocol(
            Some(&cfg),
            Some(ToolCallFormat::Native),
        );
        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_protocol_match_ok() {
        let cfg = ToolCallFormatConfig {
            format: ToolCallFormat::JsonWrapped,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        };
        let result = AgentLoopValidator::validate_tool_call_protocol(
            Some(&cfg),
            Some(ToolCallFormat::JsonWrapped),
        );
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_protocol_agent_only_warns() {
        let cfg = ToolCallFormatConfig {
            format: ToolCallFormat::Native,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        };
        let result = AgentLoopValidator::validate_tool_call_protocol(Some(&cfg), None);
        assert!(result.valid);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_validate_or_fail() {
        let registry = ToolRegistry::new();
        assert!(AgentLoopValidator::validate_or_fail(&base_config(), &registry).is_ok());

        let bad = AgentLoopConfig {
            agent_id: Id::from("".to_string()),
            ..base_config()
        };
        assert!(AgentLoopValidator::validate_or_fail(&bad, &registry).is_err());
    }

    #[test]
    fn test_tool_call_format_config_warning() {
        let cfg = ToolCallFormatConfig {
            format: ToolCallFormat::Xml,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        };
        let issues = validate_tool_call_format_config(&cfg);
        assert_eq!(issues.len(), 1);
    }
}
