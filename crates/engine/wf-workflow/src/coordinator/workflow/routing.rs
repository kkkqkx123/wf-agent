use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_core::condition::ConditionEvaluator;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult, NodeInputShape};
use wf_execution_shared::types::execution_entity::ExecutionEntity as _;
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;

use super::WorkflowCoordinator;

pub(super) fn parse_node_type(node_type_str: &str) -> WorkflowResult<StaticNodeType> {
    match node_type_str {
        "START" => Ok(StaticNodeType::Start),
        "END" => Ok(StaticNodeType::End),
        "EMBED_START" => Ok(StaticNodeType::EmbedStart),
        "EMBED_END" => Ok(StaticNodeType::EmbedEnd),
        "VARIABLE" => Ok(StaticNodeType::Variable),
        "FORK" => Ok(StaticNodeType::Fork),
        "JOIN" => Ok(StaticNodeType::Join),
        "SYNC" => Ok(StaticNodeType::Sync),
        "SUBGRAPH" => Ok(StaticNodeType::Subgraph),
        "EMBED_GRAPH" => Ok(StaticNodeType::EmbedGraph),
        "SCRIPT" => Ok(StaticNodeType::Script),
        "INTERACTIVE_SCRIPT" => Ok(StaticNodeType::InteractiveScript),
        "LLM" => Ok(StaticNodeType::Llm),
        "TOOL_VISIBILITY" => Ok(StaticNodeType::ToolVisibility),
        "USER_INTERACTION" => Ok(StaticNodeType::UserInteraction),
        "ROUTE" => Ok(StaticNodeType::Route),
        "CONTEXT_PROCESSOR" => Ok(StaticNodeType::ContextProcessor),
        "LOOP_START" => Ok(StaticNodeType::LoopStart),
        "LOOP_END" => Ok(StaticNodeType::LoopEnd),
        "AGENT_LOOP" => Ok(StaticNodeType::AgentLoop),
        "START_FROM_MESSAGE" => Ok(StaticNodeType::StartFromMessage),
        "CONTINUE_FROM_MESSAGE" => Ok(StaticNodeType::ContinueFromMessage),
        // Unknown types are kept as plugin-contributed node types; handler
        // resolution falls back to the plugin source for them.
        other => Ok(StaticNodeType::Custom(other.to_string())),
    }
}

impl WorkflowCoordinator {
    pub(super) async fn determine_next_node_without_output(
        &self,
    ) -> WorkflowResult<Option<String>> {
        let current_id = match &self.current_node_id {
            Some(id) => id.clone(),
            None => return Ok(None),
        };

        let outgoing = self.traversal.get_outgoing_edges(&current_id);
        if outgoing.is_empty() {
            return Ok(None);
        }

        if outgoing.len() == 1 {
            return Ok(Some(outgoing[0].target_node_id.clone()));
        }

        for edge in &outgoing {
            if let Some(ref condition) = edge.condition {
                let mut context_map = HashMap::new();
                for entry in self.ctx.variables.iter() {
                    context_map.insert(entry.key().clone(), entry.value().clone());
                }
                match ConditionEvaluator::evaluate(condition, &context_map) {
                    Ok(true) => return Ok(Some(edge.target_node_id.clone())),
                    // An unevaluable edge keeps the "do not take this edge"
                    // semantics, but the defect is logged so broken edge
                    // conditions stay discoverable.
                    Ok(false) => continue,
                    Err(e) => {
                        tracing::warn!(
                            edge_from = %current_id,
                            edge_to = %edge.target_node_id,
                            error = %e,
                            "edge condition failed to evaluate; edge skipped"
                        );
                        continue;
                    }
                }
            } else {
                return Ok(Some(edge.target_node_id.clone()));
            }
        }

        Ok(None)
    }

    pub(super) async fn build_node_context(
        &self,
        node_id: &str,
        node_type: &StaticNodeType,
    ) -> WorkflowResult<NodeExecutionContext> {
        let (input, input_shape) = self.compute_node_input(node_id);

        let node = self.traversal.get_node(node_id);
        let node_name = node.and_then(|n| n.name.clone());
        let node_config = node.map(|n| n.inner.clone());

        let mut ctx = NodeExecutionContext::new(
            self.ctx.execution_id.clone(),
            node_id.to_string(),
            node_type.clone(),
            input,
            self.ctx.variables.clone(),
        );
        ctx.input_shape = input_shape;

        if let Some(name) = node_name {
            ctx = ctx.with_node_name(name);
        }
        if let Some(config) = node_config {
            ctx = ctx.with_node_config(config);
        }
        if let Some(ref parent_id) = self.ctx.parent_execution_id {
            ctx = ctx.with_parent_execution(parent_id.clone());
        }
        ctx.event_bus = self.ctx.event_bus.clone();
        ctx.handler_registry = Some(self.handlers.clone());
        ctx.graph_structure = Some(Arc::new(self.traversal.graph().clone()));
        ctx.tool_registry = Some(self.ctx.tool_registry.clone());
        ctx.resource_registries = self.ctx.resource_registries.clone();
        ctx.metrics = self.ctx.metrics.clone();
        ctx.token_tracker = self.ctx.token_tracker.clone();
        ctx.cancellation = self.entity.as_ref().map(|e| e.get_abort_signal());
        ctx.interruption = self.entity.as_ref().map(|e| e.interruption().clone());
        ctx.hook_handler_registry = self.ctx.hook_handler_registry.clone();
        ctx.tool_approval_handler = self.ctx.tool_approval_handler.clone();
        ctx.tool_approval_options = self.ctx.tool_approval_options.clone();
        ctx.fork_registries = self.ctx.fork_registries.clone();
        ctx.signal_bus = self.ctx.signal_bus.clone();
        // Carry the owning execution's resolved budgets so a TRIGGER
        // sub-workflow inherits the same source (entry config / limits)
        // rather than resetting to the engine fallback.
        ctx = ctx.with_parent_timeouts(
            self.ctx.options.node_timeout,
            self.ctx.options.max_execution_time,
        );

        // Message nodes execute trigger actions within one visit; give them a
        // shared session cache so consecutive actions can exchange state.
        if matches!(
            node_type,
            StaticNodeType::StartFromMessage | StaticNodeType::ContinueFromMessage
        ) {
            ctx.session_cache = Some(Arc::new(std::sync::Mutex::new(HashMap::new())));
        }

        Ok(ctx)
    }

    /// Compute a node's input and how it was assembled from incoming edges.
    ///
    /// Shape contract (aligned with `NodeInputShape`): a node with no incoming
    /// edges receives the workflow-level input; a node with exactly one
    /// incoming edge receives that source's raw output unwrapped (`Single`);
    /// a node with multiple incoming edges receives an object merging each
    /// edge's output keyed by source node id / edge label (`Merged`).
    pub(super) fn compute_node_input(&self, node_id: &str) -> (Value, NodeInputShape) {
        let incoming_edges = self.traversal.get_incoming_edges(node_id);

        if incoming_edges.is_empty() {
            return (
                self.ctx.options.input.clone().unwrap_or(Value::Null),
                NodeInputShape::None,
            );
        }

        let mut inputs = serde_json::Map::new();
        for edge in incoming_edges {
            if let Some(output) = self.node_outputs.get(&edge.source_node_id) {
                let key = edge.label.as_deref().unwrap_or(&edge.source_node_id);
                inputs.insert(key.to_string(), output.clone());
            }
        }

        if inputs.len() == 1 {
            (
                inputs.values().next().cloned().unwrap_or(Value::Null),
                NodeInputShape::Single,
            )
        } else {
            (Value::Object(inputs), NodeInputShape::Merged)
        }
    }

    pub(super) async fn determine_next_node(
        &self,
        result: &NodeExecutionResult,
    ) -> WorkflowResult<Option<String>> {
        if !result.next_node_ids.is_empty() {
            return Ok(result.next_node_ids.first().cloned());
        }

        let current_id = match &self.current_node_id {
            Some(id) => id.clone(),
            None => return Ok(None),
        };
        let outgoing = self.traversal.get_outgoing_edges(&current_id);

        if outgoing.is_empty() {
            return Ok(None);
        }

        if self.traversal.is_end_node(&current_id) {
            return Ok(None);
        }

        if outgoing.len() == 1 {
            let edge = &outgoing[0];
            return Ok(Some(edge.target_node_id.clone()));
        }

        for edge in &outgoing {
            if let Some(ref condition) = edge.condition {
                let mut context_map = HashMap::new();
                for entry in self.ctx.variables.iter() {
                    context_map.insert(entry.key().clone(), entry.value().clone());
                }
                match ConditionEvaluator::evaluate(condition, &context_map) {
                    Ok(true) => return Ok(Some(edge.target_node_id.clone())),
                    // An unevaluable edge keeps the "do not take this edge"
                    // semantics, but the defect is logged so broken edge
                    // conditions stay discoverable.
                    Ok(false) => continue,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "edge condition failed to evaluate; edge skipped"
                        );
                        continue;
                    }
                }
            } else {
                return Ok(Some(edge.target_node_id.clone()));
            }
        }

        Ok(None)
    }

    pub(super) fn compute_final_output(&self) -> Value {
        let end_ids = self.traversal.end_node_ids();
        if end_ids.is_empty() {
            return Value::Null;
        }

        let mut outputs = serde_json::Map::new();
        for id in end_ids {
            if let Some(output) = self.node_outputs.get(id) {
                outputs.insert(id.clone(), output.clone());
            }
        }

        if outputs.len() == 1 {
            outputs.values().next().cloned().unwrap_or(Value::Null)
        } else if outputs.is_empty() {
            Value::Null
        } else {
            Value::Object(outputs)
        }
    }
}
