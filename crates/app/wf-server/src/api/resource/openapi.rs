//! Static OpenAPI-style discovery document for the HTTP transport surface.

use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::envelope::ok;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/openapi.json", get(handle_openapi))
}

async fn handle_openapi() -> impl IntoResponse {
    ok(serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Modular Agent Framework Server",
            "version": env!("CARGO_PKG_VERSION"),
            "apiVersion": "v1"
        },
        "paths": {
            "/workflows": { "get": { "summary": "List workflows" }, "post": { "summary": "Create workflow" } },
            "/workflows/{id}": { "get": { "summary": "Get workflow" }, "put": { "summary": "Update workflow" }, "delete": { "summary": "Delete workflow" } },
            "/workflows/{id}/export": { "get": { "summary": "Export a workflow as JSON or TOML" } },
            "/workflows/{id}/versions/increment": { "post": { "summary": "Create an automatically incremented workflow version" } },
            "/workflows/parse": { "post": { "summary": "Parse a workflow definition" } },
            "/workflows/transform": { "post": { "summary": "Transform workflow configuration into graph objects" } },
            "/workflows/validate": { "post": { "summary": "Validate a workflow" } },
            "/workflows/validate/node": { "post": { "summary": "Validate a workflow node" } },
            "/executions": { "get": { "summary": "List workflow executions" } },
            "/executions/{id}/error-analysis/stream": { "get": { "summary": "Stream execution error records as SSE" } },
            "/query": { "post": { "summary": "Query execution records" } },
            "/query/evaluate": { "post": { "summary": "Evaluate a filter expression against JSON" } },
            "/query/export": { "post": { "summary": "Export queried execution records" } },
            "/query/aggregate": { "post": { "summary": "Aggregate queried execution records" } },
            "/agents": { "get": { "summary": "List agent profiles" }, "post": { "summary": "Create an agent profile" } },
            "/agents/validate": { "post": { "summary": "Validate an agent definition" } },
            "/agent-loops": { "get": { "summary": "List agent loops" }, "post": { "summary": "Create an agent loop" } },
            "/agent-loops/{id}/status/transition": { "post": { "summary": "Transition an agent loop status" } },
            "/agent-loops/cleanup-completed": { "post": { "summary": "Remove terminated live agent loops" } },
            "/agent-loops/{id}/messages/dedupe": { "post": { "summary": "Remove duplicate agent messages" } },
            "/skills": { "get": { "summary": "List skills" } },
            "/skills/prompt": { "get": { "summary": "Build the enabled skills prompt" } },
            "/triggers": { "get": { "summary": "List workflow triggers" }, "post": { "summary": "Create a workflow trigger" } },
            "/triggers/{id}/enabled": { "get": { "summary": "Get workflow trigger enabled state" } },
            "/agent-triggers": { "get": { "summary": "List agent triggers" }, "post": { "summary": "Create an agent trigger" } },
            "/agent-triggers/{tid}/enabled": { "get": { "summary": "Get agent trigger enabled state" } },
            "/templates": { "get": { "summary": "List templates" } },
            "/tools": { "get": { "summary": "List tools" } },
            "/scripts": { "get": { "summary": "List scripts" }, "post": { "summary": "Create a script" } },
            "/variables": { "get": { "summary": "List variables" }, "post": { "summary": "Create a variable" } },
            "/events": { "get": { "summary": "List events" }, "delete": { "summary": "Clear events" } },
            "/events/stream": { "get": { "summary": "Stream execution events as SSE" } },
            "/ws": { "get": { "summary": "Open the execution events WebSocket" } }
        }
    }))
    .into_response()
}
