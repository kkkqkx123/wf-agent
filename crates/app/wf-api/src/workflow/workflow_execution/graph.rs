use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};
use wf_workflow::validation::GraphValidator;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError};

/// Application-facing workflow graph resolution.
///
/// Load a stored workflow definition and convert it into an executable
/// graph, running shape plus graph plus reference closure validation.
/// Stored workflows that went stale after an upstream change fail here
/// explicitly instead of failing piecemeal at node runtime.
pub async fn resolve_graph(
    ctx: &ApiContext,
    workflow_id: &str,
) -> crate::infra::error::ApiResult<WorkflowGraphStructure> {
    if ctx.is_stale(workflow_id) {
        tracing::warn!(
            workflow_id = %workflow_id,
            "workflow is marked stale after an upstream update; revalidation runs before execution"
        );
    }
    let mut definition = ctx
        .storage
        .workflow
        .load(workflow_id)
        .await?
        .ok_or_else(|| not_found("workflow", workflow_id))?;
    crate::template::composition::apply_templates_to_definition(&mut definition, &ctx.registries);
    let graph = definition_to_graph(&definition);
    let val_ctx = crate::workflow::validation::build_reference_context(ctx).await;
    let ref_ctx = crate::workflow::validation::val_ctx_to_reference_context(&val_ctx);
    let (validated, warnings) = GraphValidator::validate_with_reference_context(graph, &ref_ctx)
        .map_err(|errors| {
            let detail = errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            ApiError::Validation(format!(
                "workflow reference closure failed ({} error(s)): {}",
                errors.len(),
                detail
            ))
        })?;
    if !warnings.is_empty() {
        tracing::warn!(
            workflow_id = %workflow_id,
            warnings = %warnings.iter().map(|w| format!("{}: {}", w.field, w.message)).collect::<Vec<_>>().join("; "),
            "workflow executes with reference warnings"
        );
    }
    let tool_report = wf_workflow::reference_closure::validate_workflow_tool_lists(
        workflow_id,
        definition.available_tools.as_ref(),
        &ref_ctx,
    );
    if !tool_report.errors.is_empty() {
        let detail = tool_report
            .errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ApiError::Validation(format!(
            "workflow reference closure failed ({} error(s)): {}",
            tool_report.errors.len(),
            detail
        )));
    }
    Ok(validated.into_inner())
}

/// Convert a stored [`WorkflowDefinition`](wf_types::workflow::WorkflowDefinition) into an executable graph.
///
/// Nodes map their `config` onto the flattened `inner` field consumed by the
/// node handlers; edges map directly. Graph boundaries are derived from node
/// types (the START / END nodes, or the message pair for triggered
/// subgraphs): node order carries no semantics. A definition without a
/// boundary node yields an empty boundary and fails graph validation.
pub fn definition_to_graph(
    definition: &wf_types::workflow::WorkflowDefinition,
) -> WorkflowGraphStructure {
    use wf_types::node::StaticNodeType;
    let nodes: Vec<WorkflowNode> = definition
        .nodes
        .iter()
        .map(|node| WorkflowNode {
            id: node.id.clone(),
            name: node.name.clone(),
            node_type: node_type_string(&node.node_type),
            inner: node_inner_config(node),
        })
        .collect();
    let edges: Vec<WorkflowEdge> = definition
        .edges
        .iter()
        .map(|edge| WorkflowEdge {
            id: edge.id.clone(),
            source_node_id: edge.source_node_id.clone(),
            target_node_id: edge.target_node_id.clone(),
            r#type: edge.r#type.clone(),
            condition: edge.condition.clone(),
            label: edge.label.clone(),
            description: edge.description.clone(),
            error_route: edge.error_route.clone(),
        })
        .collect();
    WorkflowGraphStructure {
        start_node_id: definition
            .nodes
            .iter()
            .find(|n| n.node_type == StaticNodeType::Start)
            .or_else(|| {
                definition
                    .nodes
                    .iter()
                    .find(|n| n.node_type == StaticNodeType::StartFromMessage)
            })
            .map(|n| n.id.clone()),
        end_node_ids: {
            let ends: Vec<String> = definition
                .nodes
                .iter()
                .filter(|n| n.node_type == StaticNodeType::End)
                .map(|n| n.id.clone())
                .collect();
            if ends.is_empty() {
                definition
                    .nodes
                    .iter()
                    .filter(|n| n.node_type == StaticNodeType::ContinueFromMessage)
                    .map(|n| n.id.clone())
                    .collect()
            } else {
                ends
            }
        },
        nodes,
        edges,
        error_default: definition
            .config
            .as_ref()
            .and_then(|config| config.error_default.clone()),
    }
}

fn node_type_string(node_type: &wf_types::node::StaticNodeType) -> String {
    node_type.canonical_name().to_string()
}

/// Node config plus execution config merged into a single JSON object
/// (execution config wins on conflicts). The runtime reads node-level
/// behavior (`checkpoint`, `timeout_seconds`, retry policy, ...) from the
/// node's `inner` blob, so the typed `execution_config` surface is merged
/// here instead of being dropped at graph conversion.
fn node_inner_config(node: &wf_types::node::BaseStaticNode) -> Value {
    match (&node.config, &node.execution_config) {
        (Some(config), Some(execution_config)) => {
            let mut map = match config {
                Value::Object(m) => m.clone(),
                _ => serde_json::Map::new(),
            };
            map.extend(execution_config.config_fields());
            Value::Object(map)
        }
        (Some(config), None) => config.clone(),
        (None, Some(execution_config)) => Value::Object(execution_config.config_fields()),
        (None, None) => Value::Null,
    }
}
