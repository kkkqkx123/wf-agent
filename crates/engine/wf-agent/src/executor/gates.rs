use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, ParentLink};
use wf_tools::registry::ToolRegistry;

use crate::error::{AgentError, AgentResult};

use super::AgentLoopExecutor;

impl AgentLoopExecutor {
    pub(super) fn gate_input_with_config(
        config: &AgentLoopConfig,
        input: &AgentLoopInput,
    ) -> AgentResult<()> {
        Self::gate_input_with_config_and_schema(input, config.context_schema.as_deref())
    }

    pub(super) fn gate_input_with_config_and_schema(
        input: &AgentLoopInput,
        schema: Option<&[wf_types::TemplateVariableDefinition]>,
    ) -> AgentResult<()> {
        match crate::validation::AgentLoopValidator::validate_or_fail_input_with_schema(
            input, schema,
        ) {
            Ok(()) => Ok(()),
            Err(errors) => {
                let detail = errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                Err(AgentError::Validation(format!(
                    "agent input validation failed ({} error(s)): {detail}",
                    errors.len()
                )))
            }
        }
    }

    /// Execution gate: reject error-level config issues before the loop
    /// starts instead of failing piecemeal mid-execution. Warnings pass.
    pub(super) fn gate_config(
        config: &AgentLoopConfig,
        registry: &ToolRegistry,
        cap: u32,
    ) -> AgentResult<()> {
        match crate::validation::AgentLoopValidator::validate_or_fail_with_cap(
            config, registry, cap,
        ) {
            Ok(_) => Ok(()),
            Err(errors) => {
                let detail = errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                Err(AgentError::Validation(format!(
                    "agent config validation failed ({} error(s)): {}",
                    errors.len(),
                    detail
                )))
            }
        }
    }

    /// Shared sub-agent depth gate for the sync and spawn paths: a child
    /// whose resolved depth would exceed `max_sub_agent_depth` is rejected
    /// before any slot is reserved. The depth resolves from the typed link;
    /// a missing link runs as a root and the entity build stays
    /// authoritative over the final association.
    pub(super) fn check_sub_agent_depth(&self, parent: Option<&ParentLink>) -> AgentResult<()> {
        let parent_depth = parent.map(|link| link.manager.depth()).unwrap_or(0);
        if !self.agent_registry.depth_allowed(parent_depth) {
            return Err(AgentError::HierarchyLimitReached(format!(
                "sub-agent depth {} exceeds max {}",
                parent_depth.saturating_add(1),
                self.agent_registry.max_sub_agent_depth()
            )));
        }
        Ok(())
    }

    pub(super) fn inherit_file_scope(
        mut config: AgentLoopConfig,
        parent: Option<&ParentLink>,
    ) -> AgentResult<AgentLoopConfig> {
        let parent_scope = parent.and_then(|link| link.file_scope.inner.clone());
        let Some(parent_scope) = parent_scope else {
            return Ok(config);
        };
        let child_resolved = wf_checkpoint::resolve_execution_scope_checked(
            None,
            Some(&parent_scope),
            None,
            config.file_scope.as_ref(),
            None,
        )
        .map_err(AgentError::Validation)?;
        config.file_scope = Some(wf_types::config::file_scope::ExecutionFileScope {
            enabled: Some(child_resolved.enabled),
            read_prefixes: Some(child_resolved.read_prefixes.clone()),
            write_prefixes: Some(child_resolved.write_prefixes.clone()),
            checkpoint_prefixes: Some(child_resolved.checkpoint_prefixes.clone()),
            workdir: child_resolved.workdir.clone(),
        });
        Ok(config)
    }
}
