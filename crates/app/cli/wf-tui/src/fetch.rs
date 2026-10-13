//! Background data fetching for the TUI screens.
//!
//! Each screen kind maps to an async fetch function that produces a
//! [`ScreenData`] model. The fetch runs on a background task against
//! `wf-server`; the draw path only sees the cached result.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{
    short_id, CheckpointRow, DashboardData, ExecRow, ExecStatusFilter, InsightTable, InsightsData,
    LoopRow, ProfileRow, ScreenData, ScreenKind, SearchData, SearchRow, SettingsData, WorkflowRow,
};

/// Fetch data for the given screen kind.
pub async fn fetch_for(
    client: &WfClient,
    kind: ScreenKind,
    query: &str,
    filter: ExecStatusFilter,
) -> CliResult<ScreenData> {
    match kind {
        ScreenKind::Dashboard => fetch_dashboard(client).await,
        ScreenKind::Workflow => fetch_workflows(client).await,
        ScreenKind::Executions => fetch_executions(client, filter).await,
        ScreenKind::AgentLoops => fetch_agent_loops(client).await,
        ScreenKind::Insights => fetch_insights(client).await,
        ScreenKind::Checkpoints => fetch_checkpoints(client).await,
        ScreenKind::Search => fetch_search(client, query).await,
        ScreenKind::Settings => fetch_settings(client).await,
        ScreenKind::Interactive | ScreenKind::Help => Ok(ScreenData::None),
    }
}

fn items_of(data: &serde_json::Value) -> Vec<serde_json::Value> {
    data.get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| data.as_array().cloned())
        .unwrap_or_default()
}

async fn fetch_dashboard(client: &WfClient) -> CliResult<ScreenData> {
    let (workflows, executions, sessions, checkpoints) = tokio::join!(
        client.get_json::<serde_json::Value>("/api/v1/workflows/summaries"),
        client.get_json::<serde_json::Value>("/api/v1/agent-executions"),
        client.get_json::<serde_json::Value>("/api/v1/agent-loops/summaries"),
        client.get_json::<serde_json::Value>("/api/v1/checkpoints"),
    );
    let workflows = items_of(&workflows.unwrap_or_default());
    let mut executions = items_of(&executions.unwrap_or_default());
    let sessions = items_of(&sessions.unwrap_or_default());
    let checkpoints = items_of(&checkpoints.unwrap_or_default());

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
        recent,
    }))
}

async fn fetch_workflows(client: &WfClient) -> CliResult<ScreenData> {
    let data: serde_json::Value = client.get_json("/api/v1/workflows/summaries").await?;
    let rows = items_of(&data)
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
    Ok(ScreenData::Workflow(rows))
}

async fn fetch_executions(client: &WfClient, filter: ExecStatusFilter) -> CliResult<ScreenData> {
    let (executions, sessions) = tokio::join!(
        client.get_json::<serde_json::Value>("/api/v1/agent-executions"),
        client.get_json::<serde_json::Value>("/api/v1/agent-loops/summaries"),
    );
    let mut rows: Vec<ExecRow> = items_of(&executions.unwrap_or_default())
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
    rows.extend(
        items_of(&sessions.unwrap_or_default())
            .into_iter()
            .filter_map(|s| {
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
            }),
    );

    Ok(ScreenData::Executions(rows))
}

async fn fetch_agent_loops(client: &WfClient) -> CliResult<ScreenData> {
    let data: serde_json::Value = client.get_json("/api/v1/agent-loops/summaries").await?;
    let rows: Vec<LoopRow> = items_of(&data)
        .into_iter()
        .filter_map(|s| {
            Some(LoopRow {
                id: s.get("id")?.as_str()?.to_string(),
                status: s
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
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
        })
        .collect();
    Ok(ScreenData::AgentLoops(rows))
}

async fn fetch_insights(client: &WfClient) -> CliResult<ScreenData> {
    let (workflows, executions, sessions) = tokio::join!(
        client.get_json::<serde_json::Value>("/api/v1/workflows/summaries"),
        client.get_json::<serde_json::Value>("/api/v1/agent-executions"),
        client.get_json::<serde_json::Value>("/api/v1/agent-loops/summaries"),
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

async fn fetch_checkpoints(client: &WfClient) -> CliResult<ScreenData> {
    let data: serde_json::Value = client.get_json("/api/v1/checkpoints").await?;
    let rows = items_of(&data)
        .into_iter()
        .filter_map(|c| {
            Some(CheckpointRow {
                id: c.get("id")?.as_str()?.to_string(),
                entity: c
                    .get("entity_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                timestamp: format_ts(c.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0)),
                entity_type: c
                    .get("entity_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                checkpoint_type: c
                    .get("checkpoint_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                status: c
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect();
    Ok(ScreenData::Checkpoints(rows))
}

/// Render one execution checkpoint record as human-readable detail lines.
pub fn format_checkpoint_detail(checkpoint: &wf_types::storage::checkpoint::Checkpoint) -> String {
    let mut out = vec![
        format!("id: {}", checkpoint.id),
        format!(
            "entity: {} ({})",
            checkpoint.entity_id, checkpoint.entity_type
        ),
        format!(
            "type: {:?} · status: {:?}",
            checkpoint.checkpoint_type, checkpoint.status
        ),
        format!("timestamp: {}", format_ts(checkpoint.timestamp)),
    ];
    if let Some(position) = checkpoint.chain_position {
        out.push(format!("chain position: {position}"));
    }
    if let Some(size) = checkpoint.blob_size {
        out.push(format!("blob size: {size} bytes"));
    }
    if let Some(previous) = checkpoint.previous_checkpoint_id.as_ref() {
        out.push(format!("previous: {previous}"));
    }
    if let Some(base) = checkpoint.base_checkpoint_id.as_ref() {
        out.push(format!("base: {base}"));
    }
    if let Some(root) = checkpoint.chain_root_id.as_ref() {
        out.push(format!("chain root: {root}"));
    }
    if let Some(tags) = checkpoint.tags.as_ref() {
        out.push(format!("tags: {}", tags.join(", ")));
    }
    out.join("\n")
}

/// Render a checkpoint chain analysis as one line per checkpoint plus the
/// transition summary, for the read-only chain viewer.
pub fn format_checkpoint_chain(chain: &serde_json::Value) -> String {
    let execution_id = chain
        .get("execution_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let mut out = vec![
        format!("execution: {execution_id}"),
        format!(
            "checkpoints: {} · total elapsed: {}ms",
            chain
                .get("checkpoint_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            chain
                .get("total_elapsed")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        ),
    ];
    for checkpoint in chain
        .get("checkpoints")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
    {
        out.push(format!(
            "{} · {} · {} · {}",
            checkpoint.get("id").and_then(|v| v.as_str()).unwrap_or("?"),
            checkpoint
                .get("entity_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            checkpoint
                .get("checkpoint_type")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            checkpoint
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ));
    }
    for transition in chain
        .get("transitions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
    {
        out.push(format!(
            "{} -> {} · {}ms · {}",
            transition
                .get("from_checkpoint_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            transition
                .get("to_checkpoint_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            transition
                .get("elapsed")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            transition
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ));
    }
    out.join("\n")
}

async fn fetch_search(client: &WfClient, query: &str) -> CliResult<ScreenData> {
    let data: serde_json::Value = client
        .get_json(&format!(
            "/api/v1/search?q={query}&limit_per_type=20&limit=100"
        ))
        .await?;
    let rows = items_of(&data)
        .into_iter()
        .filter_map(|item| {
            Some(SearchRow {
                id: item.get("id")?.as_str()?.to_string(),
                kind: item
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                label: item
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                score: item.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32,
            })
        })
        .collect();
    Ok(ScreenData::Search(SearchData {
        query: query.to_string(),
        results: rows,
        total: data.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        truncated: data
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        running: false,
    }))
}

async fn fetch_settings(client: &WfClient) -> CliResult<ScreenData> {
    let (profiles, default_profile) = tokio::join!(
        client.get_json::<serde_json::Value>("/api/v1/llm/profiles"),
        client.get_json::<serde_json::Value>("/api/v1/llm/profiles/default"),
    );
    let default_id = default_profile
        .ok()
        .and_then(|v| v.get("id").and_then(|id| id.as_str()).map(str::to_string))
        .unwrap_or_default();
    let theme = crate::theme::load_theme_cache()
        .map(|t| format!("{:?}", t.kind))
        .unwrap_or_else(|| "unknown (not probed)".to_string());
    let rows = items_of(&profiles.unwrap_or_default())
        .into_iter()
        .filter_map(|p| {
            Some(ProfileRow {
                id: p.get("id")?.as_str()?.to_string(),
                name: p
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                model: p
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect();
    Ok(ScreenData::Settings(SettingsData {
        profiles: rows,
        default_profile: Some(default_id),
        theme,
    }))
}

/// Render an epoch timestamp; accepts seconds or milliseconds.
pub fn format_ts(ts: i64) -> String {
    if ts <= 0 {
        return "-".to_string();
    }
    let (secs, millis) = if ts > 10_000_000_000 {
        (ts / 1000, (ts % 1000) as u32)
    } else {
        (ts, 0u32)
    };
    match chrono::DateTime::from_timestamp(secs, millis * 1_000_000) {
        Some(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_formats_and_defaults() {
        assert_eq!(format_ts(0), "-");
        assert_eq!(format_ts(-1), "-");
        // Seconds and milliseconds of the same instant must agree.
        let secs = 1_700_000_000_i64;
        assert_eq!(format_ts(secs), format_ts(secs * 1000));
    }

    fn sample_checkpoint() -> wf_types::storage::checkpoint::Checkpoint {
        wf_types::storage::checkpoint::Checkpoint {
            id: "cp-1".to_string(),
            entity_type: "agent_loop".to_string(),
            entity_id: "loop-1".to_string(),
            parent_entity_id: None,
            checkpoint_type: wf_types::checkpoint::CheckpointType::Full,
            timestamp: 1_700_000_000,
            status: wf_types::checkpoint::CheckpointStatus::Completed,
            previous_checkpoint_id: None,
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: Some(2),
            blob_size: Some(128),
            tags: Some(vec!["t".to_string()]),
            custom_fields: None,
        }
    }

    #[test]
    fn checkpoint_detail_renders_identity_and_chain_links() {
        let text = format_checkpoint_detail(&sample_checkpoint());
        assert!(text.contains("cp-1"));
        assert!(text.contains("loop-1"));
        assert!(text.contains("agent_loop"));
        assert!(text.contains("chain position: 2"));
        assert!(text.contains("blob size: 128 bytes"));
    }

    #[test]
    fn checkpoint_chain_renders_counts_and_transitions() {
        let chain = serde_json::json!({
            "execution_id": "loop-1",
            "checkpoints": [{"id": "cp-1", "entity_id": "loop-1"}],
            "transitions": [],
            "total_elapsed": 0,
            "checkpoint_count": 1,
        });
        let text = format_checkpoint_chain(&chain);
        assert!(text.contains("execution: loop-1"));
        assert!(text.contains("cp-1"));
    }
}
