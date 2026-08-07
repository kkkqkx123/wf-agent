//! Workflow execution builder.
//!
//! Mirrors the TS `ExecutionBuilder` (`packages/sdk/api/workflow/builders`):
//! a fluent executor over [`crate::workflow::workflow_execution`] free functions with
//! execution options and `on_node_executed` / `on_progress` / `on_error`
//! callbacks, plus `execute` / `execute_stream` / `cancel` entry points.

use std::sync::Arc;

use futures::StreamExt;
use serde_json::Value;
use wf_tools::callback::WorkflowOutput;
use wf_types::workflow_execution::WorkflowExecutionOptions;

use crate::workflow::workflow_execution::ExecuteWorkflowParams;
use crate::{ApiContext, ApiResult};

/// Info handed to the `on_node_executed` callback.
#[derive(Debug, Clone)]
pub struct NodeExecutedInfo {
    pub node_id: String,
    pub node_name: Option<String>,
    pub node_type: Option<String>,
    pub duration_ms: Option<i64>,
}

/// Callback types registered on the [`ExecutionBuilder`].
pub type NodeExecutedCallback = Arc<dyn Fn(&NodeExecutedInfo) + Send + Sync>;
pub type ProgressCallback = Arc<dyn Fn(f64) + Send + Sync>;
pub type ErrorCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Consuming workflow execution builder over the workflow execution module.
pub struct ExecutionBuilder {
    workflow_id: String,
    input: Option<Value>,
    options: WorkflowExecutionOptions,
    on_node_executed: Vec<NodeExecutedCallback>,
    on_progress: Vec<ProgressCallback>,
    on_error: Vec<ErrorCallback>,
}

impl ExecutionBuilder {
    /// Start building an execution for the given workflow id.
    pub fn new(workflow_id: impl Into<String>) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            input: None,
            options: default_options(),
            on_node_executed: Vec::new(),
            on_progress: Vec::new(),
            on_error: Vec::new(),
        }
    }

    /// Set the top-level execution input (exposed as the `input` variable).
    pub fn with_input(mut self, input: Value) -> Self {
        self.input = Some(input);
        self
    }

    /// Cap the number of execution steps.
    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        self.options.max_steps = Some(max_steps);
        self
    }

    /// Set the wall-clock timeout in milliseconds.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.options.timeout = Some(timeout_ms);
        self
    }

    /// Enable execution checkpoints.
    pub fn with_checkpoints(mut self, enabled: bool) -> Self {
        self.options.enable_checkpoints = Some(enabled);
        self
    }

    /// Set the workflow-level failure strategy (`fail` | `continue` | `retry`).
    pub fn with_on_failure(mut self, on_failure: impl Into<String>) -> Self {
        self.options.on_failure = Some(on_failure.into());
        self
    }

    /// Register a callback invoked whenever a node completes.
    pub fn on_node_executed(
        mut self,
        callback: impl Fn(&NodeExecutedInfo) + Send + Sync + 'static,
    ) -> Self {
        self.on_node_executed.push(Arc::new(callback));
        self
    }

    /// Register a callback invoked with the execution progress (0.0 ..= 1.0).
    pub fn on_progress(mut self, callback: impl Fn(f64) + Send + Sync + 'static) -> Self {
        self.on_progress.push(Arc::new(callback));
        self
    }

    /// Register a callback invoked when a node / workflow fails.
    pub fn on_error(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_error.push(Arc::new(callback));
        self
    }

    /// Execute the workflow to completion and await its output. The registered
    /// callbacks are dispatched from engine events via a bus subscription
    /// opened *before* the execution is spawned, so no event is missed
    /// regardless of whether the stream forwarder task is scheduled first.
    pub async fn execute(self, ctx: &Arc<ApiContext>) -> ApiResult<WorkflowOutput> {
        let callbacks = self.callbacks();
        // Subscribe to the bus before spawning the workflow so the
        // subscription exists when the engine starts publishing events.
        let mut sub = ctx.event_bus.subscribe();
        let params = ExecuteWorkflowParams {
            workflow_id: self.workflow_id,
            input: self.input,
            options: Some(self.options),
        };
        let (execution_id, mut stream) =
            crate::workflow::workflow_execution::stream(ctx.clone(), params).await?;
        let execution_id_filter = execution_id.to_string();

        // Dispatch engine events from the bus for this execution, and
        // wait for the terminal Completed/Failed from the stream.
        let result = loop {
            tokio::select! {
                biased;
                event = sub.recv() => {
                    if let Ok(event) = event {
                        if event.execution_id.as_deref() == Some(execution_id_filter.as_str()) {
                            callbacks.dispatch(&event);
                        }
                    }
                }
                next = stream.next() => {
                    match next {
                        Some(crate::infra::stream::ExecutionStreamEvent::Completed { result: value, .. }) => {
                            break Some(value);
                        }
                        Some(crate::infra::stream::ExecutionStreamEvent::Failed { error }) => {
                            callbacks.dispatch_error(&error);
                            return Err(crate::ApiError::execution(error));
                        }
                        _ => {}
                    }
                }
            }
        };
        callbacks.dispatch_progress(1.0);
        result
            .map(|result| WorkflowOutput {
                execution_id,
                result,
            })
            .ok_or_else(|| {
                crate::ApiError::execution("stream ended without a terminal event".to_string())
            })
    }

    /// Execute the workflow and stream engine events
    /// ([`crate::infra::stream::ExecutionEventStream`]) while dispatching the
    /// registered callbacks. Returns the generated `execution_id` alongside
    /// the stream so the caller can `pause` / `cancel` the execution.
    ///
    /// The bus subscription is opened *before* the execution is spawned, so
    /// the callbacks receive every engine event even if the caller never
    /// polls the returned stream.
    pub async fn execute_stream(
        self,
        ctx: &Arc<ApiContext>,
    ) -> ApiResult<(wf_types::Id, crate::infra::stream::ExecutionEventStream)> {
        let callbacks = self.callbacks();
        // Subscribe ahead of the execution so no engine event is missed.
        let bus = ctx.event_bus.clone();
        let mut sub = bus.subscribe();
        let (execution_id, stream) = crate::workflow::workflow_execution::stream(
            ctx.clone(),
            ExecuteWorkflowParams {
                workflow_id: self.workflow_id,
                input: self.input,
                options: Some(self.options),
            },
        )
        .await?;

        // Forward matching engine events onto the callbacks.
        let execution_id_filter = execution_id.to_string();
        tokio::spawn(async move {
            while let Ok(event) = sub.recv().await {
                if event.execution_id.as_deref() != Some(execution_id_filter.as_str()) {
                    continue;
                }
                callbacks.dispatch(&event);
            }
        });

        Ok((execution_id, stream))
    }

    /// Cancel a running workflow execution by its id (delegates to
    /// [`crate::workflow::workflow_execution::cancel`]).
    pub async fn cancel(&self, ctx: &Arc<ApiContext>, execution_id: &str) -> ApiResult<()> {
        crate::workflow::workflow_execution::cancel(ctx, execution_id).await
    }

    fn callbacks(&self) -> CallbackPack {
        CallbackPack {
            on_node_executed: self.on_node_executed.clone(),
            on_progress: self.on_progress.clone(),
            on_error: self.on_error.clone(),
        }
    }
}

/// Cloned callback set handed to the event dispatch loop.
#[derive(Clone)]
struct CallbackPack {
    on_node_executed: Vec<NodeExecutedCallback>,
    on_progress: Vec<ProgressCallback>,
    on_error: Vec<ErrorCallback>,
}

impl CallbackPack {
    /// Dispatch a workflow engine event onto the registered callbacks.
    fn dispatch(&self, event: &wf_types::events::BaseEvent) {
        match event.r#type {
            wf_types::events::EventType::NodeCompleted => {
                if let Some(info) = node_info(event) {
                    for cb in &self.on_node_executed {
                        cb(&info);
                    }
                }
            }
            wf_types::events::EventType::NodeFailed
            | wf_types::events::EventType::WorkflowExecutionFailed => {
                if let Some(message) = event
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("error"))
                    .and_then(|v| v.as_str())
                {
                    for cb in &self.on_error {
                        cb(message);
                    }
                }
            }
            _ => {}
        }
    }

    fn dispatch_progress(&self, progress: f64) {
        for cb in &self.on_progress {
            cb(progress);
        }
    }

    fn dispatch_error(&self, message: &str) {
        for cb in &self.on_error {
            cb(message);
        }
    }
}

/// Extract the node execution info from a `NodeCompleted` event payload.
fn node_info(event: &wf_types::events::BaseEvent) -> Option<NodeExecutedInfo> {
    let metadata = event.metadata.as_ref()?;
    let node_id = metadata.get("node_id")?.as_str()?.to_string();
    Some(NodeExecutedInfo {
        node_id,
        node_name: metadata
            .get("node_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        node_type: metadata
            .get("node_type")
            .and_then(|v| v.as_str())
            .map(String::from),
        duration_ms: metadata.get("duration_ms").and_then(|v| v.as_i64()),
    })
}

fn default_options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: None,
        node_timeout: None,
        max_pause_duration: None,
        retry_budget: None,
        on_failure: None,
        max_retries: None,
        retry_delay_ms: None,
        exponential_backoff: None,
        fallback_output: None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::node::BaseStaticNode;
    use wf_types::node::StaticNodeType;
    use wf_types::workflow::edge::EdgeType;
    use wf_types::workflow::WorkflowDefinition;

    fn make_workflow(id: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {}", id),
            description: None,
            r#type: None,
            version: Some("1.0.0".into()),
            nodes: vec![
                BaseStaticNode {
                    id: "start".into(),
                    node_type: StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "v1".into(),
                    node_type: StaticNodeType::Variable,
                    name: Some("v1".into()),
                    description: None,
                    config: Some(serde_json::json!({
                        "variable_name": "final",
                        "expression": "${input.greeting}",
                    })),
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "end".into(),
                    node_type: StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![
                wf_types::workflow::Edge {
                    id: "e1".into(),
                    source_node_id: "start".into(),
                    target_node_id: "v1".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
                wf_types::workflow::Edge {
                    id: "e2".into(),
                    source_node_id: "v1".into(),
                    target_node_id: "end".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
            ],
            config: None,
            variables: None,
            triggers: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn executes_workflow_and_fires_callbacks() {
        let ctx = make_ctx();
        ctx.storage
            .workflow
            .save(&make_workflow("wf-builder-exec-1"))
            .await
            .unwrap();

        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let progress = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let completed_clone = completed.clone();
        let progress_clone = progress.clone();

        let output = ExecutionBuilder::new("wf-builder-exec-1")
            .with_input(serde_json::json!({"greeting": "hi"}))
            .on_node_executed(move |_info| {
                completed_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .on_progress(move |p| {
                assert!(p > 0.0 && p <= 1.0);
                progress_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .execute(&ctx)
            .await
            .expect("workflow completes");
        assert_eq!(output.result, serde_json::json!({"greeting": "hi"}));
        assert_eq!(
            completed.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "start/v1/end must each complete"
        );
        assert_eq!(progress.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_stream_returns_execution_id() {
        let ctx = make_ctx();
        ctx.storage
            .workflow
            .save(&make_workflow("wf-builder-exec-2"))
            .await
            .unwrap();

        let (execution_id, mut stream) = ExecutionBuilder::new("wf-builder-exec-2")
            .with_input(serde_json::json!({"greeting": "stream"}))
            .execute_stream(&ctx)
            .await
            .expect("stream starts");
        assert!(!execution_id.is_empty());

        let mut saw_terminal = false;
        while let Some(event) = stream.next().await {
            if let crate::infra::stream::ExecutionStreamEvent::Completed { .. } = event {
                saw_terminal = true;
            }
        }
        assert!(saw_terminal, "stream must end with Completed");
    }

    #[tokio::test]
    async fn cancel_running_execution() {
        let ctx = make_ctx();
        ctx.storage
            .workflow
            .save(&make_workflow("wf-builder-exec-3"))
            .await
            .unwrap();

        let (execution_id, _stream) = ExecutionBuilder::new("wf-builder-exec-3")
            .with_input(serde_json::json!({"greeting": "x"}))
            .execute_stream(&ctx)
            .await
            .expect("stream starts");

        let builder = ExecutionBuilder::new("wf-builder-exec-3");
        // Cancelling a completed execution is tolerated by the entity layer;
        // the call must not error for an unknown id.
        let result = builder.cancel(&ctx, &execution_id.to_string()).await;
        assert!(result.is_ok() || matches!(result, Err(crate::ApiError::ExecutionNotFound { .. })));
    }
}
