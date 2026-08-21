use std::sync::Arc;

use async_trait::async_trait;

use wf_core::scheduler::{TaskCallback, TaskPriority, TaskScheduler};
use wf_execution_shared::hooks::HookRegistry;
use wf_execution_shared::types::execution_entity::IExecutionEntity;
use wf_llm::LlmGateway;
use wf_tools::callback::{
    AgentLoopConfig, AgentLoopInput, AgentLoopOutput, ExecutionCallback, ExecutionStatus,
    SpawnedAgentLoop, WorkflowInput, WorkflowOutput,
};
use wf_tools::error::{ToolError, ToolResult};
use wf_tools::registry::ToolRegistry;
use wf_types::Id;

use crate::coordinator::lifecycle::AgentLoopCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::registry::{AgentLoopRegistry, DEFAULT_MAX_SUB_AGENT_DEPTH};

/// Agent loop engine entry point: runs agent loops (sync), dispatches them
/// asynchronously (`spawn_agent_loop`) and serves status queries and
/// cancellation over the shared execution registry.
pub struct AgentLoopExecutor {
    gateway: std::sync::Arc<LlmGateway>,
    registry: std::sync::Arc<ToolRegistry>,
    agent_registry: std::sync::Arc<AgentLoopRegistry>,
    max_iterations: u32,
    max_sub_agent_depth: u32,
    event_bus: Option<Arc<wf_core::EventBus>>,
    hook_registry: Option<Arc<HookRegistry>>,
    /// Shared task scheduler for fire-and-forget agent loop executions.
    /// When set, `spawn_agent_loop` submits the coordinator task through
    /// the scheduler instead of using raw `tokio::spawn`, enabling
    /// priority-based scheduling and concurrency control.
    scheduler: Option<Arc<TaskScheduler>>,
}

impl AgentLoopExecutor {
    pub fn new(
        gateway: std::sync::Arc<LlmGateway>,
        registry: std::sync::Arc<ToolRegistry>,
    ) -> Self {
        Self {
            gateway,
            registry,
            agent_registry: std::sync::Arc::new(AgentLoopRegistry::new()),
            max_iterations: 10,
            max_sub_agent_depth: DEFAULT_MAX_SUB_AGENT_DEPTH,
            event_bus: None,
            hook_registry: None,
            scheduler: None,
        }
    }

    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    /// Maximum sub-agent recursion depth (root = depth 0). A nested spawn
    /// whose resolved depth would exceed the limit is rejected with
    /// `AgentError::ExecutionLimitReached`.
    pub fn with_max_sub_agent_depth(mut self, max: u32) -> Self {
        self.max_sub_agent_depth = max;
        self.agent_registry.set_max_sub_agent_depth(max);
        self
    }

    /// Concurrent-execution limit enforced by the registry's capacity gate.
    pub fn with_max_concurrent(self, max: usize) -> Self {
        self.agent_registry.set_max_concurrent(max);
        self
    }

    /// Inject a shared registry so external consumers (wf-api execution
    /// views, wf-runtime composite callback) observe the same executions as
    /// this executor. Without injection a private instance is used. The
    /// executor's capacity/depth limits are applied to the injected registry.
    pub fn with_shared_registry(
        mut self,
        agent_registry: std::sync::Arc<AgentLoopRegistry>,
    ) -> Self {
        agent_registry.set_max_sub_agent_depth(self.max_sub_agent_depth);
        self.agent_registry = agent_registry;
        self
    }

    /// The registry this executor registers its executions into.
    pub fn agent_registry(&self) -> &std::sync::Arc<AgentLoopRegistry> {
        &self.agent_registry
    }

    /// Inject the shared event bus; loops started here publish their hook
    /// executions as `HOOK_TRIGGERED` events on it.
    pub fn with_event_bus(mut self, event_bus: Arc<wf_core::EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Inject the shared hook receiver registry; hook points and engine
    /// signals of loops started here dispatch through it.
    pub fn with_hook_registry(mut self, registry: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(registry);
        self
    }

    /// Inject the shared task scheduler for fire-and-forget agent loop
    /// executions. When set, `spawn_agent_loop` submits the coordinator
    /// task through the scheduler instead of `tokio::spawn`.
    pub fn with_scheduler(mut self, scheduler: Arc<TaskScheduler>) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    fn coordinator(&self, input: &AgentLoopInput) -> AgentLoopCoordinator {
        let parent_execution_id = input
            .context
            .get("parent_execution_id")
            .and_then(|v| v.as_str())
            .map(Id::from);
        let mut coordinator =
            AgentLoopCoordinator::new(self.gateway.clone(), self.registry.clone())
                .with_entity_registry(self.agent_registry.clone())
                .with_parent_execution_id(parent_execution_id);
        if let Some(bus) = &self.event_bus {
            coordinator = coordinator.with_event_bus(bus.clone());
        }
        if let Some(registry) = &self.hook_registry {
            coordinator = coordinator.with_hook_registry(registry.clone());
        }
        coordinator
    }

    pub async fn execute(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let coordinator = self.coordinator(&input);
        let output = coordinator.execute(config, input).await;
        // Sync path writes the result slot too, keeping both dispatch paths
        // consistent: a later query_execution_status returns the output.
        if let Ok(output) = &output {
            self.agent_registry
                .store_result(output.agent_loop_id.clone(), output.clone());
        }
        output
    }

    /// Dispatch an agent loop in the background and return a handle
    /// immediately. The execution id is pre-generated and injected into the
    /// coordinator so the registry handle exists before the task runs:
    /// `query_execution_status` / `cancel_execution` work right away. The
    /// terminal output is stored in the registry result slot. When the
    /// caller is itself a registered agent loop (parent id in the context),
    /// the background task is cancelled through the parent's abort signal
    /// (trigger.rs pattern).
    pub async fn spawn_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<SpawnedAgentLoop> {
        let execution_id = Id::from(wf_common::generate_id());

        // Depth gate: a nested spawn whose resolved depth would exceed
        // `max_sub_agent_depth` is rejected before any slot is reserved. The
        // parent depth resolves from the registered parent entity; an
        // unknown parent (external registry) is treated as a root parent.
        if let Some(parent_id) = input
            .context
            .get("parent_execution_id")
            .and_then(|v| v.as_str())
            .map(Id::from)
        {
            let parent_depth = self
                .agent_registry
                .get(&parent_id)
                .map(|p| p.get_hierarchy_depth())
                .unwrap_or(0);
            if !self.agent_registry.depth_allowed(parent_depth) {
                return Err(AgentError::ExecutionLimitReached(format!(
                    "sub-agent depth {} exceeds max {}",
                    parent_depth.saturating_add(1),
                    self.agent_registry.max_sub_agent_depth()
                )));
            }
        }

        // Pre-register a placeholder entity so the execution is queryable
        // and cancellable from the moment spawn returns; the coordinator
        // replaces it with the fully-built entity on its first steps. The
        // capacity gate is enforced here: overflow surfaces immediately.
        self.agent_registry
            .register(Arc::new(AgentLoopEntity::new(execution_id.clone())))?;

        let parent_token = input
            .context
            .get("parent_execution_id")
            .and_then(|v| v.as_str())
            .map(Id::from)
            .and_then(|parent_id| self.agent_registry.get(&parent_id))
            .map(|parent| parent.get_abort_signal());

        let agent_registry = self.agent_registry.clone();
        let run_id = execution_id.clone();
        let coordinator = self
            .coordinator(&input)
            .with_agent_loop_id(execution_id.clone());

        if let Some(scheduler) = &self.scheduler {
            let callback: TaskCallback = Box::new(move || Box::pin(async move {
                match coordinator.execute(config, input).await {
                    Ok(output) => {
                        agent_registry.store_result(run_id.clone(), output);
                    }
                    Err(e) => {
                        tracing::warn!(
                            execution_id = %run_id,
                            error = %e,
                            "spawned agent loop failed"
                        );
                    }
                }
                agent_registry.unregister_task(&run_id);
            }));
            let _ = scheduler.submit_and_forget(
                execution_id.to_string(),
                "agent_loop".to_string(),
                callback,
                TaskPriority::Normal,
                None,
            );
        } else {
            let handle = tokio::spawn(async move {
                match coordinator.execute(config, input).await {
                    Ok(output) => {
                        agent_registry.store_result(run_id.clone(), output);
                    }
                    Err(e) => {
                        tracing::warn!(
                            execution_id = %run_id,
                            error = %e,
                            "spawned agent loop failed"
                        );
                    }
                }
                agent_registry.unregister_task(&run_id);
            });
            self.agent_registry
                .register_task(execution_id.clone(), handle);
        }

        // Parent cancellation propagation: when the parent loop stops, the
        // child execution is stopped as well.
        if let Some(token) = parent_token {
            let agent_registry = self.agent_registry.clone();
            let child_id = execution_id.clone();
            tokio::spawn(async move {
                token.cancelled().await;
                if let Some(entity) = agent_registry.get(&child_id) {
                    let _ = entity.stop().await;
                }
            });
        }

        Ok(SpawnedAgentLoop {
            agent_loop_id: execution_id.clone(),
            execution_id,
            status: "started".to_string(),
        })
    }
}

fn status_string(status: &wf_execution_shared::types::execution_entity::ExecutionStatus) -> String {
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
impl ExecutionCallback for AgentLoopExecutor {
    async fn execute_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput> {
        self.execute(config, input)
            .await
            .map_err(|e| wf_tools::error::ToolError::ExecutionError(e.to_string()))
    }

    async fn spawn_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<SpawnedAgentLoop> {
        self.spawn_agent_loop(config, input)
            .await
            .map_err(|e| wf_tools::error::ToolError::ExecutionError(e.to_string()))
    }

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        _input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput> {
        Err(ToolError::ExecutionError(format!(
            "workflow execution is not supported by AgentLoopExecutor: {}",
            workflow_id
        )))
    }

    async fn query_execution_status(&self, execution_id: &str) -> ToolResult<ExecutionStatus> {
        let id = Id::from(execution_id.to_string());
        let entity = self
            .agent_registry
            .get(&id)
            .ok_or_else(|| ToolError::NotFound(format!("execution {} not found", execution_id)))?;
        let state = entity.state.read().await;
        let status = state.status();
        // Terminal executions hand over their stored output once.
        let result = if matches!(
            status,
            wf_execution_shared::types::execution_entity::ExecutionStatus::Completed
                | wf_execution_shared::types::execution_entity::ExecutionStatus::Failed
                | wf_execution_shared::types::execution_entity::ExecutionStatus::Cancelled
                | wf_execution_shared::types::execution_entity::ExecutionStatus::Stopped
        ) {
            self.agent_registry
                .take_result(&id)
                .map(|output| output.result)
        } else {
            None
        };
        Ok(ExecutionStatus {
            execution_id: id,
            status: status_string(&status),
            progress: None,
            result,
        })
    }

    async fn cancel_execution(&self, execution_id: &str) -> ToolResult<()> {
        let id = Id::from(execution_id.to_string());
        let entity = self
            .agent_registry
            .get(&id)
            .ok_or_else(|| ToolError::NotFound(format!("execution {} not found", execution_id)))?;
        entity
            .stop()
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
        self.agent_registry.abort_task(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use wf_llm::mock::{LlmResponseSpec, MockLlmClient};
    use wf_tools::callback::AgentLoopInput;

    fn agent_config(agent_id: &str) -> AgentLoopConfig {
        AgentLoopConfig {
            agent_id: Id::from(agent_id.to_string()),
            model: "mock".to_string(),
            max_iterations: Some(3),
            max_execution_time: None,
            hooks: Vec::new(),
            available_tool_names: Vec::new(),
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            activated_tool_names: Vec::new(),
            hidden_tool_names: Vec::new(),
            tool_call_format: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: Some(false),
            general_description: None,
            discoverable_metadata_block: None,
        }
    }

    fn agent_input(message: &str, parent: Option<&str>) -> AgentLoopInput {
        let mut context = std::collections::HashMap::new();
        if let Some(parent) = parent {
            context.insert(
                "parent_execution_id".to_string(),
                serde_json::Value::String(parent.to_string()),
            );
        }
        AgentLoopInput {
            message: message.to_string(),
            context,
            conversation: Vec::new(),
        }
    }

    async fn make_executor() -> AgentLoopExecutor {
        let gateway = Arc::new(LlmGateway::new());
        let mock = Arc::new(MockLlmClient::new());
        mock.default(LlmResponseSpec::text("done").with_usage(10, 5));
        gateway.register_mock("mock", mock);
        let registry = Arc::new(wf_tools::create_default_tool_registry());
        AgentLoopExecutor::new(gateway, registry)
    }

    #[tokio::test]
    async fn test_spawn_returns_immediately_and_background_advances() {
        let executor = make_executor().await;
        let spawned = executor
            .spawn_agent_loop(agent_config("agent-a"), agent_input("run", None))
            .await
            .expect("spawn must return immediately");
        assert_eq!(spawned.status, "started");
        assert_eq!(spawned.execution_id, spawned.agent_loop_id);

        // The execution is registered and progresses in the background.
        for _ in 0..200 {
            let status = executor
                .query_execution_status(&spawned.execution_id.to_string())
                .await;
            match status {
                Ok(status) if status.status == "completed" => return,
                Ok(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                Err(e) => panic!("query failed: {e}"),
            }
        }
        panic!("spawned execution did not complete in time");
    }

    #[tokio::test]
    async fn test_spawn_query_returns_result_at_terminal() {
        let executor = make_executor().await;
        let spawned = executor
            .spawn_agent_loop(agent_config("agent-b"), agent_input("run", None))
            .await
            .expect("spawn must succeed");

        let mut result = None;
        for _ in 0..200 {
            let status = executor
                .query_execution_status(&spawned.execution_id.to_string())
                .await
                .expect("query must succeed");
            if status.status == "completed" {
                result = status.result;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let result = result.expect("completed execution must carry its result");
        assert_eq!(result, serde_json::Value::String("done".to_string()));
    }

    #[tokio::test]
    async fn test_sync_execution_registers_and_is_queryable() {
        let executor = make_executor().await;
        let output = executor
            .execute(agent_config("agent-c"), agent_input("run", None))
            .await
            .expect("sync execution must succeed");

        let status = executor
            .query_execution_status(&output.agent_loop_id.to_string())
            .await
            .expect("registered execution must be queryable");
        assert_eq!(status.status, "completed");
    }

    #[tokio::test]
    async fn test_before_and_after_agent_hooks_fire_in_order() {
        use wf_core::EventBus;
        use wf_types::events::EventType;

        let bus = Arc::new(EventBus::new(32));
        let mut config = agent_config("agent-hooks");
        config.hooks = vec![
            wf_tools::callback::HookConfig {
                hook_type: "BEFORE_AGENT".to_string(),
                condition: None,
                enabled: true,
                parallel: None,
                continue_on_error: None,
                receiver: None,
            },
            wf_tools::callback::HookConfig {
                hook_type: "AFTER_AGENT".to_string(),
                condition: None,
                enabled: true,
                parallel: None,
                continue_on_error: None,
                receiver: None,
            },
        ];

        let gateway = Arc::new(LlmGateway::new());
        let mock = Arc::new(MockLlmClient::new());
        mock.default(LlmResponseSpec::text("done").with_usage(10, 5));
        gateway.register_mock("mock", mock);
        let tool_registry = Arc::new(wf_tools::create_default_tool_registry());
        let executor = AgentLoopExecutor::new(gateway, tool_registry).with_event_bus(bus.clone());

        let mut sub = bus.subscribe();

        executor
            .execute(config, agent_input("run", None))
            .await
            .expect("execution with hooks must succeed");

        // Each hook batch publishes one HOOK_TRIGGERED event; BEFORE_AGENT
        // must be observable before AFTER_AGENT.
        let mut hook_events = Vec::new();
        for _ in 0..16 {
            match sub.try_recv() {
                Ok(event) if event.r#type == EventType::HookTriggered => {
                    let hook_types = event
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("hook_type"))
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    hook_events.push(
                        hook_types
                            .first()
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert_eq!(hook_events.len(), 2, "both agent lifecycle hooks must fire");
        let before = hook_events
            .iter()
            .position(|t| t == "BEFORE_AGENT")
            .expect("BEFORE_AGENT must fire");
        let after = hook_events
            .iter()
            .position(|t| t == "AFTER_AGENT")
            .expect("AFTER_AGENT must fire");
        assert!(before < after, "BEFORE_AGENT runs before AFTER_AGENT");
    }

    #[tokio::test]
    async fn test_after_agent_fires_on_failure_path_with_error_details() {
        use std::collections::HashMap;

        use wf_execution_shared::hooks::{HookContext, HookOutcome, HookReceiver, HookRegistry};
        use wf_llm::error::LlmError;

        // The LLM fails hard: the loop errors out and AFTER_AGENT must fire
        // on the failure path (success=false + error summary), not only on
        // the success path.
        let bus = Arc::new(wf_core::EventBus::new(32));
        let hook_registry = Arc::new(HookRegistry::new());
        let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
        struct FailureRecorder(
            #[allow(clippy::type_complexity)]
            Arc<std::sync::Mutex<Vec<(String, HashMap<String, serde_json::Value>)>>>,
        );
        #[async_trait::async_trait]
        impl HookReceiver for FailureRecorder {
            fn name(&self) -> &str {
                "after_agent_failure_recorder"
            }

            async fn on_hook(&self, ctx: &HookContext) -> HookOutcome {
                self.0
                    .lock()
                    .unwrap()
                    .push((ctx.hook_type.clone(), ctx.data.clone()));
                HookOutcome::Continue
            }
        }
        hook_registry.register(
            "AFTER_AGENT",
            Arc::new(FailureRecorder(captured.clone())),
            0,
        );

        let gateway = Arc::new(LlmGateway::new());
        let mock = Arc::new(MockLlmClient::new());
        // Exhaust the retry budget (max_retries = 3): every attempt fails,
        // so the loop must error out instead of falling back to a success.
        for _ in 0..4 {
            mock.script_error(LlmError::ProviderError("LLM provider exploded".to_string()));
        }
        gateway.register_mock("mock", mock);
        let tool_registry = Arc::new(wf_tools::create_default_tool_registry());
        let executor = AgentLoopExecutor::new(gateway, tool_registry)
            .with_event_bus(bus)
            .with_hook_registry(hook_registry);

        let result = executor
            .execute(agent_config("agent-fail"), agent_input("run", None))
            .await;
        assert!(result.is_err(), "a failing LLM must fail the loop");

        let recorded = captured.lock().unwrap();
        assert_eq!(
            recorded.len(),
            1,
            "AFTER_AGENT must fire exactly once on the failure path"
        );
        let (hook_type, data) = &recorded[0];
        assert_eq!(hook_type, "AFTER_AGENT");
        assert_eq!(data.get("success"), Some(&serde_json::Value::Bool(false)));
        let error = data
            .get("error")
            .and_then(|v| v.as_str())
            .expect("failure path must carry an error summary");
        assert!(
            error.contains("LLM provider exploded"),
            "error summary must surface the failure: {error}"
        );
    }

    #[tokio::test]
    async fn test_cancel_interrupts_in_flight_execution() {
        let gateway = Arc::new(LlmGateway::new());
        let mock = Arc::new(MockLlmClient::new());
        // A slow LLM keeps the loop in flight long enough to be cancelled:
        // the mock would otherwise answer so fast the loop already reached a
        // terminal state and cancelling it becomes an illegal transition.
        mock.default(
            LlmResponseSpec::text("final")
                .with_usage(10, 5)
                .with_delay(300),
        );
        gateway.register_mock("mock", mock);
        let registry = Arc::new(wf_tools::create_default_tool_registry());
        let executor = AgentLoopExecutor::new(gateway, registry);

        let spawned = executor
            .spawn_agent_loop(
                agent_config("agent-d"),
                agent_input("run and take your time", None),
            )
            .await
            .expect("spawn must succeed");

        // Give the background task a moment to start, then cancel.
        tokio::time::sleep(Duration::from_millis(20)).await;
        executor
            .cancel_execution(&spawned.execution_id.to_string())
            .await
            .expect("cancel must succeed");

        let status = executor
            .query_execution_status(&spawned.execution_id.to_string())
            .await
            .expect("query after cancel must succeed");
        assert_eq!(status.status, "cancelled");
    }

    #[tokio::test]
    async fn test_pause_suspends_and_resume_reenters_the_loop() {
        use wf_execution_shared::types::execution_entity::ExecutionStatus;
        use wf_execution_shared::types::execution_entity::IExecutionEntity;

        // A slow LLM keeps the first iteration in flight so the pause lands
        // on a running loop instead of racing a completed one.
        let gateway = Arc::new(LlmGateway::new());
        let mock = Arc::new(MockLlmClient::new());
        mock.default(
            LlmResponseSpec::text("final")
                .with_usage(10, 5)
                .with_delay(300),
        );
        gateway.register_mock("mock", mock);
        let registry = Arc::new(wf_tools::create_default_tool_registry());
        let executor = AgentLoopExecutor::new(gateway, registry);

        let spawned = executor
            .spawn_agent_loop(agent_config("agent-pause"), agent_input("run", None))
            .await
            .expect("spawn must succeed");

        // Pause the running loop mid-iteration.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let entity = executor
            .agent_registry()
            .get(&spawned.execution_id)
            .expect("entity present");
        entity.pause().await.expect("pause must succeed");
        assert_eq!(
            entity.state.read().await.status(),
            ExecutionStatus::Paused,
            "pause must flip the state machine"
        );

        // While paused the loop is suspended: no result, status stays paused.
        let status = executor
            .query_execution_status(&spawned.execution_id.to_string())
            .await
            .expect("query during pause must succeed");
        assert_eq!(status.status, "paused");

        // Resume: the loop re-enters its iteration loop and completes.
        entity.resume().await.expect("resume must succeed");
        for _ in 0..300 {
            let status = executor
                .query_execution_status(&spawned.execution_id.to_string())
                .await
                .expect("query after resume must succeed");
            if status.status == "completed" {
                assert_eq!(
                    status.result,
                    Some(serde_json::Value::String("final".to_string())),
                    "resumed loop must deliver its real result, not a pause artifact"
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("paused loop did not complete after resume");
    }

    #[tokio::test]
    async fn test_query_unknown_returns_not_found() {
        let executor = make_executor().await;
        let err = executor
            .query_execution_status("missing-id")
            .await
            .expect_err("unknown execution must not be found");
        assert!(matches!(err, ToolError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_spawn_links_parent_and_propagates_cancel() {
        // A delayed mock keeps both parent and child in flight so the parent
        // cancel reaches a running (not yet completed) child.
        let gateway = Arc::new(LlmGateway::new());
        let mock = Arc::new(MockLlmClient::new());
        mock.default(
            LlmResponseSpec::text("done")
                .with_usage(10, 5)
                .with_delay(300),
        );
        gateway.register_mock("mock", mock);
        let registry = Arc::new(wf_tools::create_default_tool_registry());
        let executor = AgentLoopExecutor::new(gateway, registry);

        // The parent loop is itself a registered execution.
        let parent = executor
            .spawn_agent_loop(agent_config("agent-parent"), agent_input("run", None))
            .await
            .expect("parent spawn must succeed");
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Child carries the parent execution id in its context.
        let child = executor
            .spawn_agent_loop(
                agent_config("agent-child"),
                agent_input("run", Some(&parent.execution_id.to_string())),
            )
            .await
            .expect("child spawn must succeed");

        // The parent entity tracks the child once the child task registers
        // the linkage; poll for it.
        let parent_entity = executor
            .agent_registry()
            .get(&parent.execution_id)
            .expect("parent entity present");
        let mut linked = false;
        for _ in 0..100 {
            if parent_entity
                .child_execution_ids()
                .read()
                .await
                .contains(&child.execution_id)
            {
                linked = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(linked, "child must be linked onto the parent");

        // Cancelling the parent stops the child too.
        executor
            .cancel_execution(&parent.execution_id.to_string())
            .await
            .expect("parent cancel must succeed");
        for _ in 0..100 {
            let status = executor
                .query_execution_status(&child.execution_id.to_string())
                .await
                .expect("child query must succeed");
            if status.status == "cancelled" || status.status == "completed" {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("child did not settle after parent cancel");
    }

    /// A nested spawn whose resolved depth exceeds `max_sub_agent_depth`
    /// is rejected before any slot is reserved.
    #[tokio::test]
    async fn test_spawn_rejects_beyond_depth_limit() {
        let executor = make_executor()
            .await
            .with_max_sub_agent_depth(1)
            .with_max_concurrent(16);

        // A registered parent at depth 1 would push the child to depth 2.
        let deep_parent = Arc::new(
            AgentLoopEntity::new(Id::from("depth-parent".to_string())).with_hierarchy_depth(1),
        );
        executor
            .agent_registry()
            .register(deep_parent)
            .expect("parent register must succeed");

        let err = executor
            .spawn_agent_loop(
                agent_config("agent-deep"),
                agent_input("run", Some("depth-parent")),
            )
            .await
            .expect_err("depth 2 must be rejected");
        assert!(
            matches!(err, AgentError::ExecutionLimitReached(_)),
            "depth overflow must surface as ExecutionLimitReached: {err}"
        );

        // A root-parent depth (0 -> child depth 1) is within the limit.
        executor
            .agent_registry()
            .register(Arc::new(AgentLoopEntity::new(Id::from(
                "root-parent".to_string(),
            ))))
            .expect("root parent register must succeed");
        executor
            .spawn_agent_loop(
                agent_config("agent-ok"),
                agent_input("run", Some("root-parent")),
            )
            .await
            .expect("depth 1 must be allowed");
    }

    /// Spawning beyond the concurrent-execution limit is rejected at
    /// spawn time; the sync path is gated through the same registry.
    #[tokio::test]
    async fn test_spawn_rejects_when_concurrency_limit_reached() {
        let executor = make_executor().await.with_max_concurrent(1);

        executor
            .spawn_agent_loop(agent_config("agent-conc-1"), agent_input("run", None))
            .await
            .expect("first spawn fits the single slot");

        let err = executor
            .spawn_agent_loop(agent_config("agent-conc-2"), agent_input("run", None))
            .await
            .expect_err("second spawn must hit the capacity gate");
        assert!(
            matches!(err, AgentError::ExecutionLimitReached(_)),
            "overflow must surface as ExecutionLimitReached: {err}"
        );
    }

    /// The sync `execute` path is gated through the shared registry too.
    #[tokio::test]
    async fn test_sync_execute_honors_concurrency_limit() {
        let executor = make_executor().await.with_max_concurrent(1);

        executor
            .execute(agent_config("agent-sync-1"), agent_input("run", None))
            .await
            .expect("first sync run fits the slot");

        // The first execution stays registered (no cleanup) so the second run
        // is rejected before it starts.
        let err = executor
            .execute(agent_config("agent-sync-2"), agent_input("run", None))
            .await
            .expect_err("second sync run must hit the capacity gate");
        assert!(matches!(err, AgentError::ExecutionLimitReached(_)));
    }
}
