use std::sync::Arc;

use wf_core::registry::{ConcurrentRegistry, MutableRegistry, Registry, RegistryResult};
use wf_script::ScriptFlow;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::entity::WorkflowExecutionEntity;

pub type WorkflowGraphRegistry = ConcurrentRegistry<WorkflowGraphStructure>;
pub type WorkflowExecutionRegistry = ConcurrentRegistry<WorkflowExecutionEntity>;
pub type WorkflowScriptFlowRegistry = ConcurrentRegistry<ScriptFlow>;

pub use wf_script::ScriptDefinition;

/// Thread-safe registry of named scripts.
///
/// Scripts are stored as full blueprint definitions (template, argument
/// declarations, security policy), so every execution path renders and
/// validates through the same definition. The language-plus-code entry
/// point only builds the ad-hoc shape of the same definition.
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
                name: name.to_string(),
                content: Some(code.to_string()),
                template: None,
                arguments: None,
                language: Some(language.to_string()),
                executor_mode: None,
                interactive: None,
                security_policy: None,
                description: None,
                enabled: None,
            },
        );
    }

    /// Register a full blueprint definition under its own name.
    pub fn register_definition(&self, definition: ScriptDefinition) {
        self.scripts.insert(definition.name.clone(), definition);
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
        self.scripts
            .get(key)
            .map(|entry| Arc::new(entry.value().clone()))
    }

    fn has(&self, key: &str) -> bool {
        self.scripts.contains_key(key)
    }

    fn list(&self) -> Vec<String> {
        self.scripts
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
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

    fn register_or_replace(
        &self,
        key: String,
        item: Arc<ScriptDefinition>,
    ) -> Option<Arc<ScriptDefinition>> {
        self.scripts.insert(key, (*item).clone()).map(Arc::new)
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
    flows: WorkflowScriptFlowRegistry,
}

impl WorkflowRegistry {
    pub fn new() -> Self {
        Self {
            graphs: ConcurrentRegistry::new(),
            scripts: ScriptRegistry::new(),
            flows: ConcurrentRegistry::new(),
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
        let _ = self
            .graphs
            .register(workflow_id.to_string(), Arc::new(graph));
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

    /// Register a full blueprint definition.
    pub fn register_definition(&self, definition: ScriptDefinition) {
        self.scripts.register_definition(definition);
    }

    /// Look up a previously registered script by name.
    pub fn lookup_script(&self, name: &str) -> Option<ScriptDefinition> {
        self.scripts.get(name)
    }

    /// Register a named script flow for `flow_id` references.
    pub fn register_flow(&self, flow: ScriptFlow) {
        let _ = self
            .flows
            .register_or_replace(flow.name.clone(), Arc::new(flow));
    }

    /// Look up a previously registered script flow by name.
    pub fn lookup_flow(&self, name: &str) -> Option<ScriptFlow> {
        self.flows.get(name).map(|flow| flow.as_ref().clone())
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

/// Register a full blueprint definition (process-wide default).
pub fn register_definition(definition: ScriptDefinition) {
    WorkflowRegistry::global().register_definition(definition);
}

/// Look up a previously registered script by name (process-wide default).
pub fn lookup_script(name: &str) -> Option<ScriptDefinition> {
    WorkflowRegistry::global().lookup_script(name)
}

/// Register a named script flow (process-wide default).
pub fn register_flow(flow: ScriptFlow) {
    WorkflowRegistry::global().register_flow(flow);
}

/// Look up a previously registered script flow (process-wide default).
pub fn lookup_flow(name: &str) -> Option<ScriptFlow> {
    WorkflowRegistry::global().lookup_flow(name)
}

pub fn create_graph_registry() -> WorkflowGraphRegistry {
    ConcurrentRegistry::new()
}

pub fn create_execution_registry() -> WorkflowExecutionRegistry {
    ConcurrentRegistry::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(start: &str) -> WorkflowGraphStructure {
        serde_json::from_str(&format!(
            r#"{{
                "nodes": [{{ "id": "{start}", "node_type": "Script" }}],
                "edges": [],
                "adjacency_list": {{}},
                "reverse_adjacency_list": {{}},
                "end_node_ids": ["{start}"]
            }}"#
        ))
        .expect("test graph must deserialize")
    }

    fn definition(name: &str) -> ScriptDefinition {
        serde_json::from_str(&format!(
            r#"{{ "name": "{name}", "content": "print(1)", "language": "lua" }}"#
        ))
        .expect("test script definition must deserialize")
    }

    #[test]
    fn script_registry_round_trips_definitions() {
        let registry = ScriptRegistry::new();
        registry.register_script("s1", "lua", "return 1");
        assert!(registry.get("s1").is_some());
        let got = registry.get("s1").expect("s1 must exist");
        assert_eq!(got.language.as_deref(), Some("lua"));
        assert_eq!(got.content.as_deref(), Some("return 1"));
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn mutable_registry_rejects_duplicate_keys() {
        let registry = ScriptRegistry::new();
        registry
            .register("dup".to_string(), Arc::new(definition("dup")))
            .expect("first register must succeed");
        assert!(matches!(
            registry.register("dup".to_string(), Arc::new(definition("dup"))),
            Err(wf_core::registry::RegistryError::AlreadyExists { .. })
        ));
    }

    #[test]
    fn mutable_registry_replace_unregister_and_clear() {
        let registry = ScriptRegistry::new();
        registry
            .register("k".to_string(), Arc::new(definition("k")))
            .expect("register");
        let old = registry
            .register_or_replace("k".to_string(), Arc::new(definition("k2")))
            .expect("replace returns the old value");
        assert_eq!(old.name, "k");
        assert_eq!(registry.len(), 1);
        assert!(registry.has("k"));

        let removed = registry.unregister("k").expect("unregister returns value");
        assert_eq!(removed.name, "k2");
        assert!(registry.is_empty());
        assert_eq!(registry.list().len(), 0);
        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn workflow_registry_stores_graphs_scripts_and_flows() {
        let registry = WorkflowRegistry::new();
        registry.register_graph("wf-1", graph("start"));
        assert!(registry.lookup_graph("wf-1").is_some());
        assert!(registry.lookup_graph("wf-2").is_none());

        registry.register_script("sc-1", "lua", "return 2");
        let script = registry.lookup_script("sc-1").expect("script");
        assert_eq!(script.content.as_deref(), Some("return 2"));

        registry.register_flow(ScriptFlow {
            name: "flow-1".to_string(),
            branches: vec![],
        });
        assert!(registry.lookup_flow("flow-1").is_some());
        // Re-registering a flow replaces it (register_or_replace semantics).
        registry.register_flow(ScriptFlow {
            name: "flow-1".to_string(),
            branches: vec![],
        });
        assert!(registry.lookup_flow("flow-1").is_some());
    }

    #[test]
    fn global_registry_helpers_use_the_shared_instance() {
        register_graph("global-wf", graph("n0"));
        assert!(lookup_graph("global-wf").is_some());
        register_script("global-sc", "lua", "return 3");
        assert!(lookup_script("global-sc").is_some());
        register_flow(ScriptFlow {
            name: "global-flow".to_string(),
            branches: vec![],
        });
        assert!(lookup_flow("global-flow").is_some());

        // The free functions must land in the same process-wide registry.
        assert!(WorkflowRegistry::global().lookup_graph("global-wf").is_some());
    }
}
