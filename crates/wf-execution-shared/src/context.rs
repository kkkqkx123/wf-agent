use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use wf_core::EventBus;
use wf_tools::registry::ToolRegistry;
use wf_types::Id;
use wf_types::workflow_execution::WorkflowExecutionOptions;

pub struct ExecutorContext {
    pub execution_id: Id,
    pub workflow_id: Id,
    pub event_bus: Arc<EventBus>,
    pub tool_registry: Arc<ToolRegistry>,
    pub variables: Arc<DashMap<String, Value>>,
    pub options: WorkflowExecutionOptions,
    pub parent_execution_id: Option<Id>,
}

impl ExecutorContext {
    pub fn new(
        execution_id: Id,
        workflow_id: Id,
        event_bus: Arc<EventBus>,
        tool_registry: Arc<ToolRegistry>,
        options: WorkflowExecutionOptions,
    ) -> Self {
        Self {
            execution_id,
            workflow_id,
            event_bus,
            tool_registry,
            variables: Arc::new(DashMap::new()),
            options,
            parent_execution_id: None,
        }
    }

    pub fn with_parent_execution(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }
}

pub struct NodeExecutionContext {
    pub execution_id: Id,
    pub node_id: String,
    pub node_type: wf_types::node::StaticNodeType,
    pub node_name: Option<String>,
    pub node_config: Option<Value>,
    pub input: Value,
    pub variables: Arc<DashMap<String, Value>>,
    pub parent_execution_id: Option<Id>,
    pub depth: u32,
}

impl NodeExecutionContext {
    pub fn new(
        execution_id: Id,
        node_id: String,
        node_type: wf_types::node::StaticNodeType,
        input: Value,
        variables: Arc<DashMap<String, Value>>,
    ) -> Self {
        Self {
            execution_id,
            node_id,
            node_type,
            node_name: None,
            node_config: None,
            input,
            variables,
            parent_execution_id: None,
            depth: 0,
        }
    }

    pub fn with_node_name(mut self, name: impl Into<String>) -> Self {
        self.node_name = Some(name.into());
        self
    }

    pub fn with_node_config(mut self, config: Value) -> Self {
        self.node_config = Some(config);
        self
    }

    pub fn with_parent_execution(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }

    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    pub fn get_variable(&self, name: &str) -> Option<Value> {
        self.variables.get(name).map(|v| v.clone())
    }

    pub fn set_variable(&self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }
}

pub struct NodeExecutionResult {
    pub output: Value,
    pub next_node_ids: Vec<String>,
    pub metadata: std::collections::HashMap<String, Value>,
}

impl NodeExecutionResult {
    pub fn simple(output: Value) -> Self {
        Self {
            output,
            next_node_ids: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_next_nodes(output: Value, next_node_ids: Vec<String>) -> Self {
        Self {
            output,
            next_node_ids,
            metadata: std::collections::HashMap::new(),
        }
    }
}
