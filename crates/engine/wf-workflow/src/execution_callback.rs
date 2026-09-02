use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::registry::{MutableRegistry, Registry};
use wf_core::EventBus;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_execution_shared::hooks::HookRegistry;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_metrics::MetricsRegistry;
use wf_storage::backend::StorageBackend;
use wf_tools::callback::{
    AgentLoopConfig, AgentLoopInput, AgentLoopOutput, ExecutionCallback, ExecutionStatus,
    SpawnedWorkflow, WorkflowInput, WorkflowOutput,
};
use wf_tools::error::{ToolError, ToolResult};
use wf_tools::registry::ToolRegistry;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};
use wf_types::Id;

use crate::checkpoint::NodeCheckpointStrategy;
use crate::coordinator::{WorkflowExecutionParams, WorkflowLifecycleCoordinator};
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::{HandlerRegistry, NodeHandler};
use crate::registry::{
    create_execution_registry, create_graph_registry, WorkflowExecutionRegistry,
};

/// ExecutionCallback implementation backed by the workflow engine.
///
/// Registers workflow templates (graph by id) and launches executions through
/// `WorkflowLifecycleCoordinator`. Running executions are tracked in an
/// execution registry so their status can be queried and cancelled; terminal
/// outputs are kept in the result slot of each entity.
pub struct WorkflowExecutionCallback {
    graphs: crate::registry::WorkflowGraphRegistry,
    executions: Arc<WorkflowExecutionRegistry>,
    handlers: Option<Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>>,
    gateway: Arc<wf_llm::LlmGateway>,
    event_bus: Option<Arc<EventBus>>,
    /// Typed signal bus for internal workflow/agent signals.
    signal_bus: Option<Arc<InternalSignalBus>>,
    tool_registry: Arc<ToolRegistry>,
    checkpoint_strategy: Option<NodeCheckpointStrategy>,
    store: Arc<StorageBackend>,
    metrics: Option<Arc<MetricsRegistry>>,
    /// Shared hook receiver registry; hook points dispatch through it.
    hook_registry: Option<Arc<HookRegistry>>,
    hooks: Vec<BaseHookDefinition>,
    /// Per-workflow hooks, keyed by workflow id (from the workflow
    /// definition); overrides the shared `hooks` for that workflow.
    workflow_hooks: std::sync::RwLock<HashMap<String, Vec<BaseHookDefinition>>>,
    /// Shared sandbox runtime (global profiles + routing rules); injected
    /// into the script handlers of executions launched here. `None` uses
    /// per-handler defaults.
    sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
}

impl WorkflowExecutionCallback {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            graphs: create_graph_registry(),
            executions: Arc::new(create_execution_registry()),
            handlers: None,
            gateway: Arc::new(wf_llm::LlmGateway::new()),
            event_bus: None,
            signal_bus: None,
            tool_registry,
            checkpoint_strategy: None,
            store: Arc::new(StorageBackend::new_memory()),
            metrics: None,
            hook_registry: None,
            hooks: Vec::new(),
            workflow_hooks: std::sync::RwLock::new(HashMap::new()),
            sandbox: None,
        }
    }

    pub fn with_gateway(mut self, gateway: Arc<wf_llm::LlmGateway>) -> Self {
        self.gateway = gateway;
        self
    }

    pub fn with_handlers(
        mut self,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    ) -> Self {
        self.handlers = Some(handlers);
        self
    }

    /// Inject a shared sandbox runtime (compiled global profiles + routing
    /// rules) into the script handlers of executions launched here.
    pub fn with_sandbox(mut self, sandbox: Arc<wf_sandbox::SandboxRuntime>) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Inject the typed signal bus: control signals from trigger actions
    /// reach the coordinator loop of executions launched here.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    pub fn with_checkpoint_strategy(mut self, strategy: NodeCheckpointStrategy) -> Self {
        self.checkpoint_strategy = Some(strategy);
        self
    }

    pub fn with_store(mut self, store: Arc<StorageBackend>) -> Self {
        self.store = store;
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Inject the shared hook receiver registry into executions started
    /// through this callback (hook points + engine signals dispatch through
    /// it).
    pub fn with_hook_registry(mut self, registry: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(registry);
        self
    }

    pub fn with_hooks(mut self, hooks: Vec<BaseHookDefinition>) -> Self {
        self.hooks = hooks;
        self
    }

    pub fn register_workflow(&self, workflow_id: Id, graph: WorkflowGraphStructure) -> bool {
        self.register_workflow_with_hooks(workflow_id, graph, Vec::new())
    }

    /// Register a workflow graph together with its definition-level hooks.
    /// Executions of this workflow run the hooks (BEFORE_EXECUTE /
    /// AFTER_EXECUTE per node).
    pub fn register_workflow_with_hooks(
        &self,
        workflow_id: Id,
        graph: WorkflowGraphStructure,
        hooks: Vec<BaseHookDefinition>,
    ) -> bool {
        let registered = self
            .graphs
            .register(workflow_id.to_string(), Arc::new(graph))
            .is_ok();
        if registered {
            wf_common::lock::write_ok(self.workflow_hooks.write())
                .insert(workflow_id.to_string(), hooks);
        }
        registered
    }

    pub fn unregister_workflow(&self, workflow_id: &Id) {
        self.graphs.unregister(&workflow_id.to_string());
    }

    pub fn has_workflow(&self, workflow_id: &str) -> bool {
        self.graphs.has(workflow_id)
    }

    pub fn registered_workflows(&self) -> Vec<String> {
        self.graphs.list()
    }

    pub fn executions(&self) -> &Arc<WorkflowExecutionRegistry> {
        &self.executions
    }

    fn handlers(&self) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
        match &self.handlers {
            Some(h) => h.clone(),
            None => {
                let mut registry = HandlerRegistry::new();
                registry.register_defaults_with_sandbox(self.gateway.clone(), self.sandbox.clone());
                registry.into_arc()
            }
        }
    }

    /// Resolve the execution params for `workflow_id` with a fresh
    /// execution id. Fails when the workflow is not registered.
    fn build_params(
        &self,
        workflow_id: &str,
        execution_id: Id,
        options: WorkflowExecutionOptions,
        input: Option<Value>,
    ) -> WorkflowResult<WorkflowExecutionParams> {
        let graph = self.graphs.get(workflow_id).ok_or_else(|| {
            WorkflowError::CoordinatorError(format!(
                "Workflow {} not registered for execution",
                workflow_id
            ))
        })?;

        let hooks = wf_common::lock::read_ok(self.workflow_hooks.read())
            .get(workflow_id)
            .cloned()
            .unwrap_or_else(|| self.hooks.clone());

        Ok(WorkflowExecutionParams {
            execution_id,
            workflow_id: Id::from(workflow_id.to_string()),
            graph: (*graph).clone(),
            options,
            handlers: self.handlers(),
            tool_registry: self.tool_registry.clone(),
            resource_registries: None,
            input,
            hooks,
        })
    }

    fn build_lifecycle(&self) -> WorkflowLifecycleCoordinator {
        let mut lifecycle =
            WorkflowLifecycleCoordinator::with_store(self.event_bus.clone(), self.store.clone());
        if let Some(ref strategy) = self.checkpoint_strategy {
            lifecycle = lifecycle.with_checkpoint_strategy(strategy.clone());
        }
        if let Some(ref metrics) = self.metrics {
            lifecycle = lifecycle.with_metrics(metrics.clone());
        }
        if let Some(ref registry) = self.hook_registry {
            lifecycle = lifecycle.with_hook_registry(registry.clone());
        }
        if let Some(ref bus) = self.signal_bus {
            lifecycle = lifecycle.with_signal_bus(bus.clone());
        }
        lifecycle
    }

    /// Mirror the final outcome onto the entity exposed through the
    /// executions registry: terminal state plus the result slot, keeping
    /// `query_execution_status` accurate on both sync and spawned paths.
    async fn mirror_outcome(
        executions: &WorkflowExecutionRegistry,
        execution_id: &str,
        result: &WorkflowResult<WorkflowOutput>,
    ) {
        let Some(entity) = executions.get(execution_id) else {
            return;
        };
        let mut state = entity.state.write().await;
        match result {
            Ok(output) => {
                let _ = state.start();
                let _ = state.complete();
                entity.set_output(output.result.clone()).await;
            }
            Err(e) => {
                let _ = state.start();
                let _ = state.fail(e.to_string());
            }
        }
    }

    async fn launch(
        &self,
        workflow_id: &str,
        options: WorkflowExecutionOptions,
    ) -> WorkflowResult<WorkflowOutput> {
        let execution_id = wf_common::generate_id();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            execution_id.clone(),
            Id::from(workflow_id.to_string()),
        ));
        let _ = self.executions.register(execution_id.to_string(), entity);

        let params = self.build_params(workflow_id, execution_id.clone(), options, None)?;

        let result = self.build_lifecycle().execute_workflow(params).await;

        // The lifecycle coordinator owns its internal entity; mirror the
        // final status and result onto the entity exposed through the
        // executions registry so query_execution_status stays accurate.
        Self::mirror_outcome(&self.executions, &execution_id, &result).await;
        result
    }

    /// Dispatch a workflow execution in the background and return
    /// immediately. The execution id is pre-generated and the entity
    /// registered up front, so `query_execution_status` /
    /// `cancel_execution` work right away; the terminal output is mirrored
    /// into the entity result slot by the background task.
    pub async fn spawn_workflow(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
    ) -> WorkflowResult<SpawnedWorkflow> {
        if !self.graphs.has(workflow_id) {
            return Err(WorkflowError::CoordinatorError(format!(
                "Workflow {} not registered for execution",
                workflow_id
            )));
        }

        let variables = if input.variables.is_empty() {
            None
        } else {
            Some(Value::Object(input.variables.into_iter().collect()))
        };

        let execution_id = wf_common::generate_id();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            execution_id.clone(),
            Id::from(workflow_id.to_string()),
        ));
        let _ = self.executions.register(execution_id.to_string(), entity);

        let params = self.build_params(
            workflow_id,
            execution_id.clone(),
            default_options(variables),
            None,
        )?;
        let lifecycle = self.build_lifecycle();
        let executions = self.executions.clone();
        let mirror_id = execution_id.to_string();

        tokio::spawn(async move {
            let result = lifecycle.execute_workflow(params).await;
            Self::mirror_outcome(&executions, &mirror_id, &result).await;
        });

        Ok(SpawnedWorkflow {
            execution_id,
            status: "started".to_string(),
        })
    }
}

impl Default for WorkflowExecutionCallback {
    fn default() -> Self {
        Self::new(Arc::new(ToolRegistry::new()))
    }
}

fn default_options(input: Option<Value>) -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(false),
        node_timeout: None,
        max_pause_duration: None,
        retry_budget: None,
        on_failure: None,
        max_retries: None,
        retry_delay_ms: None,
        exponential_backoff: None,
        fallback_output: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

fn status_string(status: wf_execution_shared::types::execution_entity::ExecutionStatus) -> String {
    use wf_execution_shared::types::execution_entity::ExecutionStatus as S;
    match status {
        S::Created => "created".to_string(),
        S::Running => "running".to_string(),
        S::Paused => "paused".to_string(),
        S::Completed => "completed".to_string(),
        S::Failed => "failed".to_string(),
        S::Cancelled => "cancelled".to_string(),
        S::Stopped => "stopped".to_string(),
        S::Timeout => "timeout".to_string(),
    }
}

#[async_trait]
impl ExecutionCallback for WorkflowExecutionCallback {
    async fn execute_agent_loop(
        &self,
        _config: AgentLoopConfig,
        _input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput> {
        Err(ToolError::ExecutionError(
            "agent loop execution is not supported by WorkflowExecutionCallback".to_string(),
        ))
    }

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput> {
        if !self.graphs.has(workflow_id) {
            return Err(ToolError::NotFound(format!(
                "workflow {} is not registered",
                workflow_id
            )));
        }

        let variables = if input.variables.is_empty() {
            None
        } else {
            Some(Value::Object(input.variables.into_iter().collect()))
        };

        self.launch(workflow_id, default_options(variables))
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))
    }

    async fn spawn_workflow(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
    ) -> ToolResult<SpawnedWorkflow> {
        self.spawn_workflow(workflow_id, input)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))
    }

    async fn query_execution_status(&self, execution_id: &str) -> ToolResult<ExecutionStatus> {
        match self.executions.get(execution_id) {
            Some(entity) => {
                let result = if entity.status().is_terminal() {
                    entity.output().await
                } else {
                    None
                };
                Ok(ExecutionStatus {
                    execution_id: Id::from(execution_id.to_string()),
                    status: status_string(entity.status()),
                    progress: None,
                    result,
                })
            }
            None => Err(ToolError::NotFound(format!(
                "execution {} not found",
                execution_id
            ))),
        }
    }

    async fn cancel_execution(&self, execution_id: &str) -> ToolResult<()> {
        match self.executions.get(execution_id) {
            Some(entity) => entity
                .stop()
                .await
                .map_err(|e| ToolError::ExecutionError(e.to_string())),
            None => Err(ToolError::NotFound(format!(
                "execution {} not found",
                execution_id
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowNode};

    fn make_graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        }
    }

    fn make_node(id: &str, node_type: &str) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner: serde_json::json!({}),
        }
    }

    fn make_node_with_inner(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn make_edge(source: &str, target: &str) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{}-{}", source, target),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
        }
    }

    fn linear_graph() -> WorkflowGraphStructure {
        make_graph(
            vec![
                make_node("start", "START"),
                make_node_with_inner(
                    "var",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "var",
                        "expression": "${input.greeting}"
                    }),
                ),
                make_node("end", "END"),
            ],
            vec![make_edge("start", "var"), make_edge("var", "end")],
        )
    }

    #[tokio::test]
    async fn test_execute_workflow_tool() {
        let callback = WorkflowExecutionCallback::default();
        let workflow_id = wf_common::generate_id();
        assert!(callback.register_workflow(workflow_id.clone(), linear_graph()));
        assert!(callback.has_workflow(&workflow_id.to_string()));

        let output = callback
            .execute_workflow(
                &workflow_id.to_string(),
                WorkflowInput {
                    variables: HashMap::from([(
                        "greeting".to_string(),
                        serde_json::json!("hello"),
                    )]),
                },
            )
            .await
            .expect("workflow should execute");
        assert!(!output.execution_id.is_empty());

        let status = callback
            .query_execution_status(&output.execution_id.to_string())
            .await
            .expect("status should be queryable");
        assert_eq!(status.status, "completed");

        let result = callback
            .execute_workflow(
                &workflow_id.to_string(),
                WorkflowInput {
                    variables: HashMap::new(),
                },
            )
            .await
            .expect("second execution should succeed");
        let _ = result;

        callback
            .cancel_execution(&output.execution_id.to_string())
            .await
            .expect("cancel on completed execution should be idempotent");

        let err = callback
            .execute_workflow(
                "missing-id",
                WorkflowInput {
                    variables: HashMap::new(),
                },
            )
            .await
            .expect_err("unknown workflow must fail");
        assert!(matches!(err, ToolError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_spawn_workflow_roundtrip_with_result() {
        let callback = WorkflowExecutionCallback::default();
        let workflow_id = wf_common::generate_id();
        assert!(callback.register_workflow(workflow_id.clone(), linear_graph()));

        let spawned = callback
            .spawn_workflow(
                &workflow_id.to_string(),
                WorkflowInput {
                    variables: HashMap::from([("greeting".to_string(), serde_json::json!("hi"))]),
                },
            )
            .await
            .expect("spawn must return immediately");
        assert_eq!(spawned.status, "started");

        // The execution is registered and progresses in the background.
        let mut result = None;
        for _ in 0..200 {
            let status = callback
                .query_execution_status(&spawned.execution_id.to_string())
                .await
                .expect("status must be queryable right after spawn");
            if status.status == "completed" {
                result = status.result;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let result = result.expect("terminal query must carry the workflow result");
        assert_eq!(result, serde_json::json!({"greeting": "hi"}));
    }

    #[tokio::test]
    async fn test_sync_execution_writes_result_slot() {
        let callback = WorkflowExecutionCallback::default();
        let workflow_id = wf_common::generate_id();
        assert!(callback.register_workflow(workflow_id.clone(), linear_graph()));

        let output = callback
            .execute_workflow(
                &workflow_id.to_string(),
                WorkflowInput {
                    variables: HashMap::from([("greeting".to_string(), serde_json::json!("hey"))]),
                },
            )
            .await
            .expect("sync workflow should execute");

        let status = callback
            .query_execution_status(&output.execution_id.to_string())
            .await
            .expect("status must be queryable");
        assert_eq!(status.status, "completed");
        assert_eq!(status.result, Some(serde_json::json!({"greeting": "hey"})));
    }

    #[tokio::test]
    async fn test_spawn_unknown_workflow_fails() {
        let callback = WorkflowExecutionCallback::default();
        let err = callback
            .spawn_workflow(
                "missing-id",
                WorkflowInput {
                    variables: HashMap::new(),
                },
            )
            .await
            .expect_err("unknown workflow must fail");
        assert!(matches!(err, WorkflowError::CoordinatorError(_)));
    }

    #[tokio::test]
    async fn test_per_workflow_hooks_run_and_publish_events() {
        use wf_core::EventBus;
        use wf_types::events::EventType;

        let bus = Arc::new(EventBus::new(32));
        let callback = WorkflowExecutionCallback::default().with_event_bus(bus.clone());
        let workflow_id = wf_common::generate_id();
        callback.register_workflow_with_hooks(
            workflow_id.clone(),
            linear_graph(),
            vec![BaseHookDefinition {
                id: wf_types::Id::new(),
                hook_type: "BEFORE_EXECUTE".to_string(),
                weight: 1,
                condition: None,
                enabled: true,
                payload: None,
                receiver: None,
            }],
        );

        let mut sub = bus.subscribe();
        let output = callback
            .execute_workflow(
                &workflow_id.to_string(),
                WorkflowInput {
                    variables: HashMap::from([("greeting".to_string(), serde_json::json!("hi"))]),
                },
            )
            .await
            .expect("workflow with hooks should execute");

        // The definition-level hook fired per node (start, var, end).
        let mut hook_count = 0;
        for _ in 0..64 {
            match sub.try_recv() {
                Ok(event) if event.r#type == EventType::HookTriggered => {
                    assert_eq!(
                        event.execution_id.as_deref(),
                        Some(output.execution_id.as_str())
                    );
                    hook_count += 1;
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert_eq!(hook_count, 3, "one BEFORE_EXECUTE event per node");
    }
}
