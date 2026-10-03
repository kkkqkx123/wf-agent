use serde_json::Value;

use crate::transport::post_json;

use super::retrieval::truncate_snippet;

/// Maximum files per symbols call enforced caller-side.
pub const MAX_SYMBOLS_PATHS: usize = 32;
/// Maximum children kept per symbol node when reducing the symbol tree.
pub const MAX_SYMBOL_CHILDREN: usize = 32;
/// Maximum recursion depth when reducing nested symbol children.
pub const MAX_SYMBOL_DEPTH: usize = 8;
/// Service endpoints for code navigation tools.
pub const SYMBOLS_ENDPOINT: &str = "/api/tools/symbols";
pub const REFERENCES_ENDPOINT: &str = "/api/tools/references";
pub const DEFINITION_ENDPOINT: &str = "/api/tools/definition";

/// Build the symbols request body over the service wire shape.
pub fn symbols_request_body(project_id: i64, paths: &[String]) -> Value {
    serde_json::json!({
        "project_id": project_id,
        "paths": paths,
    })
}

/// Build the references request body over the service wire shape.
pub fn references_request_body(
    project_id: i64,
    path: &str,
    line: usize,
    column: Option<usize>,
    symbol: Option<&str>,
) -> Value {
    let mut body = serde_json::json!({
        "project_id": project_id,
        "path": path,
        "line": line,
    });
    if let Some(column) = column {
        body["column"] = Value::from(column as u64);
    }
    if let Some(symbol) = symbol.filter(|s| !s.trim().is_empty()) {
        body["symbol"] = Value::String(symbol.to_string());
    }
    body
}

/// Build the definition request body over the service wire shape.
pub fn definition_request_body(
    project_id: i64,
    path: &str,
    line: usize,
    column: Option<usize>,
    symbol: Option<&str>,
    include_body: bool,
) -> Value {
    let mut body = references_request_body(project_id, path, line, column, symbol);
    body["include_body"] = Value::Bool(include_body);
    body
}

fn summarize_symbol(info: &Value, depth: usize) -> Value {
    let children = info
        .get("children")
        .and_then(|v| v.as_array())
        .map(|kids| {
            if depth >= MAX_SYMBOL_DEPTH {
                Vec::new()
            } else {
                kids.iter()
                    .take(MAX_SYMBOL_CHILDREN)
                    .map(|child| summarize_symbol(child, depth + 1))
                    .collect::<Vec<_>>()
            }
        })
        .unwrap_or_default();
    serde_json::json!({
        "name": info.get("name"),
        "kind": info.get("kind"),
        "line": info.get("line"),
        "end_line": info.get("end_line"),
        "detail": info.get("detail"),
        "children": children,
    })
}

/// Reduce a symbols response to the model-facing shape. Symbol entries
/// carry no internal keys, so the payload passes through with only
/// per-file success accounting preserved.
pub fn summarize_symbols_payload(payload: &Value) -> Result<Value, String> {
    let result = payload
        .get("result")
        .ok_or_else(|| "Symbols response missing 'result'".to_string())?;
    let files = result
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Symbols response missing 'result.results'".to_string())?;
    let files: Vec<Value> = files
        .iter()
        .map(|file| {
            let symbols = file
                .get("symbols")
                .and_then(|v| v.as_array())
                .map(|list| {
                    list.iter()
                        .map(|entry| summarize_symbol(entry, 0))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            serde_json::json!({
                "path": file.get("path"),
                "success": file.get("success"),
                "symbol_count": file.get("symbol_count"),
                "symbols": symbols,
                "error": file.get("error"),
            })
        })
        .collect();
    Ok(serde_json::json!({
        "success_count": result.get("success_count"),
        "fail_count": result.get("fail_count"),
        "files": files,
    }))
}

/// Reduce a references response to the model-facing shape. Numeric caller
/// entity keys stay inside the service boundary; only the caller name and
/// kind travel with the path plus line location.
pub fn summarize_references_payload(payload: &Value) -> Result<Value, String> {
    let result = payload
        .get("result")
        .ok_or_else(|| "References response missing 'result'".to_string())?;
    let groups = result
        .get("references")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "References response missing 'result.references'".to_string())?;
    let groups: Vec<Value> = groups
        .iter()
        .map(|group| {
            let locations = group
                .get("references")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let locations: Vec<Value> = locations
                .into_iter()
                .map(|loc| {
                    let (snippet, truncated) = loc
                        .get("snippet")
                        .and_then(|v| v.as_str())
                        .map(truncate_snippet)
                        .unwrap_or_default();
                    serde_json::json!({
                        "line": loc.get("line"),
                        "column": loc.get("column"),
                        "end_line": loc.get("end_line"),
                        "end_column": loc.get("end_column"),
                        "snippet": snippet,
                        "truncated": truncated,
                        "caller": {
                            "name": loc.get("caller_entity").and_then(|v| v.get("name")),
                            "kind": loc.get("caller_entity").and_then(|v| v.get("kind")),
                        },
                    })
                })
                .collect();
            serde_json::json!({
                "path": group.get("path"),
                "count": group.get("count"),
                "references": locations,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "symbol": result.get("symbol"),
        "total_count": result.get("total_count"),
        "file_count": result.get("file_count"),
        "references": groups,
    }))
}

/// Reduce a definition response to the model-facing shape. Numeric
/// definition keys stay inside the service boundary; the model navigates
/// by path plus line range with the returned code.
pub fn summarize_definition_payload(payload: &Value) -> Result<Value, String> {
    let result = payload
        .get("result")
        .ok_or_else(|| "Definition response missing 'result'".to_string())?;
    let definitions = result
        .get("definitions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Definition response missing 'result.definitions'".to_string())?;
    let definitions: Vec<Value> = definitions
        .iter()
        .map(|def| {
            let (code, truncated) = def
                .get("code")
                .and_then(|v| v.as_str())
                .map(truncate_snippet)
                .unwrap_or_default();
            serde_json::json!({
                "path": def.get("location").and_then(|v| v.get("path")),
                "line": def.get("location").and_then(|v| v.get("line")),
                "column": def.get("location").and_then(|v| v.get("column")),
                "end_line": def.get("location").and_then(|v| v.get("end_line")),
                "end_column": def.get("location").and_then(|v| v.get("end_column")),
                "name": def.get("name"),
                "kind": def.get("kind"),
                "code": code,
                "truncated": truncated,
                "signature": def.get("signature"),
            })
        })
        .collect();
    Ok(serde_json::json!({
        "symbol": result.get("symbol"),
        "definitions": definitions,
    }))
}

fn service_error(payload: &Value, endpoint: &str) -> String {
    payload
        .get("error")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{endpoint} reported failure without detail"))
}

/// Require a non-blank `path` parameter.
pub fn require_path(parameters: &Value) -> Result<String, String> {
    parameters
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Missing or invalid 'path' parameter".to_string())
}

/// Require a 1-based `line` parameter.
pub fn require_line(parameters: &Value) -> Result<usize, String> {
    parameters
        .get("line")
        .and_then(|v| v.as_u64())
        .filter(|n| *n >= 1)
        .map(|n| n as usize)
        .ok_or_else(|| "Missing or invalid 'line' parameter (1-based)".to_string())
}

/// Resolve an optional 1-based `column` parameter. An explicitly present
/// but invalid value is rejected rather than silently dropped.
pub fn optional_column(parameters: &Value) -> Result<Option<usize>, String> {
    match parameters.get("column") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let valid = value.as_u64().filter(|n| *n >= 1).map(|n| n as usize);
            match valid {
                Some(n) => Ok(Some(n)),
                None => Err("Invalid 'column' parameter (1-based)".to_string()),
            }
        }
    }
}

/// Resolve an optional `symbol` hint. Blank values count as absent.
pub fn optional_symbol(parameters: &Value) -> Option<String> {
    parameters
        .get("symbol")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

/// Resolve the file list from a `paths` array or a single `path` value.
pub fn resolve_paths(parameters: &Value) -> Result<Vec<String>, String> {
    if let Some(list) = parameters.get("paths").and_then(|v| v.as_array()) {
        let mut paths = Vec::with_capacity(list.len());
        for entry in list {
            let path = entry
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| "Each 'paths' entry must be a non-empty string".to_string())?;
            paths.push(path.to_string());
        }
        if paths.is_empty() {
            return Err("Missing or invalid 'paths' parameter".to_string());
        }
        if paths.len() > MAX_SYMBOLS_PATHS {
            return Err(format!(
                "Too many paths: got {}, want at most {MAX_SYMBOLS_PATHS}",
                paths.len()
            ));
        }
        return Ok(paths);
    }
    require_path(parameters).map(|path| vec![path])
}

/// Symbols for project files. The caller provides the HTTP client so
/// connections stay pooled across calls.
pub async fn symbols(
    client: &reqwest::Client,
    base_url: &str,
    timeout_ms: u64,
    project_id: i64,
    paths: &[String],
) -> Result<Value, String> {
    let body = symbols_request_body(project_id, paths);
    let payload = post_json(
        client,
        &format!("{base_url}{SYMBOLS_ENDPOINT}"),
        timeout_ms,
        &body,
    )
    .await?;
    if payload.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return Err(service_error(&payload, SYMBOLS_ENDPOINT));
    }
    summarize_symbols_payload(&payload)
}

/// Path plus line location for symbol navigation. Navigation runs on
/// file paths plus line numbers so no internal service key crosses the
/// caller boundary.
#[derive(Debug, Clone)]
pub struct LocationQuery {
    pub project_id: i64,
    pub path: String,
    pub line: usize,
    pub column: Option<usize>,
    pub symbol: Option<String>,
}

/// References of the symbol at a path plus line location.
pub async fn references(
    client: &reqwest::Client,
    base_url: &str,
    timeout_ms: u64,
    query: &LocationQuery,
) -> Result<Value, String> {
    let body = references_request_body(
        query.project_id,
        &query.path,
        query.line,
        query.column,
        query.symbol.as_deref(),
    );
    let payload = post_json(
        client,
        &format!("{base_url}{REFERENCES_ENDPOINT}"),
        timeout_ms,
        &body,
    )
    .await?;
    if payload.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return Err(service_error(&payload, REFERENCES_ENDPOINT));
    }
    summarize_references_payload(&payload)
}

/// Definition of the symbol at a path plus line location.
pub async fn definition(
    client: &reqwest::Client,
    base_url: &str,
    timeout_ms: u64,
    query: &LocationQuery,
    include_body: bool,
) -> Result<Value, String> {
    let body = definition_request_body(
        query.project_id,
        &query.path,
        query.line,
        query.column,
        query.symbol.as_deref(),
        include_body,
    );
    let payload = post_json(
        client,
        &format!("{base_url}{DEFINITION_ENDPOINT}"),
        timeout_ms,
        &body,
    )
    .await?;
    if payload.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return Err(service_error(&payload, DEFINITION_ENDPOINT));
    }
    summarize_definition_payload(&payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_shape_requests_and_responses() {
        let body = symbols_request_body(7, &["a.rs".to_string()]);
        assert_eq!(body["project_id"], 7);
        assert_eq!(body["paths"][0], "a.rs");

        let summary = summarize_symbols_payload(&serde_json::json!({
            "result": {
                "success_count": 1,
                "fail_count": 0,
                "results": [{
                    "path": "a.rs",
                    "success": true,
                    "symbol_count": 1,
                    "symbols": [{"name": "main", "kind": "Function", "line": 1, "end_line": 5}],
                }],
            },
        }))
        .expect("symbols payload summarizes");
        assert_eq!(summary["success_count"], 1);
        assert_eq!(summary["files"].as_array().unwrap().len(), 1);
        assert_eq!(summary["files"][0]["symbols"][0]["name"], "main");
    }

    #[test]
    fn symbols_rejects_missing_result_shape() {
        assert!(summarize_symbols_payload(&serde_json::json!({})).is_err());
        assert!(summarize_symbols_payload(&serde_json::json!({"result": {}})).is_err());
        assert!(summarize_references_payload(&serde_json::json!({"result": {}})).is_err());
        assert!(summarize_definition_payload(&serde_json::json!({"result": {}})).is_err());
    }

    #[test]
    fn references_and_definition_hide_numeric_keys() {
        let refs = summarize_references_payload(&serde_json::json!({
            "result": {
                "total_count": 1,
                "file_count": 1,
                "references": [{
                    "path": "b.rs",
                    "count": 1,
                    "references": [{
                        "line": 4,
                        "column": 2,
                        "end_line": 4,
                        "end_column": 8,
                        "snippet": "main()",
                        "caller_entity": {"name": "run", "kind": "Function", "entity_id": 9},
                    }],
                }],
            },
        }))
        .expect("references payload summarizes");
        let caller = &refs["references"][0]["references"][0]["caller"];
        assert_eq!(caller["name"], "run");
        assert!(caller.get("entity_id").is_none());

        let defs = summarize_definition_payload(&serde_json::json!({
            "result": {
                "definitions": [{
                    "location": {"path": "a.rs", "entity_id": 3, "line": 1, "end_line": 5},
                    "name": "main",
                    "kind": "Function",
                    "code": "fn main() {}",
                    "signature": "fn main()",
                }],
            },
        }))
        .expect("definition payload summarizes");
        assert_eq!(defs["definitions"][0]["path"], "a.rs");
        assert!(defs["definitions"][0].get("entity_id").is_none());
    }

    #[test]
    fn path_and_line_validation() {
        assert!(require_path(&serde_json::json!({"path": "a.rs"})).is_ok());
        assert!(require_path(&serde_json::json!({})).is_err());
        assert!(require_line(&serde_json::json!({"line": 2})).is_ok());
        assert!(require_line(&serde_json::json!({"line": 0})).is_err());
        assert!(optional_column(&serde_json::json!({}))
            .expect("absent column")
            .is_none());
        assert!(optional_column(&serde_json::json!({"column": 3}))
            .expect("valid column")
            .is_some());
        assert!(optional_column(&serde_json::json!({"column": 0})).is_err());
        assert_eq!(
            optional_symbol(&serde_json::json!({"symbol": "run"})).as_deref(),
            Some("run")
        );
        assert!(optional_symbol(&serde_json::json!({"symbol": "  "})).is_none());
        let paths =
            resolve_paths(&serde_json::json!({"paths": ["a.rs", "b.rs"]})).expect("paths resolve");
        assert_eq!(paths.len(), 2);
        assert!(resolve_paths(&serde_json::json!({"paths": []})).is_err());
    }
}
