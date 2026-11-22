//! Conversation branch tree endpoints.
//!
//! Two halves that no longer share a request: `/turns` starts a turn and returns
//! at once, `/stream` observes the session. A client that drops the stream keeps
//! the answer; a client that reconnects gets a snapshot plus whatever it missed.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;

use wf_types::events::{BaseEvent, EventType};

use crate::envelope::{error_response, ok};
use crate::router::ApiState;
use crate::sse::sse_response;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/chat/sessions", post(handle_create_session))
        .route(
            "/chat/sessions/{id}",
            get(handle_snapshot).delete(handle_delete_session),
        )
        .route("/chat/sessions/{id}/path", get(handle_path))
        .route("/chat/sessions/{id}/dirty", get(handle_dirty))
        .route("/chat/sessions/{id}/stream", get(handle_stream))
        .route("/chat/sessions/{id}/grafts", get(handle_grafts))
        .route("/chat/sessions/{id}/turns", post(handle_submit_turn))
        .route(
            "/chat/sessions/{id}/turns/{turn_id}/cancel",
            post(handle_cancel_turn),
        )
        .route("/chat/sessions/{id}/step", post(handle_step))
        .route("/chat/sessions/{id}/leaf", post(handle_set_leaf))
        .route("/chat/sessions/{id}/detach", post(handle_detach))
        .route("/chat/sessions/{id}/attach", post(handle_attach))
        .route(
            "/chat/sessions/{id}/nodes/{node_id}",
            delete(handle_delete_node),
        )
        .route(
            "/chat/sessions/{id}/nodes/{node_id}/insert-before",
            post(handle_insert_before),
        )
        .route(
            "/chat/sessions/{id}/nodes/{node_id}/splice",
            post(handle_splice),
        )
        .route(
            "/chat/sessions/{id}/nodes/{node_id}/reparent",
            post(handle_reparent),
        )
        .route(
            "/chat/sessions/{id}/nodes/{node_id}/content",
            post(handle_write_content),
        )
        .route(
            "/chat/sessions/{id}/nodes/{node_id}/edit",
            post(handle_edit_branch),
        )
        .route(
            "/chat/sessions/{id}/nodes/{node_id}/reply",
            post(handle_write_reply),
        )
        .route(
            "/chat/sessions/{id}/nodes/{node_id}/reply/discard",
            post(handle_discard_reply),
        )
        .route(
            "/chat/sessions/{id}/nodes/{node_id}/reply/restore",
            post(handle_restore_reply),
        )
        .route(
            "/chat/sessions/{id}/nodes/{node_id}/acknowledge",
            post(handle_acknowledge),
        )
}

#[derive(Deserialize)]
struct SessionPath {
    id: String,
}

#[derive(Deserialize)]
struct NodePath {
    id: String,
    node_id: String,
}

#[derive(Deserialize)]
struct TurnPath {
    id: String,
    turn_id: String,
}

#[derive(Deserialize)]
struct LeafQuery {
    #[serde(default)]
    leaf: Option<String>,
}

#[derive(Deserialize)]
struct StreamQuery {
    /// Replay events strictly newer than this timestamp.
    #[serde(default)]
    since: Option<i64>,
}

#[derive(Deserialize)]
struct CreateBody {
    #[serde(default)]
    profile_id: Option<String>,
}

#[derive(Deserialize)]
struct TurnBody {
    #[serde(default)]
    profile_id: Option<String>,
    #[serde(default)]
    message: String,
    #[serde(default)]
    images: Option<Vec<wf_types::message::UserImageInput>>,
    /// Parent node. Absent appends under the active leaf.
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    if_version: Option<u64>,
}

#[derive(Deserialize)]
struct StepBody {
    #[serde(default)]
    leaf: Option<String>,
    /// `single` (default) or `forward`.
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    stop_at: Option<String>,
    #[serde(default)]
    max_steps: Option<usize>,
    #[serde(default)]
    profile_id: Option<String>,
    #[serde(default)]
    if_version: Option<u64>,
}

#[derive(Deserialize)]
struct LeafBody {
    node_id: String,
    #[serde(default)]
    if_version: Option<u64>,
}

#[derive(Deserialize)]
struct InsertBody {
    text: String,
    /// Insert a premise instead of a turn.
    #[serde(default)]
    premise: bool,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    if_version: Option<u64>,
}

#[derive(Deserialize)]
struct ReparentBody {
    new_parent: String,
    #[serde(default)]
    if_version: Option<u64>,
}

#[derive(Deserialize)]
struct ContentBody {
    text: String,
    #[serde(default)]
    if_version: Option<u64>,
}

#[derive(Deserialize)]
struct DetachBody {
    node_id: String,
    /// Copy the ancestors into the graft so the subtree keeps its freshness.
    #[serde(default)]
    carry_prefix: bool,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    if_version: Option<u64>,
}

#[derive(Deserialize)]
struct AttachBody {
    graft_id: String,
    parent: String,
    /// Predecessor to replace. Absent links the subtree as a new child.
    #[serde(default)]
    replace: Option<String>,
    #[serde(default)]
    if_version: Option<u64>,
}

#[derive(Deserialize)]
struct VersionBody {
    #[serde(default)]
    if_version: Option<u64>,
}

fn surgery(session_id: &str, if_version: Option<u64>) -> wf_api::conversation::SurgeryRequest {
    wf_api::conversation::SurgeryRequest {
        session_id: session_id.to_string(),
        if_version,
    }
}

async fn handle_create_session(
    State(state): State<ApiState>,
    Json(body): Json<CreateBody>,
) -> Response {
    match wf_api::conversation::create_session(&state.ctx, body.profile_id, None).await {
        Ok(tree) => ok(json!({
            "session_id": tree.session_id,
            "tree_id": tree.id,
            "version": tree.version,
        }))
        .into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_snapshot(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
    Query(query): Query<LeafQuery>,
) -> Response {
    match wf_api::conversation::snapshot(&state.ctx, &path.id, query.leaf.as_deref()).await {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_path(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
    Query(query): Query<LeafQuery>,
) -> Response {
    match wf_api::conversation::path_messages(&state.ctx, &path.id, query.leaf.as_deref()).await {
        Ok(entries) => ok(entries).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_dirty(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
    Query(query): Query<LeafQuery>,
) -> Response {
    match wf_api::conversation::dirty_list(&state.ctx, &path.id, query.leaf.as_deref()).await {
        Ok(dirty) => ok(dirty).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_grafts(State(state): State<ApiState>, Path(path): Path<SessionPath>) -> Response {
    match wf_api::conversation::list_grafts(&state.ctx, &path.id).await {
        Ok(grafts) => ok(grafts).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_submit_turn(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
    Json(body): Json<TurnBody>,
) -> Response {
    let request = wf_api::conversation::TurnRequest {
        session_id: path.id,
        profile_id: body.profile_id,
        message: body.message,
        images: body.images.unwrap_or_default(),
        parent: body.parent,
        if_version: body.if_version,
        directory: None,
    };
    match wf_api::conversation::submit_turn(state.ctx.clone(), request).await {
        Ok(handle) => (
            axum::http::StatusCode::ACCEPTED,
            ok(handle),
        )
            .into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_cancel_turn(State(state): State<ApiState>, Path(path): Path<TurnPath>) -> Response {
    let cancelled = wf_api::conversation::cancel_turn(&path.id, Some(&path.turn_id)).await;
    let _ = &state;
    ok(json!({ "cancelled": cancelled })).into_response()
}

async fn handle_step(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
    Json(body): Json<StepBody>,
) -> Response {
    let mode = match body.mode.as_deref() {
        None | Some("single") => wf_api::conversation::StepMode::Single,
        Some("forward") => wf_api::conversation::StepMode::Forward,
        Some(other) => {
            return error_response(wf_api::ApiError::Validation(format!(
                "unknown step mode '{other}'"
            )))
        }
    };
    let request = wf_api::conversation::StepRequest {
        session_id: path.id,
        leaf: body.leaf,
        mode,
        stop_at: body.stop_at,
        max_steps: body.max_steps.unwrap_or(0),
        profile_id: body.profile_id,
        if_version: body.if_version,
    };
    match wf_api::conversation::start_step(state.ctx.clone(), request).await {
        Ok(handle) => (axum::http::StatusCode::ACCEPTED, ok(handle)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_set_leaf(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
    Json(body): Json<LeafBody>,
) -> Response {
    match wf_api::conversation::set_active_leaf(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &body.node_id,
    )
    .await
    {
        Ok(version) => ok(json!({ "version": version })).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_insert_before(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Json(body): Json<InsertBody>,
) -> Response {
    let insert = wf_api::conversation::InsertRequest {
        text: body.text,
        premise: body.premise,
        label: body.label,
    };
    match wf_api::conversation::insert_before(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &path.node_id,
        insert,
    )
    .await
    {
        Ok(outcome) => (axum::http::StatusCode::ACCEPTED, ok(outcome)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_splice(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Json(body): Json<VersionBody>,
) -> Response {
    match wf_api::conversation::splice_out(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &path.node_id,
    )
    .await
    {
        Ok(change) => ok(change).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_reparent(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Json(body): Json<ReparentBody>,
) -> Response {
    match wf_api::conversation::reparent(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &path.node_id,
        &body.new_parent,
    )
    .await
    {
        Ok(change) => ok(change).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_write_content(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Json(body): Json<ContentBody>,
) -> Response {
    match wf_api::conversation::write_user_content(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &path.node_id,
        &body.text,
    )
    .await
    {
        Ok(change) => (axum::http::StatusCode::ACCEPTED, ok(change)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_edit_branch(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Json(body): Json<ContentBody>,
) -> Response {
    match wf_api::conversation::edit_branch(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &path.node_id,
        &body.text,
    )
    .await
    {
        Ok(outcome) => (axum::http::StatusCode::ACCEPTED, ok(outcome)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_write_reply(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Json(body): Json<ContentBody>,
) -> Response {
    match wf_api::conversation::write_reply_body(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &path.node_id,
        &body.text,
    )
    .await
    {
        Ok(change) => ok(change).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_discard_reply(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Json(body): Json<VersionBody>,
) -> Response {
    match wf_api::conversation::discard_reply(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &path.node_id,
    )
    .await
    {
        Ok(change) => ok(change).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_restore_reply(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Json(body): Json<VersionBody>,
) -> Response {
    match wf_api::conversation::restore_reply(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &path.node_id,
    )
    .await
    {
        Ok(change) => ok(change).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_acknowledge(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Json(body): Json<VersionBody>,
) -> Response {
    match wf_api::conversation::acknowledge_reply(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &path.node_id,
    )
    .await
    {
        Ok(change) => ok(change).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_detach(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
    Json(body): Json<DetachBody>,
) -> Response {
    match wf_api::conversation::detach_subtree(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &body.node_id,
        body.carry_prefix,
        body.label,
    )
    .await
    {
        Ok(view) => (axum::http::StatusCode::CREATED, ok(view)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_attach(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
    Json(body): Json<AttachBody>,
) -> Response {
    match wf_api::conversation::attach_graft(
        &state.ctx,
        surgery(&path.id, body.if_version),
        &body.graft_id,
        &body.parent,
        body.replace.as_deref(),
    )
    .await
    {
        Ok(view) => ok(view).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_delete_node(
    State(state): State<ApiState>,
    Path(path): Path<NodePath>,
    Query(query): Query<VersionQuery>,
) -> Response {
    match wf_api::conversation::delete_node(
        &state.ctx,
        surgery(&path.id, query.if_version),
        &path.node_id,
    )
    .await
    {
        Ok(change) => ok(change).into_response(),
        Err(error) => error_response(error),
    }
}

#[derive(Deserialize)]
struct VersionQuery {
    #[serde(default)]
    if_version: Option<u64>,
}

async fn handle_delete_session(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
) -> Response {
    match wf_api::conversation::delete_session(&state.ctx, &path.id).await {
        Ok(()) => ok(json!({ "deleted": true })).into_response(),
        Err(error) => error_response(error),
    }
}

async fn handle_stream(
    State(state): State<ApiState>,
    Path(path): Path<SessionPath>,
    Query(query): Query<StreamQuery>,
) -> Response {
    // The snapshot is the authority: a subscriber that missed events still sees
    // the tree as it is, so a lagging stream never leaves the client guessing.
    let snapshot = match wf_api::conversation::snapshot(&state.ctx, &path.id, None).await {
        Ok(snapshot) => snapshot,
        Err(error) => return error_response(error),
    };
    let events = match wf_api::conversation::subscribe(state.ctx.clone(), path.id.clone(), query.since)
        .await
    {
        Ok(events) => events,
        Err(error) => return error_response(error),
    };
    let head = serde_json::to_string(&snapshot).unwrap_or_default();
    let head_frame = format!("event: session.snapshot\ndata: {head}\n\n");
    let stream = futures::stream::once(async move { Ok::<Bytes, Infallible>(Bytes::from(head_frame)) })
        .chain(events.map(|event| {
            let (name, payload) = frame_of(&event);
            Ok::<Bytes, Infallible>(Bytes::from(format!(
                "event: {name}\ndata: {payload}\n\n"
            )))
        }));
    sse_response(Box::pin(stream))
}

/// Map an event onto the wire frame name and payload.
///
/// One `LlmStreamChunk` carries three different frames, told apart by `kind`, so
/// the delta/usage frame names stay the ones the existing front-end accumulator
/// already understands.
fn frame_of(event: &BaseEvent) -> (&'static str, String) {
    let metadata = event.metadata.clone().unwrap_or_default();
    let kind = metadata
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let reason = metadata
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = match event.r#type {
        EventType::ConversationTurnStarted => "turn.started",
        EventType::ConversationTurnCompleted => "turn.completed",
        EventType::ConversationTurnFailed => {
            if reason == "cancelled" {
                "turn.cancelled"
            } else {
                "turn.failed"
            }
        }
        EventType::LlmStreamChunk => match kind {
            "reasoning" => "turn.reasoning",
            _ => "turn.delta",
        },
        EventType::LlmStreamDone => "turn.usage",
        EventType::ConversationTreeChanged => "tree.changed",
        EventType::ConversationDirtyChanged => "dirty.changed",
        EventType::ConversationStepStarted => "step.started",
        EventType::ConversationStepCompleted => {
            if reason == "step" {
                "step.progress"
            } else {
                "step.completed"
            }
        }
        _ => "session.event",
    };
    let payload = serde_json::to_string(&metadata).unwrap_or_else(|_| "{}".to_string());
    (name, payload)
}
