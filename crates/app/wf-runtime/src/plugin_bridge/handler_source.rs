use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_types::enums::MiddlewarePhase;

use wf_api::infra::handler_chain::{
    PluginHandlerSource, PluginMiddlewareBridge, PluginNodeExecutor,
};
use wf_plugin::ContributionManager;

pub struct WfPluginHandlerSource {
    manager: Arc<ContributionManager>,
}

impl WfPluginHandlerSource {
    pub fn new(manager: Arc<ContributionManager>) -> Self {
        Self { manager }
    }
}

impl PluginHandlerSource for WfPluginHandlerSource {
    fn node_executor(&self, type_name: &str) -> Option<Arc<dyn PluginNodeExecutor>> {
        self.manager
            .get_node_handler(type_name)
            .map(|handler| Arc::new(WfPluginNodeExecutor(handler)) as Arc<dyn PluginNodeExecutor>)
    }

    fn plugin_node_types(&self) -> Vec<String> {
        self.manager
            .all_node_types()
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    fn llm_provider_names(&self) -> Vec<String> {
        self.manager
            .all_llm_providers()
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    fn event_handler_event_types(&self) -> Vec<String> {
        self.manager
            .all_event_handlers()
            .into_iter()
            .map(|(event_type, _)| event_type)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn middleware(&self, phase: &MiddlewarePhase) -> Vec<Arc<dyn PluginMiddlewareBridge>> {
        if self.manager.get_middleware(phase).is_empty() {
            return Vec::new();
        }
        vec![Arc::new(WfPluginMiddlewareRunner(self.manager.clone()))
            as Arc<dyn PluginMiddlewareBridge>]
    }

    fn cli_snapshot(&self) -> wf_types::cli::CliSnapshot {
        super::cli_snapshot::build_cli_snapshot(&self.manager)
    }
}

struct WfPluginNodeExecutor(Arc<dyn wf_plugin::PluginNodeHandler>);

#[async_trait]
impl PluginNodeExecutor for WfPluginNodeExecutor {
    async fn execute(
        &self,
        node_id: &str,
        inputs: &Value,
        config: &Value,
    ) -> wf_api::ApiResult<Value> {
        let ctx = wf_plugin::PluginExecutionContext {
            node_id: node_id.to_string(),
            inputs: inputs.clone(),
            config: config.clone(),
        };
        let result = self
            .0
            .execute(ctx)
            .await
            .map_err(wf_api::ApiError::execution_with_source)?;
        Ok(result.outputs)
    }
}

struct WfPluginMiddlewareRunner(Arc<ContributionManager>);

#[async_trait]
impl PluginMiddlewareBridge for WfPluginMiddlewareRunner {
    async fn handle(&self, phase: &MiddlewarePhase, context: &Value) -> wf_api::ApiResult<()> {
        let _ = self
            .0
            .run_middleware(phase, context.clone())
            .await
            .map_err(wf_api::ApiError::execution_with_source)?;
        Ok(())
    }
}
