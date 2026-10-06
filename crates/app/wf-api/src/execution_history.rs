//! Unified history view over workflow and agent loop executions.
//!
//! Each engine answers the sections it actually records: node execution
//! trails and status transitions belong to the workflow engine, iteration and
//! context-evolution records to the agent engine, and the event timeline and
//! variable map are shared. Sections an engine does not record stay empty so
//! a consumer reads one shape for both engines instead of guessing which
//! fields are meaningful.

use std::collections::BTreeMap;

use serde::Serialize;
use wf_types::events::base::BaseEvent;
use wf_types::execution::ExecutionType;

use crate::agent::agent_loop_registry::{self, ContextEvolutionEntry, IterationDetail};
use crate::audit::{self, NodeExecutionAuditView};
use crate::entity::variable;
use crate::execution_hierarchy;
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::workflow::execution_state::{self, StateTransitionView};

/// Which history sections a query loads. Excluded sections stay empty in the
/// response rather than disappearing, so the payload shape never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionHistorySections {
    pub timeline: bool,
    pub nodes: bool,
    pub iterations: bool,
    pub variables: bool,
    pub context: bool,
    pub transitions: bool,
}

impl Default for ExecutionHistorySections {
    fn default() -> Self {
        Self::all()
    }
}

impl ExecutionHistorySections {
    /// Every section, which is what an absent `include` parameter selects.
    pub fn all() -> Self {
        Self {
            timeline: true,
            nodes: true,
            iterations: true,
            variables: true,
            context: true,
            transitions: true,
        }
    }

    /// Section names accepted by the `include` parameter. An unknown name is
    /// rejected rather than ignored, so a typo never silently narrows the
    /// history to less than the caller asked for.
    pub fn parse(include: Option<&str>) -> ApiResult<Self> {
        let Some(include) = include else {
            return Ok(Self::all());
        };
        let mut sections = Self {
            timeline: false,
            nodes: false,
            iterations: false,
            variables: false,
            context: false,
            transitions: false,
        };
        for name in include.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            match name {
                "timeline" => sections.timeline = true,
                "nodes" => sections.nodes = true,
                "iterations" => sections.iterations = true,
                "variables" => sections.variables = true,
                "context" => sections.context = true,
                "transitions" => sections.transitions = true,
                other => {
                    return Err(ApiError::Validation(format!(
                        "unknown history section [{other}]: expected one of \
                         timeline,nodes,iterations,variables,context,transitions"
                    )))
                }
            }
        }
        Ok(sections)
    }
}

/// Everything an execution recorded, grouped by section.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionHistoryView {
    pub execution_id: String,
    pub execution_type: ExecutionType,
    /// Cap on `timeline`: one read never carries more lifecycle events than
    /// this, and a run past the cap has its later events left out.
    pub timeline_limit: usize,
    /// Lifecycle events oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub timeline: Vec<BaseEvent>,
    /// Per-node trail; workflow executions only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub node_executions: Vec<NodeExecutionAuditView>,
    /// Per-iteration trail; agent loop executions only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub iterations: Vec<IterationDetail>,
    /// Variables bound to the execution, shared by both engines.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, serde_json::Value>,
    /// Context growth per iteration; agent loop executions only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub context_evolution: Vec<ContextEvolutionEntry>,
    /// Status transitions; workflow executions only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub status_transitions: Vec<StateTransitionView>,
}

/// The recorded history of one execution, limited to `sections`.
pub async fn history(
    ctx: &ApiContext,
    id: &str,
    sections: &ExecutionHistorySections,
) -> ApiResult<ExecutionHistoryView> {
    let execution_type = execution_hierarchy::execution_type(ctx, id).await?;
    let mut view = ExecutionHistoryView {
        execution_id: id.to_string(),
        execution_type: execution_type.clone(),
        timeline_limit: crate::infra::events::TIMELINE_LIMIT,
        timeline: Vec::new(),
        node_executions: Vec::new(),
        iterations: Vec::new(),
        variables: BTreeMap::new(),
        context_evolution: Vec::new(),
        status_transitions: Vec::new(),
    };

    match execution_type {
        ExecutionType::Workflow => {
            if sections.timeline {
                view.timeline = crate::infra::events::timeline(ctx, id).await?;
            }
            if sections.nodes {
                view.node_executions = audit::list_node_executions(ctx, id).await?;
            }
            if sections.transitions {
                view.status_transitions =
                    execution_state::workflow_execution_status_transitions(ctx, id).await?;
            }
        }
        ExecutionType::AgentLoop => {
            if sections.timeline {
                view.timeline = crate::infra::events::agent_timeline(ctx, id).await?;
            }
            if sections.iterations {
                view.iterations = agent_loop_registry::iteration_history(ctx, id).await?;
            }
            if sections.context {
                view.context_evolution = agent_loop_registry::context_evolution(ctx, id).await?;
            }
        }
    }

    if sections.variables {
        view.variables = variable::export(ctx, id).await?;
    }

    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_agent::entity::AgentLoopEntity;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;
    use wf_types::Id;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    async fn register_agent(ctx: &ApiContext, id: &str) {
        let entity = Arc::new(AgentLoopEntity::new(Id::from(id.to_string())));
        {
            let mut state = entity.state.write().await;
            state.start().unwrap();
            state.start_iteration();
            state.record_tool_call("search", 10, true);
            state.end_iteration();
            state.complete().unwrap();
        }
        let _ = ctx.agent_loops.register(entity);
    }

    #[test]
    fn include_selects_named_sections() {
        let sections = ExecutionHistorySections::parse(Some("timeline,iterations")).unwrap();
        assert!(sections.timeline);
        assert!(sections.iterations);
        assert!(!sections.nodes);
        assert!(!sections.variables);
        assert!(!sections.context);
        assert!(!sections.transitions);
    }

    #[test]
    fn absent_include_loads_every_section() {
        assert_eq!(
            ExecutionHistorySections::parse(None).unwrap(),
            ExecutionHistorySections::all()
        );
    }

    #[test]
    fn unknown_section_is_rejected() {
        let err = ExecutionHistorySections::parse(Some("timeline,nope")).unwrap_err();
        match err {
            ApiError::Validation(msg) => assert!(msg.contains("nope"), "{msg}"),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn agent_history_carries_iterations() {
        let ctx = make_ctx();
        register_agent(&ctx, "loop-h").await;

        let view = history(&ctx, "loop-h", &ExecutionHistorySections::all())
            .await
            .unwrap();
        assert_eq!(view.execution_id, "loop-h");
        assert_eq!(view.execution_type, ExecutionType::AgentLoop);
        assert_eq!(view.iterations.len(), 1);
        assert_eq!(view.iterations[0].tool_calls.len(), 1);
        // The workflow-only sections stay empty rather than borrowing data
        // from another engine.
        assert!(view.node_executions.is_empty());
        assert!(view.status_transitions.is_empty());
    }

    #[tokio::test]
    async fn excluded_sections_stay_empty() {
        let ctx = make_ctx();
        register_agent(&ctx, "loop-s").await;

        let sections = ExecutionHistorySections::parse(Some("variables")).unwrap();
        let view = history(&ctx, "loop-s", &sections).await.unwrap();
        assert!(view.iterations.is_empty());
        assert!(view.timeline.is_empty());
    }

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        assert!(history(&ctx, "missing", &ExecutionHistorySections::all())
            .await
            .is_err());
    }
}
