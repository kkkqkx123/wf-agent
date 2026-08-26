use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::fork_join::find_fork_by_path;
use crate::handler::variable_mapping::set_variable_path;
use crate::handler::NodeHandler;

pub struct SyncHandler;

impl SyncHandler {
    pub fn new() -> Self {
        Self
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

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl SyncHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config: Value = ctx.node_config.clone().unwrap_or(Value::Null);

        let source_path_id = config
            .get("source_path_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&ctx.node_id)
            .to_string();
        // By default the SYNC waits for the source branch
        // to settle before reading its variables.
        let wait_for_completion = config
            .get("wait_for_completion")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let timeout = config.get("timeout").and_then(|v| v.as_u64());

        emit_sync_event(ctx.event_bus.as_deref(), EventType::NodeSyncStarted, ctx).await;

        // Locate the fork registry of the fork that launched the source
        // branch: the branch's live variables are published after every
        // completed node, so the SYNC reads the branch's current state.
        // Inside a fork branch the graph is the branch subgraph (it contains
        // no FORK node), so the registry is also matched by the fork's
        // recorded path ids.
        let registry = ctx
            .graph_structure
            .as_ref()
            .and_then(|g| find_fork_by_path(g, &source_path_id))
            .and_then(|fork_id| ctx.fork_registries.get(&fork_id))
            .cloned()
            .or_else(|| {
                ctx.fork_registries
                    .iter()
                    .find(|(_, reg)| reg.path_ids().iter().any(|p| p == &source_path_id))
                    .map(|(_, reg)| reg.clone())
            });

        // Wait for the source branch to settle (bounded by `timeout`), then
        // re-read its variables once it settles. Without a registry (e.g.
        // the fork ran in an earlier execution whose registry is gone) the
        // SYNC falls back to the recorded fork output variable.
        if wait_for_completion {
            if let Some(registry) = &registry {
                if !registry.wait_for(&source_path_id, timeout).await {
                    return Err(WorkflowError::CoordinatorError(format!(
                        "SYNC node '{}' timed out waiting for source branch '{}'",
                        ctx.node_id, source_path_id
                    )));
                }
            }
        }

        let source_variables = if let Some(registry) = &registry {
            registry.get(&source_path_id).map(|record| record.variables)
        } else {
            // Compatibility fallback: the fork hands over
            // `__fork_outputs_<fork_id>` carrying per-branch results
            // including a public-variable snapshot.
            config
                .get("variable_mappings")
                .and_then(|v| v.as_array())
                .and_then(|_| {
                    ctx.graph_structure
                        .as_ref()
                        .and_then(|g| find_fork_by_path(g, &source_path_id))
                        .and_then(|fork_id| {
                            ctx.get_variable(&format!("__fork_outputs_{}", fork_id))
                        })
                        .and_then(|output| {
                            output
                                .get("results")
                                .and_then(|v| v.as_array())
                                .and_then(|records| {
                                    records.iter().find_map(|record| {
                                        let branch_id = record
                                            .get("branch_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default();
                                        if branch_id == source_path_id {
                                            record.get("variables").and_then(|v| v.as_object()).map(
                                                |obj| {
                                                    obj.iter()
                                                        .map(|(k, v)| (k.clone(), v.clone()))
                                                        .collect()
                                                },
                                            )
                                        } else {
                                            None
                                        }
                                    })
                                })
                        })
                })
        };

        let mut synced_variables: Vec<String> = Vec::new();

        // 1. Explicit variable mappings (deep clone of the source branch's
        // exported variables, `source_path -> internal_name`).
        if let (Some(mappings), Some(source_vars)) = (
            config.get("variable_mappings").and_then(|v| v.as_array()),
            source_variables.as_ref(),
        ) {
            for mapping in mappings {
                let source_path = mapping
                    .get("source_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let internal_name = mapping
                    .get("internal_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if source_path.is_empty() || internal_name.is_empty() {
                    continue;
                }
                if let Some(value) = resolve_path_in_map(source_vars, source_path) {
                    let cloned = deep_clone(&value);
                    set_variable_path(&ctx.variables, internal_name, cloned)?;
                    synced_variables.push(internal_name.to_string());
                }
            }
        }

        // 2. Legacy `variable_exchanges` (single-level copy between the local
        // scope variables), kept for backward compatibility.
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
                    ctx.set_variable(target_variable.to_string(), resolved)?;
                    synced_variables.push(target_variable.to_string());
                }
            }
        }

        // 3. Message context sync: copy named message arrays from the source
        // branch's exported variables into the target scope.
        if let (Some(message_inputs), Some(source_vars)) = (
            config
                .get("message_inputs")
                .or_else(|| config.get("messageInputs"))
                .and_then(|v| v.as_array()),
            source_variables.as_ref(),
        ) {
            for entry in message_inputs {
                let source_context_id = entry
                    .get("source_context_id")
                    .or_else(|| entry.get("sourceContextId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let internal_name = entry
                    .get("internal_name")
                    .or_else(|| entry.get("internalName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let required = entry
                    .get("required")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if source_context_id.is_empty() || internal_name.is_empty() {
                    continue;
                }
                let key = format!(
                    "{}{}",
                    crate::message_context::CONTEXT_PREFIX,
                    source_context_id
                );
                if let Some(value) = source_vars.get(&key) {
                    if let Ok(messages) =
                        serde_json::from_value::<Vec<wf_types::message::Message>>(value.clone())
                    {
                        crate::message_context::register_context(
                            &ctx.variables,
                            internal_name,
                            messages,
                        );
                    }
                } else if required {
                    return Err(WorkflowError::ForkJoinError(format!(
                        "SYNC node '{}': required message context '{}' not found in source branch '{}'",
                        ctx.node_id, source_context_id, source_path_id
                    )));
                }
            }
        }

        // 4. Data input mapping: `data_inputs` (`parent_field ->
        // internal_name`) copies values from the workflow input object into
        // variables.
        if let Some(data_inputs) = config
            .get("data_inputs")
            .or_else(|| config.get("dataInputs"))
            .and_then(|v| v.as_array())
        {
            let input_obj = ctx.input.as_object().cloned().unwrap_or_default();
            for entry in data_inputs {
                let parent_field = entry
                    .get("parent_field")
                    .or_else(|| entry.get("parentField"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let internal_name = entry
                    .get("internal_name")
                    .or_else(|| entry.get("internalName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let required = entry
                    .get("required")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if parent_field.is_empty() || internal_name.is_empty() {
                    continue;
                }
                if let Some(value) = input_obj.get(parent_field) {
                    ctx.set_variable(internal_name.to_string(), value.clone())?;
                    synced_variables.push(internal_name.to_string());
                } else if required {
                    return Err(WorkflowError::VariableError(format!(
                        "SYNC node '{}': required data input '{}' (mapped to '{}') is missing",
                        ctx.node_id, parent_field, internal_name
                    )));
                }
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

/// Deep-clone a JSON value (JSON values are already owned trees; this is a
/// defensive no-op that documents the deep-clone isolation intent).
fn deep_clone(value: &Value) -> Value {
    value.clone()
}

/// Resolve a dotted path (`a.b.c`) within a map of exported variables.
fn resolve_path_in_map(
    map: &std::collections::HashMap<String, Value>,
    path: &str,
) -> Option<Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let first = *parts.first()?;
    let mut current = map.get(first)?.clone();
    for part in &parts[1..] {
        let Value::Object(obj) = current else {
            return None;
        };
        current = obj.get(*part)?.clone();
    }
    Some(current)
}

async fn emit_sync_event(
    event_bus: Option<&wf_core::EventBus>,
    event_type: EventType,
    ctx: &NodeExecutionContext,
) {
    let Some(bus) = event_bus else {
        tracing::debug!(execution_id = %ctx.execution_id, node_id = %ctx.node_id, ?event_type, "no event bus, skipping sync event");
        return;
    };
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(ctx.execution_id.clone()),
        agent_loop_id: None,

        event_name: None,
        metadata: Some(std::collections::HashMap::from([(
            "node_id".to_string(),
            Value::String(ctx.node_id.clone()),
        )])),
    };
    bus.publish_logged(
        event,
        &format!("workflow={} sync={}", ctx.execution_id, ctx.node_id),
    )
    .ok();
}
