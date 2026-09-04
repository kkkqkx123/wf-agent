//! Shared validation types and functions used by workflow, agent and trigger
//! validators. Consolidates duplicate tool / profile / hook validation logic
//! into a single location.

use wf_core::registry::Registry;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;

/// Re-export validation types from wf-types for backward compatibility
pub use wf_types::{
    validate_hook_type, validate_profile_list, validate_profile_reference, validate_tool_list,
    validate_tool_reference, ValidationContext, ValidationError, ValidationResult,
};

/// Build a ValidationContext from an ApiContext by reading live registry
/// snapshots and stored metadata. All data is memory-known; no network or
/// file access.
pub async fn build_validation_context(ctx: &ApiContext) -> ValidationContext {
    let mut tool_names = std::collections::HashSet::new();
    let mut disabled_tools = std::collections::HashSet::new();

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

    let mut profile_ids = std::collections::HashSet::new();
    let mut profile_formats = std::collections::HashMap::new();
    for profile in ctx.llm_gateway.profile_registry().list() {
        let format = profile.tool_call_format.as_ref().map(|c| c.format.clone());
        profile_ids.insert(profile.id.clone());
        if let Some(format) = format {
            profile_formats.insert(profile.id, format);
        }
    }

    let mut script_names = std::collections::HashSet::new();
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

    let mut workflow_ids = std::collections::HashSet::new();
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

    let mut trigger_ids = std::collections::HashSet::new();
    for id in ctx.registries.trigger_templates.list() {
        trigger_ids.insert(id);
    }
    if let Ok(stored) = ctx.storage.trigger_template.list(None).await {
        for meta in &stored {
            trigger_ids.insert(meta.id.to_string());
            trigger_ids.insert(meta.name.clone());
        }
    }

    ValidationContext {
        tool_names,
        disabled_tools,
        profile_ids,
        profile_formats,
        script_names,
        workflow_ids,
        workflow_graphs: std::collections::HashMap::new(),
        trigger_ids,
        trigger_templates: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context() -> ValidationContext {
        let mut tool_names = std::collections::HashSet::new();
        let mut disabled_tools = std::collections::HashSet::new();
        tool_names.insert("read_file".to_string());
        tool_names.insert("tool_read_file".to_string());
        tool_names.insert("write_file".to_string());
        tool_names.insert("tool_write_file".to_string());
        tool_names.insert("disabled_tool".to_string());
        disabled_tools.insert("disabled_tool".to_string());

        let mut profile_ids = std::collections::HashSet::new();
        profile_ids.insert("default".to_string());
        profile_ids.insert("gpt4".to_string());

        let mut script_names = std::collections::HashSet::new();
        script_names.insert("my_script".to_string());

        let mut workflow_ids = std::collections::HashSet::new();
        workflow_ids.insert("wf-1".to_string());

        let mut trigger_ids = std::collections::HashSet::new();
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
