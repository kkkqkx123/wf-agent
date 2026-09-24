//! Workflow execution options at the composition boundary.
//!
//! Canonical home of the workflow defaults merge (caller options win, stored
//! `WorkflowConfig` supplies defaults, final fallback enables checkpoints).
//! The execution module stays a thin executor: it loads the definition,
//! delegates the merge here, then persists the resolved options on the entity.

use serde_json::Value;
use wf_types::workflow_execution::WorkflowExecutionOptions;

/// Apply stored workflow defaults under caller options.
pub fn apply_workflow_config_defaults(
    options: &mut WorkflowExecutionOptions,
    config: &wf_types::workflow::WorkflowConfig,
) {
    if options.max_steps.is_none() {
        options.max_steps = config.max_steps;
    }
    if options.timeout.is_none() {
        options.timeout = config.timeout;
    }
    if options.enable_checkpoints.is_none() {
        if let Some(checkpoint) = config.checkpoint.as_ref() {
            options.enable_checkpoints = Some(checkpoint.enabled);
        }
    }
}

/// Empty options (all unset) used as the merge base so stored defaults can
/// win over the process default. The final fallback (`enable_checkpoints`)
/// is applied after the merge.
pub fn empty_options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: None,
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

/// Pure merge: caller options over stored definition defaults plus input
/// fallback. No storage or entity access here.
pub fn resolve_options(
    definition: Option<&wf_types::workflow::WorkflowDefinition>,
    input: Option<Value>,
    options: Option<WorkflowExecutionOptions>,
) -> WorkflowExecutionOptions {
    let mut merged = options.unwrap_or_else(empty_options);
    if let Some(definition) = definition {
        if let Some(config) = definition.config.as_ref() {
            apply_workflow_config_defaults(&mut merged, config);
        }
    }
    if merged.enable_checkpoints.is_none() {
        merged.enable_checkpoints = Some(true);
    }
    if merged.input.is_none() {
        merged.input = input;
    }
    merged
}

/// Workflow definition hooks into executable definitions.
pub fn workflow_hooks_to_definitions(
    hooks: &[wf_types::hook::HookPointConfig],
) -> Vec<wf_execution_shared::hooks::types::HookDefinition> {
    hooks.iter().map(Into::into).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition_with(
        timeout: Option<u64>,
        max_steps: Option<u32>,
        checkpoints: Option<bool>,
    ) -> wf_types::workflow::WorkflowDefinition {
        wf_types::workflow::WorkflowDefinition {
            id: "wf-1".into(),
            name: "wf".into(),
            description: None,
            r#type: None,
            version: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            config: Some(wf_types::workflow::WorkflowConfig {
                timeout,
                max_steps,
                checkpoint: checkpoints.map(|enabled| {
                    wf_types::checkpoint::workflow::WorkflowCheckpointConfig {
                        enabled,
                        interval_nodes: None,
                        on_error: None,
                        on_completion: None,
                        content: None,
                    }
                }),
                retry_policy: None,
                tool_approval: None,
                available_tools: None,
                initial_messages: None,
                system_prompt_template_id: None,
                system_prompt_template_variables: None,
                system_prompt: None,
                static_contexts: None,
                error_default: None,
            }),
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            available_tools: None,
            hooks: None,
        }
    }

    #[test]
    fn stored_defaults_fill_unset_caller_fields() {
        let definition = definition_with(Some(1000), Some(9), Some(false));
        let merged = resolve_options(Some(&definition), None, None);
        assert_eq!(merged.timeout, Some(1000));
        assert_eq!(merged.max_steps, Some(9));
        assert_eq!(merged.enable_checkpoints, Some(false));
    }

    #[test]
    fn caller_fields_win_over_stored_defaults() {
        let definition = definition_with(Some(1000), Some(9), Some(false));
        let caller = WorkflowExecutionOptions {
            timeout: Some(5),
            ..empty_options()
        };
        let merged = resolve_options(Some(&definition), None, Some(caller));
        assert_eq!(merged.timeout, Some(5));
        assert_eq!(merged.max_steps, Some(9));
    }

    #[test]
    fn checkpoints_default_to_enabled_without_definition() {
        let merged = resolve_options(None, None, None);
        assert_eq!(merged.enable_checkpoints, Some(true));
    }
}
