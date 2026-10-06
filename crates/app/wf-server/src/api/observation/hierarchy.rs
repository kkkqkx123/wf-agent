//! Execution hierarchy domain: where one execution sits in the parent/child
//! tree of nested workflow and agent runs. Sibling of `history`, which
//! answers what a single execution recorded rather than how it relates to
//! others.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use utoipa::ToSchema;

use wf_api::execution_hierarchy;
use wf_types::execution::ExecutionType;
use wf_types::ExecutionStatus;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/executions/{id}/hierarchy", get(handle_hierarchy))
        .route("/executions/{id}/subtree", get(handle_subtree))
}

/// One execution as referenced from a hierarchy view: the id and the engine
/// that owns it.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ExecutionRefDoc {
    execution_id: String,
    #[schema(value_type = String)]
    execution_type: ExecutionType,
}

impl From<execution_hierarchy::ExecutionRef> for ExecutionRefDoc {
    fn from(r: execution_hierarchy::ExecutionRef) -> Self {
        Self {
            execution_id: r.execution_id,
            execution_type: r.execution_type,
        }
    }
}

/// Where one execution sits in the parent/child tree of nested runs.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct HierarchyDoc {
    execution_id: String,
    #[schema(value_type = String)]
    execution_type: ExecutionType,
    #[schema(value_type = String)]
    status: ExecutionStatus,
    /// Nesting level below the root; a root execution is `0`.
    depth: u32,
    /// Absent for a root execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<ExecutionRefDoc>,
    root: ExecutionRefDoc,
    /// Root-to-parent id chain, oldest first, excluding this execution.
    ancestors: Vec<String>,
}

impl From<execution_hierarchy::ExecutionHierarchyView> for HierarchyDoc {
    fn from(view: execution_hierarchy::ExecutionHierarchyView) -> Self {
        Self {
            execution_id: view.execution_id,
            execution_type: view.execution_type,
            status: view.status,
            depth: view.depth,
            parent: view.parent.map(ExecutionRefDoc::from),
            root: ExecutionRefDoc::from(view.root),
            ancestors: view.ancestors,
        }
    }
}

/// One node of a subtree listing.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SubtreeNodeDoc {
    execution_id: String,
    #[schema(value_type = String)]
    execution_type: ExecutionType,
    /// Absent only for a live execution whose record has not been written yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    status: Option<ExecutionStatus>,
    /// Nesting level relative to the queried root, which is `0`.
    depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_execution_id: Option<String>,
}

impl From<execution_hierarchy::ExecutionSubtreeNode> for SubtreeNodeDoc {
    fn from(node: execution_hierarchy::ExecutionSubtreeNode) -> Self {
        Self {
            execution_id: node.execution_id,
            execution_type: node.execution_type,
            status: node.status,
            depth: node.depth,
            parent_execution_id: node.parent_execution_id,
        }
    }
}

/// One row a scan reached but could not read, reported beside the rows that
/// could be read so a single broken record cannot hide the rest of a tree.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RejectedRowDoc {
    execution_id: String,
    reason: String,
}

impl From<execution_hierarchy::RejectedRow> for RejectedRowDoc {
    fn from(row: execution_hierarchy::RejectedRow) -> Self {
        Self {
            execution_id: row.execution_id,
            reason: row.reason,
        }
    }
}

/// Every execution below a root, breadth-first.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SubtreeDoc {
    root_execution_id: String,
    /// Set when the node cap dropped descendants, so a caller can tell a
    /// complete tree from a clipped one.
    truncated: bool,
    /// How many descendants the node cap dropped. Query a descendant for the
    /// part of the tree this response left out.
    #[serde(skip_serializing_if = "is_zero")]
    omitted: usize,
    /// Root first, then each level in child order.
    nodes: Vec<SubtreeNodeDoc>,
    rejected: Vec<RejectedRowDoc>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

impl From<execution_hierarchy::ExecutionSubtree> for SubtreeDoc {
    fn from(tree: execution_hierarchy::ExecutionSubtree) -> Self {
        Self {
            root_execution_id: tree.root_execution_id,
            truncated: tree.truncated,
            omitted: tree.omitted,
            nodes: tree.nodes.into_iter().map(SubtreeNodeDoc::from).collect(),
            rejected: tree
                .rejected
                .into_iter()
                .map(RejectedRowDoc::from)
                .collect(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/hierarchy",
    operation_id = "get_executions_id_hierarchy",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::observation::hierarchy::HierarchyDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_hierarchy(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match execution_hierarchy::hierarchy(&state.ctx, &path.id).await {
        Ok(view) => ok(HierarchyDoc::from(view)).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/subtree",
    operation_id = "get_executions_id_subtree",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::observation::hierarchy::SubtreeDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_subtree(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match execution_hierarchy::subtree(&state.ctx, &path.id).await {
        Ok(tree) => ok(SubtreeDoc::from(tree)).into_response(),
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use axum::response::Response;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;
    use wf_storage::adapter::base::BaseStorageAdapter;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    async fn save_agent(ctx: &ApiContext, id: &str) {
        let record = wf_types::AgentExecution {
            id: wf_types::Id::from(id.to_string()),
            definition_id: wf_types::Id::from("agent-1".to_string()),
            status: wf_types::ExecutionStatus::Completed,
            current_iteration: 1,
            tool_call_count: 0,
            iteration_history: None,
            started_at: 1,
            completed_at: Some(2),
            error: None,
            context: None,
            loop_config: None,
            permanently_failed_tools: None,
            hierarchy: None,
        };
        ctx.storage.agent_execution.save(&record).await.unwrap();
    }

    async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn hierarchy_routes_answer_persisted_executions() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-1").await;

        let response = send(ctx.clone(), "/api/v1/executions/loop-1/hierarchy").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["data"]["execution_id"], "loop-1");
        assert_eq!(body["data"]["execution_type"], "agent_loop");
        assert_eq!(body["data"]["depth"], 0);

        let tree = send(ctx.clone(), "/api/v1/executions/loop-1/subtree").await;
        assert_eq!(tree.status(), StatusCode::OK);
        let tree_body = json_body(tree).await;
        assert_eq!(tree_body["data"]["root_execution_id"], "loop-1");
        assert_eq!(tree_body["data"]["truncated"], false);
        assert_eq!(tree_body["data"]["nodes"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn hierarchy_routes_map_unknown_execution_to_not_found() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/executions/missing/hierarchy",
            "/api/v1/executions/missing/subtree",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }
    }
}
