//! OpenAPI 3.1 to 3.0 normalization for progenitor codegen.
//!
//! The committed `wf-server/openapi.json` snapshot is OpenAPI 3.1 (emitted by
//! utoipa) but progenitor only accepts 3.0.x. This module rewrites an in-memory
//! `serde_json::Value` of the document into a 3.0-compatible shape:
//!
//! - top-level `openapi: "3.1.x"` becomes `"3.0.3"`;
//! - `type: ["T", "null"]` arrays collapse to `{ "type": "T", "nullable": true }`;
//! - operations whose success content is `text/event-stream` are removed; the
//!   hand-written SSE layer owns streaming and the generator must not emit
//!   broken JSON methods for them.
//!
//! Only `serde_json` is used so both `build.rs` (via `#[path]`) and this crate
//! share the exact same code.

use serde_json::Value;

/// Normalize a whole OpenAPI document value in place.
pub fn normalize_document(value: &mut Value) {
    normalize_version(value);
    normalize_schema_node(value);
    drop_sse_operations(value);
    drop_contentless_responses(value);
}

fn normalize_version(value: &mut Value) {
    if let Some(openapi) = value.get_mut("openapi") {
        if openapi.as_str().is_some_and(|v| v.starts_with("3.1.")) {
            *openapi = Value::String("3.0.3".to_string());
        }
    }
}

/// Recursively collapse 3.1 nullable idioms into 3.0 `nullable: true`.
fn normalize_schema_node(node: &mut Value) {
    match node {
        Value::Object(map) => {
            collapse_null_branches(map, "oneOf");
            collapse_null_branches(map, "anyOf");
            if let Some(Value::Array(types)) = map.get("type") {
                let mut non_null: Vec<String> = Vec::new();
                let mut nullable = false;
                for entry in types {
                    if let Some(name) = entry.as_str() {
                        if name == "null" {
                            nullable = true;
                        } else {
                            non_null.push(name.to_string());
                        }
                    }
                }
                if nullable || !non_null.is_empty() {
                    let primary = non_null.first().cloned().unwrap_or("string".to_string());
                    map.insert("type".to_string(), Value::String(primary));
                    if nullable {
                        map.insert("nullable".to_string(), Value::Bool(true));
                    }
                }
            }
            for value in map.values_mut() {
                normalize_schema_node(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_schema_node(item);
            }
        }
        _ => {}
    }
}

/// Drop `{"type": "null"}` branches from a `oneOf`/`anyOf` combinator (utoipa
/// encodes `Option<T>` this way in 3.1) and mark the schema nullable instead.
fn collapse_null_branches(map: &mut serde_json::Map<String, Value>, key: &str) {
    let Some(Value::Array(options)) = map.get_mut(key) else {
        return;
    };
    let before = options.len();
    options.retain(|branch| branch.get("type").and_then(|t| t.as_str()) != Some("null"));
    if options.len() != before && !options.is_empty() {
        map.entry("nullable".to_string())
            .or_insert(Value::Bool(true));
    }
}
/// Remove operations served as `text/event-stream` from `paths`.
///
/// They stay in the committed 3.1 snapshot for the TypeScript client; the Rust
/// client streams them through the hand-written SSE layer instead.
fn drop_sse_operations(value: &mut Value) {
    const METHODS: [&str; 8] = [
        "get", "put", "post", "delete", "options", "head", "patch", "trace",
    ];
    let Some(paths) = value.get_mut("paths").and_then(|p| p.as_object_mut()) else {
        return;
    };
    let mut empty_paths: Vec<String> = Vec::new();
    for (path, item) in paths.iter_mut() {
        let Some(obj) = item.as_object_mut() else {
            continue;
        };
        let mut drop_methods: Vec<String> = Vec::new();
        for method in METHODS {
            let Some(op) = obj.get(method) else {
                continue;
            };
            if operation_is_sse(op) {
                drop_methods.push(method.to_string());
            }
        }
        for method in drop_methods {
            obj.remove(&method);
        }
        if obj.is_empty() {
            empty_paths.push(path.clone());
        }
    }
    for path in empty_paths {
        paths.remove(&path);
    }
}

/// Drop responses without a `content` body (the middleware-level 401/403/429/
/// 503/504 injected by `GlobalErrorResponses`) when the operation keeps at
/// least one documented body.
///
/// Progenitor requires all error responses of one operation to share a single
/// type; mixing the content-less middleware responses with the typed `500`
/// `ErrorResponse` body trips `assertion failed: response_types.len() <= 1`.
/// Auth/rate-limit failures still surface at runtime through
/// `Error::UnexpectedResponse` and the hand-written status mapping.
fn drop_contentless_responses(value: &mut Value) {
    const METHODS: [&str; 8] = [
        "get", "put", "post", "delete", "options", "head", "patch", "trace",
    ];
    let Some(paths) = value.get_mut("paths").and_then(|p| p.as_object_mut()) else {
        return;
    };
    for item in paths.values_mut() {
        let Some(obj) = item.as_object_mut() else {
            continue;
        };
        for method in METHODS {
            let Some(op) = obj.get_mut(method) else {
                continue;
            };
            let Some(responses) = op.get_mut("responses").and_then(|r| r.as_object_mut()) else {
                continue;
            };
            let has_body = responses.values().any(|r| {
                r.get("content")
                    .and_then(|c| c.as_object())
                    .is_some_and(|c| !c.is_empty())
            });
            if !has_body {
                continue;
            }
            let bare: Vec<String> = responses
                .iter()
                .filter(|(_, r)| {
                    r.get("content")
                        .and_then(|c| c.as_object())
                        .is_none_or(|c| c.is_empty())
                })
                .map(|(k, _)| k.clone())
                .collect();
            for status in bare {
                responses.remove(&status);
            }
        }
    }
}

fn operation_is_sse(op: &Value) -> bool {
    let Some(responses) = op.get("responses") else {
        return false;
    };
    let Some(map) = responses.as_object() else {
        return false;
    };
    for response in map.values() {
        if let Some(content) = response.get("content").and_then(|c| c.as_object()) {
            if content.contains_key("text/event-stream") {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_is_rewritten() {
        let mut doc = json!({"openapi": "3.1.0"});
        normalize_document(&mut doc);
        assert_eq!(doc["openapi"], "3.0.3");
    }

    #[test]
    fn nullable_type_array_collapses() {
        let mut doc = json!({
            "openapi": "3.0.3",
            "components": {
                "schemas": {
                    "Row": {
                        "type": "object",
                        "properties": {
                            "name": {"type": ["string", "null"]},
                            "count": {"type": ["integer", "null"]}
                        }
                    }
                }
            }
        });
        normalize_document(&mut doc);
        assert_eq!(
            doc["components"]["schemas"]["Row"]["properties"]["name"],
            json!({"type": "string", "nullable": true})
        );
        assert_eq!(
            doc["components"]["schemas"]["Row"]["properties"]["count"],
            json!({"type": "integer", "nullable": true})
        );
    }

    #[test]
    fn nullable_oneof_branches_collapse() {
        let mut doc = json!({
            "error": {
                "oneOf": [{"type": "null"}, {"$ref": "#/components/schemas/ApiErrorBody"}]
            }
        });
        normalize_schema_node(&mut doc);
        assert_eq!(
            doc["error"]["oneOf"],
            json!([{"$ref": "#/components/schemas/ApiErrorBody"}])
        );
        assert_eq!(doc["error"]["nullable"], true);
    }

    #[test]
    fn contentless_error_responses_are_dropped() {
        let mut doc = json!({
            "paths": {
                "/health": {
                    "get": {
                        "responses": {
                            "200": {"content": {"application/json": {}}},
                            "401": {"description": "Unauthorized"},
                            "500": {"content": {"application/json": {}}}
                        }
                    }
                }
            }
        });
        normalize_document(&mut doc);
        let responses = &doc["paths"]["/health"]["get"]["responses"];
        assert!(responses.get("200").is_some());
        assert!(responses.get("500").is_some());
        assert!(responses.get("401").is_none());
    }

    #[test]
    fn sse_operations_are_dropped_but_json_kept() {
        let mut doc = json!({
            "openapi": "3.1.0",
            "paths": {
                "/api/v1/events/stream": {
                    "get": {
                        "operationId": "handle_event_stream",
                        "responses": {"200": {"content": {"text/event-stream": {}}}}
                    }
                },
                "/health": {
                    "get": {
                        "operationId": "handle_health",
                        "responses": {"200": {"content": {"application/json": {}}}}
                    }
                }
            }
        });
        normalize_document(&mut doc);
        assert!(doc["paths"].get("/api/v1/events/stream").is_none());
        assert!(doc["paths"]["/health"]["get"]["operationId"] == "handle_health");
    }
}
