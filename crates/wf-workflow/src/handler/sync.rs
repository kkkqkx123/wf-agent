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

        let sync_id = config.get("sync_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&ctx.node_id)
            .to_string();

        let source_paths = config.get("source_paths")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let data_inputs = config.get("dataInputs")
            .or_else(|| config.get("data_inputs"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().filter_map(|v| {
                    let source = v.get("source").and_then(|s| s.as_str())?;
                    let target = v.get("target").and_then(|t| t.as_str())?;
                    Some((source.to_string(), target.to_string()))
                }).collect::<Vec<_>>()
            })
            .unwrap_or_default();

        emit_sync_event(ctx.event_bus.as_deref(), EventType::NodeSyncStarted, ctx).await;

        for (source_var, target_var) in &data_inputs {
            if let Some(val) = ctx.get_variable(source_var) {
                let resolved = crate::variable::VariableResolver::resolve(&val, &ctx.variables);
                ctx.set_variable(target_var.clone(), resolved);
            }
        }

        if !source_paths.is_empty() {
            if let Some(barrier) = self.get_barrier(&sync_id).await {
                barrier.wait_for_all().await;
            }
        }

        emit_sync_event(ctx.event_bus.as_deref(), EventType::NodeSyncCompleted, ctx).await;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("sync_id".to_string(), Value::String(sync_id));
        metadata.insert("source_paths".to_string(), Value::Array(
            source_paths.iter().map(|s| Value::String(s.clone())).collect()
        ));

        Ok(NodeExecutionResult {
            output: ctx.input.clone(),
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

async fn emit_sync_event(event_bus: Option<&wf_core::EventBus>, event_type: EventType, ctx: &NodeExecutionContext) {
    let Some(bus) = event_bus else { return };
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(ctx.execution_id.clone()),
        agent_loop_id: None,
        metadata: Some(std::collections::HashMap::from([
            ("node_id".to_string(), Value::String(ctx.node_id.clone())),
        ])),
    };
    let _ = bus.publish(event);
}
