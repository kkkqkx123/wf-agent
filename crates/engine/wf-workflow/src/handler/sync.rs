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
                    // Typed transport timeout: the wait was bounded and
                    // expired, so error-branch routing sees a timeout rather
                    // than a generic coordinator failure.
                    return Err(WorkflowError::NodeFailure {
                        node_id: ctx.node_id.clone(),
                        category:
                            wf_types::workflow::error_branch::NodeErrorCategory::TransportTimeout,
                        detail: format!(
                            "SYNC node '{}' timed out waiting for source branch '{}'",
                            ctx.node_id, source_path_id
                        ),
                        failure_source:
                            wf_types::workflow::error_branch::NodeFailureSource::Handler,
                    });
                }
            }
        }

        // Source branch state for variable and message sync. The live
        // registry is preferred; without it (e.g. the fork ran in an
        // earlier execution whose registry is gone) the SYNC falls back to
        // the recorded fork output variable. The fallback is intentionally
        // decoupled from `variable_mappings`: a message-only SYNC must also
        // reach the recorded branch results.
        struct BranchSource {
            variables: std::collections::HashMap<String, Value>,
            message_contexts: std::collections::HashMap<String, Value>,
            message_histories: std::collections::HashMap<String, Value>,
        }
        let source: Option<BranchSource> = if let Some(registry) = &registry {
            registry.get(&source_path_id).map(|record| BranchSource {
                variables: record.variables,
                message_contexts: record.message_contexts,
                message_histories: record.message_histories,
            })
        } else {
            // Compatibility fallback: the fork hands over
            // `__fork_outputs_<fork_id>` (see
            // `crate::handler::fork_join::fork_output_key`) carrying
            // per-branch results including the explicit message export.
            ctx.graph_structure
                .as_ref()
                .and_then(|g| find_fork_by_path(g, &source_path_id))
                .and_then(|fork_id| {
                    ctx.get_variable(&crate::handler::fork_join::fork_output_key(&fork_id))
                })
                .and_then(|output| {
                    let (records, _) =
                        crate::handler::fork_join::merge::collect_branch_records(&output);
                    records.into_iter().find_map(|record| {
                        if record.branch_id == source_path_id {
                            Some(BranchSource {
                                variables: record.variables.unwrap_or_default(),
                                message_contexts: record.message_contexts.unwrap_or_default(),
                                message_histories: record.message_histories.unwrap_or_default(),
                            })
                        } else {
                            None
                        }
                    })
                })
        };
        let source_variables = source.as_ref().map(|s| &s.variables);

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
                    let system = ctx.system_context();
                    let resolved = crate::variable::VariableResolver::resolve(
                        &val,
                        &ctx.variables,
                        Some(&system),
                    );
                    ctx.set_variable(target_variable.to_string(), resolved)?;
                    synced_variables.push(target_variable.to_string());
                }
            }
        }

        // 3. Message context sync: copy named message arrays from the source
        // branch's explicit message export into the target scope (copy, not
        // append: the source active view becomes the target active view and
        // the source history merges into the target history). Falls back to
        // the legacy `variables["__msg_ctx__*"]` lookup for hand-built and
        // old snapshot records.
        if let (Some(message_inputs), Some(branch)) = (
            config.get("message_inputs").and_then(|v| v.as_array()),
            source.as_ref(),
        ) {
            for entry in message_inputs {
                let source_context_id = entry
                    .get("source_context_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let internal_name = entry
                    .get("internal_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let required = entry
                    .get("required")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if source_context_id.is_empty() || internal_name.is_empty() {
                    continue;
                }
                let value = branch.message_contexts.get(source_context_id).or_else(|| {
                    branch.variables.get(&format!(
                        "{}{}",
                        crate::message_context::CONTEXT_PREFIX,
                        source_context_id
                    ))
                });
                let history = branch.message_histories.get(source_context_id).and_then(
                    |history_value| match serde_json::from_value::<Vec<wf_types::message::Message>>(
                        history_value.clone(),
                    ) {
                        Ok(history) => Some(history),
                        Err(e) => {
                            tracing::warn!(
                                node_id = %ctx.node_id,
                                context_id = %source_context_id,
                                error = %e,
                                "SYNC message history failed to parse; skipping history merge"
                            );
                            crate::degradation::emit_data_degradation(
                                ctx.event_bus.as_deref(),
                                None,
                                &ctx.execution_id,
                                "sync_message_histories",
                                &format!(
                                    "message history '{}' failed to parse: {}",
                                    source_context_id, e
                                ),
                            );
                            None
                        }
                    },
                );
                let Some(value) = value else {
                    // No active view: still merge a valid history so archived
                    // content is not lost, then enforce the required gate.
                    if let Some(history) = history {
                        crate::message_context::merge_archived_history(
                            &ctx.variables,
                            internal_name,
                            history,
                        );
                    }
                    if required {
                        return Err(WorkflowError::ForkJoinError(format!(
                            "SYNC node '{}': required message context '{}' not found in source branch '{}'",
                            ctx.node_id, source_context_id, source_path_id
                        )));
                    }
                    continue;
                };
                match serde_json::from_value::<Vec<wf_types::message::Message>>(value.clone()) {
                    Ok(messages) => {
                        // Register first so the target's previous active view
                        // is archived before the source history merges in.
                        crate::message_context::register_context(
                            &ctx.variables,
                            internal_name,
                            messages,
                        );
                        if let Some(history) = history {
                            crate::message_context::merge_archived_history(
                                &ctx.variables,
                                internal_name,
                                history,
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            node_id = %ctx.node_id,
                            context_id = %source_context_id,
                            error = %e,
                            "SYNC message context failed to parse; registering nothing"
                        );
                        crate::degradation::emit_data_degradation(
                            ctx.event_bus.as_deref(),
                            None,
                            &ctx.execution_id,
                            "sync_message_inputs",
                            &format!(
                                "message context '{}' failed to parse: {}",
                                source_context_id, e
                            ),
                        );
                        // The active view is unreadable, but a valid history
                        // is still worth preserving.
                        if let Some(history) = history {
                            crate::message_context::merge_archived_history(
                                &ctx.variables,
                                internal_name,
                                history,
                            );
                        }
                    }
                }
            }
        }

        // 4. Data input mapping: `data_inputs` (`parent_field ->
        // internal_name`) copies values from the workflow input object into
        // variables.
        if let Some(data_inputs) = config.get("data_inputs").and_then(|v| v.as_array()) {
            let input_obj = ctx.input.as_object().cloned().unwrap_or_default();
            for entry in data_inputs {
                let parent_field = entry
                    .get("parent_field")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let internal_name = entry
                    .get("internal_name")
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
        {
            use wf_execution_shared::context::node_metadata as keys;
            metadata.insert(
                keys::SOURCE_PATH_ID.to_string(),
                Value::String(source_path_id),
            );
            if !synced_variables.is_empty() {
                metadata.insert(
                    keys::SYNCED_VARIABLES.to_string(),
                    Value::Array(
                        synced_variables
                            .iter()
                            .map(|s| Value::String(s.clone()))
                            .collect(),
                    ),
                );
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use dashmap::DashMap;
    use wf_types::node::StaticNodeType;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

    fn graph_with_fork(fork_id: &str, path_id: &str) -> Arc<WorkflowGraphStructure> {
        Arc::new(WorkflowGraphStructure {
            nodes: vec![WorkflowNode {
                id: fork_id.to_string(),
                name: None,
                node_type: "FORK".to_string(),
                inner: serde_json::json!({
                    "fork_paths": [{"path_id": path_id, "child_node_id": "a"}]
                }),
            }],
            edges: vec![WorkflowEdge {
                id: "e1".to_string(),
                source_node_id: fork_id.to_string(),
                target_node_id: "a".to_string(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                error_route: None,
            }],
            start_node_id: Some(fork_id.to_string()),
            end_node_ids: vec!["a".to_string()],
            error_default: None,
        })
    }

    fn ctx_with_fallback(
        vars: Arc<DashMap<String, Value>>,
        graph: Arc<WorkflowGraphStructure>,
        config: Value,
    ) -> NodeExecutionContext {
        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "sync1".to_string(),
            StaticNodeType::Sync,
            Value::Null,
            vars,
        );
        ctx.graph_structure = Some(graph);
        ctx.node_config = Some(config);
        ctx
    }

    #[tokio::test]
    async fn corrupted_message_context_degrades_without_failing() {
        let vars = Arc::new(DashMap::new());
        // Checkpoint-resume fallback: no live registry, the fork output
        // variable carries a branch record whose message array is corrupted.
        vars.insert(
            crate::handler::fork_join::fork_output_key("fork1"),
            serde_json::json!({
                "results": [{
                    "branch_id": "p1",
                    "output": {},
                    "success": true,
                    "variables": {"__msg_ctx__chat": "not-a-message-array"}
                }],
                "outputs": [],
                "outcome": "Succeeded"
            }),
        );
        let mut ctx = ctx_with_fallback(
            vars.clone(),
            graph_with_fork("fork1", "p1"),
            serde_json::json!({
                "source_path_id": "p1",
                "wait_for_completion": false,
                "variable_mappings": [],
                "message_inputs": [{
                    "source_context_id": "chat",
                    "internal_name": "synced",
                    "required": false
                }]
            }),
        );
        SyncHandler
            .execute(&mut ctx)
            .await
            .expect("corrupted message context must degrade, not fail");
        assert!(
            crate::message_context::get_context(&vars, "synced").is_empty(),
            "nothing is registered from an unparseable array"
        );
    }

    #[tokio::test]
    async fn message_only_fallback_needs_no_variable_mappings() {
        let vars = Arc::new(DashMap::new());
        // Explicit message export in the recorded fork output; no
        // `variable_mappings` configured at all (the old gate would have
        // returned an empty source and silently skipped the sync).
        vars.insert(
            crate::handler::fork_join::fork_output_key("fork1"),
            serde_json::json!({
                "results": [{
                    "branch_id": "p1",
                    "output": {},
                    "success": true,
                    "variables": {"value": 1},
                    "message_contexts": {"chat": [{
                        "id": "m1", "role": "assistant",
                        "content": "from branch", "timestamp": 0
                    }]},
                    "message_histories": {"chat": [{
                        "id": "m0", "role": "user",
                        "content": "archived", "timestamp": 0
                    }]}
                }],
                "outputs": [],
                "outcome": "Succeeded"
            }),
        );
        let mut ctx = ctx_with_fallback(
            vars.clone(),
            graph_with_fork("fork1", "p1"),
            serde_json::json!({
                "source_path_id": "p1",
                "wait_for_completion": false,
                "message_inputs": [{
                    "source_context_id": "chat",
                    "internal_name": "synced",
                    "required": true
                }]
            }),
        );
        SyncHandler
            .execute(&mut ctx)
            .await
            .expect("message-only fallback must copy without variable_mappings");
        let active = crate::message_context::get_context(&vars, "synced");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "m1");
        let archived = crate::message_context::archived_history(&vars, "synced");
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, "m0");
    }

    #[tokio::test]
    async fn missing_required_message_context_fails() {
        let vars = Arc::new(DashMap::new());
        vars.insert(
            crate::handler::fork_join::fork_output_key("fork1"),
            serde_json::json!({
                "results": [{
                    "branch_id": "p1",
                    "output": {},
                    "success": true,
                    "variables": {}
                }],
                "outputs": [],
                "outcome": "Succeeded"
            }),
        );
        let mut ctx = ctx_with_fallback(
            vars,
            graph_with_fork("fork1", "p1"),
            serde_json::json!({
                "source_path_id": "p1",
                "wait_for_completion": false,
                "variable_mappings": [],
                "message_inputs": [{
                    "source_context_id": "chat",
                    "internal_name": "synced",
                    "required": true
                }]
            }),
        );
        let err = match SyncHandler.execute(&mut ctx).await {
            Ok(_) => panic!("missing required context must fail"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("required message context"),
            "{err:?}"
        );
    }
}
