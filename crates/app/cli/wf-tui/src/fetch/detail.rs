//! Detail fetchers for the execution and workflow viewers.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{short_id, ExecView, WorkflowView};

use super::common::{cap_detail_lines, format_ts, items_of, page_of, text_of, truncate_value};

/// Fetch detail lines for the selected execution in a non-overview view.
/// Loop-only dimensions report the server error for workflow executions so
/// the notice explains the gap instead of failing silently.
pub async fn fetch_exec_detail(
    client: &WfClient,
    exec_id: &str,
    view: ExecView,
) -> CliResult<Vec<String>> {
    let path = |suffix: &str| format!("/api/v1/agent-loops/{exec_id}{suffix}");
    let lines = match view {
        ExecView::Overview => Vec::new(),
        ExecView::Messages => {
            let data: serde_json::Value = client
                .get_json(&format!("{}?limit=50", path("/conversation")))
                .await?;
            let (items, _) = page_of(&data);
            items
                .iter()
                .map(|m| {
                    let role = text_of(m, &["role"]);
                    let content = text_of(m, &["content", "text"]);
                    let excerpt: String = content.chars().take(160).collect();
                    let excerpt = if content.chars().count() > 160 {
                        format!("{excerpt}...")
                    } else {
                        excerpt
                    };
                    format!("{role}: {excerpt}")
                })
                .collect()
        }
        ExecView::Tools => {
            let data: serde_json::Value = client
                .get_json(&format!("{}?limit=100", path("/iteration-history")))
                .await?;
            let (items, _) = page_of(&data);
            items
                .iter()
                .map(|item| {
                    let iteration = item
                        .get("iteration")
                        .and_then(|v| v.as_u64())
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| text_of(item, &["n"]));
                    let status = text_of(item, &["status"]);
                    let tools = item
                        .get("tool_calls")
                        .and_then(|v| v.as_u64())
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| text_of(item, &["tools"]));
                    format!("iter {iteration} · {status} · {tools} tools")
                })
                .collect()
        }
        ExecView::Timeline => {
            let data: serde_json::Value = client
                .get_json(&format!("{}?limit=100", path("/timeline")))
                .await?;
            let (items, _) = page_of(&data);
            items
                .iter()
                .map(|item| {
                    let when = item
                        .get("timestamp")
                        .and_then(|v| v.as_i64())
                        .map(format_ts)
                        .unwrap_or_default();
                    let event = text_of(item, &["event", "type", "kind"]);
                    let message = text_of(item, &["message", "summary"]);
                    format!("{when} · {event} · {message}")
                })
                .collect()
        }
        ExecView::Variables => {
            let loop_data: serde_json::Value = client
                .get_json(&format!("{}?limit=200", path("/variables")))
                .await?;
            let data = if items_of(&loop_data).is_empty() {
                client
                    .get_json(&format!("/api/v1/executions/{exec_id}/variables"))
                    .await
                    .unwrap_or(loop_data)
            } else {
                loop_data
            };
            let (items, _) = page_of(&data);
            items
                .iter()
                .map(|item| {
                    if let Some(pair) = item.as_array() {
                        let name = pair.first().and_then(|v| v.as_str()).unwrap_or("?");
                        let value = pair
                            .get(1)
                            .map(|v| truncate_value(&v.to_string(), 120))
                            .unwrap_or_default();
                        return format!("{name} = {value}");
                    }
                    let name = text_of(item, &["name", "key"]);
                    let value = truncate_value(&text_of(item, &["value"]), 120);
                    format!("{name} = {value}")
                })
                .collect()
        }
    };
    Ok(cap_detail_lines(lines))
}

/// Fetch detail lines for the selected workflow in a non-summary view.
pub async fn fetch_workflow_detail(
    client: &WfClient,
    workflow_id: &str,
    view: WorkflowView,
) -> CliResult<Vec<String>> {
    let lines = match view {
        WorkflowView::Summary => Vec::new(),
        WorkflowView::Nodes => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/workflows/{workflow_id}"))
                .await?;
            let nodes = data
                .get("nodes")
                .and_then(|v| v.as_array())
                .cloned()
                .or_else(|| {
                    data.get("definition")
                        .and_then(|d| d.get("nodes"))
                        .and_then(|v| v.as_array())
                        .cloned()
                })
                .unwrap_or_default();
            nodes
                .iter()
                .map(|n| {
                    let name = text_of(n, &["name", "id"]);
                    let kind = text_of(n, &["type", "kind"]);
                    format!("{name} · {kind}")
                })
                .collect()
        }
        WorkflowView::Runs => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions?workflow_id={workflow_id}"))
                .await?;
            let (items, _) = page_of(&data);
            items
                .iter()
                .map(|e| {
                    let id = text_of(e, &["id", "execution_id"]);
                    let status = text_of(e, &["status"]);
                    let started = e
                        .get("start_time")
                        .and_then(|v| v.as_i64())
                        .map(format_ts)
                        .unwrap_or_default();
                    format!("{} · {} · {}", short_id(&id), status, started)
                })
                .collect()
        }
    };
    Ok(cap_detail_lines(lines))
}
