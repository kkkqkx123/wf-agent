use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Semaphore;
use wf_core::registry::{ConcurrentRegistry, MutableRegistry, Registry, RegistryResult};
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

/// Thread-safe registry of named scripts.
///
/// A local instance can be injected into a `TriggerContext` for isolation
/// (unit tests) instead of the process-wide default used by production
/// wiring, so tests never share or clobber script names.
pub struct ScriptRegistry {
    scripts: dashmap::DashMap<String, ScriptDefinition>,
}

impl ScriptRegistry {
    pub fn new() -> Self {
        Self {
            scripts: dashmap::DashMap::new(),
        }
    }

    /// Register a script definition (convenience method).
    pub fn register_script(&self, name: &str, language: &str, code: &str) {
        self.scripts.insert(
            name.to_string(),
            ScriptDefinition {
                language: language.to_string(),
                code: code.to_string(),
            },
        );
    }

    pub fn get(&self, name: &str) -> Option<ScriptDefinition> {
        self.scripts.get(name).map(|entry| entry.value().clone())
    }
}

impl Default for ScriptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Registry<ScriptDefinition> implementation ──

impl Registry<ScriptDefinition> for ScriptRegistry {
    fn get(&self, key: &str) -> Option<Arc<ScriptDefinition>> {
        self.scripts.get(key).map(|entry| Arc::new(entry.value().clone()))
    }

    fn has(&self, key: &str) -> bool {
        self.scripts.contains_key(key)
    }

    fn list(&self) -> Vec<String> {
        self.scripts.iter().map(|entry| entry.key().clone()).collect()
    }

    fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }

    fn len(&self) -> usize {
        self.scripts.len()
    }
}

impl MutableRegistry<ScriptDefinition> for ScriptRegistry {
    fn register(&self, key: String, item: Arc<ScriptDefinition>) -> RegistryResult<()> {
        if self.scripts.contains_key(&key) {
            return Err(wf_core::registry::RegistryError::AlreadyExists { key });
        }
        self.scripts.insert(key, (*item).clone());
        Ok(())
    }

    fn register_or_replace(&self, key: String, item: Arc<ScriptDefinition>) -> Option<Arc<ScriptDefinition>> {
        self.scripts.insert(key, (*item).clone())
            .map(Arc::new)
    }

    fn unregister(&self, key: &str) -> Option<Arc<ScriptDefinition>> {
        self.scripts.remove(key).map(|(_, v)| Arc::new(v))
    }

    fn clear(&self) {
        self.scripts.clear();
    }
}

// ── WorkflowRegistry ──

/// Combined registry for workflow graphs and scripts.
///
/// Provides a process-wide default via [`WorkflowRegistry::global()`] and
/// supports isolated instances for testing via [`WorkflowRegistry::new()`].
pub struct WorkflowRegistry {
    graphs: WorkflowGraphRegistry,
    scripts: ScriptRegistry,
}

impl WorkflowRegistry {
    pub fn new() -> Self {
        Self {
            graphs: ConcurrentRegistry::new(),
            scripts: ScriptRegistry::new(),
        }
    }

    /// Access the process-wide default registry.
    pub fn global() -> &'static Self {
        static GLOBAL: std::sync::OnceLock<WorkflowRegistry> = std::sync::OnceLock::new();
        GLOBAL.get_or_init(Self::new)
    }

    pub fn graphs(&self) -> &WorkflowGraphRegistry {
        &self.graphs
    }

    pub fn scripts(&self) -> &ScriptRegistry {
        &self.scripts
    }

    /// Register a graph so a trigger action can execute it as a sub-workflow.
    pub fn register_graph(&self, workflow_id: &str, graph: WorkflowGraphStructure) {
        let _ = self.graphs.register(workflow_id.to_string(), Arc::new(graph));
    }

    /// Look up a previously registered graph by workflow id.
    pub fn lookup_graph(&self, workflow_id: &str) -> Option<WorkflowGraphStructure> {
        self.graphs
            .get(workflow_id)
            .map(|graph| graph.as_ref().clone())
    }

    /// Register a named script.
    pub fn register_script(&self, name: &str, language: &str, code: &str) {
        self.scripts.register_script(name, language, code);
    }

    /// Look up a previously registered script by name.
    pub fn lookup_script(&self, name: &str) -> Option<ScriptDefinition> {
        self.scripts.get(name)
    }
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Register a graph so a trigger action can execute it as a sub-workflow
/// (process-wide default).
pub fn register_graph(workflow_id: &str, graph: WorkflowGraphStructure) {
    WorkflowRegistry::global().register_graph(workflow_id, graph);
}

/// Look up a previously registered graph by workflow id (process-wide default).
pub fn lookup_graph(workflow_id: &str) -> Option<WorkflowGraphStructure> {
    WorkflowRegistry::global().lookup_graph(workflow_id)
}

/// Register a named script for trigger actions (process-wide default).
pub fn register_script(name: &str, language: &str, code: &str) {
    WorkflowRegistry::global().register_script(name, language, code);
}

/// Look up a previously registered script by name (process-wide default).
pub fn lookup_script(name: &str) -> Option<ScriptDefinition> {
    WorkflowRegistry::global().lookup_script(name)
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
        handlers: Option<Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>>,
        hooks: Vec<BaseHookDefinition>,
        resource_registries: Option<Arc<wf_resource::registry::ResourceRegistries>>,
    ) -> WorkflowResult<WorkflowOutput> {
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            crate::error::WorkflowError::CoordinatorError(
                "workflow execution pool is closed".to_string(),
            )
        })?;
        executor
            .execute_workflow(
                workflow_id,
                graph,
                options,
                tool_registry,
                handlers,
                hooks,
                resource_registries,
            )
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
