//! WebSocket endpoint `GET /api/v1/ws`: real-time event streaming over one
//! connection with per-dimension subscriptions.
//!
//! - Server → client: `connection` (welcome), `execution_event`,
//!   `agent_loop_event`, `workflow_event`, `subscribed`, `unsubscribed`,
//!   `pong`, `error`
//! - Client → server: `subscribe`, `unsubscribe`, `ping` (JSON text frames)
//!
//! `subscribe` takes one of `executionId`, `agentLoopId` or `workflowId`;
//! event payloads carry metadata only (fetch details over REST). The server
//! also sends protocol `Ping` frames every 30s so intermediaries keep the
//! connection alive.
//!
//! Each connection owns an outbound mpsc channel; every subscription spawns a
//! forwarder task on `wf_api::infra::events::subscribe` that exits when the
//! execution reaches a terminal event (the event subscription closes itself),
//! notifying the connection loop so the subscription is removed. Closing the
//! connection aborts all forwarder tasks. Auth: the connection is checked
//! against the auth config using the `api_key` query parameter (browsers
//! cannot set WebSocket headers, so the header path is unavailable here);
//! `/api/v1/ws` is subject to the same auth and rate-limit gates as the REST
//! surface.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::Request;
use axum::response::Response;
use axum::Router;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use wf_api::infra::events::subscribe;
use wf_api::{now, timestamp_to_iso};
use wf_api::{ApiContext, EventSubscriptionOptions};

use crate::middleware::AuthConfig;
use crate::router::ApiState;

/// Interval between protocol `Ping` frames keeping the connection alive.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

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

    let client_id = format!("ws_{}", wf_api::generate_id());
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
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    // Skip the immediate first tick; heartbeats start after one interval.
    heartbeat.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if sender.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
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

    for (key, handle) in subscriptions {
        handle.abort();
        tracing::debug!(target: "wf_server", %client_id, %key, "websocket subscription aborted");
    }
    tracing::debug!(target: "wf_server", %client_id, "websocket client disconnected");
}

/// One subscribable dimension: the map key, the event filter, the id field
/// echoed in payloads and the payload type name.
struct SubscriptionTarget {
    key: String,
    options: EventSubscriptionOptions,
    id_field: &'static str,
    id_value: String,
    event_type: &'static str,
}

fn subscription_target(message: &Value) -> Option<SubscriptionTarget> {
    if let Some(id) = message["executionId"].as_str() {
        return Some(SubscriptionTarget {
            key: format!("exec:{id}"),
            options: EventSubscriptionOptions::for_execution(id),
            id_field: "executionId",
            id_value: id.to_string(),
            event_type: "execution_event",
        });
    }
    if let Some(id) = message["agentLoopId"].as_str() {
        return Some(SubscriptionTarget {
            key: format!("loop:{id}"),
            options: EventSubscriptionOptions {
                agent_loop_id: Some(id.to_string()),
                ..Default::default()
            },
            id_field: "agentLoopId",
            id_value: id.to_string(),
            event_type: "agent_loop_event",
        });
    }
    if let Some(id) = message["workflowId"].as_str() {
        return Some(SubscriptionTarget {
            key: format!("flow:{id}"),
            options: EventSubscriptionOptions {
                workflow_id: Some(id.to_string()),
                ..Default::default()
            },
            id_field: "workflowId",
            id_value: id.to_string(),
            event_type: "workflow_event",
        });
    }
    None
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

    match message_type {
        "subscribe" => match subscription_target(&message) {
            Some(target) => {
                if subscriptions.contains_key(&target.key) {
                    send_error(out_tx, format!("Already subscribed to [{}]", target.key)).await;
                    return;
                }
                let handle = tokio::spawn(forward_events(
                    Arc::clone(ctx),
                    target.options,
                    target.key.clone(),
                    target.id_field,
                    target.id_value.clone(),
                    target.event_type,
                    out_tx.clone(),
                ));
                subscriptions.insert(target.key.clone(), handle);
                send_text(
                    out_tx,
                    &json!({
                        "type": "subscribed",
                        "data": id_payload(target.id_field, &target.id_value),
                        "timestamp": timestamp_to_iso(now())
                    }),
                )
                .await;
                tracing::debug!(target: "wf_server", %client_id, key = %target.key, "websocket subscribed");
            }
            None => {
                send_error(
                    out_tx,
                    "subscribe requires one of executionId, agentLoopId, workflowId",
                )
                .await;
            }
        },
        "unsubscribe" => match subscription_target(&message) {
            Some(target) => {
                if let Some(handle) = subscriptions.remove(&target.key) {
                    handle.abort();
                }
                send_text(
                    out_tx,
                    &json!({
                        "type": "unsubscribed",
                        "data": id_payload(target.id_field, &target.id_value),
                        "timestamp": timestamp_to_iso(now())
                    }),
                )
                .await;
                tracing::debug!(target: "wf_server", %client_id, key = %target.key, "websocket unsubscribed");
            }
            None => {
                send_error(
                    out_tx,
                    "unsubscribe requires one of executionId, agentLoopId, workflowId",
                )
                .await;
            }
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

/// Forward matching events until the subscription closes (terminal event),
/// then report the removal to the connection loop.
async fn forward_events(
    ctx: Arc<ApiContext>,
    options: EventSubscriptionOptions,
    key: String,
    id_field: &'static str,
    id_value: String,
    event_type: &'static str,
    out_tx: mpsc::Sender<Outbound>,
) {
    let mut sub = subscribe(&ctx, options);
    while let Some(event) = sub.next().await {
        let mut payload = serde_json::Map::with_capacity(5);
        payload.insert("type".to_string(), Value::String(event_type.to_string()));
        payload.insert(id_field.to_string(), Value::String(id_value.clone()));
        payload.insert(
            "eventType".to_string(),
            Value::String(event.r#type.as_str().to_string()),
        );
        payload.insert(
            "data".to_string(),
            event
                .metadata
                .clone()
                .map(|m| Value::Object(m.into_iter().collect()))
                .unwrap_or_else(|| json!({})),
        );
        payload.insert(
            "timestamp".to_string(),
            Value::Number(event.timestamp.into()),
        );
        if out_tx
            .send(Outbound::Text(Value::Object(payload).to_string()))
            .await
            .is_err()
        {
            return;
        }
    }
    let _ = out_tx.send(Outbound::SubEnded(key)).await;
}

/// Build the `data` object echoing the subscribed dimension id. `json!`
/// cannot use variable keys, hence the explicit map.
fn id_payload(id_field: &str, id_value: &str) -> Value {
    let mut map = serde_json::Map::with_capacity(1);
    map.insert(id_field.to_string(), Value::String(id_value.to_string()));
    Value::Object(map)
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

/// API-key authentication via header or the `api_key` query parameter.
/// No-op when auth is disabled. Mirrors the HTTP middleware (which also
/// gates this path); the query fallback exists because browsers cannot set
/// WebSocket headers.
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

#[cfg(test)]
mod tests {
    use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
    use tokio_tungstenite::tungstenite::Error as WsError;
    use wf_metrics::MetricsRegistry;
    use wf_storage::context::StorageContext;

    use super::*;
    use crate::router::serve_full_with_middleware;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
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
        serve_full_with_middleware(
            registry,
            ctx,
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(crate::middleware::ServerMiddlewareConfig::default()),
        )
        .await
        .expect("server should bind")
    }

    fn make_event(
        execution_id: &str,
        event_type: wf_types::events::EventType,
    ) -> wf_types::events::BaseEvent {
        wf_types::events::BaseEvent {
            id: wf_api::generate_id(),
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
    async fn ws_agent_loop_subscription() {
        let ctx = make_ctx();
        let handle = start_server(ctx.clone()).await;

        let mut socket = connect(handle.addr(), "").await.expect("ws connect");
        let _welcome = read_text(&mut socket).await;

        socket
            .send(WsMessage::Text(
                r#"{"type":"subscribe","agentLoopId":"loop-ws-1"}"#.into(),
            ))
            .await
            .unwrap();
        let subscribed = read_text(&mut socket).await;
        let subscribed: Value = serde_json::from_str(&subscribed).unwrap();
        assert_eq!(subscribed["type"], "subscribed");
        assert_eq!(subscribed["data"]["agentLoopId"], "loop-ws-1");

        let mut event = make_event("exec-ws-loop", wf_types::events::EventType::NodeStarted);
        event.agent_loop_id = Some("loop-ws-1".to_string());
        wf_api::infra::events::dispatch(&ctx, event).await.unwrap();
        let event_msg = read_text(&mut socket).await;
        let event_msg: Value = serde_json::from_str(&event_msg).unwrap();
        assert_eq!(event_msg["type"], "agent_loop_event");
        assert_eq!(event_msg["agentLoopId"], "loop-ws-1");
        assert_eq!(event_msg["eventType"], "NODE_STARTED");

        socket
            .send(WsMessage::Text(
                r#"{"type":"unsubscribe","agentLoopId":"loop-ws-1"}"#.into(),
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
        let handle = serve_full_with_middleware(
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

        // Without a key the path is guarded twice: the HTTP middleware
        // rejects the upgrade (handshake fails) or the WS handler closes
        // with 4001. Either outcome proves the guard.
        if let Ok(mut socket) = connect(handle.addr(), "").await {
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
        }
        handle.shutdown().await;
    }
}
