//! Webhook ingress gateway: external events enter the trigger system here.
//!
//! Each trigger template carrying a `webhook_spec` is mounted as
//! `POST /api/v1/hooks/{name}` (the template name; the spec `path` is the
//! documented external path operators map onto it). The gateway's job ends at
//! ingress: authenticate, route the execution, fold the body into event
//! metadata and publish a `NODE_CUSTOM_EVENT` through
//! `TriggerSource::translate_webhook_to_condition`. Matching, competition,
//! quota and action execution all stay in the listener, so webhooks add no
//! competition dimension.
//!
//! Execution routing: a body field `execution_id` names a live execution
//! (execution-scoped targets); without one only creation targets are served
//! (published without `execution_id` for the cold-start actions).

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Json;
use wf_core::registry::Registry;
use wf_types::trigger::{
    ScheduleTarget, WebhookAuth, WebhookSpec, FIRE_ID_METADATA_KEY, PRODUCER_SOURCE_METADATA_KEY,
    WEBHOOK_SPEC_METADATA_KEY,
};

use crate::envelope::{err, ok, ApiError};
use crate::router::ApiState;

/// Body keys never copied into event metadata.
const RESERVED_BODY_KEYS: [&str; 1] = ["execution_id"];

pub(crate) fn routes() -> axum::Router<ApiState> {
    axum::Router::new().route("/hooks/{name}", post(handle_webhook_fire))
}

#[derive(serde::Serialize)]
struct FireResponse {
    fired: bool,
    execution_id: Option<String>,
    fire_id: String,
}

async fn handle_webhook_fire(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let Some(template) = state.ctx.registries.trigger_templates.get(&name) else {
        return err::<serde_json::Value>(ApiError::validation(format!(
            "unknown webhook '{}'",
            name
        )))
        .into_response();
    };
    let spec_value = template
        .metadata
        .as_ref()
        .and_then(|meta| meta.get(WEBHOOK_SPEC_METADATA_KEY).cloned());
    let Some(spec_value) = spec_value else {
        return err::<serde_json::Value>(ApiError::validation(format!(
            "trigger '{}' is not a webhook",
            name
        )))
        .into_response();
    };
    let spec: WebhookSpec = match serde_json::from_value(spec_value) {
        Ok(spec) => spec,
        Err(e) => {
            return err::<serde_json::Value>(ApiError::validation(format!(
                "trigger '{}' has an unreadable webhook_spec: {}",
                name, e
            )))
            .into_response();
        }
    };
    if let Err(e) = spec.validate(&name) {
        return err::<serde_json::Value>(ApiError::validation(e)).into_response();
    }

    if !check_auth(&spec.auth, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "success": false,
                "data": null,
                "error": { "code": "UNAUTHORIZED", "message": "webhook authentication failed" },
            })),
        )
            .into_response();
    }

    let body_execution_id = body
        .get("execution_id")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let creating = spec.target.is_creating();
    // Execution routing: scoped targets need a live execution id; creation
    // targets publish execution-less for the cold-start actions.
    let execution_id = match (&spec.target, body_execution_id) {
        (ScheduleTarget::ExecutionScoped, Some(id)) if !id.is_empty() => Some(id),
        (ScheduleTarget::ExecutionScoped, _) => {
            return err::<serde_json::Value>(ApiError::validation(format!(
                "webhook '{}' targets a live execution but the body carries no execution_id",
                name
            )))
            .into_response();
        }
        (ScheduleTarget::Create { .. }, _) => None,
    };

    let fire_id = format!("{}:{}", name, wf_common::now());
    let mut metadata: HashMap<String, serde_json::Value> = HashMap::from([
        (
            PRODUCER_SOURCE_METADATA_KEY.to_string(),
            serde_json::json!("webhook"),
        ),
        ("fired_at".to_string(), serde_json::json!(wf_common::now())),
        ("path".to_string(), serde_json::json!(spec.path)),
        (
            FIRE_ID_METADATA_KEY.to_string(),
            serde_json::json!(fire_id.clone()),
        ),
    ]);
    let mapped = fold_body(&body, spec.input_mapping.as_deref());
    if creating {
        // The creation runners read their input from one well-known key
        // (shared with the scheduler contract).
        metadata.insert(
            wf_runtime::trigger_listener::TRIGGER_INPUT_METADATA_KEY.to_string(),
            mapped.clone(),
        );
    }
    if let serde_json::Value::Object(map) = mapped {
        for (key, value) in map {
            if RESERVED_BODY_KEYS.contains(&key.as_str()) {
                continue;
            }
            metadata.insert(key, value);
        }
    }

    let event = wf_types::events::BaseEvent {
        id: wf_common::generate_id(),
        r#type: wf_types::events::EventType::NodeCustomEvent,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: execution_id.clone().map(wf_types::Id::from),
        agent_loop_id: None,
        event_name: Some(name.clone()),
        metadata: Some(metadata),
    };
    if let Err(e) = state.ctx.event_bus.publish(event) {
        return err::<serde_json::Value>(ApiError::validation(format!(
            "webhook '{}' failed to publish: {}",
            name, e
        )))
        .into_response();
    }
    ok(FireResponse {
        fired: true,
        execution_id,
        fire_id,
    })
    .into_response()
}

/// Fold the request body into event metadata: the `input_mapping` whitelist
/// picks body keys, absent mapping copies the whole object body.
fn fold_body(body: &serde_json::Value, mapping: Option<&[String]>) -> serde_json::Value {
    let Some(object) = body.as_object() else {
        return serde_json::Value::Object(Default::default());
    };
    match mapping {
        Some(keys) => {
            let mut picked = serde_json::Map::new();
            for key in keys {
                if let Some(value) = object.get(key) {
                    picked.insert(key.clone(), value.clone());
                }
            }
            serde_json::Value::Object(picked)
        }
        None => body.clone(),
    }
}

/// Per-hook authentication: `None` passes, `Token` compares against the
/// `Authorization: Bearer` or `x-hook-token` header in constant time.
fn check_auth(auth: &WebhookAuth, headers: &HeaderMap) -> bool {
    match auth {
        WebhookAuth::None => true,
        WebhookAuth::Token { token } => {
            let presented = headers
                .get("x-hook-token")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
                .or_else(|| {
                    headers
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.strip_prefix("Bearer "))
                        .map(str::to_string)
                });
            presented
                .as_deref()
                .is_some_and(|p| constant_time_eq(p, token))
        }
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
