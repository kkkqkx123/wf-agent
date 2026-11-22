//! HTTP handlers for detached execution keepalive.
//!
//! * `dispatch` submits a streaming agent run in the background and returns
//!   its execution id immediately. The run is decoupled from this connection.
//! * `attach` subscribes to a (possibly already finished) execution by id:
//!   it replays the retained event window, follows the live stream, and ends
//!   on the terminal event. Repeated reconnects never have to guess whether
//!   the run exists or already ended.
//!
//! Both share the [`StreamRuntime`] from `ApiState`, which enforces the
//! per-principal quota and tracks subscribers for idle reaping.

use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::detached::{principal_from_api_key, StreamRuntime};
use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;
use crate::sse::sse_response;

use super::{params_from_body, RunAgentLoopBody};

/// Response for a dispatched (background) run.
#[derive(Debug, serde::Serialize, ToSchema)]
pub(crate) struct DispatchView {
    pub(crate) execution_id: String,
    pub(crate) detached: bool,
}

/// Query for `attach`: an opaque cursor for reconnect replay.
#[derive(Debug, Deserialize)]
pub(crate) struct AttachQuery {
    /// Last event timestamp seen by the client; events older than it are
    /// skipped in the replay window.
    pub(crate) since: Option<String>,
}

/// Derive the principal bucket from the request's API key header.
fn principal_for(state: &ApiState, headers: &HeaderMap) -> String {
    let header_name = state.config.auth.header_name.as_str();
    let key = headers
        .get(header_name)
        .and_then(|value| value.to_str().ok());
    principal_from_api_key(key)
}

/// POST /api/v1/agent-loops/{id}/dispatch
///
/// Submit the run to the background, return the execution id immediately,
/// and keep the run alive independent of this request's connection.
#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/dispatch",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    request_body = RunAgentLoopBody,
    responses(
        (status = 200, description = "Run dispatched", body = crate::envelope::ApiEnvelope<DispatchView>),
        (status = 503, description = "Detached quota reached", body = crate::envelope::ErrorResponse),
        (status = 404, description = "Agent loop not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_dispatch_loop(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    headers: HeaderMap,
    Path(_path): Path<IdPath>,
    Json(body): Json<RunAgentLoopBody>,
) -> Response {
    // The `{id}` path segment selects the loop definition; the run config is
    // resolved from the body by `params_from_body`, matching run/stream.
    // Dispatch only has an effect with keep semantics. With cancel policy it
    // still runs in the background (the server owns the drain task), so the
    // endpoint remains usable; the policy only changes observer-detach rules.
    let principal = principal_for(&state, &headers);
    let execution_id = wf_types::Id::from(wf_common::generate_id());
    let id_str = execution_id.to_string();

    if let Err(reason) = state
        .stream
        .registry
        .admit(id_str.clone(), &principal, false)
    {
        crate::detached::emit_quota_rejection(&state.ctx);
        return crate::envelope::service_unavailable(reason);
    }
    crate::detached::emit_active_gauge(&state.stream.registry, &state.ctx);

    let mut params = match params_from_body(&state, &scope, body).await {
        Ok(params) => params,
        Err(e) => {
            state.stream.registry.remove(&id_str);
            crate::detached::emit_active_gauge(&state.stream.registry, &state.ctx);
            return error_response(e);
        }
    };
    params.agent_loop_id = Some(execution_id.clone());

    let stream = match wf_api::agent::agent_execution::stream_with_policy(
        state.ctx.clone(),
        params,
        wf_api::agent::agent_execution::StreamDetachPolicy::Keep,
    )
    .await
    {
        Ok(stream) => stream,
        Err(e) => {
            state.stream.registry.remove(&id_str);
            crate::detached::emit_active_gauge(&state.stream.registry, &state.ctx);
            return error_response(e);
        }
    };

    // Own the stream in a server task so the engine driver runs to terminal
    // regardless of any observer. The registry entry is removed on settle.
    let registry = state.stream.registry.clone();
    let metrics_ctx = state.ctx.clone();
    let reaped_id = id_str.clone();
    tokio::spawn(async move {
        let mut stream = stream;
        while let Some(event) = stream.next().await {
            let terminal = matches!(
                event,
                wf_api::infra::stream::ExecutionStreamEvent::Completed { .. }
                    | wf_api::infra::stream::ExecutionStreamEvent::Failed { .. }
                    | wf_api::infra::stream::ExecutionStreamEvent::Interrupted { .. }
            );
            registry.touch(&reaped_id);
            if terminal {
                break;
            }
        }
        // Settle terminal first (the engine already persisted it), then drop
        // the accounting record so the quota slot is released.
        registry.remove(&reaped_id);
        crate::detached::emit_active_gauge(&registry, &metrics_ctx);
    });

    ok(DispatchView {
        execution_id: id_str,
        detached: true,
    })
    .into_response()
}

/// GET /api/v1/agent-loops/{id}/attach
///
/// Replay the retained event window, then follow live events until the
/// terminal event. Works whether the run is still active or already finished.
#[utoipa::path(
    get,
    path = "/api/v1/agent-loops/{id}/attach",
    tag = "agent",
    params(
        ("id" = String, Path, description = "Execution ID"),
        ("since" = Option<String>, Query, description = "Opaque replay cursor")
    ),
    responses(
        (status = 200, description = "Server-sent events stream", content_type = "text/event-stream"),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_attach_loop(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<AttachQuery>,
) -> Response {
    let execution_id = path.id.clone();

    // Optimistic reaping: sweep idle detached runs before admitting a new
    // subscriber. Keeps orphaned accounting bounded without a dedicated
    // background timer that would need shutdown wiring.
    sweep_idle(&state);

    if state.stream.registry.contains(&execution_id) {
        state.stream.registry.add_subscriber(&execution_id);
    }

    let options =
        wf_api::infra::subscription::EventSubscriptionOptions::for_execution(execution_id.clone());
    let replay_query = wf_api::EventQueryOptions {
        execution_id: Some(execution_id.clone()),
        limit: Some(500),
        ..Default::default()
    };
    let since = query.since.as_deref().and_then(crate::ws::decode_sse_since);
    let ctx = state.ctx.clone();
    let backlog: Vec<wf_types::events::BaseEvent> =
        match wf_api::infra::events::history(&ctx, &replay_query).await {
            Ok(mut events) => {
                events.sort_by_key(|event| event.timestamp);
                events
                    .into_iter()
                    .filter(|event| since.map(|after| event.timestamp > after).unwrap_or(false))
                    .take(200)
                    .collect()
            }
            Err(_) => Vec::new(),
        };
    let sub = wf_api::infra::events::subscribe(&ctx, options);

    // A guard removes the subscriber on stream drop (disconnect), so idle
    // reaping can correctly see "no subscriber" afterwards.
    let registry = state.stream.registry.clone();
    let guard_id = execution_id.clone();
    let guard = SubscriberGuard {
        registry,
        id: guard_id,
    };
    let terminal_id = execution_id.clone();

    // Replay first, then live events. When a terminal event is seen, stop the
    // live subscription and emit one synthetic terminal marker so clients do
    // not need a separate status request to know the run ended.
    let backlog_stream = futures::stream::iter(backlog.into_iter().map(event_frame));
    let live = futures::stream::unfold(Some(sub), |maybe_sub| async move {
        let mut sub = maybe_sub?;
        match sub.next().await {
            Some(event) if is_terminal(&event) => Some((event_frame(event), None)),
            Some(event) => Some((event_frame(event), Some(sub))),
            None => None,
        }
    })
    .fuse();

    let terminal = futures::stream::once(async move {
        let _guard = guard;
        let payload = serde_json::json!({
            "type": "execution.terminal",
            "executionId": terminal_id,
        });
        Ok::<_, Infallible>(Bytes::from(format!("data: {payload}\n\n")))
    });

    sse_response(backlog_stream.chain(live).chain(terminal))
}

/// Serialize one event as an SSE data frame.
fn event_frame(event: wf_types::events::BaseEvent) -> Result<Bytes, Infallible> {
    let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
    Ok(Bytes::from(format!("data: {payload}\n\n")))
}

/// Whether an event marks a terminal execution state. Mirrors the wf-api
/// subscription helper (kept here because that helper is crate-private).
fn is_terminal(event: &wf_types::events::BaseEvent) -> bool {
    use wf_types::events::EventType::*;
    matches!(
        event.r#type,
        WorkflowExecutionCompleted
            | WorkflowExecutionFailed
            | WorkflowExecutionCancelled
            | AgentCompleted
            | AgentFailed
            | AgentCancelled
    )
}

fn sweep_idle(state: &ApiState) {
    let ctx = state.ctx.clone();
    let runtime: Arc<StreamRuntime> = state.stream.clone();
    tokio::spawn(async move {
        let settled = crate::detached::reap(&runtime.registry, wf_common::epoch_ms(), |id| {
            let ctx = ctx.clone();
            async move {
                let _ = ctx.cancel_execution(&id).await;
            }
        })
        .await;
        if settled > 0 {
            crate::detached::emit_reaped(&ctx, settled);
            crate::detached::emit_active_gauge(&runtime.registry, &ctx);
            tracing::info!(target: "wf_server", settled, "reaped idle detached runs");
        }
    });
}

struct SubscriberGuard {
    registry: crate::detached::DetachedRunRegistry,
    id: String,
}

impl Drop for SubscriberGuard {
    fn drop(&mut self) {
        self.registry.remove_subscriber(&self.id);
    }
}
