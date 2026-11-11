//! Execution list queries: agent, workflow and loop summaries with paging.

use wf_client::WfClient;

use crate::error::CliResult;

pub(crate) async fn list_agent_executions(
    client: &WfClient,
    limit: Option<usize>,
    offset: Option<usize>,
    status: Option<&str>,
    agent_id: &str,
    order: Option<&str>,
) -> CliResult<serde_json::Value> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(o) = offset {
        params.push(format!("offset={o}"));
    }
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    params.push(format!("agent_id={agent_id}"));
    if let Some(o) = order {
        params.push(format!("order={o}"));
    }
    let data: serde_json::Value = client
        .get_json(&format!(
            "{}?{}",
            crate::paths::AGENT_EXECUTIONS,
            params.join("&")
        ))
        .await?;
    Ok(paged_or_array(data, limit, offset, order))
}

pub(crate) async fn list_workflow_executions(
    client: &WfClient,
    limit: Option<usize>,
    offset: Option<usize>,
    status: Option<&str>,
    workflow: &str,
    order: Option<&str>,
) -> CliResult<serde_json::Value> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(o) = offset {
        params.push(format!("offset={o}"));
    }
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    params.push(format!("workflow_id={workflow}"));
    if let Some(o) = order {
        params.push(format!("order={o}"));
    }
    let data: serde_json::Value = client
        .get_json(&format!(
            "{}?{}",
            crate::paths::EXECUTIONS,
            params.join("&")
        ))
        .await?;
    Ok(paged_or_array(data, limit, offset, order))
}

pub(crate) async fn list_loop_summaries(
    client: &WfClient,
    limit: Option<usize>,
    offset: Option<usize>,
    status: Option<&str>,
    order: Option<&str>,
) -> CliResult<serde_json::Value> {
    let mut params: Vec<String> = Vec::new();
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(o) = offset {
        params.push(format!("offset={o}"));
    }
    let path = if params.is_empty() {
        crate::paths::AGENT_LOOPS_SUMMARIES.to_string()
    } else {
        format!(
            "{}?{}",
            crate::paths::AGENT_LOOPS_SUMMARIES,
            params.join("&")
        )
    };
    let data: serde_json::Value = client.get_json(&path).await?;
    Ok(paged_or_array(data, limit, offset, order))
}

/// Normalize list responses for display: page shells keep their shape while
/// bare arrays take client-side paging and order.
pub(crate) fn paged_or_array(
    data: serde_json::Value,
    limit: Option<usize>,
    offset: Option<usize>,
    order: Option<&str>,
) -> serde_json::Value {
    let items = data.get("items").and_then(|v| v.as_array()).cloned();
    match items {
        None => data,
        Some(mut entries) => {
            if order.is_some_and(|o| o.eq_ignore_ascii_case("asc")) {
                entries.reverse();
            }
            let off = offset.unwrap_or(0);
            let lim = limit.unwrap_or(usize::MAX);
            serde_json::Value::Array(entries.into_iter().skip(off).take(lim).collect())
        }
    }
}
