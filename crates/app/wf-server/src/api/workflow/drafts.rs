//! Workflow draft lifecycle: editable drafts that never execute directly,
//! publish preview validation, promotion to formal definitions and the
//! Draft/Formal/Expired lifecycle view. Thin transport adapters over
//! `wf-api::workflow::draft`.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/workflows/drafts",
            get(handle_list_drafts).post(handle_save_draft),
        )
        .route("/workflows/drafts/promote-all", post(handle_promote_all))
        .route(
            "/workflows/drafts/{id}",
            get(handle_get_draft).delete(handle_delete_draft),
        )
        .route("/workflows/drafts/{id}/promote", post(handle_promote_draft))
        .route(
            "/workflows/drafts/{id}/validate",
            get(handle_validate_draft),
        )
        .route("/workflows/{id}/lifecycle", get(handle_lifecycle))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/drafts",
    operation_id = "get_workflows_drafts",
    tag = "workflow",
    responses((status = 200, description = "Draft list: array of full workflow definitions (free-form; drafts may be incomplete and never execute directly)", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_drafts(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::workflow::draft::list_drafts(&state.ctx).await {
        Ok(drafts) => ok(drafts).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/drafts",
    operation_id = "post_workflows_drafts",
    tag = "workflow",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_draft(
    State(state): State<ApiState>,
    Json(workflow): Json<wf_api::WorkflowDefinition>,
) -> impl IntoResponse {
    match wf_api::workflow::draft::save_draft(&state.ctx, &workflow).await {
        Ok(()) => ok(workflow.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/drafts/{id}",
    operation_id = "get_workflows_drafts_id",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Draft detail: full workflow definition (free-form; may be incomplete)", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::draft::get_draft(&state.ctx, &path.id).await {
        Ok(draft) => ok(draft).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/workflows/drafts/{id}",
    operation_id = "delete_workflows_drafts_id",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::draft::delete_draft(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

/// One validation issue on a draft: dotted field path plus message.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ValidationIssueDoc {
    field: String,
    message: String,
}

impl From<wf_types::ValidationError> for ValidationIssueDoc {
    fn from(view: wf_types::ValidationError) -> Self {
        Self {
            field: view.field,
            message: view.message,
        }
    }
}

/// Per-dependent revalidation result of a draft promotion.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct DependentImpactDoc {
    workflow_id: String,
    workflow_name: String,
    node_id: String,
    field: String,
    level: String,
    errors: Vec<String>,
    warnings: Vec<String>,
}

/// Impact report returned when a draft is promoted to formal.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct PromoteReportDoc {
    resource_kind: String,
    resource_id: String,
    dependents: Vec<DependentImpactDoc>,
    error_count: usize,
    warning_count: usize,
    pass_count: usize,
}

impl From<wf_api::infra::dependency::UpdateImpactReport> for PromoteReportDoc {
    fn from(report: wf_api::infra::dependency::UpdateImpactReport) -> Self {
        Self {
            resource_kind: report.resource_kind.as_str().to_string(),
            resource_id: report.resource_id,
            dependents: report
                .dependents
                .into_iter()
                .map(|d| DependentImpactDoc {
                    workflow_id: d.workflow_id,
                    workflow_name: d.workflow_name,
                    node_id: d.node_id,
                    field: d.field,
                    level: match d.level {
                        wf_api::infra::dependency::ImpactLevel::Pass => "pass",
                        wf_api::infra::dependency::ImpactLevel::Warning => "warning",
                        wf_api::infra::dependency::ImpactLevel::Error => "error",
                    }
                    .to_string(),
                    errors: d.errors,
                    warnings: d.warnings,
                })
                .collect(),
            error_count: report.error_count,
            warning_count: report.warning_count,
            pass_count: report.pass_count,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/drafts/{id}/promote",
    operation_id = "post_workflows_drafts_id_promote",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Promotion impact report", body = crate::envelope::ApiEnvelope<PromoteReportDoc>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_promote_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::draft::promote_draft(&state.ctx, &path.id).await {
        Ok(report) => ok(PromoteReportDoc::from(report)).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/drafts/promote-all",
    tag = "workflow",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_promote_all(State(state): State<ApiState>) -> impl IntoResponse {
    let outcomes = wf_api::workflow::draft::promote_all_drafts(&state.ctx).await;
    let views: Vec<serde_json::Value> = outcomes
        .into_iter()
        .map(|(id, result)| match result {
            Ok(report) => serde_json::json!({"id": id, "ok": true, "report": report}),
            Err(e) => serde_json::json!({"id": id, "ok": false, "error": e.to_string()}),
        })
        .collect();
    ok(views).into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/drafts/{id}/validate",
    operation_id = "get_workflows_drafts_id_validate",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Draft validation issues (empty when promotable)", body = crate::envelope::ApiEnvelope<Vec<ValidationIssueDoc>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_validate_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::draft::validate_draft_complete(&state.ctx, &path.id).await {
        Ok(warnings) => ok(warnings
            .into_iter()
            .map(ValidationIssueDoc::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/lifecycle",
    operation_id = "get_workflows_id_lifecycle",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_lifecycle(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    ok(wf_api::workflow::draft::lifecycle_of(&state.ctx, &path.id).await).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    fn formal_workflow(id: &str) -> wf_api::WorkflowDefinition {
        wf_api::WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {id}"),
            description: None,
            r#type: None,
            version: None,
            nodes: vec![
                wf_api::BaseStaticNode {
                    id: "start".into(),
                    node_type: wf_types::node::StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                wf_api::BaseStaticNode {
                    id: "end".into(),
                    node_type: wf_types::node::StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![wf_api::Edge {
                id: "e1".into(),
                source_node_id: "start".into(),
                target_node_id: "end".into(),
                r#type: wf_types::workflow::EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
                error_route: None,
            }],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            created_at: wf_api::now(),
            updated_at: wf_api::now(),
            available_tools: None,
            hooks: None,
        }
    }

    async fn send(
        ctx: Arc<ApiContext>,
        method: &str,
        uri: &str,
        body: Option<String>,
    ) -> StatusCode {
        let builder = Request::builder().method(method).uri(uri);
        let request = if let Some(body) = body {
            builder
                .header("content-type", "application/json")
                .body(AxBody::from(body))
                .unwrap()
        } else {
            builder.body(AxBody::empty()).unwrap()
        };
        crate::router::api_router(ctx)
            .oneshot(request)
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn draft_lifecycle_round_trip() {
        let ctx = make_ctx();
        let body = serde_json::to_string(&formal_workflow("wf-draft")).unwrap();
        assert_eq!(
            send(ctx.clone(), "POST", "/api/v1/workflows/drafts", Some(body)).await,
            StatusCode::OK
        );
        assert_eq!(
            send(ctx.clone(), "GET", "/api/v1/workflows/drafts", None).await,
            StatusCode::OK
        );
        assert_eq!(
            send(
                ctx.clone(),
                "GET",
                "/api/v1/workflows/wf-draft/lifecycle",
                None
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            send(
                ctx.clone(),
                "POST",
                "/api/v1/workflows/drafts/wf-draft/promote",
                None
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            send(ctx, "GET", "/api/v1/workflows/wf-draft", None).await,
            StatusCode::OK
        );
    }
}
