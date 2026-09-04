//! Shared validation types and functions used by workflow, agent and trigger
//! validators. Consolidates duplicate tool / profile / hook validation logic
//! into a single location.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use wf_core::registry::Registry;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;

/// Unified validation error with a dotted field path and human-readable
/// message. Replaces the scattered error types across workflow, agent and
/// trigger validation modules.
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

/// Assembled validation context from live registry snapshots. Provides the
/// shared reference data that tool / profile / hook / script / workflow /
/// trigger validators all need.
pub struct ValidationContext {
    pub tool_names: HashSet<String>,
    pub disabled_tools: HashSet<String>,
    pub profile_ids: HashSet<String>,
    pub script_names: HashSet<String>,
    pub workflow_ids: HashSet<String>,
    pub trigger_ids: HashSet<String>,
}

impl ValidationContext {
    /// Create an empty context (no registries). Useful for draft validation
    /// where only shape and graph checks are needed.
    pub fn empty() -> Self {
        Self {
            tool_names: HashSet::new(),
            disabled_tools: HashSet::new(),
            profile_ids: HashSet::new(),
            script_names: HashSet::new(),
            workflow_ids: HashSet::new(),
            trigger_ids: HashSet::new(),
        }
    }

    /// Build from [`ApiContext`] by reading live registry snapshots and stored
    /// metadata. All data is memory-known; no network or file access.
    pub async fn from_api_context(ctx: &ApiContext) -> Self {
        let mut tool_names = HashSet::new();
        let mut disabled_tools = HashSet::new();

        for tool in ctx.tool_registry.list_tools() {
            let is_enabled = tool.enabled.unwrap_or(true);
            tool_names.insert(tool.name.clone());
            tool_names.insert(tool.id.to_string());
            if !is_enabled {
                disabled_tools.insert(tool.name.clone());
                disabled_tools.insert(tool.id.to_string());
            }
        }
        if let Ok(stored) = ctx.storage.tool.list(None).await {
            for meta in &stored {
                tool_names.insert(meta.id.to_string());
                tool_names.insert(meta.tool_id.clone());
                if !meta.enabled {
                    disabled_tools.insert(meta.id.to_string());
                    disabled_tools.insert(meta.tool_id.clone());
                }
            }
        }

        let profile_ids: HashSet<String> = ctx
            .llm_gateway
            .profile_registry()
            .list()
            .into_iter()
            .map(|p| p.id)
            .collect();

        let mut script_names = HashSet::new();
        if let Ok(scripts) = ctx.storage.script.list(None).await {
            for meta in &scripts {
                script_names.insert(meta.id.to_string());
                script_names.insert(meta.name.clone());
            }
        }
        for name in wf_workflow::registry::WorkflowRegistry::global()
            .scripts()
            .list()
        {
            script_names.insert(name);
        }

        let mut workflow_ids = HashSet::new();
        if let Ok(stored) = ctx.storage.workflow.list(None).await {
            for wf in &stored {
                workflow_ids.insert(wf.id.to_string());
            }
        }
        for id in ctx.registries.workflows.list() {
            workflow_ids.insert(id);
        }
        for id in wf_workflow::registry::WorkflowRegistry::global()
            .graphs()
            .list()
        {
            workflow_ids.insert(id);
        }

        let mut trigger_ids = HashSet::new();
        for id in ctx.registries.trigger_templates.list() {
            trigger_ids.insert(id);
        }
        if let Ok(stored) = ctx.storage.trigger_template.list(None).await {
            for meta in &stored {
                trigger_ids.insert(meta.id.to_string());
                trigger_ids.insert(meta.name.clone());
            }
        }

        Self {
            tool_names,
            disabled_tools,
            profile_ids,
            script_names,
            workflow_ids,
            trigger_ids,
        }
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
    if !wf_types::hook::is_known_hook_type(hook_type) {
        Some(ValidationError {
            field: "hook".to_string(),
            message: format!("Unknown hook type '{}'", hook_type),
        })
    } else {
        None
    }
}

/// Validate a batch of tool names against the context.
pub fn validate_tool_list(
    tool_names: &[String],
    ctx: &ValidationContext,
) -> Vec<ValidationError> {
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
            script_names,
            workflow_ids,
            trigger_ids,
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
    fn unknown_hook_type_returns_warning() {
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
