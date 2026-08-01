use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;

use crate::barrier::SyncBarrier;
use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

pub struct SyncHandler {
    barriers: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Arc<SyncBarrier>>>>,
}

impl SyncHandler {
    pub fn new() -> Self {
        Self {
            barriers: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub async fn register_barrier(&self, sync_id: &str, count: usize) -> Arc<SyncBarrier> {
        let mut map = self.barriers.write().await;
        let barrier = Arc::new(SyncBarrier::new(count));
        map.insert(sync_id.to_string(), barrier.clone());
        barrier
    }

    pub async fn get_barrier(&self, sync_id: &str) -> Option<Arc<SyncBarrier>> {
        let map = self.barriers.read().await;
        map.get(sync_id).cloned()
    }
}

impl Default for SyncHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NodeHandler for SyncHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Sync
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let source_path_id = config
            .get("source_path_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&ctx.node_id)
            .to_string();
        let wait_for_completion = config
            .get("wait_for_completion")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut synced_variables: Vec<String> = Vec::new();
        if let Some(exchanges) = config.get("variable_exchanges").and_then(|v| v.as_array()) {
            for exchange in exchanges {
                let source_variable = exchange.get("source_variable").and_then(|v| v.as_str());
                let target_variable = exchange.get("target_variable").and_then(|v| v.as_str());
                let (Some(source_variable), Some(target_variable)) =
                    (source_variable, target_variable)
                else {
                    continue;
                };
                if let Some(val) = ctx.get_variable(source_variable) {
                    let resolved = crate::variable::VariableResolver::resolve(&val, &ctx.variables);
                    ctx.set_variable(target_variable.to_string(), resolved);
                    synced_variables.push(target_variable.to_string());
                }
            }
        }

        emit_sync_event(ctx.event_bus.as_deref(), EventType::NodeSyncStarted, ctx).await;

        if wait_for_completion {
            if let Some(barrier) = self.get_barrier(&source_path_id).await {
                barrier.wait_for_all().await;
            }
        }

        emit_sync_event(ctx.event_bus.as_deref(), EventType::NodeSyncCompleted, ctx).await;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("source_path_id".to_string(), Value::String(source_path_id));
        if !synced_variables.is_empty() {
            metadata.insert(
                "synced_variables".to_string(),
                Value::Array(
                    synced_variables
                        .iter()
                        .map(|s| Value::String(s.clone()))
                        .collect(),
                ),
            );
        }

        Ok(NodeExecutionResult {
            output: ctx.input.clone(),
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

async fn emit_sync_event(
    event_bus: Option<&wf_core::EventBus>,
    event_type: EventType,
    ctx: &NodeExecutionContext,
) {
    let Some(bus) = event_bus else { return };
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(ctx.execution_id.clone()),
        agent_loop_id: None,
        metadata: Some(std::collections::HashMap::from([(
            "node_id".to_string(),
            Value::String(ctx.node_id.clone()),
        )])),
    };
    let _ = bus.publish(event);
}
