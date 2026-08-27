use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::execution_events::ExecutionEventBus;
use wf_core::EventBus;
use wf_core::WorkflowStateMachine;
use wf_core::internal_signal::InternalSignalBus;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_execution_shared::hooks::HookRegistry;
use wf_metrics::MetricsRegistry;
use wf_storage::backend::StorageBackend;
use wf_tools::callback::WorkflowOutput;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::checkpoint::{NodeCheckpointStrategy, WorkflowCheckpointIntegration};
use crate::coordinator::WorkflowCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::trigger_states::TriggerStateRegistry;

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
    pub hooks: Vec<BaseHookDefinition>,
}

pub struct WorkflowLifecycleCoordinator {
    event_bus: Option<Arc<EventBus>>,
    /// Typed signal bus for internal workflow/agent signals (replaces the
    /// `__`-prefixed variable protocol).
    signal_bus: Option<Arc<InternalSignalBus>>,
    store: Arc<StorageBackend>,
    checkpoint_strategy: Option<NodeCheckpointStrategy>,
    checkpoint_event_bus: Option<CheckpointEventBus>,
    checkpoint_execution_events: Option<ExecutionEventBus>,
    metrics: Option<Arc<MetricsRegistry>>,
    trigger_state_registry: Option<Arc<TriggerStateRegistry>>,
    /// Shared hook receiver registry; hook points dispatch through it.
    hook_registry: Option<Arc<HookRegistry>>,
    /// Optional file checkpoint manager: file snapshots are created on
    /// checkpoint persistence and restored after workflow restore
    /// (best-effort).
    file_checkpoint_manager: Option<wf_checkpoint::file::FileCheckpointManager>,
}

impl WorkflowLifecycleCoordinator {
    pub fn new(event_bus: Option<Arc<EventBus>>) -> Self {
        Self::with_store(event_bus, Arc::new(StorageBackend::new_memory()))
    }

    pub fn with_store(event_bus: Option<Arc<EventBus>>, store: Arc<StorageBackend>) -> Self {
        Self {
            event_bus,
            signal_bus: None,
            store,
            checkpoint_strategy: None,
            checkpoint_event_bus: None,
            checkpoint_execution_events: None,
            metrics: None,
            trigger_state_registry: None,
            hook_registry: None,
            file_checkpoint_manager: None,
        }
    }

    /// Attach the file checkpoint manager: file snapshots of executions are
    /// created/restored through it (best-effort).
    pub fn with_file_checkpoint_manager(
        mut self,
        manager: wf_checkpoint::file::FileCheckpointManager,
    ) -> Self {
        self.file_checkpoint_manager = Some(manager);
        self
    }

    /// Inject the typed signal bus: control signals from trigger actions
    /// reach the coordinator loop of executions started here.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    pub fn with_checkpoint_strategy(mut self, strategy: NodeCheckpointStrategy) -> Self {
        self.checkpoint_strategy = Some(strategy);
        self
    }

    pub fn with_checkpoint_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.checkpoint_event_bus = Some(bus);
        self
    }

    /// Register the execution event bus; `state_changed` events are published
    /// after every checkpoint creation.
    pub fn with_checkpoint_execution_events(mut self, bus: ExecutionEventBus) -> Self {
        self.checkpoint_execution_events = Some(bus);
        self
    }

    /// Register the trigger runtime state registry; checkpoints of
    /// executions started here capture the `trigger_states` audit trail.
    pub fn with_trigger_state_registry(mut self, registry: Arc<TriggerStateRegistry>) -> Self {
        self.trigger_state_registry = Some(registry);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Inject the shared hook receiver registry: hook points and engine
    /// signals of executions started here dispatch through it.
    pub fn with_hook_registry(mut self, registry: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(registry);
        self
    }

    pub async fn execute_workflow(
        &self,
        params: WorkflowExecutionParams,
    ) -> WorkflowResult<WorkflowOutput> {
        let WorkflowExecutionParams {
            execution_id,
            workflow_id,
            graph,
            mut options,
            handlers,
            tool_registry,
            resource_registries,
            input,
            hooks,
        } = params;
        let workflow_id_metrics = workflow_id.clone();

        // Reject structurally invalid graphs before execution starts.
        let _validated =
            crate::validation::GraphValidator::validate(graph.clone()).map_err(|errors| {
                let detail = errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                WorkflowError::GraphError(format!(
                    "Workflow graph validation failed ({} error(s)): {}",
                    errors.len(),
                    detail
                ))
            })?;

        let mut wf_state = WorkflowStateMachine::new(&execution_id);
        wf_state
            .start()
            .map_err(|e| WorkflowError::StateTransitionError(e.to_string()))?;

        if options.input.is_none() {
            options.input = input;
        }

        let entity = WorkflowExecutionEntity::new(execution_id.clone(), workflow_id.clone());

        if let Some(ref input) = options.input {
            entity.set_variable("input", input.clone());
        }

        // Checkpoints are skipped unless explicitly enabled: sub-workflows
        // (SUBGRAPH/EMBED/trigger) pass `enable_checkpoints: Some(false)`.
        let checkpoints_enabled = options.enable_checkpoints.unwrap_or(true);

        let mut ctx = ExecutorContext::new(
            execution_id.clone(),
            workflow_id,
            self.event_bus.clone(),
            tool_registry,
            options,
        );
        // The coordinator and the entity share one variable map so that
        // checkpoints (built from the entity) capture live variables.
        ctx.variables = entity.variables().clone();
        if let Some(ref bus) = self.signal_bus {
            ctx = ctx.with_signal_bus(bus.clone());
        }
        if let Some(ref regs) = resource_registries {
            ctx = ctx.with_resource_registries(regs.clone());
        }
        if let Some(ref metrics) = self.metrics {
            metrics
                .workflow()
                .record_execution_start(&workflow_id_metrics);
            ctx = ctx.with_metrics(metrics.clone());
        }
        if let Some(ref registry) = self.hook_registry {
            ctx = ctx.with_hook_registry(registry.clone());
        }

        let mut coordinator = WorkflowCoordinator::new(ctx, graph, handlers)?
            .with_entity(entity)
            .with_hooks(hooks);

        // Checkpoints are skipped unless explicitly enabled: sub-workflows
        // (SUBGRAPH/EMBED/trigger) pass `enable_checkpoints: Some(false)`.
        if checkpoints_enabled {
            if let Some(ref strategy) = self.checkpoint_strategy {
                let mut cp =
                    WorkflowCheckpointIntegration::new(self.store.clone(), strategy.clone());
                if let Some(ref manager) = self.file_checkpoint_manager {
                    cp = cp.with_file_checkpoint_manager(manager.clone());
                }
                if let Some(ref registry) = self.trigger_state_registry {
                    cp = cp.with_trigger_state_registry(registry.clone());
                }
                if let Some(ref bus) = self.checkpoint_event_bus {
                    cp = cp.with_event_bus(bus.clone());
                }
                if let Some(ref core_bus) = self.event_bus {
                    cp = cp.with_core_event_bus(core_bus.clone());
                }
                if let Some(ref bus) = self.checkpoint_execution_events {
                    cp = cp.with_execution_event_bus(bus.clone());
                }
                coordinator = coordinator.with_checkpoint(cp);
            }
        }

        let start = wf_common::now();
        let result = coordinator.execute().await;
        let duration_ms = (wf_common::now() - start) as f64;

        match result {
            Ok(output) => {
                wf_state
                    .complete(Some(output.clone()))
                    .map_err(|e| WorkflowError::StateTransitionError(e.to_string()))?;
                if let Some(ref metrics) = self.metrics {
                    metrics.workflow().record_execution_complete(
                        &workflow_id_metrics,
                        None,
                        true,
                        duration_ms,
                        None,
                    );
                }

                Ok(WorkflowOutput {
                    execution_id,
                    result: output,
                })
            }
            Err(e) => {
                wf_state
                    .fail(e.to_string())
                    .map_err(|e| WorkflowError::StateTransitionError(e.to_string()))?;
                if let Some(ref metrics) = self.metrics {
                    metrics.workflow().record_execution_complete(
                        &workflow_id_metrics,
                        None,
                        false,
                        duration_ms,
                        Some("workflow_error"),
                    );
                }
                Err(e)
            }
        }
    }

    /// Resume a workflow execution from its latest checkpoint.
    ///
    /// Loads the newest snapshot for `execution_id`, rebuilds the entity,
    /// variables and node outputs, then continues from the checkpointed
    /// node. Completed nodes are skipped by the coordinator.
    pub async fn resume_workflow(
        &self,
        execution_id: &str,
        workflow_id: wf_types::Id,
        graph: WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
        hooks: Vec<BaseHookDefinition>,
    ) -> WorkflowResult<WorkflowOutput> {
        use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
        use wf_checkpoint::coordinator::CheckpointCoordinator;
        use wf_checkpoint::state::CheckpointStateManager;
        use wf_checkpoint::state::WorkflowCheckpointStateManager;

        let state_manager = WorkflowCheckpointStateManager::new(self.store.clone());
        let mut cp_coordinator = WorkflowCheckpointCoordinator::new(state_manager);
        if let Some(ref manager) = self.file_checkpoint_manager {
            cp_coordinator = cp_coordinator.with_file_checkpoint_manager(manager.clone());
        }

        let metadata = cp_coordinator
            .state_manager()
            .get_latest(execution_id)
            .await
            .map_err(|e| {
                WorkflowError::CoordinatorError(format!("checkpoint query failed: {}", e))
            })?
            .ok_or_else(|| {
                WorkflowError::CoordinatorError(format!(
                    "no checkpoint found for execution {}",
                    execution_id
                ))
            })?;

        let restored = cp_coordinator.restore(&metadata.id).await.map_err(|e| {
            WorkflowError::CoordinatorError(format!("checkpoint restore failed: {}", e))
        })?;
        let snapshot = restored.snapshot;

        let entity = WorkflowExecutionEntity::new(
            wf_types::Id::from(snapshot.execution_id.clone()),
            workflow_id.clone(),
        );
        {
            let mut state = entity.state.write().await;
            state.start()?;
            for node_id in snapshot
                .node_results
                .as_ref()
                .map(|m| m.keys())
                .into_iter()
                .flatten()
            {
                state.mark_node_completed(node_id.clone());
            }
        }

        let mut ctx = ExecutorContext::new(
            wf_types::Id::from(snapshot.execution_id.clone()),
            workflow_id,
            self.event_bus.clone(),
            tool_registry,
            WorkflowExecutionOptions {
                // The input lives in the restored "input" variable; without
                // it, restarted nodes would compute a Null input.
                input: snapshot.variable_state.variables.get("input").cloned(),
                max_steps: None,
                timeout: None,
                max_execution_time: None,
                // A resumed execution continues checkpointing; sub-workflow
                // resumes opt out explicitly.
                enable_checkpoints: Some(true),
                node_timeout: None,
                max_pause_duration: None,
                retry_budget: None,
                on_failure: None,
                max_retries: None,
                retry_delay_ms: None,
                exponential_backoff: None,
                fallback_output: None,
                max_navigation_multiplier: None,
            },
        );
        ctx.variables = entity.variables().clone();
        for (name, value) in &snapshot.variable_state.variables {
            ctx.variables.insert(name.clone(), value.clone());
        }
        if let Some(ref bus) = self.signal_bus {
            ctx = ctx.with_signal_bus(bus.clone());
        }
        if let Some(ref metrics) = self.metrics {
            ctx = ctx.with_metrics(metrics.clone());
        }
        if let Some(ref registry) = self.hook_registry {
            ctx = ctx.with_hook_registry(registry.clone());
        }

        // A resumed execution continues checkpointing unless the options
        // explicitly disable it (sub-workflow resume semantics).
        let checkpoints_enabled = ctx.options.enable_checkpoints.unwrap_or(true);

        let mut coordinator = WorkflowCoordinator::new(ctx, graph, handlers)?
            .with_entity(entity)
            .with_hooks(hooks);
        coordinator.resume_from(&snapshot);

        // Same sub-workflow skip semantics as `execute`: checkpoints are
        // wired only when the (resumed) execution enables them.
        if checkpoints_enabled {
            if let Some(ref strategy) = self.checkpoint_strategy {
                let mut cp =
                    WorkflowCheckpointIntegration::new(self.store.clone(), strategy.clone());
                if let Some(ref manager) = self.file_checkpoint_manager {
                    cp = cp.with_file_checkpoint_manager(manager.clone());
                }
                if let Some(ref registry) = self.trigger_state_registry {
                    cp = cp.with_trigger_state_registry(registry.clone());
                }
                if let Some(ref bus) = self.checkpoint_event_bus {
                    cp = cp.with_event_bus(bus.clone());
                }
                if let Some(ref core_bus) = self.event_bus {
                    cp = cp.with_core_event_bus(core_bus.clone());
                }
                if let Some(ref bus) = self.checkpoint_execution_events {
                    cp = cp.with_execution_event_bus(bus.clone());
                }
                coordinator = coordinator.with_checkpoint(cp);
            }
        }

        let result = coordinator.execute().await;
        match result {
            Ok(output) => Ok(WorkflowOutput {
                execution_id: wf_types::Id::from(snapshot.execution_id),
                result: output,
            }),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowNode};

    use crate::checkpoint::strategy::NodeCheckpointStrategy;
    use crate::handler::HandlerRegistry;

    fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn edge(source: &str, target: &str) -> WorkflowEdge {
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

    fn make_graph() -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "v1",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "mid",
                        "expression": "${input.greeting}"
                    }),
                ),
                node(
                    "v2",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "final",
                        "expression": "${mid}"
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            edges: vec![edge("start", "v1"), edge("v1", "v2"), edge("v2", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        }
    }

    fn make_handlers() -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
        let mut reg = HandlerRegistry::new();
        reg.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
        reg.into_arc()
    }

    fn make_lifecycle(store: Arc<StorageBackend>) -> WorkflowLifecycleCoordinator {
        WorkflowLifecycleCoordinator::with_store(None, store)
            .with_checkpoint_strategy(NodeCheckpointStrategy::every_node())
    }

    #[tokio::test]
    async fn test_execute_then_resume() {
        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = make_lifecycle(store.clone());
        let workflow_id = wf_types::Id::from("wf-resume-1".to_string());
        let tool_registry = Arc::new(wf_tools::registry::ToolRegistry::new());

        let handlers = make_handlers();
        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-resume-1".to_string()),
            workflow_id: workflow_id.clone(),
            graph: make_graph(),
            options: WorkflowExecutionOptions {
                input: Some(serde_json::json!({"greeting": "hello"})),
                max_steps: Some(2),
                timeout: None,
                max_execution_time: None,
                enable_checkpoints: Some(true),
                node_timeout: None,
                max_pause_duration: None,
                retry_budget: None,
                on_failure: None,
                max_retries: None,
                retry_delay_ms: None,
                exponential_backoff: None,
                fallback_output: None,
                max_navigation_multiplier: None,
            },
            handlers: handlers.clone(),
            tool_registry: tool_registry.clone(),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        };

        let first = lifecycle
            .execute_workflow(params)
            .await
            .expect("first run should complete");
        assert_eq!(first.execution_id, "exec-resume-1");

        // Snapshot must have captured variables written by v1
        use wf_checkpoint::state::CheckpointStateManager;
        let sm = wf_checkpoint::state::WorkflowCheckpointStateManager::new(store.clone());
        let latest = sm
            .get_latest("exec-resume-1")
            .await
            .expect("checkpoint exists");
        assert!(latest.is_some(), "checkpoint should be persisted");
        assert!(latest.is_some(), "checkpoint should be persisted");

        let resumed = lifecycle
            .resume_workflow(
                "exec-resume-1",
                workflow_id,
                make_graph(),
                handlers,
                tool_registry,
                Vec::new(),
            )
            .await
            .expect("resume should complete the workflow");
        assert_eq!(resumed.execution_id, "exec-resume-1");

        // Single end node -> final output is the end node's output directly.
        assert_eq!(resumed.result, serde_json::json!({"greeting": "hello"}));

        // The resumed run must have continued checkpointing; the new
        // snapshot proves v2 ran with the restored "mid" variable.
        let resumed_cp = sm
            .get_latest("exec-resume-1")
            .await
            .expect("checkpoint exists")
            .expect("checkpoint persisted");
        use wf_checkpoint::coordinator::CheckpointCoordinator;
        let coord = wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator::new(
            wf_checkpoint::state::WorkflowCheckpointStateManager::new(store.clone()),
        );
        let restored = coord.restore(&resumed_cp.id).await.expect("restore ok");
        let vars = &restored.snapshot.variable_state.variables;
        assert_eq!(vars.get("mid"), Some(&serde_json::json!("hello")));
        assert_eq!(vars.get("final"), Some(&serde_json::json!("hello")));
    }

    #[tokio::test]
    async fn test_resume_without_checkpoint_fails() {
        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = make_lifecycle(store.clone());

        let err = lifecycle
            .resume_workflow(
                "exec-missing",
                wf_types::Id::from("wf-x".to_string()),
                make_graph(),
                make_handlers(),
                Arc::new(wf_tools::registry::ToolRegistry::new()),
                Vec::new(),
            )
            .await
            .expect_err("resume without checkpoint must fail");
        assert!(err.to_string().contains("no checkpoint"));
    }

    #[tokio::test]
    async fn test_checkpoints_disabled_execution_skips_checkpointing() {
        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = make_lifecycle(store.clone());
        let handlers = make_handlers();

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-no-cp".to_string()),
            workflow_id: wf_types::Id::from("wf-no-cp".to_string()),
            graph: make_graph(),
            options: WorkflowExecutionOptions {
                input: Some(serde_json::json!({"greeting": "hello"})),
                max_steps: Some(2),
                timeout: None,
                max_execution_time: None,
                // Sub-workflows pass enable_checkpoints=false; the strategy is
                // configured but must be skipped.
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
            },
            handlers,
            tool_registry: Arc::new(wf_tools::registry::ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        };

        let output = lifecycle
            .execute_workflow(params)
            .await
            .expect("execution should complete without checkpoints");
        assert_eq!(output.execution_id, "exec-no-cp");

        use wf_checkpoint::state::CheckpointStateManager;
        let sm = wf_checkpoint::state::WorkflowCheckpointStateManager::new(store.clone());
        let latest = sm.get_latest("exec-no-cp").await.expect("query ok");
        assert!(latest.is_none(), "no checkpoint created when disabled");
    }

    fn options_with(input: Option<Value>) -> WorkflowExecutionOptions {
        WorkflowExecutionOptions {
            input,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(true),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
            max_navigation_multiplier: None,
        }
    }

    #[tokio::test]
    async fn test_workflow_hooks_publish_events_per_node() {
        use wf_core::EventBus;
        use wf_execution_shared::hooks::types::BaseHookDefinition;
        use wf_types::events::EventType;

        let bus = Arc::new(EventBus::new(32));
        let hooks = vec![
            BaseHookDefinition {
                id: "h-before".to_string(),
                hook_type: "BEFORE_EXECUTE".to_string(),
                weight: 1,
                condition: None,
                enabled: true,
                payload: None,
                receiver: None,
            },
            BaseHookDefinition {
                id: "h-after".to_string(),
                hook_type: "AFTER_EXECUTE".to_string(),
                weight: 1,
                condition: None,
                enabled: true,
                payload: None,
                receiver: None,
            },
        ];

        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = WorkflowLifecycleCoordinator::with_store(Some(bus.clone()), store);

        let mut sub = bus.subscribe();

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-hooks-1".to_string()),
            workflow_id: wf_types::Id::from("wf-hooks-1".to_string()),
            graph: make_graph(),
            options: options_with(Some(serde_json::json!({"greeting": "hello"}))),
            handlers: make_handlers(),
            tool_registry: Arc::new(wf_tools::registry::ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks,
        };

        let output = lifecycle
            .execute_workflow(params)
            .await
            .expect("workflow with hooks should complete");
        assert_eq!(output.result, serde_json::json!({"greeting": "hello"}));

        let mut hook_types: Vec<String> = Vec::new();
        for _ in 0..64 {
            match sub.try_recv() {
                Ok(event) if event.r#type == EventType::HookTriggered => {
                    assert_eq!(
                        event.execution_id.as_deref(),
                        Some("exec-hooks-1"),
                        "hook events are routable by execution id"
                    );
                    let t = event
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("hook_type"))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    hook_types.push(t);
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        // 4 nodes (start, v1, v2, end) x 2 hook types, one event per batch.
        assert_eq!(hook_types.len(), 8);
        assert_eq!(
            hook_types.iter().filter(|t| *t == "BEFORE_EXECUTE").count(),
            4
        );
        assert_eq!(
            hook_types.iter().filter(|t| *t == "AFTER_EXECUTE").count(),
            4
        );
    }

    #[tokio::test]
    async fn test_workflow_scope_and_error_hooks_fire() {
        use async_trait::async_trait;
        use wf_execution_shared::context::NodeExecutionContext;
        use wf_execution_shared::hooks::types::BaseHookDefinition;

        // A handler that always fails, registered for a node type used in
        // the failing graph below (the ON_ERROR hook must fire for it).
        struct FailingHandler;
        #[async_trait]
        impl NodeHandler for FailingHandler {
            fn node_type(&self) -> StaticNodeType {
                StaticNodeType::Variable
            }
            async fn execute(
                &self,
                _ctx: &mut NodeExecutionContext,
            ) -> wf_execution_shared::error::ExecutionSharedResult<crate::handler::NodeHandlerResult>
            {
                Err(crate::error::WorkflowError::VariableError("boom".to_string()).into())
            }
        }

        let bus = Arc::new(wf_core::EventBus::new(32));
        let hooks: Vec<BaseHookDefinition> = [
            "WORKFLOW_BEFORE",
            "WORKFLOW_AFTER",
            "ON_ERROR",
            "BEFORE_EXECUTE",
            "AFTER_EXECUTE",
        ]
        .iter()
        .map(|hook_type| BaseHookDefinition {
            id: format!("h-{}", hook_type),
            hook_type: hook_type.to_string(),
            weight: 1,
            condition: None,
            enabled: true,
            payload: None,
            receiver: None,
        })
        .collect();

        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = WorkflowLifecycleCoordinator::with_store(Some(bus.clone()), store);

        let mut sub = bus.subscribe();

        // A valid graph whose middle node fails at runtime (failing handler),
        // so ON_ERROR fires and the run still returns an error.
        let graph = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "boom",
                    "VARIABLE",
                    serde_json::json!({ "variable_name": "x", "expression": "1" }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            edges: vec![edge("start", "boom"), edge("boom", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        };

        let mut handlers = HandlerRegistry::new();
        handlers.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
        handlers.register(Box::new(FailingHandler));
        let handlers = handlers.into_arc();

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-scope-hooks".to_string()),
            workflow_id: wf_types::Id::from("wf-scope-hooks".to_string()),
            graph,
            options: options_with(Some(serde_json::json!({"greeting": "hello"}))),
            handlers,
            tool_registry: Arc::new(wf_tools::registry::ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks,
        };

        let result = lifecycle.execute_workflow(params).await;
        assert!(result.is_err(), "the node failure must propagate");

        let mut hook_types: Vec<String> = Vec::new();
        for _ in 0..32 {
            match sub.try_recv() {
                Ok(event) if event.r#type == wf_types::events::EventType::HookTriggered => {
                    let t = event
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("hook_type"))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    hook_types.push(t);
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert!(hook_types.iter().any(|t| t == "WORKFLOW_BEFORE"));
        assert!(hook_types.iter().any(|t| t == "WORKFLOW_AFTER"));
        assert!(hook_types.iter().any(|t| t == "ON_ERROR"));
        // The failing node still ran its node-level hooks.
        assert!(hook_types.iter().any(|t| t == "BEFORE_EXECUTE"));
        assert!(hook_types.iter().any(|t| t == "AFTER_EXECUTE"));
    }

    fn linear_graph_with_count(n: u32) -> WorkflowGraphStructure {
        let mut nodes = vec![node("start", "START", serde_json::json!({}))];
        for i in 0..n {
            nodes.push(node(
                &format!("v{}", i),
                "VARIABLE",
                serde_json::json!({ "variable_name": "v", "expression": "1" }),
            ));
        }
        nodes.push(node("end", "END", serde_json::json!({})));

        let mut edges = vec![edge("start", "v0")];
        for i in 0..n - 1 {
            edges.push(edge(&format!("v{}", i), &format!("v{}", i + 1)));
        }
        edges.push(edge(&format!("v{}", n - 1), "end"));

        WorkflowGraphStructure {
            nodes,
            edges,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        }
    }

    #[tokio::test]
    async fn test_max_execution_timeout_interrupts_workflow() {
        use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
        use wf_checkpoint::coordinator::CheckpointCoordinator;
        use wf_checkpoint::state::CheckpointStateManager;
        use wf_checkpoint::state::WorkflowCheckpointStateManager;
        use wf_core::EventBus;
        use wf_types::events::EventType;

        let store = Arc::new(StorageBackend::new_memory());
        let event_bus = Arc::new(EventBus::new(64));
        let mut sub = event_bus.subscribe();
        let lifecycle = WorkflowLifecycleCoordinator::with_store(Some(event_bus), store.clone())
            .with_checkpoint_strategy(NodeCheckpointStrategy::every_node());

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-timeout-1".to_string()),
            workflow_id: wf_types::Id::from("wf-timeout-1".to_string()),
            graph: linear_graph_with_count(50),
            options: WorkflowExecutionOptions {
                max_execution_time: Some(1),
                ..options_with(None)
            },
            handlers: make_handlers(),
            tool_registry: Arc::new(wf_tools::registry::ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        };

        let err = lifecycle
            .execute_workflow(params)
            .await
            .expect_err("wall-clock timeout must fail the workflow");
        assert!(
            err.to_string().contains("max_execution_time"),
            "unexpected error: {}",
            err
        );

        let mut saw_cancelled = false;
        while let Ok(ev) = sub.try_recv() {
            if ev.r#type == EventType::WorkflowExecutionCancelled {
                saw_cancelled = true;
            }
        }
        assert!(saw_cancelled, "cancelled event must be published");

        // Interruption checkpoint persisted with failed status. Several
        // checkpoints may share the same millisecond, so scan rather than
        // relying on get_latest (tie-breaking is arbitrary).
        let sm = WorkflowCheckpointStateManager::new(store.clone());
        let all = sm
            .list_by_entity("exec-timeout-1")
            .await
            .expect("checkpoints listed");
        assert!(!all.is_empty(), "at least the start checkpoint must exist");
        let mut found_failed = false;
        for meta in &all {
            let coord = WorkflowCheckpointCoordinator::new(WorkflowCheckpointStateManager::new(
                store.clone(),
            ));
            let restored = coord.restore(&meta.id).await.expect("restore ok");
            if restored.snapshot.status == "Failed" {
                found_failed = true;
                break;
            }
        }
        assert!(
            found_failed,
            "interruption checkpoint with failed status must exist"
        );
    }

    #[tokio::test]
    async fn test_fallback_output_used_on_continue() {
        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = make_lifecycle(store.clone());

        let graph = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "v1",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "__forbidden",
                        "expression": "1"
                    }),
                ),
                node(
                    "v2",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "final",
                        "expression": "${input.greeting}"
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            edges: vec![edge("start", "v1"), edge("v1", "v2"), edge("v2", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        };

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-fallback-1".to_string()),
            workflow_id: wf_types::Id::from("wf-fallback-1".to_string()),
            graph,
            options: WorkflowExecutionOptions {
                on_failure: Some("continue".to_string()),
                max_retries: Some(0),
                fallback_output: Some(serde_json::json!({"fallback": "used"})),
                ..options_with(Some(serde_json::json!({"greeting": "hello"})))
            },
            handlers: make_handlers(),
            tool_registry: Arc::new(wf_tools::registry::ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        };

        let output = lifecycle
            .execute_workflow(params)
            .await
            .expect("fallback path must complete the workflow");
        assert_eq!(output.result, serde_json::json!({"fallback": "used"}));
    }

    #[tokio::test]
    async fn test_continue_without_fallback_produces_empty_output() {
        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = make_lifecycle(store.clone());

        let graph = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "v1",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "__forbidden",
                        "expression": "1"
                    }),
                ),
                node(
                    "v2",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "final",
                        "expression": "${input.greeting}"
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            edges: vec![edge("start", "v1"), edge("v1", "v2"), edge("v2", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        };

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-fallback-2".to_string()),
            workflow_id: wf_types::Id::from("wf-fallback-2".to_string()),
            graph,
            options: WorkflowExecutionOptions {
                on_failure: Some("continue".to_string()),
                max_retries: Some(0),
                fallback_output: None,
                max_navigation_multiplier: None,
                ..options_with(Some(serde_json::json!({"greeting": "hello"})))
            },
            handlers: make_handlers(),
            tool_registry: Arc::new(wf_tools::registry::ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        };

        let output = lifecycle
            .execute_workflow(params)
            .await
            .expect("continue path must complete the workflow");
        assert_eq!(output.result, serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_before_node_checkpoint_persisted() {
        use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
        use wf_checkpoint::coordinator::CheckpointCoordinator;
        use wf_checkpoint::state::CheckpointStateManager;
        use wf_checkpoint::state::WorkflowCheckpointStateManager;

        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = WorkflowLifecycleCoordinator::with_store(None, store.clone())
            .with_checkpoint_strategy(NodeCheckpointStrategy::always());

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-before-1".to_string()),
            workflow_id: wf_types::Id::from("wf-before-1".to_string()),
            graph: make_graph(),
            options: WorkflowExecutionOptions {
                max_steps: Some(2),
                ..options_with(None)
            },
            handlers: make_handlers(),
            tool_registry: Arc::new(wf_tools::registry::ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        };

        lifecycle
            .execute_workflow(params)
            .await
            .expect("workflow should complete");

        let sm = WorkflowCheckpointStateManager::new(store.clone());
        let all = sm
            .list_by_entity("exec-before-1")
            .await
            .expect("checkpoints listed");
        assert!(
            all.len() >= 4,
            "expected start + node checkpoints, got {}",
            all.len()
        );

        let coord =
            WorkflowCheckpointCoordinator::new(WorkflowCheckpointStateManager::new(store.clone()));
        let mut found_before_v1 = false;
        let mut found_after_v1 = false;
        for meta in &all {
            let restored = coord.restore(&meta.id).await.expect("restore ok");
            let snap = restored.snapshot;
            if snap.current_node_id.as_deref() == Some("v1") {
                let has_v1 = snap
                    .node_results
                    .as_ref()
                    .is_some_and(|m| m.contains_key("v1"));
                if !has_v1 {
                    found_before_v1 = true;
                } else {
                    found_after_v1 = true;
                }
            }
        }
        assert!(found_before_v1, "BeforeNode checkpoint for v1 must exist");
        assert!(found_after_v1, "AfterNode checkpoint for v1 must exist");
    }

    #[tokio::test]
    async fn test_node_checkpoint_config_skips_disabled_node_snapshots() {
        use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
        use wf_checkpoint::coordinator::CheckpointCoordinator;
        use wf_checkpoint::state::CheckpointStateManager;
        use wf_checkpoint::state::WorkflowCheckpointStateManager;

        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = WorkflowLifecycleCoordinator::with_store(None, store.clone())
            .with_checkpoint_strategy(NodeCheckpointStrategy::every_node());

        // v2 opts out of node-level checkpoints while v1 keeps the workflow
        // policy (snapshot after every node).
        let graph = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "v1",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "mid",
                        "expression": "${input.greeting}"
                    }),
                ),
                node(
                    "v2",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "final",
                        "expression": "${mid}",
                        "checkpoint": {"enabled": false}
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            edges: vec![edge("start", "v1"), edge("v1", "v2"), edge("v2", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        };

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-node-cp-1".to_string()),
            workflow_id: wf_types::Id::from("wf-node-cp-1".to_string()),
            graph: graph.clone(),
            options: options_with(Some(serde_json::json!({"greeting": "hello"}))),
            handlers: make_handlers(),
            tool_registry: Arc::new(wf_tools::registry::ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        };

        lifecycle
            .execute_workflow(params)
            .await
            .expect("workflow should complete");

        let sm = WorkflowCheckpointStateManager::new(store.clone());
        let all = sm
            .list_by_entity("exec-node-cp-1")
            .await
            .expect("checkpoints listed");
        let coord =
            WorkflowCheckpointCoordinator::new(WorkflowCheckpointStateManager::new(store.clone()));
        let mut after_v1 = false;
        let mut saw_v2 = false;
        for meta in &all {
            let restored = coord.restore(&meta.id).await.expect("restore ok");
            let snap = restored.snapshot;
            match snap.current_node_id.as_deref() {
                Some("v1") => {
                    if snap
                        .node_results
                        .as_ref()
                        .is_some_and(|m| m.contains_key("v1"))
                    {
                        after_v1 = true;
                    }
                }
                Some("v2") => {
                    saw_v2 = true;
                }
                _ => {}
            }
        }
        assert!(after_v1, "AfterNode checkpoint for v1 must exist");
        assert!(!saw_v2, "v2 snapshot must be suppressed by node config");
        // Manual + After(start) + After(v1) + After(end) + OnComplete; v2
        // contributes no checkpoint to the chain.
        assert_eq!(all.len(), 5, "disabled node must not appear in the chain");
    }

    #[tokio::test]
    async fn test_resume_from_before_node_checkpoint() {
        use wf_checkpoint::state::CheckpointStateManager;
        use wf_checkpoint::state::WorkflowCheckpointStateManager;
        use wf_types::checkpoint::{CheckpointTiming, UnifiedCheckpointPolicy};

        let store = Arc::new(StorageBackend::new_memory());
        let strategy = NodeCheckpointStrategy::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTiming::BeforeExecute],
            content: None,
            retention: None,
            error_handling: None,
        });
        let lifecycle = WorkflowLifecycleCoordinator::with_store(None, store.clone())
            .with_checkpoint_strategy(strategy);
        let workflow_id = wf_types::Id::from("wf-before-2".to_string());
        let tool_registry = Arc::new(wf_tools::registry::ToolRegistry::new());
        let handlers = make_handlers();

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-before-2".to_string()),
            workflow_id: workflow_id.clone(),
            graph: make_graph(),
            options: WorkflowExecutionOptions {
                max_steps: Some(2),
                ..options_with(Some(serde_json::json!({"greeting": "hello"})))
            },
            handlers: handlers.clone(),
            tool_registry: tool_registry.clone(),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        };
        lifecycle
            .execute_workflow(params)
            .await
            .expect("first run should complete");

        let sm = WorkflowCheckpointStateManager::new(store.clone());
        let count_before_resume = sm
            .list_by_entity("exec-before-2")
            .await
            .expect("checkpoints listed")
            .len();
        assert_eq!(
            count_before_resume, 4,
            "Manual + Before(start) + Before(v1) + OnComplete expected"
        );

        let resumed = lifecycle
            .resume_workflow(
                "exec-before-2",
                workflow_id,
                make_graph(),
                handlers,
                tool_registry,
                Vec::new(),
            )
            .await
            .expect("resume should complete the workflow");
        assert_eq!(
            resumed.result,
            serde_json::json!({"greeting": "hello"}),
            "completed nodes must not re-execute; their outputs feed downstream"
        );

        // Resume run adds between 4 and 6 checkpoints depending on which
        // same-millisecond checkpoint is selected as the resume source:
        // Manual + one Before(node) per re-executed node + OnComplete. Nodes
        // recorded as completed in the selected snapshot are never
        // re-executed, keeping the total within this range.
        let count_after = sm
            .list_by_entity("exec-before-2")
            .await
            .expect("checkpoints listed")
            .len();
        assert!(
            (8..=10).contains(&count_after),
            "completed nodes must not re-execute after resume, got {}",
            count_after
        );
    }

    #[tokio::test]
    async fn test_resume_from_before_node_checkpoint_reruns_incomplete_node() {
        use wf_checkpoint::state::CheckpointStateManager;
        use wf_checkpoint::state::WorkflowCheckpointStateManager;
        use wf_types::checkpoint::{CheckpointTiming, UnifiedCheckpointPolicy};

        let store = Arc::new(StorageBackend::new_memory());
        let strategy = NodeCheckpointStrategy::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTiming::BeforeExecute],
            content: None,
            retention: None,
            error_handling: None,
        });
        let lifecycle = WorkflowLifecycleCoordinator::with_store(None, store.clone())
            .with_checkpoint_strategy(strategy);
        let workflow_id = wf_types::Id::from("wf-before-3".to_string());
        let tool_registry = Arc::new(wf_tools::registry::ToolRegistry::new());
        let handlers = make_handlers();

        // v1 always fails (read-only assignment); on_failure defaults to
        // "fail" so run 1 ends with an error and no OnComplete checkpoint.
        let graph = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "v1",
                    "VARIABLE",
                    serde_json::json!({
                        "variable_name": "__forbidden",
                        "expression": "1"
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            edges: vec![edge("start", "v1"), edge("v1", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        };

        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-before-3".to_string()),
            workflow_id: workflow_id.clone(),
            graph: graph.clone(),
            options: options_with(Some(serde_json::json!({"greeting": "hello"}))),
            handlers: handlers.clone(),
            tool_registry: tool_registry.clone(),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        };
        lifecycle
            .execute_workflow(params)
            .await
            .expect_err("run 1 must fail at v1");

        let sm = WorkflowCheckpointStateManager::new(store.clone());
        let count_before_resume = sm
            .list_by_entity("exec-before-3")
            .await
            .expect("checkpoints listed")
            .len();
        assert_eq!(
            count_before_resume, 3,
            "Manual + Before(start) + Before(v1), no OnComplete on failure"
        );

        // Resume from the Before(v1) checkpoint: v1 never completed, so it
        // must execute again and fail again with the same structured error.
        let err = lifecycle
            .resume_workflow(
                "exec-before-3",
                workflow_id,
                graph,
                handlers,
                tool_registry,
                Vec::new(),
            )
            .await
            .expect_err("incomplete node must re-execute and fail");
        assert!(err.to_string().contains("read-only"), "unexpected: {}", err);

        let count_after = sm
            .list_by_entity("exec-before-3")
            .await
            .expect("checkpoints listed")
            .len();
        // Resume adds Manual + one Before(node) per re-executed node: 2 if
        // resumed from Before(v1), 3 if a start-level snapshot is selected.
        assert!(
            (5..=6).contains(&count_after),
            "start must not re-execute beyond the selected snapshot, got {}",
            count_after
        );
    }
}
