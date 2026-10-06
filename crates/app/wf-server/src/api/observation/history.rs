//! Execution history domain: everything one execution recorded, grouped by
//! section. One endpoint covers both engines, so a consumer reads the same
//! shape for a workflow run and an agent loop; sections the owning engine
//! does not record come back empty rather than absent.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use wf_api::execution_history::{self, ExecutionHistorySections};
use wf_types::execution::ExecutionType;
use wf_types::ExecutionStatus;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/executions/{id}/history", get(handle_history))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct HistoryQuery {
    /// Comma-separated sections to load; omit for all of
    /// `timeline,nodes,iterations,variables,context,transitions`.
    include: Option<String>,
}

/// One lifecycle event on the history timeline.
///
/// Every field mirrors `wf_types::events::BaseEvent` except the
/// discriminator: that enum's variants grow with the engines, so a schema
/// mirroring it would have to be rebuilt on every addition. The wire declares
/// the string the enum actually serializes to instead, taking its
/// `SCREAMING_SNAKE_CASE` names such as `WORKFLOW_EXECUTION_STARTED`.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TimelineEventDoc {
    id: String,
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    r#type: String,
    timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_loop_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl From<wf_types::events::BaseEvent> for TimelineEventDoc {
    fn from(event: wf_types::events::BaseEvent) -> Self {
        Self {
            id: event.id,
            r#type: event.r#type.as_str().to_string(),
            timestamp: event.timestamp,
            event_name: event.event_name,
            workflow_id: event.workflow_id,
            execution_id: event.execution_id,
            agent_loop_id: event.agent_loop_id,
            metadata: event.metadata,
        }
    }
}

/// One workflow node execution attempt.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct NodeExecutionDoc {
    node_id: String,
    node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Value>)]
    input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Value>)]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_at: Option<i64>,
    duration_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch_id: Option<String>,
}

impl From<wf_api::NodeExecutionAuditView> for NodeExecutionDoc {
    fn from(view: wf_api::NodeExecutionAuditView) -> Self {
        Self {
            node_id: view.node_id,
            node_type: view.node_type,
            input: view.input,
            result: view.result,
            error: view.error,
            started_at: view.started_at,
            completed_at: view.completed_at,
            duration_ms: view.duration_ms,
            branch_id: view.branch_id,
        }
    }
}

/// One tool call inside an agent iteration.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ToolCallDoc {
    name: String,
    arguments: serde_json::Value,
    result: Option<serde_json::Value>,
    error: Option<String>,
    tool_call_id: Option<String>,
    duration_ms: i64,
    success: bool,
}

/// One agent loop iteration.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct IterationDoc {
    iteration: u32,
    start_time: i64,
    end_time: i64,
    /// Duration in ms, `-1` while the iteration is still in progress.
    duration: i64,
    tool_call_count: u32,
    tool_calls: Vec<ToolCallDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_content: Option<String>,
}

impl From<wf_api::IterationDetail> for IterationDoc {
    fn from(detail: wf_api::IterationDetail) -> Self {
        Self {
            iteration: detail.iteration,
            start_time: detail.start_time,
            end_time: detail.end_time,
            duration: detail.duration,
            tool_call_count: detail.tool_call_count,
            tool_calls: detail
                .tool_calls
                .into_iter()
                .map(|call| ToolCallDoc {
                    name: call.name,
                    arguments: call.arguments,
                    result: call.result,
                    error: call.error,
                    tool_call_id: call.tool_call_id,
                    duration_ms: call.duration_ms,
                    success: call.success,
                })
                .collect(),
            response_content: detail.response_content,
        }
    }
}

/// Context growth at one point in an agent loop.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ContextEvolutionDoc {
    timestamp: i64,
    iteration: u32,
    #[schema(value_type = String)]
    status: ExecutionStatus,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<u32>,
}

impl From<wf_api::ContextEvolutionEntry> for ContextEvolutionDoc {
    fn from(entry: wf_api::ContextEvolutionEntry) -> Self {
        Self {
            timestamp: entry.timestamp,
            iteration: entry.iteration,
            status: entry.status,
            description: entry.description,
            tool_calls: entry.tool_calls,
        }
    }
}

/// One status transition of a workflow execution.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct StateTransitionDoc {
    from: String,
    to: String,
    timestamp: i64,
}

impl From<wf_api::StateTransitionView> for StateTransitionDoc {
    fn from(view: wf_api::StateTransitionView) -> Self {
        Self {
            from: view.from,
            to: view.to,
            timestamp: view.timestamp,
        }
    }
}

/// Everything an execution recorded, grouped by section. A section that was
/// not asked for comes back empty rather than absent, so the payload shape
/// never changes.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct HistoryDoc {
    execution_id: String,
    #[schema(value_type = String)]
    execution_type: ExecutionType,
    /// Cap on `timeline`: one read never carries more lifecycle events than
    /// this, and a run past the cap has its later events left out.
    timeline_limit: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    timeline: Vec<TimelineEventDoc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    node_executions: Vec<NodeExecutionDoc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    iterations: Vec<IterationDoc>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    variables: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    context_evolution: Vec<ContextEvolutionDoc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    status_transitions: Vec<StateTransitionDoc>,
}

impl From<execution_history::ExecutionHistoryView> for HistoryDoc {
    fn from(view: execution_history::ExecutionHistoryView) -> Self {
        Self {
            execution_id: view.execution_id,
            execution_type: view.execution_type,
            timeline_limit: view.timeline_limit,
            timeline: view
                .timeline
                .into_iter()
                .map(TimelineEventDoc::from)
                .collect(),
            node_executions: view
                .node_executions
                .into_iter()
                .map(NodeExecutionDoc::from)
                .collect(),
            iterations: view
                .iterations
                .into_iter()
                .map(IterationDoc::from)
                .collect(),
            variables: view.variables,
            context_evolution: view
                .context_evolution
                .into_iter()
                .map(ContextEvolutionDoc::from)
                .collect(),
            status_transitions: view
                .status_transitions
                .into_iter()
                .map(StateTransitionDoc::from)
                .collect(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/history",
    operation_id = "get_executions_id_history",
    tag = "observation",
    params(IdPath, HistoryQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::observation::history::HistoryDoc>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_history(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<HistoryQuery>,
) -> impl IntoResponse {
    let sections = match ExecutionHistorySections::parse(query.include.as_deref()) {
        Ok(sections) => sections,
        Err(e) => return error_response(e),
    };
    match execution_history::history(&state.ctx, &path.id, &sections).await {
        Ok(view) => ok(HistoryDoc::from(view)).into_response(),
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
            iteration_history: Some(vec![wf_types::agent_execution::IterationRecord {
                iteration: 1,
                started_at: 10,
                completed_at: Some(20),
                tool_calls: None,
                response_content: None,
                llm_calls: None,
                error: None,
            }]),
            started_at: 1,
            completed_at: Some(30),
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
    async fn history_answers_agent_loop_iterations() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-h").await;

        let response = send(ctx.clone(), "/api/v1/executions/loop-h/history").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["data"]["execution_id"], "loop-h");
        assert_eq!(body["data"]["execution_type"], "agent_loop");
        assert_eq!(body["data"]["iterations"].as_array().unwrap().len(), 1);
        // Workflow-only sections are omitted rather than reported as empty.
        assert!(body["data"].get("node_executions").is_none());
    }

    #[tokio::test]
    async fn include_narrows_the_loaded_sections() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-i").await;

        let response = send(ctx, "/api/v1/executions/loop-i/history?include=timeline").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert!(body["data"].get("iterations").is_none());
    }

    #[tokio::test]
    async fn unknown_section_is_rejected() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-bad").await;

        let response = send(ctx, "/api/v1/executions/loop-bad/history?include=bogus").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        let response = send(ctx, "/api/v1/executions/missing/history").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
