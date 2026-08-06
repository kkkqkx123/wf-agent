//! Agent decision graph analysis (TS `AgentDecisionGraphAPI` counterpart).
//!
//! Pure query functions over the agent loop's iteration history: the
//! per-iteration decision sequence (LLM-only vs. tool calls), the ordered
//! tool-selection chain, explored vs. unexplored branches (registered tools
//! never called) and a path-efficiency ratio (tool calls per iteration).

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// One tool call of an iteration.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallView {
    pub name: String,
    pub duration_ms: i64,
    pub success: bool,
}

/// The decision the agent made in one iteration.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionNode {
    pub iteration: u32,
    /// The primary action of the iteration: `llm` or the first tool called.
    pub decision: String,
    pub tool_calls: Vec<ToolCallView>,
    pub duration_ms: i64,
}

/// Decision graph of an agent loop execution.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionGraph {
    pub agent_loop_id: String,
    pub iterations: Vec<AgentDecisionNode>,
    /// The ordered tool-selection chain across all iterations.
    pub tool_sequence: Vec<String>,
    /// Distinct tools that were actually called.
    pub explored_branches: u32,
    /// Registered tools available to the agent but never called.
    pub unexplored_branches: Vec<String>,
    /// Tool calls per iteration (>= 1 means the agent leveraged tools; lower
    /// values hint at LLM-only loops).
    pub path_efficiency: f64,
}

/// Agent decision-graph queries.
pub struct AgentGraphApi {
    ctx: Arc<ApiContext>,
}

impl AgentGraphApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Build the decision graph from the live entity's iteration history, or
    /// the persisted `AgentExecution` record when the loop is gone.
    pub async fn analyze(&self, agent_loop_id: &str) -> ApiResult<AgentDecisionGraph> {
        let iterations: Vec<(u32, i64, Vec<ToolCallView>)> =
            if let Some(entity) = self.ctx.agent_loop(agent_loop_id) {
                let state = entity.state.read().await;
                state
                    .iteration_history()
                    .iter()
                    .map(|record| {
                        let duration = record
                            .end_time
                            .map(|end| (end - record.start_time).max(0))
                            .unwrap_or(0);
                        let calls = record
                            .tool_calls
                            .iter()
                            .map(|call| ToolCallView {
                                name: call.name.clone(),
                                duration_ms: call.duration_ms,
                                success: call.success,
                            })
                            .collect();
                        (record.iteration, duration, calls)
                    })
                    .collect()
            } else {
                let record = self
                    .ctx
                    .storage
                    .agent_execution
                    .load(agent_loop_id)
                    .await?
                    .ok_or_else(|| ApiError::execution_not_found(agent_loop_id))?;
                record
                    .iteration_history
                    .unwrap_or_default()
                    .into_iter()
                    .map(|iteration| {
                        let duration = iteration
                            .completed_at
                            .map(|end| (end - iteration.started_at).max(0))
                            .unwrap_or(0);
                        let calls = iteration
                            .tool_calls
                            .unwrap_or_default()
                            .into_iter()
                            .map(|call| ToolCallView {
                                name: call.name,
                                duration_ms: call
                                    .completed_at
                                    .map(|end| (end - call.started_at).max(0))
                                    .unwrap_or(0),
                                success: call.error.is_none(),
                            })
                            .collect();
                        (iteration.iteration, duration, calls)
                    })
                    .collect()
            };

        let nodes: Vec<AgentDecisionNode> = iterations
            .into_iter()
            .map(|(iteration, duration, calls)| {
                let decision = calls
                    .first()
                    .map(|call| format!("tool:{}", call.name))
                    .unwrap_or_else(|| "llm".to_string());
                AgentDecisionNode {
                    iteration,
                    decision,
                    tool_calls: calls,
                    duration_ms: duration,
                }
            })
            .collect();

        let tool_sequence: Vec<String> = nodes
            .iter()
            .flat_map(|node| node.tool_calls.iter().map(|call| call.name.clone()))
            .collect();
        let explored: std::collections::BTreeSet<String> = tool_sequence.iter().cloned().collect();
        let explored_branches = explored.len() as u32;

        let unexplored_branches = self.unexplored_tools(agent_loop_id, &explored).await;

        let total_tool_calls = tool_sequence.len();
        let total_iterations = nodes.len().max(1) as f64;
        let path_efficiency = total_tool_calls as f64 / total_iterations;

        Ok(AgentDecisionGraph {
            agent_loop_id: agent_loop_id.to_string(),
            iterations: nodes,
            tool_sequence,
            explored_branches,
            unexplored_branches,
            path_efficiency,
        })
    }

    /// Tools registered in the shared registry (restricted by the loop's
    /// available set when the live entity carries one) that were never called.
    async fn unexplored_tools(
        &self,
        agent_loop_id: &str,
        explored: &std::collections::BTreeSet<String>,
    ) -> Vec<String> {
        let available: Vec<String> = match self.ctx.agent_loop(agent_loop_id) {
            Some(entity) => {
                let names = entity.available_tool_names();
                if names.is_empty() {
                    self.ctx
                        .tool_registry
                        .list_tools()
                        .into_iter()
                        .map(|tool| tool.name)
                        .collect()
                } else {
                    names.to_vec()
                }
            }
            None => self
                .ctx
                .tool_registry
                .list_tools()
                .into_iter()
                .map(|tool| tool.name)
                .collect(),
        };
        let mut unexplored: Vec<String> = available
            .into_iter()
            .filter(|name| !explored.contains(name))
            .collect();
        unexplored.sort();
        unexplored
    }

    /// Tool-call frequency across the iterations (for the analysis views).
    pub fn tool_frequency(&self, graph: &AgentDecisionGraph) -> BTreeMap<String, u32> {
        let mut frequency = BTreeMap::new();
        for name in &graph.tool_sequence {
            *frequency.entry(name.clone()).or_insert(0) += 1;
        }
        frequency
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_agent::entity::AgentLoopEntity;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn builds_decision_graph_from_live_entity() {
        let ctx = make_ctx();
        let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
            "agent-graph-1".to_string(),
        )));
        entity.state.write().await.start();
        entity.state.write().await.start_iteration();
        entity
            .state
            .write()
            .await
            .record_tool_call("http", 10, true);
        entity.state.write().await.end_iteration();
        entity.state.write().await.start_iteration();
        entity.state.write().await.end_iteration();
        ctx.agent_loops.register(entity.clone());

        let api = AgentGraphApi::new(ctx);
        let graph = api.analyze("agent-graph-1").await.unwrap();
        assert_eq!(graph.iterations.len(), 2);
        assert_eq!(graph.iterations[0].decision, "tool:http");
        assert_eq!(graph.iterations[1].decision, "llm");
        assert_eq!(graph.tool_sequence, vec!["http"]);
        assert_eq!(graph.explored_branches, 1);
        assert_eq!(graph.path_efficiency, 0.5);
    }

    #[tokio::test]
    async fn unknown_loop_is_not_found() {
        let ctx = make_ctx();
        let api = AgentGraphApi::new(ctx);
        let err = api.analyze("missing").await.unwrap_err();
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }
}
