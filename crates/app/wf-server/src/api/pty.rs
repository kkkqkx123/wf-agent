use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, Request};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
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
        .route(
            "/pty/{id}",
            get(handle_get_pty)
                .put(handle_update_pty)
                .delete(handle_delete_pty),
        )
        .route("/pty/{id}/connect", get(handle_connect_pty))
        .route("/pty/{id}/connect-token", post(handle_pty_connect_token))
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
    pub title: String,
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
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ConnectQuery {
    cursor: Option<i64>,
    /// Single-use connect ticket; when present the handshake authenticates
    /// with it instead of the API key.
    ticket: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct UpdatePtyBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    rows: Option<u32>,
    #[serde(default)]
    cols: Option<u32>,
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
        title: None,
    });
    match wf_api::pty::create_pty_session(
        &state.ctx,
        body.cwd,
        body.command,
        body.rows,
        body.cols,
        body.title,
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

#[utoipa::path(put, path = "/api/v1/pty/{id}", operation_id = "put_pty_id", tag = "pty", params(IdPath), request_body = UpdatePtyBody, responses((status = 200, description = "PTY session updated", body = crate::envelope::ApiEnvelope<PtySession>)), security(("api_key" = [])))]
pub(crate) async fn handle_update_pty(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    body: Option<axum::Json<UpdatePtyBody>>,
) -> impl IntoResponse {
    let _ = state;
    let body = body.map(|b| b.0).unwrap_or(UpdatePtyBody {
        title: None,
        rows: None,
        cols: None,
    });
    match wf_api::pty::update_pty_session(
        &id,
        body.title,
        body.rows.map(|v| v.min(u16::MAX as u32) as u16),
        body.cols.map(|v| v.min(u16::MAX as u32) as u16),
    ) {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(post, path = "/api/v1/pty/{id}/connect-token", operation_id = "post_pty_connect_token", tag = "pty", params(IdPath), responses((status = 200, description = "WebSocket connect ticket", body = crate::envelope::ApiEnvelope<crate::api::ticket::StreamTicket>), (status = 403, description = "Origin or marker check failed", body = crate::envelope::ErrorResponse), (status = 404, description = "PTY session not found", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_pty_connect_token(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !crate::api::ticket::ticket_request_authorized(&headers, &state.config.cors) {
        return crate::envelope::forbidden("invalid PTY connect-token request");
    }
    // The session must exist before a ticket is bound to it.
    if let Err(err) = wf_api::pty::get_pty_session(&id) {
        return error_response(err.into());
    }
    let scope = crate::api::ticket::TicketScope::Pty { id };
    let ticket = crate::api::ticket::issue(scope);
    ok(crate::api::ticket::StreamTicket {
        ticket,
        expires_in: crate::api::ticket::TICKET_TTL_SECS,
    })
    .into_response()
}

#[utoipa::path(get, path = "/api/v1/pty/{id}/connect", operation_id = "get_pty_connect", tag = "pty", params(IdPath, ConnectQuery), responses((status = 200, description = "PTY websocket stream", body = serde_json::Value)), security(("api_key" = [])))]
pub(crate) async fn handle_connect_pty(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(query): Query<ConnectQuery>,
    ws: WebSocketUpgrade,
    request: Request<Body>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| {
        handle_pty_socket(socket, state, id, query.cursor, query.ticket, request)
    })
}

/// A ticket (when present) is the credential: consume it and require an
/// allowed origin. Otherwise fall back to the API key handshake.
fn authorize_pty_handshake(
    state: &ApiState,
    id: &str,
    ticket: Option<&str>,
    request: &Request<Body>,
) -> Result<(), (u16, String)> {
    if let Some(ticket) = ticket {
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        let scope = crate::api::ticket::TicketScope::Pty { id: id.to_string() };
        if crate::middleware::origin_allowed(origin, &state.config.cors)
            && crate::api::ticket::consume(ticket, &scope)
        {
            return Ok(());
        }
        return Err((4003, "invalid PTY connect ticket".to_string()));
    }
    authenticate_connection(&state.config.auth, request).map_err(|msg| (4001, msg))
}

async fn handle_pty_socket(
    socket: WebSocket,
    state: ApiState,
    id: String,
    cursor: Option<i64>,
    ticket: Option<String>,
    request: Request<Body>,
) {
    match authorize_pty_handshake(&state, &id, ticket.as_deref(), &request) {
        Ok(()) => {}
        Err((code, reason)) => {
            let mut socket = socket;
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code,
                    reason: reason.into(),
                })))
                .await;
            return;
        }
    }

    let wf_api::pty::PtyAttachment {
        replay,
        cursor: cursor0,
        mut rx,
    } = match wf_api::pty::attach_pty_session(&id, cursor) {
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

    let mut offset = 0;
    while offset < replay.len() {
        let mut end = (offset + 65536).min(replay.len());
        while end < replay.len() && !replay.is_char_boundary(end) {
            end -= 1;
        }
        if end <= offset {
            break;
        }
        if sender
            .send(Message::Text(replay[offset..end].to_string().into()))
            .await
            .is_err()
        {
            return;
        }
        offset = end;
    }

    // One control frame carries the absolute cursor after replay so the client can
    // resume later; live output frames stay raw text and the client accumulates
    // its own cursor from frame lengths.
    let meta = serde_json::json!({ "cursor": cursor0 });
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
                frame = rx.recv() => {
                    match frame {
                        None => break,
                        Some(wf_api::pty::PtyFrame::Data(t)) => {
                            let len = t.len();
                            if sender.send(Message::Text(t.into())).await.is_err() {
                                break;
                            }
                            rx.credit(len);
                        }
                        Some(wf_api::pty::PtyFrame::Exited { exit_code, reason }) => {
                            let meta = serde_json::json!({
                                "event": "exited",
                                "exit_code": exit_code,
                                "reason": reason,
                            });
                            let mut bytes = vec![0u8];
                            bytes.extend_from_slice(meta.to_string().as_bytes());
                            let _ = sender.send(Message::Binary(Bytes::from(bytes))).await;
                            let _ = sender
                                .send(Message::Close(Some(CloseFrame {
                                    code: 4010,
                                    reason: format!("pty exited ({reason})").into(),
                                })))
                                .await;
                            break;
                        }
                        Some(wf_api::pty::PtyFrame::Overflow) => {
                            let _ = sender
                                .send(Message::Close(Some(CloseFrame {
                                    code: 4008,
                                    reason: "output backlog exceeded, resume with cursor".into(),
                                })))
                                .await;
                            break;
                        }
                    }
                }
            }
        }
        // `rx` drops here: the attachment guard detaches the subscriber.
        drop(rx);
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
