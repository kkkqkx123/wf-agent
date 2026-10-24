//! Trigger template draft lifecycle: editable drafts that never execute
//! directly, publish preview validation, promotion to formal templates and
//! the Draft/Formal lifecycle view. Thin transport adapters over the
//! shared draft service at `wf-api::definition::draft`.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::api::definition::drafts::ValidationIssueDoc;
use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

/// The kind of definition these routes carry.
const KIND: wf_api::definition::draft::DefinitionKind =
    wf_api::definition::draft::DefinitionKind::Trigger;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/templates/trigger/drafts",
            get(handle_list_drafts).post(handle_save_draft),
        )
        .route(
            "/templates/trigger/drafts/{id}",
            get(handle_get_draft).delete(handle_delete_draft),
        )
        .route(
            "/templates/trigger/drafts/{id}/promote",
            post(handle_promote_draft),
        )
        .route(
            "/templates/trigger/drafts/{id}/validate",
            get(handle_validate_draft),
        )
        .route("/templates/trigger/validate", post(handle_validate_body))
        .route("/templates/trigger/{id}/lifecycle", get(handle_lifecycle))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct DraftListQuery {
    /// Narrow the list to the drafts of one definition.
    definition_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger/drafts",
    operation_id = "get_templates_trigger_drafts",
    tag = "template",
    params(DraftListQuery),
    responses((status = 200, description = "List of trigger template drafts", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_drafts(
    State(state): State<ApiState>,
    Query(query): Query<DraftListQuery>,
) -> impl IntoResponse {
    let drafts = match query.definition_id {
        Some(definition_id) => {
            wf_api::trigger::trigger_draft::list_drafts_of(&state.ctx, &definition_id).await
        }
        None => wf_api::trigger::trigger_draft::list_drafts(&state.ctx).await,
    };
    match drafts {
        Ok(drafts) => ok(drafts).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/trigger/drafts",
    operation_id = "post_templates_trigger_drafts",
    tag = "template",
    request_body = serde_json::Value,
    responses((status = 200, description = "Trigger template draft created", body = crate::envelope::ApiEnvelope<String>), (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_draft(
    State(state): State<ApiState>,
    Json(template): Json<wf_api::TriggerTemplateStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::trigger::trigger_draft::save_draft(&state.ctx, &template).await {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger/drafts/{id}",
    operation_id = "get_templates_trigger_drafts_id",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Trigger template draft found", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::trigger::trigger_draft::get_draft(&state.ctx, &path.id).await {
        Ok(draft) => ok(draft).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/templates/trigger/drafts/{id}",
    operation_id = "delete_templates_trigger_drafts_id",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Trigger template draft deleted", body = crate::envelope::ApiEnvelope<bool>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::definition::draft::delete_draft(&state.ctx, KIND, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/trigger/drafts/{id}/promote",
    operation_id = "post_templates_trigger_drafts_id_promote",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Draft promoted", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_promote_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::definition::draft::promote_draft(&state.ctx, KIND, &path.id).await {
        Ok(outcome) => ok(outcome
            .warnings
            .into_iter()
            .map(ValidationIssueDoc::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger/drafts/{id}/validate",
    operation_id = "get_templates_trigger_drafts_id_validate",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Draft validation", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_validate_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::definition::draft::validate_draft_complete(&state.ctx, KIND, &path.id).await {
        Ok(warnings) => ok(warnings
            .into_iter()
            .map(ValidationIssueDoc::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

/// Dry-run compile gate over an unsaved trigger template body: full shape
/// plus reference closure, read-only, never persists.
#[utoipa::path(
    post,
    path = "/api/v1/templates/trigger/validate",
    operation_id = "post_templates_trigger_validate",
    tag = "template",
    request_body = serde_json::Value,
    responses((status = 200, description = "Dry-run validation issues (empty when valid)", body = crate::envelope::ApiEnvelope<Vec<ValidationIssueDoc>>), (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_validate_body(
    State(state): State<ApiState>,
    Json(template): Json<wf_api::TriggerTemplateStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::trigger::template::validate_dry_run(&state.ctx, &template).await {
        Ok(result) => ok(result
            .errors
            .into_iter()
            .map(ValidationIssueDoc::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger/{id}/lifecycle",
    operation_id = "get_templates_trigger_id_lifecycle",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Trigger template lifecycle", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_lifecycle(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    ok(wf_api::definition::draft::lifecycle_of(&state.ctx, KIND, &path.id).await).into_response()
}
