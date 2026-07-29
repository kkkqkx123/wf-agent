use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Semaphore;
use wf_core::registry::ConcurrentRegistry;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;
use crate::executor::WorkflowExecutor;
use crate::handler::NodeHandler;

pub type WorkflowGraphRegistry = ConcurrentRegistry<WorkflowGraphStructure>;
pub type WorkflowExecutionRegistry = ConcurrentRegistry<WorkflowExecutionEntity>;

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

    pub async fn execute(
        &self,
        executor: &WorkflowExecutor,
        workflow_id: wf_types::Id,
        graph: WorkflowGraphStructure,
        options: WorkflowExecutionOptions,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
        handlers: Option<Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>>,
    ) -> WorkflowResult<serde_json::Value> {
        let _permit = self.semaphore.acquire().await
            .expect("semaphore closed");
        executor.execute_workflow(workflow_id, graph, options, tool_registry, handlers).await
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
