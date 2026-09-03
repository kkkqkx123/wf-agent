//! Workflow validation utilities.

use std::collections::{HashMap, HashSet};

use wf_core::registry::Registry;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_workflow::reference_closure::{validate_workflow_tool_lists, ReferenceContext};

use crate::ApiContext;
use crate::ApiError;
use wf_types::WorkflowDefinition;

use super::definition::upsert_workflow_registry;
use super::workflow_execution::definition_to_graph;

/// Validate shape and graph structure only (draft-friendly, no external
/// references). Used by draft saves and internal checks.
pub fn validate_workflow(workflow: &WorkflowDefinition) -> crate::ApiResult<()> {
    wf_config::processor::workflow::validate_workflow_definition(workflow)
        .map_err(|e| ApiError::Validation(e.to_string()))?;

    let graph = definition_to_graph(workflow);
    wf_workflow::validation::GraphValidator::validate(graph)
        .map(|_| ())
        .map_err(|errors| {
            let detail = errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            ApiError::Validation(format!(
                "workflow graph validation failed ({} error(s)): {}",
                errors.len(),
                detail
            ))
        })
}

/// Assemble the definition-time reference context from live registry
/// snapshots and stored workflows. All sources are memory-known state;
/// no network or file access is performed.
pub async fn build_reference_context(ctx: &ApiContext) -> ReferenceContext {
    let mut ref_ctx = ReferenceContext::new();

    for profile in ctx.llm_gateway.profile_registry().list() {
        let format = profile.tool_call_format.as_ref().map(|c| c.format.clone());
        ref_ctx.profile_ids.insert(profile.id.clone());
        if let Some(format) = format {
            ref_ctx.profile_formats.insert(profile.id, format);
        }
    }

    let mut tool_enabled: HashMap<String, bool> = HashMap::new();
    for tool in ctx.tool_registry.list_tools() {
        ref_ctx.tool_names.insert(tool.name.clone());
        ref_ctx.tool_names.insert(tool.id.to_string());
        let enabled = tool.enabled.unwrap_or(true);
        tool_enabled.insert(tool.name.clone(), enabled);
        tool_enabled.insert(tool.id.to_string(), enabled);
    }
    if let Ok(stored) = ctx.storage.tool.list(None).await {
        for meta in &stored {
            ref_ctx.tool_names.insert(meta.id.to_string());
            ref_ctx.tool_names.insert(meta.tool_id.clone());
            tool_enabled
                .entry(meta.id.to_string())
                .or_insert(meta.enabled);
            tool_enabled
                .entry(meta.tool_id.clone())
                .or_insert(meta.enabled);
        }
    }
    let mut disabled: HashSet<String> = HashSet::new();
    for (name, enabled) in &tool_enabled {
        if !enabled {
            disabled.insert(name.clone());
        }
    }
    ref_ctx.disabled_tools = disabled;

    if let Ok(scripts) = ctx.storage.script.list(None).await {
        for meta in &scripts {
            ref_ctx.script_names.insert(meta.id.to_string());
            ref_ctx.script_names.insert(meta.name.clone());
        }
    }
    for name in wf_workflow::registry::WorkflowRegistry::global()
        .scripts()
        .list()
    {
        ref_ctx.script_names.insert(name);
    }

    let mut workflow_ids: HashSet<String> = HashSet::new();
    let mut workflow_graphs: HashMap<String, wf_types::workflow_execution::WorkflowGraphStructure> =
        HashMap::new();
    if let Ok(stored) = ctx.storage.workflow.list(None).await {
        for wf in &stored {
            workflow_ids.insert(wf.id.to_string());
            workflow_graphs.insert(wf.id.to_string(), definition_to_graph(wf));
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
    ref_ctx.workflow_ids = workflow_ids;
    ref_ctx.workflow_graphs = workflow_graphs;

    for id in ctx.registries.trigger_templates.list() {
        ref_ctx.trigger_ids.insert(id);
    }
    if let Ok(stored) = ctx.storage.trigger_template.list(None).await {
        for meta in &stored {
            ref_ctx.trigger_ids.insert(meta.id.to_string());
            ref_ctx.trigger_ids.insert(meta.name.clone());
        }
    }

    ref_ctx
}

/// Formal validation: shape plus graph plus reference closure. Warnings
/// allow registration; errors reject it. Returns warnings on success.
pub async fn validate_workflow_for_publish(
    ctx: &ApiContext,
    workflow: &WorkflowDefinition,
) -> crate::ApiResult<Vec<wf_workflow::validation::ValidationError>> {
    wf_config::processor::workflow::validate_workflow_definition(workflow)
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    let graph = definition_to_graph(workflow);
    let ref_ctx = build_reference_context(ctx).await;
    match wf_workflow::validation::GraphValidator::validate_with_reference_context(graph, &ref_ctx)
    {
        Ok((_, warnings)) => {
            let tool_report = validate_workflow_tool_lists(
                &workflow.id.to_string(),
                workflow.available_tools.as_ref(),
                &ref_ctx,
            );
            if !tool_report.errors.is_empty() {
                let detail = tool_report
                    .errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(ApiError::Validation(format!(
                    "workflow reference closure failed ({} error(s)): {}",
                    tool_report.errors.len(),
                    detail
                )));
            }
            let mut warnings = warnings;
            warnings.extend(tool_report.warnings);
            Ok(warnings)
        }
        Err(errors) => {
            let detail = errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            Err(ApiError::Validation(format!(
                "workflow reference closure failed ({} error(s)): {}",
                errors.len(),
                detail
            )))
        }
    }
}

/// Save a workflow and sync the registry (convenience wrapper).
pub async fn save_workflow(
    ctx: &ApiContext,
    workflow: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    validate_workflow(workflow)?;
    ctx.storage.workflow.save(workflow).await?;
    upsert_workflow_registry(&ctx.registries, workflow)?;
    Ok(())
}
