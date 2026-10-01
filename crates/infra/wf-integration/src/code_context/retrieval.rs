use serde_json::Value;

use crate::transport::post_json;

/// Maximum results returned to the model per call.
const MAX_RESULTS: usize = 20;
/// Maximum snippet characters kept per result.
const MAX_SNIPPET_CHARS: usize = 2000;

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

/// Build the hybrid search request body (pure adapter over the service
/// wire shape; the handler only adds transport).
pub fn search_request_body(
    query: &str,
    project_id: i64,
    limit: usize,
    directory_prefix: Option<&str>,
) -> Value {
    let mut body = serde_json::json!({
        "project_id": project_id,
        "query": query,
        "limit": limit,
    });
    if let Some(prefix) = directory_prefix {
        body["directory_prefix"] = Value::String(prefix.to_string());
    }
    body
}

/// Build the keyword search request body.
pub fn keyword_request_body(query: &str, project_id: i64, top_n: usize) -> Value {
    serde_json::json!({
        "query": query,
        "top_n": top_n,
        "project_id": project_id,
    })
}

/// Reduce a hybrid search response to the model-facing shape.
pub fn summarize_search_payload(payload: &Value, query: &str, limit: usize) -> Value {
    let items = payload
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let total = payload
        .get("total")
        .and_then(|v| v.as_u64())
        .unwrap_or(items.len() as u64);
    let results: Vec<Value> = items
        .into_iter()
        .take(limit)
        .map(|item| {
            serde_json::json!({
                "file_path": item.get("file_path"),
                "score": item.get("score"),
                "start_line": item.get("start_line"),
                "end_line": item.get("end_line"),
                "snippet": item.get("code_chunk").and_then(|v| v.as_str()).map(|s| truncate_chars(s, MAX_SNIPPET_CHARS)),
            })
        })
        .collect();
    serde_json::json!({
        "query": query,
        "total": total,
        "results": results,
    })
}

/// Reduce a keyword search response to the model-facing shape.
pub fn summarize_keyword_payload(payload: &Value, query: &str, top_n: usize) -> Value {
    let items = payload
        .get("result")
        .and_then(|v| v.get("results"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let results: Vec<Value> = items
        .into_iter()
        .take(top_n)
        .map(|item| {
            serde_json::json!({
                "file_path": item.get("file_path"),
                "score": item.get("score"),
                "title": item.get("title"),
                "snippet": item.get("highlighted_snippet").and_then(|v| v.as_str()).map(|s| truncate_chars(s, MAX_SNIPPET_CHARS)),
            })
        })
        .collect();
    serde_json::json!({
        "query": query,
        "total": results.len(),
        "results": results,
    })
}

/// Require a non-blank `query` parameter.
pub fn require_query(parameters: &Value) -> Result<String, String> {
    parameters
        .get("query")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Missing or invalid 'query' parameter".to_string())
}

/// Resolve the project id from the call or the configured default.
pub fn resolve_project_id(parameters: &Value, default: Option<i64>) -> Result<i64, String> {
    parameters
        .get("project_id")
        .and_then(|v| v.as_i64())
        .or(default)
        .ok_or_else(|| "Missing 'project_id' and no default project is configured".to_string())
}

/// Clamp a caller-supplied limit to the model-facing maximum.
pub fn clamp_limit(raw: Option<u64>) -> usize {
    raw.unwrap_or(10).min(MAX_RESULTS as u64) as usize
}

/// Hybrid search over an indexed project. The caller provides the HTTP
/// client so connections stay pooled across calls; failures are plain
/// strings.
pub async fn search(
    client: &reqwest::Client,
    base_url: &str,
    timeout_ms: u64,
    query: &str,
    project_id: i64,
    limit: usize,
    directory_prefix: Option<&str>,
) -> Result<Value, String> {
    let body = search_request_body(query, project_id, limit, directory_prefix);
    let payload = post_json(client, &format!("{base_url}/api/search"), timeout_ms, &body).await?;
    if payload.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return Err("Search reported failure".to_string());
    }
    Ok(summarize_search_payload(&payload, query, limit))
}

/// BM25 keyword search. The caller provides the HTTP client so
/// connections stay pooled across calls.
pub async fn keyword_search(
    client: &reqwest::Client,
    base_url: &str,
    timeout_ms: u64,
    query: &str,
    project_id: i64,
    top_n: usize,
) -> Result<Value, String> {
    let body = keyword_request_body(query, project_id, top_n);
    let payload = post_json(
        client,
        &format!("{base_url}/api/tools/keyword-search"),
        timeout_ms,
        &body,
    )
    .await?;
    if payload.get("success").and_then(|v| v.as_bool()) == Some(false) {
        let reason = payload
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("keyword search reported failure");
        return Err(reason.to_string());
    }
    Ok(summarize_keyword_payload(&payload, query, top_n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapters_shape_requests_and_responses() {
        let body = search_request_body("fold batch", 7, 5, Some("src/"));
        assert_eq!(body["project_id"], 7);
        assert_eq!(body["limit"], 5);
        assert_eq!(body["directory_prefix"], "src/");

        let summary = summarize_search_payload(
            &serde_json::json!({
                "total": 2,
                "items": [
                    {"file_path": "a.rs", "score": 0.9, "start_line": 1, "end_line": 4, "code_chunk": "fn a() {}"},
                    {"file_path": "b.rs", "score": 0.5}
                ]
            }),
            "fold batch",
            10,
        );
        assert_eq!(summary["total"], 2);
        assert_eq!(summary["results"].as_array().unwrap().len(), 2);

        let keywords = summarize_keyword_payload(
            &serde_json::json!({
                "result": {"results": [{"file_path": "a.rs", "highlighted_snippet": "hit"}]}
            }),
            "fold_batch",
            10,
        );
        assert_eq!(keywords["total"], 1);
    }

    #[test]
    fn query_and_project_validation() {
        assert!(require_query(&serde_json::json!({ "query": "fold" })).is_ok());
        assert!(require_query(&serde_json::json!({})).is_err());
        assert_eq!(
            resolve_project_id(&serde_json::json!({}), Some(3)).unwrap(),
            3
        );
        assert!(resolve_project_id(&serde_json::json!({}), None).is_err());
        assert_eq!(clamp_limit(Some(99)), MAX_RESULTS);
    }
}
