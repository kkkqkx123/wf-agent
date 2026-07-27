use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_common::now;
use wf_core::EventBus;
use wf_execution_shared::condition::ConditionEvaluator;
use wf_execution_shared::context::{ExecutorContext, NodeExecutionContext, NodeExecutionResult};
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::error::{WorkflowError, WorkflowResult};
use crate::graph::GraphTraversal;
use crate::handler::NodeHandler;

fn parse_node_type(node_type_str: &str) -> WorkflowResult<StaticNodeType> {
    match node_type_str {
        "START" => Ok(StaticNodeType::Start),
        "END" => Ok(StaticNodeType::End),
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
        "START_FROM_TRIGGER" => Ok(StaticNodeType::StartFromTrigger),
        "CONTINUE_FROM_TRIGGER" => Ok(StaticNodeType::ContinueFromTrigger),
        other => Err(WorkflowError::HandlerNotFound {
            node_type: other.to_string(),
        }),
    }
}

pub struct WorkflowCoordinator {
    ctx: ExecutorContext,
    traversal: GraphTraversal,
    handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    current_node_id: Option<String>,
    completed_nodes: Vec<String>,
    node_outputs: HashMap<String, Value>,
    node_errors: Vec<String>,
    start_time: i64,
}

impl WorkflowCoordinator {
    pub fn new(
        ctx: ExecutorContext,
        graph: WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    ) -> WorkflowResult<Self> {
        let traversal = GraphTraversal::new(graph)?;
        let start_node_id = traversal.start_node_id()
            .ok_or_else(|| WorkflowError::GraphError("Start node not found".to_string()))?
            .to_string();

        Ok(Self {
            ctx,
            traversal,
            handlers,
            current_node_id: Some(start_node_id),
            completed_nodes: Vec::new(),
            node_outputs: HashMap::new(),
            node_errors: Vec::new(),
            start_time: now(),
        })
    }

    pub async fn execute(&mut self) -> WorkflowResult<Value> {
        self.emit_event(EventType::WorkflowExecutionStarted, serde_json::json!({
            "workflow_id": self.ctx.workflow_id,
        })).await?;

        while let Some(node_id) = &self.current_node_id {
            if self.ctx.options.max_steps.is_some_and(|max| self.completed_nodes.len() as u32 >= max) {
                break;
            }

            let node = self.traversal.get_node(node_id)
                .ok_or_else(|| WorkflowError::GraphError(
                    format!("Node {} not found in graph", node_id),
                ))?;

            let node_type = parse_node_type(&node.node_type)?;

            let mut node_ctx = self.build_node_context(node_id, &node_type).await?;

            let handler = self.handlers.get(&node_type)
                .ok_or_else(|| WorkflowError::HandlerNotFound {
                    node_type: node.node_type.clone(),
                })?;

            self.emit_event(EventType::NodeStarted, serde_json::json!({
                "node_id": node_id,
                "node_type": &node.node_type,
            })).await?;

            let execute_start = now();
            let result = handler.execute(&mut node_ctx).await;
            let execute_time = now() - execute_start;

            match result {
                Ok(output) => {
                    self.node_outputs.insert(node_id.clone(), output.output.clone());
                    self.completed_nodes.push(node_id.clone());

                    for (k, v) in &output.metadata {
                        self.ctx.variables.insert(k.clone(), v.clone());
                    }

                    self.emit_event(EventType::NodeCompleted, serde_json::json!({
                        "node_id": node_id,
                        "execution_time": execute_time,
                    })).await?;

                    self.current_node_id = self.determine_next_node(&output).await?;
                }
                Err(e) => {
                    self.node_errors.push(format!("Node {}: {}", node_id, e));
                    self.emit_event(EventType::NodeFailed, serde_json::json!({
                        "node_id": node_id,
                        "error": e.to_string(),
                    })).await?;

                    return Err(WorkflowError::NodeExecutionFailed {
                        node_id: node_id.clone(),
                        reason: e.to_string(),
                    });
                }
            }
        }

        let result = self.compute_final_output();
        let execution_time = now() - self.start_time;

        self.emit_event(EventType::WorkflowExecutionCompleted, serde_json::json!({
            "execution_time": execution_time,
            "node_count": self.completed_nodes.len(),
        })).await?;

        Ok(result)
    }

    async fn build_node_context(
        &self,
        node_id: &str,
        node_type: &StaticNodeType,
    ) -> WorkflowResult<NodeExecutionContext> {
        let input = self.compute_node_input(node_id);

        let mut ctx = NodeExecutionContext::new(
            self.ctx.execution_id.clone(),
            node_id.to_string(),
            node_type.clone(),
            input,
            self.ctx.variables.clone(),
        );

        if let Some(ref parent_id) = self.ctx.parent_execution_id {
            ctx = ctx.with_parent_execution(parent_id.clone());
        }

        Ok(ctx)
    }

    fn compute_node_input(&self, node_id: &str) -> Value {
        let incoming_edges = self.traversal.get_incoming_edges(node_id);

        if incoming_edges.is_empty() {
            return self.ctx.options.input.clone().unwrap_or(Value::Null);
        }

        let mut inputs = serde_json::Map::new();
        for edge in incoming_edges {
            if let Some(output) = self.node_outputs.get(&edge.source_node_id) {
                let key = edge.label.as_deref().unwrap_or(&edge.source_node_id);
                inputs.insert(key.to_string(), output.clone());
            }
        }

        if inputs.len() == 1 {
            inputs.values().next().cloned().unwrap_or(Value::Null)
        } else {
            Value::Object(inputs)
        }
    }

    async fn determine_next_node(
        &self,
        result: &NodeExecutionResult,
    ) -> WorkflowResult<Option<String>> {
        if !result.next_node_ids.is_empty() {
            return Ok(result.next_node_ids.first().cloned());
        }

        let current_id = self.current_node_id.as_ref().unwrap();
        let outgoing = self.traversal.get_outgoing_edges(current_id);

        if outgoing.is_empty() {
            return Ok(None);
        }

        if self.traversal.is_end_node(current_id) {
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
                    Ok(false) => continue,
                    Err(_) => continue,
                }
            } else {
                return Ok(Some(edge.target_node_id.clone()));
            }
        }

        Ok(None)
    }

    fn compute_final_output(&self) -> Value {
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

    async fn emit_event(&self, event_type: EventType, data: serde_json::Value) -> WorkflowResult<()> {
        let event = BaseEvent {
            id: wf_types::Id::new(),
            r#type: event_type,
            timestamp: now(),
            workflow_id: Some(self.ctx.workflow_id.clone()),
            execution_id: Some(self.ctx.execution_id.clone()),
            agent_loop_id: None,
            metadata: Some({
                let mut m = std::collections::HashMap::new();
                m.insert("data".to_string(), data);
                m
            }),
        };

        self.ctx.event_bus.publish(event).map_err(|e| {
            WorkflowError::Internal(format!("Event publish failed: {}", e))
        })?;

        Ok(())
    }
}
