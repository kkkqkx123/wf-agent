//! List-screen fetchers: dashboard, workflows, executions, insights.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{
    short_id, DashboardData, ExecRow, ExecStatusFilter, InsightTable, InsightsData, ScreenData,
    WorkflowRow,
};

use super::common::{format_ts, items_of, page_of};

pub(super) async fn fetch_dashboard(client: &WfClient) -> CliResult<ScreenData> {
    // Escape hatch: dashboard merges list payloads for counts; field shapes stay dynamic.
    let (workflows, executions, sessions, checkpoints, store) = tokio::join!(
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::WORKFLOWS_SUMMARIES),
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::AGENT_EXECUTIONS),
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::AGENT_LOOPS_SUMMARIES),
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::CHECKPOINTS),
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::FILE_CHECKPOINT_GC_STATS),
    );
    let workflows = items_of(&workflows.unwrap_or_default());
    let mut executions = items_of(&executions.unwrap_or_default());
    let sessions = items_of(&sessions.unwrap_or_default());
    let checkpoints = items_of(&checkpoints.unwrap_or_default());
    let store = store.unwrap_or_default();

    let workflow_count = workflows.len();
    executions.extend(sessions.iter().cloned());
    let execution_count = executions.len();
    let running_count = executions
        .iter()
        .filter(|e| {
            e.get("status")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("running"))
        })
        .count();
    let checkpoint_count = checkpoints.len();

    let mut recent: Vec<String> = executions
        .iter()
        .take(5)
        .map(|e| {
            let id = e
                .get("execution_id")
                .or_else(|| e.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let status = e.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            let iteration = e
                .get("current_iteration")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!("exec {} · {} · iter {}", short_id(id), status, iteration)
        })
        .collect();
    recent.truncate(5);

    Ok(ScreenData::Dashboard(DashboardData {
        workflow_count,
        execution_count,
        running_count,
        checkpoint_count,
        store_bytes: store
            .get("store_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        store_objects: store
            .get("loose_objects")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        recent,
    }))
}

pub(super) async fn fetch_workflows(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    // Escape hatch: workflow summaries stay dynamic for list display.
    let data: serde_json::Value =
        client.get_json(wf_cli_shared::paths::WORKFLOWS_SUMMARIES).await?;
    let (items, has_more) = page_of(&data);
    let rows = items
        .into_iter()
        .filter_map(|w| {
            Some(WorkflowRow {
                id: w.get("id")?.as_str()?.to_string(),
                name: w
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                description: w
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                node_count: w.get("node_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            })
        })
        .collect();
    Ok((ScreenData::Workflow(rows), has_more))
}

pub(super) async fn fetch_executions(
    client: &WfClient,
    filter: ExecStatusFilter,
) -> CliResult<(ScreenData, bool)> {
    let (executions, sessions) = tokio::join!(
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::AGENT_EXECUTIONS),
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::AGENT_LOOPS_SUMMARIES),
    );
    let (exec_items, exec_more) = page_of(&executions.unwrap_or_default());
    let (session_items, session_more) = page_of(&sessions.unwrap_or_default());
    let mut rows: Vec<ExecRow> = exec_items
        .into_iter()
        .filter_map(|e| {
            let status = e.get("status").and_then(|v| v.as_str())?.to_string();
            if !filter.matches(&status) {
                return None;
            }
            Some(ExecRow {
                id: e
                    .get("execution_id")
                    .or_else(|| e.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                status,
                iteration: e
                    .get("current_iteration")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                tool_calls: e
                    .get("tool_call_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                started: format_ts(e.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0)),
            })
        })
        .collect();

    // Agent-loop (session) executions live in a second registry; merge them so
    // the screen shows one unified timeline.
    rows.extend(session_items.into_iter().filter_map(|s| {
        let status = s.get("status").and_then(|v| v.as_str())?.to_string();
        if !filter.matches(&status) {
            return None;
        }
        Some(ExecRow {
            id: s
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status,
            iteration: s
                .get("current_iteration")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            tool_calls: s
                .get("tool_call_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            started: format_ts(s.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0)),
        })
    }));

    Ok((ScreenData::Executions(rows), exec_more || session_more))
}

pub(super) async fn fetch_insights(client: &WfClient) -> CliResult<ScreenData> {
    // Escape hatch: insights aggregate dynamic list payloads.
    let (workflows, executions, sessions) = tokio::join!(
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::WORKFLOWS_SUMMARIES),
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::AGENT_EXECUTIONS),
        client.get_json::<serde_json::Value>(wf_cli_shared::paths::AGENT_LOOPS_SUMMARIES),
    );
    let workflows = items_of(&workflows.unwrap_or_default());
    let executions = items_of(&executions.unwrap_or_default());
    let sessions = items_of(&sessions.unwrap_or_default());

    // Status breakdown across both registries.
    let mut status_counts: Vec<(String, usize)> = Vec::new();
    let mut record = |status: &str| {
        if let Some(entry) = status_counts
            .iter_mut()
            .find(|(name, _)| name.eq_ignore_ascii_case(status))
        {
            entry.1 += 1;
        } else {
            status_counts.push((status.to_string(), 1));
        }
    };
    let mut total_tool_calls = 0u64;
    let mut total_iterations = 0u64;
    for e in executions.iter().chain(sessions.iter()) {
        if let Some(status) = e.get("status").and_then(|v| v.as_str()) {
            record(status);
        }
        total_tool_calls += e
            .get("tool_call_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        total_iterations += e
            .get("current_iteration")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
    }
    status_counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

    let overview = InsightTable {
        title: "Overview".to_string(),
        rows: vec![
            vec!["Metric".to_string(), "Value".to_string()],
            vec!["Workflows".to_string(), workflows.len().to_string()],
            vec![
                "Executions".to_string(),
                (executions.len() + sessions.len()).to_string(),
            ],
            vec!["Iterations".to_string(), total_iterations.to_string()],
            vec!["Tool calls".to_string(), total_tool_calls.to_string()],
        ],
    };
    let statuses = InsightTable {
        title: "Executions by status".to_string(),
        rows: std::iter::once(vec!["Status".to_string(), "Count".to_string()])
            .chain(
                status_counts
                    .into_iter()
                    .map(|(status, count)| vec![status, count.to_string()]),
            )
            .collect(),
    };

    Ok(ScreenData::Insights(InsightsData {
        tables: vec![overview, statuses],
    }))
}
