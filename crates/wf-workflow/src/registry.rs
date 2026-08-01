use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Semaphore;
use wf_core::registry::{ConcurrentRegistry, MutableRegistry, Registry};
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_tools::callback::WorkflowOutput;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;
use crate::executor::WorkflowExecutor;
use crate::handler::NodeHandler;

pub type WorkflowGraphRegistry = ConcurrentRegistry<WorkflowGraphStructure>;
pub type WorkflowExecutionRegistry = ConcurrentRegistry<WorkflowExecutionEntity>;

/// A named script that can be executed from trigger actions.
#[derive(Debug, Clone)]
pub struct ScriptDefinition {
    pub language: String,
    pub code: String,
}

/// Process-wide registry of graphs addressable by workflow id, used by
/// ExecuteTriggeredSubworkflow trigger actions.
static TRIGGER_GRAPHS: std::sync::OnceLock<WorkflowGraphRegistry> = std::sync::OnceLock::new();

/// Process-wide registry of named scripts, used by ExecuteScript trigger
/// actions.
static SCRIPTS: std::sync::OnceLock<dashmap::DashMap<String, ScriptDefinition>> =
    std::sync::OnceLock::new();

/// Register a graph so a trigger action can execute it as a sub-workflow.
pub fn register_graph(workflow_id: &str, graph: WorkflowGraphStructure) {
    let registry = TRIGGER_GRAPHS.get_or_init(WorkflowGraphRegistry::new);
    let _ = registry.register(workflow_id.to_string(), Arc::new(graph));
}

/// Look up a previously registered graph by workflow id.
pub fn lookup_graph(workflow_id: &str) -> Option<WorkflowGraphStructure> {
    let registry = TRIGGER_GRAPHS.get()?;
    registry
        .get(workflow_id)
        .map(|graph| graph.as_ref().clone())
}

/// Register a named script for trigger actions.
pub fn register_script(name: &str, language: &str, code: &str) {
    let scripts = SCRIPTS.get_or_init(dashmap::DashMap::new);
    scripts.insert(
        name.to_string(),
        ScriptDefinition {
            language: language.to_string(),
            code: code.to_string(),
        },
    );
}

/// Look up a previously registered script by name.
pub fn lookup_script(name: &str) -> Option<ScriptDefinition> {
    SCRIPTS
        .get()
        .and_then(|scripts| scripts.get(name).map(|entry| entry.value().clone()))
}

pub struct WorkflowExecutionPool {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
}

impl WorkflowExecutionPool {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
        }
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        executor: &WorkflowExecutor,
        workflow_id: wf_types::Id,
        graph: WorkflowGraphStructure,
        options: WorkflowExecutionOptions,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
        handlers: Option<Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>>,
        hooks: Vec<BaseHookDefinition>,
    ) -> WorkflowResult<WorkflowOutput> {
        let _permit = self.semaphore.acquire().await.expect("semaphore closed");
        executor
            .execute_workflow(workflow_id, graph, options, tool_registry, handlers, hooks)
            .await
    }
}

impl Default for WorkflowExecutionPool {
    fn default() -> Self {
        Self::new(10)
    }
}

pub fn create_graph_registry() -> WorkflowGraphRegistry {
    ConcurrentRegistry::new()
}

pub fn create_execution_registry() -> WorkflowExecutionRegistry {
    ConcurrentRegistry::new()
}
