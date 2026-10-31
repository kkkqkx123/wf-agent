use wf_tools::callback::HookConfig;
use wf_tools::registry::ToolRegistry;
use wf_types::llm::{ToolCallProtocol, ToolCallProtocolConfig};

use crate::constants::AGENT_MAX_ITERATIONS_CAP;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationSeverity {
    Error,
    Warning,
}

/// A single validation issue with a structured field reference
/// (field + message).
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

/// Agent loop configuration validator.
pub struct AgentLoopValidator;

impl AgentLoopValidator {
    /// Validate a full agent loop config against the tool registry using
    /// the default `AGENT_MAX_ITERATIONS_CAP` hard limit.
    pub fn validate_config(
        config: &wf_tools::callback::AgentLoopConfig,
        registry: &ToolRegistry,
    ) -> Vec<ValidationIssue> {
        Self::validate_config_with_cap(config, registry, AGENT_MAX_ITERATIONS_CAP)
    }

    /// Validate a full agent loop config against the tool registry with an
    /// explicit `max_iterations` hard cap (runtime-configured).
    pub fn validate_config_with_cap(
        config: &wf_tools::callback::AgentLoopConfig,
        registry: &ToolRegistry,
        max_iterations_cap: u32,
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
            } else if max_iterations > max_iterations_cap {
                issues.push(ValidationIssue::error(
                    "max_iterations",
                    format!(
                        "max_iterations ({}) exceeds the hard limit ({}). \
                         Reduce max_iterations to prevent resource exhaustion.",
                        max_iterations, max_iterations_cap
                    ),
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

        if let Some(token_limit) = config.token_limit {
            if token_limit == 0 {
                issues.push(ValidationIssue::warning(
                    "token_limit",
                    "token_limit of 0 disables token limit checks",
                ));
            }
        }

        if let Some(token_warning_threshold) = config.token_warning_threshold {
            if token_warning_threshold > 100 {
                issues.push(ValidationIssue::error(
                    "token_warning_threshold",
                    format!(
                        "token_warning_threshold must be between 0 and 100, got {}",
                        token_warning_threshold
                    ),
                ));
            }
        }

        for hook in &config.hooks {
            validate_hook(hook, &mut issues);
        }

        validate_tool_lists(config, registry, &mut issues);

        issues
    }

    /// Profile-aware validation: the `model` (profile id) must exist in the
    /// known profile set when the set is provided. Tool lists are checked
    /// against the tool registry with enabled-state distinction.
    pub fn validate_config_with_profiles(
        config: &wf_tools::callback::AgentLoopConfig,
        registry: &ToolRegistry,
        known_profile_ids: &std::collections::HashSet<String>,
    ) -> Vec<ValidationIssue> {
        let mut issues = Self::validate_config(config, registry);
        if !config.model.trim().is_empty() && !known_profile_ids.contains(&config.model) {
            issues.push(ValidationIssue::error(
                "model",
                format!(
                    "model '{}' references profile '{}' which is not registered",
                    config.model, config.model
                ),
            ));
        }
        issues
    }

    /// Validate tool call format protocol locking between the agent config
    /// and the LLM profile. Uses the same
    /// compatibility rule the gateway applies at runtime.
    pub fn validate_tool_call_protocol(
        config_format: Option<&ToolCallProtocolConfig>,
        profile_format: Option<ToolCallProtocol>,
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

    /// Validate one context map against an optional shape contract.
    ///
    /// Key names always pass `validate_template_path`; declared keys must
    /// match their shape (`variable_value_matches`) and required keys
    /// without a default must be present. Unknown keys pass for forward
    /// compatibility.
    pub fn validate_context_with_schema(
        context: &std::collections::HashMap<String, serde_json::Value>,
        schema: &[wf_types::TemplateVariableDefinition],
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        for declared in schema {
            match &context.get(&declared.name) {
                None => {
                    if declared.required && declared.default_value.is_none() {
                        issues.push(ValidationIssue::error(
                            format!("context.{}", declared.name),
                            format!("context key '{}' is required", declared.name),
                        ));
                    }
                }
                Some(value) => {
                    if !wf_types::template::variable_value_matches(&declared.r#type, value) {
                        issues.push(ValidationIssue::error(
                            format!("context.{}", declared.name),
                            format!(
                                "context key '{}' expects {:?}, got {}",
                                declared.name, declared.r#type, value
                            ),
                        ));
                    }
                }
            }
        }
        issues
    }

    /// Validate one agent loop input with an optional context shape
    /// contract: message text, context keys, imported conversation size,
    /// plus declared context value shapes when a schema is present.
    pub fn validate_input_with_schema(
        input: &wf_tools::callback::AgentLoopInput,
        schema: Option<&[wf_types::TemplateVariableDefinition]>,
    ) -> Vec<ValidationIssue> {
        let mut issues = Self::validate_input(input);
        if let Some(schema) = schema {
            issues.extend(Self::validate_context_with_schema(&input.context, schema));
        }
        issues
    }

    /// Validate one agent loop input: message text, context keys and
    /// imported conversation size. Context values stay untyped JSON; only
    /// key shapes are checked here so prompt rendering keeps its own
    /// type errors at the template boundary.
    pub fn validate_input(input: &wf_tools::callback::AgentLoopInput) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        if input.message.trim().is_empty() {
            issues.push(ValidationIssue::error(
                "message",
                "agent message must not be empty",
            ));
        }
        for key in input.context.keys() {
            if wf_types::template::validate_template_path(key).is_some() {
                issues.push(ValidationIssue::error(
                    format!("context.{key}"),
                    format!("context key '{key}' is not a valid template path"),
                ));
            }
        }
        if input.conversation.len() > 4096 {
            issues.push(ValidationIssue::error(
                "conversation",
                format!(
                    "conversation holds {} messages, exceeding the 4096-message import limit",
                    input.conversation.len()
                ),
            ));
        }
        issues
    }

    pub fn validate_or_fail_input(
        input: &wf_tools::callback::AgentLoopInput,
    ) -> Result<(), Vec<ValidationIssue>> {
        Self::validate_or_fail_input_with_schema(input, None)
    }

    pub fn validate_or_fail_input_with_schema(
        input: &wf_tools::callback::AgentLoopInput,
        schema: Option<&[wf_types::TemplateVariableDefinition]>,
    ) -> Result<(), Vec<ValidationIssue>> {
        let errors: Vec<ValidationIssue> = Self::validate_input_with_schema(input, schema)
            .into_iter()
            .filter(|i| i.severity == ValidationSeverity::Error)
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
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

    /// Cap-aware variant of [`validate_or_fail`](Self::validate_or_fail).
    pub fn validate_or_fail_with_cap(
        config: &wf_tools::callback::AgentLoopConfig,
        registry: &ToolRegistry,
        max_iterations_cap: u32,
    ) -> Result<Vec<ValidationIssue>, Vec<ValidationIssue>> {
        let issues = Self::validate_config_with_cap(config, registry, max_iterations_cap);
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

/// Validates hook configuration. Delegates pure shape checks to the
/// shared truth in `wf_types::hook`; condition syntax uses the shared
/// evaluator in `wf_core`. Config sources reject unknown hook types.
/// Tool-callback hooks arriving via model output keep runtime clamping at
/// the conversion site (`HookDefinition::from`).
/// Completeness warnings (missing sync handler on request/mutated points,
/// handler-less `BEFORE_*` definitions) mirror the config loader's
/// `validate_canonical_hook` semantics via the same `wf_types::hook`
/// effect helpers, so both entries warn on the same inputs.
fn validate_hook(hook: &HookConfig, issues: &mut Vec<ValidationIssue>) {
    use wf_types::hook::is_known_hook_point;
    if !is_known_hook_point(&hook.hook_type) {
        issues.push(ValidationIssue::error(
            "hooks",
            format!("unknown hook type '{}'", hook.hook_type),
        ));
    }
    if let Err(e) = wf_types::hook::validate_hook_priority(hook.priority) {
        issues.push(ValidationIssue::error(
            "hooks.priority",
            format!("hook '{}' {e}", hook.hook_type),
        ));
    }
    if let Err(e) = wf_types::hook::validate_hook_handler_name(hook.handler.as_deref()) {
        issues.push(ValidationIssue::error(
            "hooks.handler",
            format!("hook '{}' {e}", hook.hook_type),
        ));
    }
    if let Some(condition) = &hook.condition {
        if let Err(e) = wf_core::condition::ConditionEvaluator::validate_syntax(condition) {
            issues.push(ValidationIssue::error(
                "hooks.condition",
                format!("hook '{}' condition syntax error: {}", hook.hook_type, e),
            ));
        }
    }
    if let Some(payload) = hook.payload.as_ref() {
        if let Err(e) = wf_types::hook::validate_payload_template_syntax(payload) {
            issues.push(ValidationIssue::error(
                "hooks.payload",
                format!("hook '{}' payload invalid: {}", hook.hook_type, e),
            ));
        }
    }
    if hook.handler.as_deref().is_none_or(|h| h.trim().is_empty()) {
        if wf_types::hook::hook_requires_handler(&hook.hook_type) {
            issues.push(ValidationIssue::warning(
                "hooks.handler",
                format!(
                    "hook '{}' needs a sync handler for in-step effects; a trigger rule matching the audit event only observes asynchronously after the fire",
                    hook.hook_type
                ),
            ));
        } else if !wf_types::hook::hook_allows_trigger(&hook.hook_type) {
            issues.push(ValidationIssue::warning(
                "hooks.handler",
                format!(
                    "hook '{}' is a BEFORE_* point closed to triggers: without a sync handler the definition only writes a write-only audit event nobody may consume",
                    hook.hook_type
                ),
            ));
        }
    }
}

fn validate_tool_lists(
    config: &wf_tools::callback::AgentLoopConfig,
    registry: &ToolRegistry,
    issues: &mut Vec<ValidationIssue>,
) {
    use std::collections::{HashMap, HashSet};
    let tools = registry.list_tools();
    let mut known: HashSet<String> = HashSet::new();
    let mut disabled: HashSet<String> = HashSet::new();
    let mut enabled_by_name: HashMap<String, bool> = HashMap::new();
    for tool in &tools {
        known.insert(tool.name.clone());
        known.insert(tool.id.to_string());
        let is_enabled = tool.enabled.unwrap_or(true);
        enabled_by_name.insert(tool.name.clone(), is_enabled);
        enabled_by_name.insert(tool.id.to_string(), is_enabled);
        if !is_enabled {
            disabled.insert(tool.name.clone());
            disabled.insert(tool.id.to_string());
        }
    }
    let empty_tools: Vec<String> = Vec::new();
    let lists: [(&str, &Vec<String>); 5] = [
        (
            "available_tool_names",
            config.available_tool_names.as_ref().unwrap_or(&empty_tools),
        ),
        ("initial_tool_names", &config.initial_tool_names),
        ("discoverable_tool_names", &config.discoverable_tool_names),
        ("hidden_tool_names", &config.hidden_tool_names),
        ("activated_tool_names", &config.activated_tool_names),
    ];
    for (field, names) in lists {
        for name in names {
            if !known.contains(name) {
                issues.push(ValidationIssue::error(
                    field,
                    format!("tool '{}' is not registered in the tool registry", name),
                ));
            } else if disabled.contains(name) {
                issues.push(ValidationIssue::warning(
                    field,
                    format!("tool '{}' is registered but disabled", name),
                ));
            }
        }
    }

    // Contradictory visibility: the hidden blocklist wins at resolution
    // time, so a tool listed as both initially visible and hidden would
    // silently disappear from the schema. Report every overlap together
    // instead of failing on the first one.
    let initial_set: std::collections::HashSet<&str> = config
        .initial_tool_names
        .iter()
        .map(String::as_str)
        .collect();
    let mut contradictions: Vec<&str> = config
        .hidden_tool_names
        .iter()
        .map(String::as_str)
        .filter(|name| initial_set.contains(name))
        .collect();
    contradictions.sort_unstable();
    contradictions.dedup();
    for name in contradictions {
        issues.push(ValidationIssue::error(
            "hidden_tool_names",
            format!(
                "tool '{name}' is listed as both initially visible and hidden; hidden wins, so remove it from one list",
            ),
        ));
    }
}

/// Validate that a tool call format config is structurally sound (markers
/// present for non-native formats).
pub fn validate_tool_call_protocol_config(config: &ToolCallProtocolConfig) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    match config.format {
        ToolCallProtocol::Native => {}
        ToolCallProtocol::Xml => {
            if config.xml_tags.is_none() {
                issues.push(ValidationIssue::warning(
                    "tool_call_protocol.xml_tags",
                    "XML format without custom xml_tags falls back to defaults",
                ));
            }
        }
        ToolCallProtocol::JsonWrapped | ToolCallProtocol::JsonRaw => {}
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
            file_scope: None,
            agent_id: Id::from("agent-1".to_string()),
            model: "mock".to_string(),
            max_iterations: Some(10),
            max_execution_time: Some(1000),
            hooks: Vec::new(),
            available_tool_names: Some(Vec::new()),
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            activated_tool_names: Vec::new(),
            hidden_tool_names: Vec::new(),
            tool_call_protocol: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            general_description: None,
            discoverable_metadata_block: None,
            history_normalization: false,
            context_schema: None,
            checkpoint_message_interval: None,
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
            file_scope: None,
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
            file_scope: None,
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
            file_scope: None,
            available_tool_names: Some(vec!["nope_tool".to_string()]),
            discoverable_tool_names: Vec::new(),
            hidden_tool_names: Vec::new(),
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
            file_scope: None,
            max_iterations: Some(0),
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(issues
            .iter()
            .any(|i| i.field == "max_iterations" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn test_unknown_hook_errors() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            file_scope: None,
            hooks: vec![HookConfig {
                hook_type: "NOPE_HOOK".to_string(),
                condition: None,
                enabled: true,
                priority: 0,
                payload: None,
                handler: None,
                create_checkpoint: None,
                checkpoint_description: None,
            }],
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(issues
            .iter()
            .any(|i| i.field == "hooks" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn test_protocol_mismatch() {
        let cfg = ToolCallProtocolConfig {
            format: ToolCallProtocol::Xml,
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
            Some(ToolCallProtocol::Native),
        );
        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_protocol_match_ok() {
        let cfg = ToolCallProtocolConfig {
            format: ToolCallProtocol::JsonWrapped,
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
            Some(ToolCallProtocol::JsonWrapped),
        );
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_protocol_agent_only_warns() {
        let cfg = ToolCallProtocolConfig {
            format: ToolCallProtocol::Native,
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
            file_scope: None,
            agent_id: Id::from("".to_string()),
            ..base_config()
        };
        assert!(AgentLoopValidator::validate_or_fail(&bad, &registry).is_err());
    }

    #[test]
    fn test_tool_call_protocol_config_warning() {
        let cfg = ToolCallProtocolConfig {
            format: ToolCallProtocol::Xml,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        };
        let issues = validate_tool_call_protocol_config(&cfg);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_token_limit_zero_warns() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            file_scope: None,
            token_limit: Some(0),
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(issues
            .iter()
            .any(|i| i.field == "token_limit" && i.severity == ValidationSeverity::Warning));
    }

    #[test]
    fn test_token_limit_positive_accepted() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            file_scope: None,
            token_limit: Some(100000),
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(!issues.iter().any(|i| i.field == "token_limit"));
    }

    #[test]
    fn test_token_warning_threshold_exceeds_100_rejected() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            file_scope: None,
            token_warning_threshold: Some(150),
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(issues.iter().any(
            |i| i.field == "token_warning_threshold" && i.severity == ValidationSeverity::Error
        ));
    }

    fn valid_input() -> wf_tools::callback::AgentLoopInput {
        wf_tools::callback::AgentLoopInput {
            message: "do it".to_string(),
            context: std::collections::HashMap::new(),
            conversation: Vec::new(),
        }
    }

    #[test]
    fn test_validate_input_accepts_minimal() {
        assert!(AgentLoopValidator::validate_input(&valid_input()).is_empty());
        assert!(AgentLoopValidator::validate_or_fail_input(&valid_input()).is_ok());
    }

    #[test]
    fn test_validate_input_rejects_empty_message() {
        let mut input = valid_input();
        input.message = "   ".to_string();
        let issues = AgentLoopValidator::validate_input(&input);
        assert!(issues.iter().any(|i| i.field == "message"));
        assert!(AgentLoopValidator::validate_or_fail_input(&input).is_err());
    }

    #[test]
    fn test_validate_input_rejects_bad_context_key() {
        let mut input = valid_input();
        input
            .context
            .insert("bad key".to_string(), serde_json::json!(1));
        let issues = AgentLoopValidator::validate_input(&input);
        assert!(issues.iter().any(|i| i.field.starts_with("context.")));
    }

    fn context_schema() -> Vec<wf_types::TemplateVariableDefinition> {
        vec![
            wf_types::TemplateVariableDefinition {
                name: "count".to_string(),
                r#type: wf_types::TemplateVariableType::Number,
                required: true,
                description: None,
                default_value: None,
            },
            wf_types::TemplateVariableDefinition {
                name: "label".to_string(),
                r#type: wf_types::TemplateVariableType::String,
                required: false,
                description: None,
                default_value: None,
            },
        ]
    }

    #[test]
    fn test_validate_context_schema_accepts_declared_shapes() {
        let schema = context_schema();
        let context = std::collections::HashMap::from([
            ("count".to_string(), serde_json::json!(3)),
            ("extra".to_string(), serde_json::json!("anything")),
        ]);
        assert!(AgentLoopValidator::validate_context_with_schema(&context, &schema).is_empty());
    }

    #[test]
    fn test_validate_context_schema_reports_missing_and_mismatch() {
        let schema = context_schema();
        let empty = std::collections::HashMap::new();
        let issues = AgentLoopValidator::validate_context_with_schema(&empty, &schema);
        assert!(issues.iter().any(|i| i.field == "context.count"));
        let bad =
            std::collections::HashMap::from([("count".to_string(), serde_json::json!("many"))]);
        let issues = AgentLoopValidator::validate_context_with_schema(&bad, &schema);
        assert!(issues.iter().any(|i| i.field == "context.count"));
    }

    #[test]
    fn test_initial_hidden_overlap_reports_every_tool() {
        let registry = ToolRegistry::new();
        for (id, name) in [("a", "a_tool"), ("b", "b_tool")] {
            registry.register_tool(wf_types::tool::Tool {
                id: id.into(),
                name: name.into(),
                description: name.into(),
                tool_type: wf_types::tool::ToolType::Stateless,
                parameters: None,
                metadata: None,
                config: None,
                enabled: Some(true),
                strict: None,
                default_timeout_ms: None,
            });
        }
        let config = AgentLoopConfig {
            file_scope: None,
            initial_tool_names: vec!["b_tool".to_string(), "a_tool".to_string()],
            hidden_tool_names: vec!["a_tool".to_string(), "b_tool".to_string()],
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        let overlap: Vec<&ValidationIssue> = issues
            .iter()
            .filter(|i| i.field == "hidden_tool_names")
            .collect();
        assert_eq!(overlap.len(), 2);
        assert!(overlap[0].message.contains("a_tool"));
        assert!(overlap[1].message.contains("b_tool"));
    }

    #[test]
    fn test_token_warning_threshold_valid_accepted() {
        let registry = ToolRegistry::new();
        let config = AgentLoopConfig {
            file_scope: None,
            token_warning_threshold: Some(80),
            ..base_config()
        };
        let issues = AgentLoopValidator::validate_config(&config, &registry);
        assert!(!issues.iter().any(|i| i.field == "token_warning_threshold"));
    }
}
