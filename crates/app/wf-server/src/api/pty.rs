use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::body::{Body, Bytes};
use axum::http::Request;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::middleware::AuthConfig;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/pty", post(handle_create_pty).get(handle_list_pty))
        .route("/pty/{id}", get(handle_get_pty).delete(handle_delete_pty))
        .route("/pty/{id}/resize", post(handle_resize_pty))
        .route("/pty/{id}/connect", get(handle_connect_pty))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PtySessionStatus {
    Running,
    Exited,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct PtySession {
    pub id: String,
    pub cwd: String,
    pub status: PtySessionStatus,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreatePtyBody {
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    rows: Option<u16>,
    #[serde(default)]
    cols: Option<u16>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ConnectQuery {
    cursor: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ResizePtyBody {
    #[serde(default = "default_rows")]
    rows: u32,
    #[serde(default = "default_cols")]
    cols: u32,
}

fn default_rows() -> u32 {
    24
}

fn default_cols() -> u32 {
    80
}

#[utoipa::path(post, path = "/api/v1/pty", operation_id = "post_pty", tag = "pty", request_body = CreatePtyBody, responses((status = 200, description = "PTY session created", body = crate::envelope::ApiEnvelope<PtySession>)), security(("api_key" = [])))]
pub(crate) async fn handle_create_pty(
    State(state): State<ApiState>,
    body: Option<axum::Json<CreatePtyBody>>,
) -> impl IntoResponse {
    let body = body.map(|b| b.0).unwrap_or(CreatePtyBody {
        cwd: None,
        command: None,
        rows: None,
        cols: None,
    });
    match wf_api::pty::create_pty_session(
        &state.ctx,
        body.cwd,
        body.command,
        body.rows,
        body.cols,
    ) {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/pty", operation_id = "get_pty", tag = "pty", responses((status = 200, description = "PTY sessions", body = crate::envelope::ApiEnvelope<Vec<PtySession>>)), security(("api_key" = [])))]
pub(crate) async fn handle_list_pty(State(state): State<ApiState>) -> impl IntoResponse {
    let _ = state;
    let list = wf_api::pty::list_pty_sessions();
    ok(list).into_response()
}

#[utoipa::path(get, path = "/api/v1/pty/{id}", operation_id = "get_pty_id", tag = "pty", params(IdPath), responses((status = 200, description = "PTY session", body = crate::envelope::ApiEnvelope<PtySession>)), security(("api_key" = [])))]
pub(crate) async fn handle_get_pty(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _ = state;
    match wf_api::pty::get_pty_session(&id) {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(delete, path = "/api/v1/pty/{id}", operation_id = "delete_pty", tag = "pty", params(IdPath), responses((status = 200, description = "PTY session removed", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_delete_pty(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _ = state;
    match wf_api::pty::remove_pty_session(&id) {
        Ok(_) => ok(serde_json::json!({"removed": true, "id": id})).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(post, path = "/api/v1/pty/{id}/resize", operation_id = "post_pty_resize", tag = "pty", params(IdPath), request_body = ResizePtyBody, responses((status = 200, description = "PTY session resized", body = crate::envelope::ApiEnvelope<PtySession>)), security(("api_key" = [])))]
pub(crate) async fn handle_resize_pty(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    body: Option<axum::Json<ResizePtyBody>>,
) -> impl IntoResponse {
    let _ = state;
    let body = body.map(|b| b.0).unwrap_or(ResizePtyBody {
        rows: 24,
        cols: 80,
    });
    match wf_api::pty::resize_pty_session(&id, body.rows as u16, body.cols as u16) {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/pty/{id}/connect", operation_id = "get_pty_connect", tag = "pty", params(IdPath, ConnectQuery), responses((status = 200, description = "PTY websocket stream", body = serde_json::Value)), security(("api_key" = [])))]
pub(crate) async fn handle_connect_pty(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(query): Query<ConnectQuery>,
    ws: WebSocketUpgrade,
    request: Request<Body>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_pty_socket(socket, state, id, query.cursor, request))
}

async fn handle_pty_socket(
    socket: WebSocket,
    state: ApiState,
    id: String,
    cursor: Option<i64>,
    request: Request<Body>,
) {
    if let Err(msg) = authenticate_connection(&state.config.auth, &request) {
        let mut socket = socket;
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 4001,
                reason: msg.into(),
            })))
            .await;
        return;
    }

    let (replay, cursor0, mut rx, _state) = match wf_api::pty::attach_pty_session(&id, cursor) {
        Ok(v) => v,
        Err(e) => {
            let mut socket = socket;
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: 4004,
                    reason: e.to_string().into(),
                })))
                .await;
            return;
        }
    };

    let (mut sender, mut receiver) = socket.split();

    for chunk in replay.as_bytes().chunks(65536) {
        let text = String::from_utf8_lossy(chunk).to_string();
        if sender.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }

    // One control frame carries the absolute cursor after replay so the client can
    // resume later; live output frames stay raw text and the client accumulates
    // its own cursor from frame lengths.
    let meta = serde_json::json!({"cursor": cursor0});
    let mut bytes = vec![0u8];
    bytes.extend_from_slice(meta.to_string().as_bytes());
    let _ = sender.send(Message::Binary(Bytes::from(bytes))).await;

    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    heartbeat.tick().await;
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if sender.send(Message::Ping(Bytes::new())).await.is_err() {
                        break;
                    }
                }
                text = rx.recv() => {
                    match text {
                        Ok(t) => {
                            if sender.send(Message::Text(t.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(t) => {
                if wf_api::pty::write_pty_session(&id, &t).is_err() {
                    break;
                }
            }
            Message::Binary(b) => {
                if wf_api::pty::write_pty_bytes(&id, &b).is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
}

fn authenticate_connection(auth: &AuthConfig, request: &Request<Body>) -> Result<(), String> {
    if !auth.enabled {
        return Ok(());
    }
    let key = request
        .headers()
        .get(&auth.header_name)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned)
        .or_else(|| {
            if auth.allow_query_param {
                crate::middleware::query_param(request.uri(), &auth.query_param_name)
            } else {
                None
            }
        });
    match key {
        None => Err(format!(
            "Authentication required. Provide API key via {} header or ?{}=<key> query parameter.",
            auth.header_name, auth.query_param_name
        )),
        Some(key) if !auth.api_keys.iter().any(|k| k == &key) => {
            Err("Invalid API key.".to_string())
        }
        Some(_) => Ok(()),
    }
}

