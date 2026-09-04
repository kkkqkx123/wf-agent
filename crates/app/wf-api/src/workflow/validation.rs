//! Workflow validation utilities.

use std::collections::{HashMap, HashSet};

use wf_core::registry::Registry;
use wf_execution_shared::hooks::HookRegistry;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_workflow::reference_closure::{validate_workflow_tool_lists, ReferenceContext};

use crate::infra::validation::{ValidationContext, ValidationError, ValidationResult};
use crate::ApiContext;
use crate::ApiError;
use wf_types::WorkflowDefinition;

use super::definition::upsert_workflow_registry;
use super::workflow_execution::definition_to_graph;

/// Workflow-specific validator that uses the shared [`ValidationContext`].
///
/// Provides `validate()` (shape + graph, draft-friendly) and
/// `validate_for_publish()` (full reference closure + tool lists + hook
/// receiver checks).
pub struct WorkflowValidator<'a> {
    ctx: &'a ValidationContext,
}

impl<'a> WorkflowValidator<'a> {
    pub fn new(ctx: &'a ValidationContext) -> Self {
        Self { ctx }
    }

    /// Validate shape and graph structure only (draft-friendly, no external
    /// references). Returns a [`ValidationResult`] with errors/warnings.
    pub fn validate(&self, workflow: &WorkflowDefinition) -> ValidationResult {
        let mut result = ValidationResult::default();

        if let Err(e) = wf_config::processor::workflow::validate_workflow_definition(workflow) {
            result.push_error(ValidationError::new("definition", e.to_string()));
        }

        let graph = definition_to_graph(workflow);
        match wf_workflow::validation::GraphValidator::validate(graph) {
            Ok(_) => {}
            Err(errors) => {
                for e in errors {
                    result.push_error(ValidationError::new(e.field, e.message));
                }
            }
        }

        result
    }

    /// Full formal validation: shape + graph + reference closure + tool list
    /// + hook receiver checks. Warnings allow registration; errors reject it.
    pub fn validate_for_publish(&self, workflow: &WorkflowDefinition) -> ValidationResult {
        let mut result = self.validate(workflow);

        let graph = definition_to_graph(workflow);
        let ref_ctx = self.build_reference_context(workflow);
        let ref_report = wf_workflow::validation::GraphValidator::validate_with_reference_context(
            graph,
            &ref_ctx,
        );
        match ref_report {
            Ok((_, warnings)) => {
                for w in warnings {
                    result.push_warning(ValidationError::new(w.field, w.message));
                }
            }
            Err(errors) => {
                for e in errors {
                    result.push_error(ValidationError::new(e.field, e.message));
                }
                return result;
            }
        }

        let tool_report = validate_workflow_tool_lists(
            &workflow.id.to_string(),
            workflow.available_tools.as_ref(),
            &ref_ctx,
        );
        result.extend_errors(
            tool_report
                .errors
                .into_iter()
                .map(|e| ValidationError::new(e.field, e.message))
                .collect(),
        );
        result.extend_warnings(
            tool_report
                .warnings
                .into_iter()
                .map(|e| ValidationError::new(e.field, e.message))
                .collect(),
        );

        if let Some(tools) = &workflow.available_tools {
            result.extend_errors(crate::infra::validation::validate_tool_list(
                &tools.available,
                self.ctx,
            ));
        }

        result
    }

    /// Validate tool references in the workflow against the shared context.
    pub fn validate_tool_references(&self, tool_names: &[String]) -> Vec<ValidationError> {
        crate::infra::validation::validate_tool_list(tool_names, self.ctx)
    }

    /// Validate profile references in the workflow against the shared
    /// context.
    pub fn validate_profile_references(&self, profile_ids: &[String]) -> Vec<ValidationError> {
        crate::infra::validation::validate_profile_list(profile_ids, self.ctx)
    }

    fn build_reference_context(&self, _workflow: &WorkflowDefinition) -> ReferenceContext {
        let mut ref_ctx = ReferenceContext::new();

        for id in &self.ctx.profile_ids {
            ref_ctx.profile_ids.insert(id.clone());
        }
        for name in &self.ctx.tool_names {
            ref_ctx.tool_names.insert(name.clone());
            if self.ctx.disabled_tools.contains(name) {
                ref_ctx.disabled_tools.insert(name.clone());
            }
        }
        for name in &self.ctx.script_names {
            ref_ctx.script_names.insert(name.clone());
        }
        for id in &self.ctx.workflow_ids {
            ref_ctx.workflow_ids.insert(id.clone());
        }
        for id in &self.ctx.trigger_ids {
            ref_ctx.trigger_ids.insert(id.clone());
        }

        // Note: workflow_graphs is not populated here because the
        // ValidationContext does not store graph structures. Subgraph/embed
        // reference validation requires the full async build_reference_context
        // from the existing validate_workflow_for_publish function.
        ref_ctx
    }
}

/// Validate that all hook `receiver` names in the workflow definition are
/// present in the hook registry. Unknown receivers become errors.
fn validate_hook_receiver_references(
    workflow: &WorkflowDefinition,
    hook_registry: &HookRegistry,
    report: &mut wf_workflow::ReferenceClosureReport,
) {
    let Some(hooks) = workflow.hooks.as_ref() else {
        return;
    };
    for hook in hooks {
        if let Some(ref receiver) = hook.receiver {
            if !receiver.is_empty() && !hook_registry.contains(receiver) {
                report.errors.push(wf_workflow::validation::ValidationError::new(
                    format!("hooks.{}.receiver", hook.event_name),
                    format!(
                        "Hook '{}' references receiver '{}' which is not registered",
                        hook.event_name, receiver
                    ),
                ));
            }
        }
    }
}

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
        ref_ctx.trigger_ids.insert(id.clone());
        if let Some(template) = ctx.registries.trigger_templates.get(&id) {
            ref_ctx.trigger_templates.push((*template).clone());
        }
    }
    if let Ok(stored) = ctx.storage.trigger_template.list(None).await {
        for meta in &stored {
            ref_ctx.trigger_ids.insert(meta.id.to_string());
            ref_ctx.trigger_ids.insert(meta.name.clone());
            if let (Some(condition_val), Some(action_val)) = (&meta.condition, &meta.action_config)
            {
                if let (Ok(condition), Ok(action)) = (
                    serde_json::from_value::<wf_types::trigger::TriggerCondition>(
                        condition_val.clone(),
                    ),
                    serde_json::from_value::<wf_types::trigger::TriggerAction>(
                        action_val.clone(),
                    ),
                ) {
                    ref_ctx.trigger_templates.push(wf_types::trigger::TriggerTemplate {
                        name: meta.name.clone(),
                        description: meta.description.clone(),
                        condition: Some(condition),
                        action: Some(action),
                        enabled: Some(meta.enabled),
                        max_triggers: meta.max_triggers,
                        priority: meta.priority,
                        metadata: None,
                        created_at: meta.created_at,
                        updated_at: meta.updated_at,
                        create_checkpoint: None,
                        checkpoint_description_template: None,
                    });
                }
            }
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

    if let Some(ref config) = workflow.config {
        if let Some(ref template_id) = config.system_prompt_template_id {
            if !ctx.registries.templates.has(template_id) {
                return Err(ApiError::Validation(format!(
                    "workflow '{}' references prompt template '{}' which is not registered",
                    workflow.id, template_id
                )));
            }
        }
    }

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

            let mut receiver_report = wf_workflow::ReferenceClosureReport::default();
            if let Some(ref hook_registry) = ctx.hook_registry {
                validate_hook_receiver_references(
                    workflow,
                    hook_registry.as_ref(),
                    &mut receiver_report,
                );
            }
            if !receiver_report.errors.is_empty() {
                let detail = receiver_report
                    .errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(ApiError::Validation(format!(
                    "hook receiver validation failed ({} error(s)): {}",
                    receiver_report.errors.len(),
                    detail
                )));
            }

            let mut warnings = warnings;
            warnings.extend(tool_report.warnings);
            warnings.extend(receiver_report.warnings);
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
