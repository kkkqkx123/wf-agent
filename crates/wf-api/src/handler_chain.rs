use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::context::NodeExecutionResult as NodeHandlerResult;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::WorkflowExecutionOptions;
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::handler::NodeHandler;
use wf_workflow::WorkflowCoordinator;

use crate::error::ApiResult;

/// Plugin-agnostic node executor. Implemented by `wf-runtime` over
/// `wf_plugin::PluginNodeHandler`; keeping the trait here lets `wf-api`
/// consume plugin contributions without depending on `wf-plugin`.
#[async_trait]
pub trait PluginNodeExecutor: Send + Sync {
    /// Execute a plugin node. `node_id` / `inputs` / `config` mirror the
    /// fields of the engine's node execution context.
    async fn execute(&self, node_id: &str, inputs: &Value, config: &Value) -> ApiResult<Value>;
}

/// Plugin hook bridge (agent/workflow hook contributions).
#[async_trait]
pub trait PluginHookBridge: Send + Sync {
    async fn handle(&self, hook_type: &str, context: &Value) -> ApiResult<()>;
}

/// Plugin middleware bridge (lifecycle middleware contributions).
#[async_trait]
pub trait PluginMiddlewareBridge: Send + Sync {
    async fn handle(&self, phase: &str, context: &Value) -> ApiResult<()>;
}

/// Contribution source injected by `wf-runtime`. The handler resolution chain
/// in [`crate::context::ApiContext`] reads plugin contributions through this
/// trait (builtin → plugin → template fallback) without a `wf-plugin`
/// dependency.
pub trait PluginHandlerSource: Send + Sync {
    /// Look up a plugin node executor registered under `type_name`.
    fn node_executor(&self, type_name: &str) -> Option<Arc<dyn PluginNodeExecutor>>;

    /// Plugin hook handlers registered for `hook_type`.
    fn hook_handlers(&self, hook_type: &str) -> Vec<Arc<dyn PluginHookBridge>>;

    /// Plugin middleware handlers registered for `phase`.
    fn middleware(&self, phase: &str) -> Vec<Arc<dyn PluginMiddlewareBridge>>;
}

/// No-op contribution source; the default when no plugin engine is wired.
pub struct NoopPluginHandlerSource;

impl PluginHandlerSource for NoopPluginHandlerSource {
    fn node_executor(&self, _type_name: &str) -> Option<Arc<dyn PluginNodeExecutor>> {
        None
    }

    fn hook_handlers(&self, _hook_type: &str) -> Vec<Arc<dyn PluginHookBridge>> {
        Vec::new()
    }

    fn middleware(&self, _phase: &str) -> Vec<Arc<dyn PluginMiddlewareBridge>> {
        Vec::new()
    }
}

/// Canonical SCREAMING_SNAKE_CASE name of a static node type (mirrors the
/// serde representation used by the workflow graph and plugin registries).
pub fn node_type_name(node_type: &StaticNodeType) -> String {
    serde_json::to_value(node_type)
        .ok()
        .and_then(|v| v.as_str().map(ToOwned::to_owned))
        .unwrap_or_default()
}

/// Adapter turning a [`PluginNodeExecutor`] into an engine [`NodeHandler`].
pub struct PluginNodeAdapter {
    executor: Arc<dyn PluginNodeExecutor>,
    node_type: StaticNodeType,
}

impl PluginNodeAdapter {
    pub fn new(executor: Arc<dyn PluginNodeExecutor>, node_type: StaticNodeType) -> Self {
        Self {
            executor,
            node_type,
        }
    }
}

#[async_trait]
impl NodeHandler for PluginNodeAdapter {
    fn node_type(&self) -> StaticNodeType {
        self.node_type.clone()
    }

    async fn execute(
        &self,
        ctx: &mut wf_execution_shared::context::NodeExecutionContext,
    ) -> WorkflowResult<NodeHandlerResult> {
        let inputs = ctx.input.clone();
        let config = ctx.node_config.clone().unwrap_or(Value::Null);
        match self
            .executor
            .execute(&ctx.node_id, &inputs, &config)
            .await
        {
            Ok(output) => Ok(NodeHandlerResult {
                output,
                next_node_ids: Vec::new(),
                metadata: HashMap::new(),
            }),
            Err(err) => Err(WorkflowError::NodeExecutionFailed {
                node_id: ctx.node_id.clone(),
                reason: format!("plugin node error: {err}"),
            }),
        }
    }
}

/// Template-fallback handler: executes a workflow registered under a stored
/// node template id as a subgraph. Serves the third tier of the resolution
/// chain — a node whose type has no builtin handler can be backed by a stored
/// node template.
pub struct TemplateSubgraphHandler {
    workflow_id: String,
    graph: wf_types::workflow_execution::WorkflowGraphStructure,
    handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
}

impl TemplateSubgraphHandler {
    pub fn new(
        workflow_id: String,
        graph: wf_types::workflow_execution::WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    ) -> Self {
        Self {
            workflow_id,
            graph,
            handlers,
        }
    }
}

#[async_trait]
impl NodeHandler for TemplateSubgraphHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Subgraph
    }

    async fn execute(
        &self,
        ctx: &mut wf_execution_shared::context::NodeExecutionContext,
    ) -> WorkflowResult<NodeHandlerResult> {
        let options = WorkflowExecutionOptions {            input: Some(ctx.input.clone()),
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
        };

        let event_bus = ctx.event_bus.clone();
        let tool_registry = ctx
            .tool_registry
            .clone()
            .unwrap_or_else(|| Arc::new(wf_tools::registry::ToolRegistry::new()));

        let mut exec_ctx = ExecutorContext::new(
            wf_types::Id::new(),
            wf_types::Id::from(self.workflow_id.clone()),
            event_bus,
            tool_registry,
            options,
        )
        .with_parent_execution(ctx.execution_id.clone());
        if let Some(metrics) = &ctx.metrics {
            exec_ctx = exec_ctx.with_metrics(metrics.clone());
        }

        let mut coordinator =
            WorkflowCoordinator::new(exec_ctx, self.graph.clone(), self.handlers.clone())?;
        let output = coordinator.execute().await?;

        Ok(NodeHandlerResult {
            output,
            next_node_ids: Vec::new(),
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_execution_shared::context::NodeExecutionContext;

    struct EchoExecutor(Value);

    #[async_trait]
    impl PluginNodeExecutor for EchoExecutor {
        async fn execute(&self, _node_id: &str, inputs: &Value, _config: &Value) -> ApiResult<Value> {
            Ok(if self.0 == Value::Null {
                inputs.clone()
            } else {
                self.0.clone()
            })
        }
    }

    fn make_context(node_type: StaticNodeType, input: Value) -> NodeExecutionContext {
        NodeExecutionContext::new(
            wf_types::Id::new(),
            "n-plugin".into(),
            node_type,
            input,
            Arc::new(dashmap::DashMap::new()),
        )
    }

    #[tokio::test]
    async fn plugin_adapter_forwards_to_executor() {
        let adapter = PluginNodeAdapter::new(Arc::new(EchoExecutor(Value::Null)), StaticNodeType::Llm);
        assert_eq!(adapter.node_type(), StaticNodeType::Llm);

        let mut ctx = make_context(StaticNodeType::Llm, serde_json::json!({"x": 1}));
        let result = adapter.execute(&mut ctx).await.unwrap();
        assert_eq!(result.output, serde_json::json!({"x": 1}));
    }

    #[tokio::test]
    async fn plugin_adapter_maps_failure_to_workflow_error() {
        struct Failing;
        #[async_trait]
        impl PluginNodeExecutor for Failing {
            async fn execute(&self, _n: &str, _i: &Value, _c: &Value) -> ApiResult<Value> {
                Err(crate::ApiError::Execution("boom".into()))
            }
        }
        let adapter = PluginNodeAdapter::new(Arc::new(Failing), StaticNodeType::Script);
        let mut ctx = make_context(StaticNodeType::Script, Value::Null);
        let err = match adapter.execute(&mut ctx).await {
            Ok(_) => panic!("failing executor must error"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("boom"));
    }

    #[test]
    fn node_type_name_uses_screaming_snake_case() {
        assert_eq!(node_type_name(&StaticNodeType::Llm), "LLM");
        assert_eq!(node_type_name(&StaticNodeType::AgentLoop), "AGENT_LOOP");
    }
}
