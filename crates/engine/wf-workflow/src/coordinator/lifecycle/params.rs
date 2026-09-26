use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_execution_shared::hooks::types::HookDefinition;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::handler::NodeHandler;

use wf_resource::registry::ResourceRegistries;

pub struct WorkflowExecutionParams {
    pub execution_id: wf_types::Id,
    pub workflow_id: wf_types::Id,
    pub graph: WorkflowGraphStructure,
    pub options: WorkflowExecutionOptions,
    pub handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    pub tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    /// Shared resource registries injected into the execution context
    /// (template rendering in handlers; absent executions fall back to
    /// built-in texts).
    pub resource_registries: Option<Arc<ResourceRegistries>>,
    pub input: Option<Value>,
    pub hooks: Vec<HookDefinition>,
}
