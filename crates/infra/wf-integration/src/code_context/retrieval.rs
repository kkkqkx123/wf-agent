use serde_json::Value;

use crate::transport::post_json;

/// Fallback result count when neither the call nor the policy names one.
pub const DEFAULT_RESULT_LIMIT: usize = 10;
/// Caller-side ceiling mirroring the service hard limit; the service
/// remains the final enforcer.
pub const SERVICE_RESULT_CAP: usize = 100;
/// Display guard for a single snippet; truncation is marked per result
/// instead of applied silently.
pub const MAX_SNIPPET_CHARS: usize = 2000;

pub(crate) fn truncate_snippet(text: &str) -> (String, bool) {
    if text.chars().count() <= MAX_SNIPPET_CHARS {
        return (text.to_string(), false);
    }
    (text.chars().take(MAX_SNIPPET_CHARS).collect(), true)
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
///
/// Only navigation-safe fields are exposed: file path, line range, snippet,
/// score plus informational type and source markers. Internal join keys
/// (numeric entity ids, segment ids) stay inside the service boundary.
/// The service already enforces the requested limit, so results are mapped
/// as returned without a second client-side truncation.
pub fn summarize_search_payload(payload: &Value, query: &str) -> Value {
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
        .map(|item| {
            let (snippet, truncated) = item
                .get("code_chunk")
                .and_then(|v| v.as_str())
                .map(truncate_snippet)
                .unwrap_or_default();
            serde_json::json!({
                "file_path": item.get("file_path"),
                "score": item.get("score"),
                "start_line": item.get("start_line"),
                "end_line": item.get("end_line"),
                "snippet": snippet,
                "truncated": truncated,
                "entity_type": item.get("entity_type"),
                "source": item.get("source"),
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
///
/// Snippets come from the raw source field; line ranges are kept so the
/// model can navigate by path plus line without any internal chunk key.
/// The service already enforces the requested count.
pub fn summarize_keyword_payload(payload: &Value, query: &str) -> Value {
    let result = payload.get("result");
    let items = result
        .and_then(|v| v.get("results"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let total = result
        .and_then(|v| v.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(items.len() as u64);
    let results: Vec<Value> = items
        .into_iter()
        .map(|item| {
            let (snippet, truncated) = item
                .get("snippet")
                .and_then(|v| v.as_str())
                .map(truncate_snippet)
                .unwrap_or_default();
            serde_json::json!({
                "file_path": item.get("file_path"),
                "score": item.get("score"),
                "title": item.get("title"),
                "start_line": item.get("start_line"),
                "end_line": item.get("end_line"),
                "snippet": snippet,
                "truncated": truncated,
            })
        })
        .collect();
    serde_json::json!({
        "query": query,
        "total": total,
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

/// Clamp a caller-supplied count to the policy budget. The policy default
/// applies when the call names nothing; the policy ceiling applies above
/// it; the service applies its own hard limit beyond that.
pub fn clamp_limit(raw: Option<u64>, default: usize, max: usize) -> usize {
    let default = default.max(1) as u64;
    let max = max.max(1) as u64;
    raw.unwrap_or(default).clamp(1, max) as usize
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
    Ok(summarize_search_payload(&payload, query))
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
    Ok(summarize_keyword_payload(&payload, query))
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
                    {"file_path": "a.rs", "score": 0.9, "start_line": 1, "end_line": 4, "code_chunk": "fn a() {}", "entity_type": "function", "source": "hybrid"},
                    {"file_path": "b.rs", "score": 0.5}
                ]
            }),
            "fold batch",
        );
        assert_eq!(summary["total"], 2);
        assert_eq!(summary["results"].as_array().unwrap().len(), 2);
        assert_eq!(
            summary["results"][0]["entity_type"],
            serde_json::json!("function")
        );
        assert_eq!(summary["results"][0]["source"], serde_json::json!("hybrid"));
        assert_eq!(summary["results"][0]["truncated"], serde_json::json!(false));

        let keywords = summarize_keyword_payload(
            &serde_json::json!({
                "result": {"total": 1, "results": [{"file_path": "a.rs", "snippet": "hit", "start_line": 3, "end_line": 5}]}
            }),
            "fold_batch",
        );
        assert_eq!(keywords["total"], 1);
        assert_eq!(keywords["results"][0]["snippet"], serde_json::json!("hit"));
        assert_eq!(keywords["results"][0]["start_line"], serde_json::json!(3));
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
        assert_eq!(clamp_limit(Some(99), 10, 100), 99);
        assert_eq!(clamp_limit(Some(999), 10, 100), 100);
        assert_eq!(clamp_limit(None, 10, 100), 10);
    }
}
