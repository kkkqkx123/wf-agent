use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use wf_core::EventBus;
use wf_metrics::MetricsRegistry;
use wf_tools::registry::ToolRegistry;
use wf_types::workflow_execution::WorkflowExecutionOptions;
use wf_types::Id;

pub struct ExecutorContext {
    pub execution_id: Id,
    pub workflow_id: Id,
    pub event_bus: Option<Arc<EventBus>>,
    pub tool_registry: Arc<ToolRegistry>,
    pub variables: Arc<DashMap<String, Value>>,
    pub options: WorkflowExecutionOptions,
    pub parent_execution_id: Option<Id>,
    pub metrics: Option<Arc<MetricsRegistry>>,
}

impl ExecutorContext {
    pub fn new(
        execution_id: Id,
        workflow_id: Id,
        event_bus: Option<Arc<EventBus>>,
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
            metrics: None,
        }
    }

    pub fn with_parent_execution(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
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
    pub event_bus: Option<Arc<EventBus>>,
    pub handler_registry: Option<Arc<dyn std::any::Any + Send + Sync>>,
    pub graph_structure: Option<Arc<dyn std::any::Any + Send + Sync>>,
    /// Shared metrics registry; absent when metrics are disabled.
    pub metrics: Option<Arc<MetricsRegistry>>,
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
            event_bus: None,
            handler_registry: None,
            graph_structure: None,
            metrics: None,
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

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
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
