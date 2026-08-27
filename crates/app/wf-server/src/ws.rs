//! WebSocket endpoint `GET /api/v1/ws`: real-time execution event streaming
//! over one connection with per-execution subscriptions.
//!
//! - Server → client: `connection` (welcome), `execution_event`,
//!   `subscribed`, `unsubscribed`, `pong`, `error`
//! - Client → server: `subscribe`, `unsubscribe`, `ping` (JSON text frames)
//!
//! Each connection owns an outbound mpsc channel; every subscription spawns a
//! forwarder task on `wf_api::infra::events::subscribe` that exits when the
//! execution reaches a terminal event (the event subscription closes itself),
//! notifying the connection loop so the subscription is removed. Closing the
//! connection aborts all forwarder tasks. Auth: the connection is checked
//! against the auth config using the `api_key` query parameter (the auth
//! middleware excludes `/api/v1/ws`).

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::Request;
use axum::response::Response;
use axum::Router;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use wf_api::infra::events::subscribe;
use wf_api::{ApiContext, EventSubscriptionOptions};
use wf_common::time::{now, timestamp_to_iso};

use crate::middleware::AuthConfig;
use crate::router::ApiState;

/// Messages flowing from subscription forwarder tasks to the connection loop.
enum Outbound {
    /// Serialized JSON text to write to the socket.
    Text(String),
    /// A subscription reached its terminal event and was removed.
    SubEnded(String),
}

/// Route table for the WS endpoint; mounted under `/api/v1` by `router.rs`.
pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/ws", axum::routing::get(ws_handler))
}

async fn ws_handler(
    State(state): State<ApiState>,
    ws: WebSocketUpgrade,
    request: Request<Body>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, request))
}

async fn handle_socket(socket: WebSocket, state: ApiState, request: Request<Body>) {
    if let Err(message) = authenticate_connection(&state.config.auth, &request) {
        tracing::warn!(target: "wf_server", %message, "websocket connection rejected");
        let mut socket = socket;
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 4001,
                reason: message.into(),
            })))
            .await;
        return;
    }

    let client_id = format!("ws_{}", wf_common::generate_id());
    let (mut sender, mut receiver) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Outbound>(256);

    send_text(
        &out_tx,
        &json!({
            "type": "connection",
            "data": {
                "clientId": client_id,
                "message": "Connected to WF Agent Server"
            },
            "timestamp": timestamp_to_iso(now())
        }),
    )
    .await;

    tracing::debug!(target: "wf_server", %client_id, "websocket client connected");

    let mut subscriptions: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        handle_incoming(
                            text.to_string(),
                            &client_id,
                            &state.ctx,
                            &out_tx,
                            &mut subscriptions,
                        )
                        .await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        tracing::debug!(target: "wf_server", %client_id, error = %err, "websocket read error");
                        break;
                    }
                }
            }
            outgoing = out_rx.recv() => {
                match outgoing {
                    Some(Outbound::Text(text)) => {
                        if sender.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Outbound::SubEnded(execution_id)) => {
                        subscriptions.remove(&execution_id);
                    }
                    None => break,
                }
            }
        }
    }

    for (execution_id, handle) in subscriptions {
        handle.abort();
        tracing::debug!(target: "wf_server", %client_id, %execution_id, "websocket subscription aborted");
    }
    tracing::debug!(target: "wf_server", %client_id, "websocket client disconnected");
}

async fn handle_incoming(
    text: String,
    client_id: &str,
    ctx: &Arc<ApiContext>,
    out_tx: &mpsc::Sender<Outbound>,
    subscriptions: &mut HashMap<String, tokio::task::JoinHandle<()>>,
) {
    let message: Value = match serde_json::from_str(&text) {
        Ok(message) => message,
        Err(_) => {
            send_error(out_tx, "Invalid message format").await;
            return;
        }
    };

    let message_type = message["type"].as_str().unwrap_or_default();
    let execution_id = message["executionId"].as_str().map(ToOwned::to_owned);

    match message_type {
        "subscribe" => match execution_id {
            Some(execution_id) => {
                if subscriptions.contains_key(&execution_id) {
                    send_error(
                        out_tx,
                        format!("Already subscribed to execution [{execution_id}]"),
                    )
                    .await;
                    return;
                }
                let handle = tokio::spawn(forward_events(
                    Arc::clone(ctx),
                    execution_id.clone(),
                    out_tx.clone(),
                ));
                subscriptions.insert(execution_id.clone(), handle);
                send_text(
                    out_tx,
                    &json!({
                        "type": "subscribed",
                        "data": { "executionId": execution_id },
                        "timestamp": timestamp_to_iso(now())
                    }),
                )
                .await;
                tracing::debug!(target: "wf_server", %client_id, %execution_id, "websocket subscribed");
            }
            None => send_error(out_tx, "subscribe requires an executionId").await,
        },
        "unsubscribe" => match execution_id {
            Some(execution_id) => {
                if let Some(handle) = subscriptions.remove(&execution_id) {
                    handle.abort();
                }
                send_text(
                    out_tx,
                    &json!({
                        "type": "unsubscribed",
                        "data": { "executionId": execution_id },
                        "timestamp": timestamp_to_iso(now())
                    }),
                )
                .await;
                tracing::debug!(target: "wf_server", %client_id, %execution_id, "websocket unsubscribed");
            }
            None => send_error(out_tx, "unsubscribe requires an executionId").await,
        },
        "ping" => {
            send_text(
                out_tx,
                &json!({
                    "type": "pong",
                    "data": { "timestamp": now() },
                    "timestamp": timestamp_to_iso(now())
                }),
            )
            .await;
        }
        other => {
            send_error(out_tx, format!("Unknown message type: {other}")).await;
        }
    }
}

/// Forward matching execution events until the subscription closes (terminal
/// event), then report the removal to the connection loop.
async fn forward_events(
    ctx: Arc<ApiContext>,
    execution_id: String,
    out_tx: mpsc::Sender<Outbound>,
) {
    let mut sub = subscribe(&ctx, EventSubscriptionOptions::for_execution(&execution_id));
    while let Some(event) = sub.next().await {
        let payload = json!({
            "type": "execution_event",
            "executionId": execution_id,
            "eventType": event.r#type.as_str(),
            "data": event
                .metadata
                .clone()
                .map(|m| Value::Object(m.into_iter().collect()))
                .unwrap_or_else(|| json!({})),
            "timestamp": event.timestamp
        });
        if out_tx
            .send(Outbound::Text(payload.to_string()))
            .await
            .is_err()
        {
            return;
        }
    }
    let _ = out_tx.send(Outbound::SubEnded(execution_id)).await;
}

async fn send_text(tx: &mpsc::Sender<Outbound>, payload: &Value) {
    let _ = tx.send(Outbound::Text(payload.to_string())).await;
}

async fn send_error(tx: &mpsc::Sender<Outbound>, message: impl Into<String>) {
    send_text(
        tx,
        &json!({
            "type": "error",
            "data": { "message": message.into() },
            "timestamp": timestamp_to_iso(now())
        }),
    )
    .await;
}

/// API-key authentication via the `api_key` query parameter. No-op when
/// auth is disabled.
fn authenticate_connection(auth: &AuthConfig, request: &Request<Body>) -> Result<(), String> {
    if !auth.enabled {
        return Ok(());
    }
    let key = if auth.allow_query_param {
        crate::middleware::query_param(request.uri(), &auth.query_param_name)
    } else {
        None
    };
    match key {
        None => Err(format!(
            "Authentication required. Provide API key via ?{}={} query parameter.",
            auth.query_param_name, "<key>"
        )),
        Some(key) if !auth.api_keys.iter().any(|k| k == &key) => {
            Err("Invalid API key.".to_string())
        }
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
    use tokio_tungstenite::tungstenite::Error as WsError;
    use wf_metrics::MetricsRegistry;
    use wf_storage::context::StorageContext;

    use super::*;
    use crate::router::serve_full_with_config;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
            Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        ))
    }

    type WsStream = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn connect(addr: std::net::SocketAddr, query: &str) -> Result<WsStream, Box<WsError>> {
        let url = format!("ws://{addr}/api/v1/ws{query}");
        let (socket, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(Box::new)?;
        Ok(socket)
    }

    async fn read_text(socket: &mut WsStream) -> String {
        loop {
            match socket.next().await {
                Some(Ok(WsMessage::Text(text))) => return text.to_string(),
                Some(Ok(WsMessage::Ping(_))) | Some(Ok(WsMessage::Pong(_))) => continue,
                Some(Err(err)) => panic!("ws read error: {err}"),
                None => panic!("ws closed unexpectedly"),
                Some(Ok(_)) => panic!("unexpected ws frame"),
            }
        }
    }

    async fn start_server(ctx: Arc<ApiContext>) -> crate::server::ServerHandle {
        let registry = Arc::new(MetricsRegistry::new());
        crate::serve_full(registry, ctx, "127.0.0.1:0".parse().unwrap())
            .await
            .expect("server should bind")
    }

    fn make_event(
        execution_id: &str,
        event_type: wf_types::events::EventType,
    ) -> wf_types::events::BaseEvent {
        wf_types::events::BaseEvent {
            id: wf_common::generate_id(),
            r#type: event_type,
            timestamp: now(),
            workflow_id: None,
            execution_id: Some(execution_id.to_string()),
            agent_loop_id: None,

            event_name: None,
            metadata: Some([("nodeId".to_string(), json!("n1"))].into_iter().collect()),
        }
    }

    #[tokio::test]
    async fn ws_full_cycle() {
        let ctx = make_ctx();
        let handle = start_server(ctx.clone()).await;

        let mut socket = connect(handle.addr(), "").await.expect("ws connect");
        let welcome = read_text(&mut socket).await;
        let welcome: Value = serde_json::from_str(&welcome).unwrap();
        assert_eq!(welcome["type"], "connection");
        assert_eq!(welcome["data"]["message"], "Connected to WF Agent Server");
        let _client_id = welcome["data"]["clientId"].as_str().unwrap().to_string();

        socket
            .send(WsMessage::Text(
                r#"{"type":"subscribe","executionId":"exec-ws-1"}"#.into(),
            ))
            .await
            .unwrap();
        let subscribed = read_text(&mut socket).await;
        let subscribed: Value = serde_json::from_str(&subscribed).unwrap();
        assert_eq!(subscribed["type"], "subscribed");
        assert_eq!(subscribed["data"]["executionId"], "exec-ws-1");

        wf_api::infra::events::dispatch(
            &ctx,
            make_event("exec-ws-1", wf_types::events::EventType::NodeStarted),
        )
        .await
        .unwrap();
        let event_msg = read_text(&mut socket).await;
        let event_msg: Value = serde_json::from_str(&event_msg).unwrap();
        assert_eq!(event_msg["type"], "execution_event");
        assert_eq!(event_msg["executionId"], "exec-ws-1");
        assert_eq!(event_msg["eventType"], "NODE_STARTED");
        assert_eq!(event_msg["data"]["nodeId"], "n1");

        socket
            .send(WsMessage::Text(r#"{"type":"ping"}"#.into()))
            .await
            .unwrap();
        let pong = read_text(&mut socket).await;
        let pong: Value = serde_json::from_str(&pong).unwrap();
        assert_eq!(pong["type"], "pong");

        socket
            .send(WsMessage::Text(
                r#"{"type":"unsubscribe","executionId":"exec-ws-1"}"#.into(),
            ))
            .await
            .unwrap();
        let unsubscribed = read_text(&mut socket).await;
        let unsubscribed: Value = serde_json::from_str(&unsubscribed).unwrap();
        assert_eq!(unsubscribed["type"], "unsubscribed");

        socket.close(None).await.unwrap();
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn ws_terminal_event_ends_subscription() {
        let ctx = make_ctx();
        let handle = start_server(ctx.clone()).await;

        let mut socket = connect(handle.addr(), "").await.expect("ws connect");
        let _welcome = read_text(&mut socket).await;

        socket
            .send(WsMessage::Text(
                r#"{"type":"subscribe","executionId":"exec-terminal"}"#.into(),
            ))
            .await
            .unwrap();
        let _subscribed = read_text(&mut socket).await;

        wf_api::infra::events::dispatch(
            &ctx,
            make_event(
                "exec-terminal",
                wf_types::events::EventType::WorkflowExecutionCompleted,
            ),
        )
        .await
        .unwrap();
        let event_msg = read_text(&mut socket).await;
        let event_msg: Value = serde_json::from_str(&event_msg).unwrap();
        assert_eq!(event_msg["eventType"], "WORKFLOW_EXECUTION_COMPLETED");

        socket
            .send(WsMessage::Text(r#"{"type":"ping"}"#.into()))
            .await
            .unwrap();
        let pong = read_text(&mut socket).await;
        let pong: Value = serde_json::from_str(&pong).unwrap();
        assert_eq!(pong["type"], "pong");

        socket.close(None).await.unwrap();
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn ws_rejects_invalid_messages() {
        let ctx = make_ctx();
        let handle = start_server(ctx.clone()).await;

        let mut socket = connect(handle.addr(), "").await.expect("ws connect");
        let _welcome = read_text(&mut socket).await;

        socket
            .send(WsMessage::Text("not json".into()))
            .await
            .unwrap();
        let error_msg = read_text(&mut socket).await;
        let error_msg: Value = serde_json::from_str(&error_msg).unwrap();
        assert_eq!(error_msg["type"], "error");
        assert_eq!(error_msg["data"]["message"], "Invalid message format");

        socket
            .send(WsMessage::Text(r#"{"type":"bogus"}"#.into()))
            .await
            .unwrap();
        let error_msg = read_text(&mut socket).await;
        let error_msg: Value = serde_json::from_str(&error_msg).unwrap();
        assert_eq!(error_msg["type"], "error");
        assert_eq!(error_msg["data"]["message"], "Unknown message type: bogus");

        socket.close(None).await.unwrap();
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn ws_auth_accepts_valid_key_and_rejects_missing() {
        let ctx = make_ctx();
        let config = crate::middleware::ServerMiddlewareConfig {
            auth: crate::middleware::AuthConfig {
                enabled: true,
                api_keys: vec!["secret".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let registry = Arc::new(MetricsRegistry::new());
        let handle = serve_full_with_config(
            registry,
            ctx,
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(config),
        )
        .await
        .expect("server should bind");

        let mut socket = connect(handle.addr(), "?api_key=secret")
            .await
            .expect("ws connect with key");
        let _welcome = read_text(&mut socket).await;
        socket.close(None).await.unwrap();

        let mut socket = connect(handle.addr(), "")
            .await
            .expect("handshake should complete");
        match socket.next().await {
            Some(Ok(WsMessage::Close(Some(frame)))) => {
                assert_eq!(
                    frame.code,
                    tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Library(
                        4001
                    ),
                    "rejection must use close code 4001"
                );
            }
            other => panic!("expected close frame with code 4001, got {other:?}"),
        }
        handle.shutdown().await;
    }
}
