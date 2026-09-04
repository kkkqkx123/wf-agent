use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::llm::ToolCallFormat;
use crate::trigger::TriggerTemplate;
use crate::workflow_execution::WorkflowGraphStructure;

/// Unified validation error with a dotted field path and human-readable
/// message. This is the canonical validation error type used across all
/// layers of the application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Unified validation result carrying separate error and warning lists.
/// Warnings allow registration; errors block it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn merge(&mut self, other: Self) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }

    pub fn push_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }

    pub fn push_warning(&mut self, warning: ValidationError) {
        self.warnings.push(warning);
    }

    pub fn extend_errors(&mut self, errors: Vec<ValidationError>) {
        self.errors.extend(errors);
    }

    pub fn extend_warnings(&mut self, warnings: Vec<ValidationError>) {
        self.warnings.extend(warnings);
    }
}

impl From<Vec<ValidationError>> for ValidationResult {
    fn from(errors: Vec<ValidationError>) -> Self {
        Self {
            errors,
            warnings: Vec::new(),
        }
    }
}

impl From<ValidationError> for ValidationResult {
    fn from(error: ValidationError) -> Self {
        Self {
            errors: vec![error],
            warnings: Vec::new(),
        }
    }
}

/// Unified validation context containing all reference data needed for
/// validation across workflow, agent, and trigger validators.
/// This is the single source of truth for validation context.
#[derive(Debug, Clone, Default)]
pub struct ValidationContext {
    pub tool_names: HashSet<String>,
    pub disabled_tools: HashSet<String>,
    pub profile_ids: HashSet<String>,
    pub profile_formats: HashMap<String, ToolCallFormat>,
    pub script_names: HashSet<String>,
    pub workflow_ids: HashSet<String>,
    pub workflow_graphs: HashMap<String, WorkflowGraphStructure>,
    pub trigger_ids: HashSet<String>,
    pub trigger_templates: Vec<TriggerTemplate>,
}

impl ValidationContext {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_profile(mut self, id: impl Into<String>, format: Option<ToolCallFormat>) -> Self {
        let id = id.into();
        self.profile_ids.insert(id.clone());
        if let Some(format) = format {
            self.profile_formats.insert(id, format);
        }
        self
    }

    pub fn with_tool(mut self, name: impl Into<String>, enabled: bool) -> Self {
        let name = name.into();
        self.tool_names.insert(name.clone());
        if !enabled {
            self.disabled_tools.insert(name);
        }
        self
    }

    pub fn with_script(mut self, name: impl Into<String>) -> Self {
        self.script_names.insert(name.into());
        self
    }

    pub fn with_workflow(
        mut self,
        id: impl Into<String>,
        graph: Option<WorkflowGraphStructure>,
    ) -> Self {
        let id = id.into();
        self.workflow_ids.insert(id.clone());
        if let Some(graph) = graph {
            self.workflow_graphs.insert(id, graph);
        }
        self
    }

    pub fn with_trigger(mut self, id: impl Into<String>) -> Self {
        self.trigger_ids.insert(id.into());
        self
    }

    pub fn with_trigger_template(mut self, template: TriggerTemplate) -> Self {
        self.trigger_templates.push(template);
        self
    }

    pub fn tool_exists(&self, name: &str) -> bool {
        self.tool_names.contains(name)
    }

    pub fn tool_enabled(&self, name: &str) -> bool {
        self.tool_exists(name) && !self.disabled_tools.contains(name)
    }
}

/// Validate that a tool reference exists and is enabled.
pub fn validate_tool_reference(
    tool_name: &str,
    ctx: &ValidationContext,
) -> Option<ValidationError> {
    if !ctx.tool_names.contains(tool_name) {
        Some(ValidationError {
            field: "tool".to_string(),
            message: format!("Tool '{}' not registered", tool_name),
        })
    } else if ctx.disabled_tools.contains(tool_name) {
        Some(ValidationError {
            field: "tool".to_string(),
            message: format!("Tool '{}' is disabled", tool_name),
        })
    } else {
        None
    }
}

/// Validate that a profile reference exists.
pub fn validate_profile_reference(
    profile_id: &str,
    ctx: &ValidationContext,
) -> Option<ValidationError> {
    if !ctx.profile_ids.contains(profile_id) {
        Some(ValidationError {
            field: "profile".to_string(),
            message: format!("Profile '{}' not registered", profile_id),
        })
    } else {
        None
    }
}

/// Validate that a hook type is known (agent or workflow hook type).
pub fn validate_hook_type(hook_type: &str) -> Option<ValidationError> {
    if !crate::hook::is_known_hook_type(hook_type) {
        Some(ValidationError {
            field: "hook".to_string(),
            message: format!("Unknown hook type '{}'", hook_type),
        })
    } else {
        None
    }
}

/// Validate a batch of tool names against the context.
pub fn validate_tool_list(tool_names: &[String], ctx: &ValidationContext) -> Vec<ValidationError> {
    tool_names
        .iter()
        .filter_map(|name| validate_tool_reference(name, ctx))
        .collect()
}

/// Validate a batch of profile ids against the context.
pub fn validate_profile_list(
    profile_ids: &[String],
    ctx: &ValidationContext,
) -> Vec<ValidationError> {
    profile_ids
        .iter()
        .filter_map(|id| validate_profile_reference(id, ctx))
        .collect()
}

impl From<crate::script::sandbox::SecurityViolation> for ValidationError {
    fn from(v: crate::script::sandbox::SecurityViolation) -> Self {
        ValidationError {
            field: v.field,
            message: format!("[{:?}] {}", v.severity, v.reason),
        }
    }
}

impl From<&crate::script::sandbox::SecurityViolation> for ValidationError {
    fn from(v: &crate::script::sandbox::SecurityViolation) -> Self {
        ValidationError {
            field: v.field.clone(),
            message: format!("[{:?}] {}", v.severity, v.reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context() -> ValidationContext {
        let mut tool_names = HashSet::new();
        let mut disabled_tools = HashSet::new();
        tool_names.insert("read_file".to_string());
        tool_names.insert("tool_read_file".to_string());
        tool_names.insert("write_file".to_string());
        tool_names.insert("tool_write_file".to_string());
        tool_names.insert("disabled_tool".to_string());
        disabled_tools.insert("disabled_tool".to_string());

        let mut profile_ids = HashSet::new();
        profile_ids.insert("default".to_string());
        profile_ids.insert("gpt4".to_string());

        let mut script_names = HashSet::new();
        script_names.insert("my_script".to_string());

        let mut workflow_ids = HashSet::new();
        workflow_ids.insert("wf-1".to_string());

        let mut trigger_ids = HashSet::new();
        trigger_ids.insert("trigger-1".to_string());

        ValidationContext {
            tool_names,
            disabled_tools,
            profile_ids,
            profile_formats: std::collections::HashMap::new(),
            script_names,
            workflow_ids,
            workflow_graphs: std::collections::HashMap::new(),
            trigger_ids,
            trigger_templates: Vec::new(),
        }
    }

    #[test]
    fn tool_reference_existing_enabled_returns_none() {
        let ctx = make_context();
        assert!(validate_tool_reference("read_file", &ctx).is_none());
        assert!(validate_tool_reference("tool_read_file", &ctx).is_none());
    }

    #[test]
    fn tool_reference_missing_returns_error() {
        let ctx = make_context();
        let err = validate_tool_reference("nonexistent", &ctx).unwrap();
        assert_eq!(err.field, "tool");
        assert!(err.message.contains("not registered"));
    }

    #[test]
    fn tool_reference_disabled_returns_error() {
        let ctx = make_context();
        let err = validate_tool_reference("disabled_tool", &ctx).unwrap();
        assert_eq!(err.field, "tool");
        assert!(err.message.contains("disabled"));
    }

    #[test]
    fn profile_reference_existing_returns_none() {
        let ctx = make_context();
        assert!(validate_profile_reference("default", &ctx).is_none());
    }

    #[test]
    fn profile_reference_missing_returns_error() {
        let ctx = make_context();
        let err = validate_profile_reference("unknown", &ctx).unwrap();
        assert_eq!(err.field, "profile");
        assert!(err.message.contains("not registered"));
    }

    #[test]
    fn known_hook_type_returns_none() {
        assert!(validate_hook_type("BEFORE_ITERATION").is_none());
        assert!(validate_hook_type("AFTER_EXECUTE").is_none());
    }

    #[test]
    fn unknown_hook_type_returns_error() {
        let err = validate_hook_type("UNKNOWN_TYPE").unwrap();
        assert_eq!(err.field, "hook");
        assert!(err.message.contains("Unknown hook type"));
    }

    #[test]
    fn validate_tool_list_collects_errors() {
        let ctx = make_context();
        let names = vec![
            "read_file".to_string(),
            "nonexistent".to_string(),
            "disabled_tool".to_string(),
        ];
        let errors = validate_tool_list(&names, &ctx);
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn validate_profile_list_collects_errors() {
        let ctx = make_context();
        let ids = vec!["default".to_string(), "missing".to_string()];
        let errors = validate_profile_list(&ids, &ctx);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn validation_result_merge() {
        let mut a = ValidationResult::default();
        a.push_error(ValidationError::new("a", "error a"));
        a.push_warning(ValidationError::new("b", "warning b"));

        let mut b = ValidationResult::default();
        b.push_error(ValidationError::new("c", "error c"));

        a.merge(b);
        assert_eq!(a.errors.len(), 2);
        assert_eq!(a.warnings.len(), 1);
    }
}
