//! Background data fetching for the TUI screens.
//!
//! Each screen kind maps to an async fetch function that produces a
//! [`ScreenData`] model. The fetch runs on a background task against
//! `wf-server`; the draw path only sees the cached result.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{
    short_id, CatalogKind, CatalogRow, CheckpointRow, DashboardData, ExecRow, ExecStatusFilter,
    ExecView, GitRow, InsightTable, InsightsData, LocalSettings, ProfileRow, ScreenData,
    ScreenKind, SearchRow, SettingsData, WorkflowRow, WorkflowView,
};

/// Fetch data for the given screen kind. The flag reports server-side
/// truncation (`has_more`) so list titles stay honest about capped pages.
pub async fn fetch_for(
    client: &WfClient,
    kind: ScreenKind,
    filter: ExecStatusFilter,
) -> CliResult<(ScreenData, bool)> {
    match kind {
        ScreenKind::Dashboard => Ok((fetch_dashboard(client).await?, false)),
        ScreenKind::Workflow => fetch_workflows(client).await,
        ScreenKind::Executions => fetch_executions(client, filter).await,
        ScreenKind::Insights => Ok((fetch_insights(client).await?, false)),
        ScreenKind::Checkpoints => fetch_checkpoints(client).await,
        ScreenKind::Settings => Ok((fetch_settings(client).await?, false)),
        ScreenKind::Catalog => fetch_catalog(client).await,
        ScreenKind::Git => fetch_git(client).await,
        ScreenKind::Interactive => Ok((ScreenData::None, false)),
    }
}

fn items_of(data: &serde_json::Value) -> Vec<serde_json::Value> {
    data.get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| data.as_array().cloned())
        .unwrap_or_default()
}

/// Split a list response into items plus the server truncation flag.
/// Page responses carry `has_more`; cursor responses carry a next cursor.
fn page_of(data: &serde_json::Value) -> (Vec<serde_json::Value>, bool) {
    let has_more = data
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || data
            .get("next_cursor")
            .and_then(|v| v.as_str())
            .is_some_and(|c| !c.is_empty())
        || data
            .get("nextCursor")
            .and_then(|v| v.as_str())
            .is_some_and(|c| !c.is_empty());
    (items_of(data), has_more)
}

/// Client-side cap for fetched detail lines; the viewer shows the full
/// cached text and the pane windows into it.
const DETAIL_LINE_CAP: usize = 300;

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

async fn fetch_workflows(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let data: serde_json::Value = client.get_json("/api/v1/workflows/summaries").await?;
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

async fn fetch_executions(
    client: &WfClient,
    filter: ExecStatusFilter,
) -> CliResult<(ScreenData, bool)> {
    let (executions, sessions) = tokio::join!(
        client.get_json::<serde_json::Value>("/api/v1/agent-executions"),
        client.get_json::<serde_json::Value>("/api/v1/agent-loops/summaries"),
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

async fn fetch_checkpoints(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let data: serde_json::Value = client.get_json("/api/v1/checkpoints").await?;
    let (items, has_more) = page_of(&data);
    let rows = items
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
    Ok((ScreenData::Checkpoints(rows), has_more))
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

/// Fetch one page of global search hits for the search palette overlay.
pub(crate) async fn fetch_search_rows(
    client: &WfClient,
    query: &str,
) -> CliResult<(Vec<SearchRow>, usize, bool)> {
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
    let total = data.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let truncated = data
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok((rows, total, truncated))
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
                context_window: p
                    .get("context_window_size")
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok()),
            })
        })
        .collect();
    Ok(ScreenData::Settings(SettingsData {
        profiles: rows,
        default_profile: Some(default_id),
        theme,
        local: load_local_settings(),
    }))
}

/// First non-empty string field under any of `keys`, or empty.
fn text_of(value: &serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
            if !text.is_empty() {
                return text.to_string();
            }
        }
    }
    String::new()
}

/// Pre-rendered read-only viewer text for a list item.
fn pretty_detail(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Fetch the unified catalog: the four former screens load in parallel
/// and merge into one row list. Each source degrades independently so a
/// single failing endpoint still renders the other groups.
async fn fetch_catalog(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let (templates, resources, triggers, events) = tokio::join!(
        client.get_json::<serde_json::Value>("/api/v1/templates/library"),
        client.get_json::<serde_json::Value>("/api/v1/tools"),
        client.get_json::<serde_json::Value>("/api/v1/trigger-executions"),
        client.get_json::<serde_json::Value>("/api/v1/events"),
    );
    let mut rows: Vec<CatalogRow> = Vec::new();
    let mut has_more = false;
    let mut failures = 0u32;
    let mut first_err: Option<wf_client::ClientError> = None;
    let mut record_failure = |err: wf_client::ClientError| {
        failures += 1;
        if first_err.is_none() {
            first_err = Some(err);
        }
    };
    match templates {
        Ok(data) => {
            let (items, more) = page_of(&data);
            has_more |= more;
            rows.extend(items.into_iter().map(|item| {
                let id = text_of(&item, &["id"]);
                let name = text_of(&item, &["name", "title"]);
                let summary = text_of(&item, &["kind", "type", "category"]);
                CatalogRow {
                    kind: CatalogKind::Templates,
                    id: if id.is_empty() { name.clone() } else { id },
                    name,
                    status: String::new(),
                    summary,
                    detail: pretty_detail(&item),
                }
            }));
        }
        Err(err) => record_failure(err),
    }
    match resources {
        Ok(data) => {
            let (items, more) = page_of(&data);
            has_more |= more;
            rows.extend(items.into_iter().map(|item| {
                let id = text_of(&item, &["id", "name"]);
                let name = text_of(&item, &["name", "title", "display_name"]);
                let status = item
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .map(|enabled| {
                        if enabled {
                            "enabled".to_string()
                        } else {
                            "disabled".to_string()
                        }
                    })
                    .unwrap_or_else(|| text_of(&item, &["status"]));
                CatalogRow {
                    kind: CatalogKind::Resources,
                    id: if id.is_empty() { name.clone() } else { id },
                    name,
                    status,
                    summary: String::new(),
                    detail: pretty_detail(&item),
                }
            }));
        }
        Err(err) => record_failure(err),
    }
    match triggers {
        Ok(data) => {
            let (items, more) = page_of(&data);
            has_more |= more;
            rows.extend(items.into_iter().map(|item| {
                let id = text_of(&item, &["id"]);
                let name = text_of(&item, &["trigger_name", "name", "hook"]);
                let status = text_of(&item, &["status"]);
                CatalogRow {
                    kind: CatalogKind::Triggers,
                    id: if id.is_empty() { name.clone() } else { id },
                    name,
                    status,
                    summary: String::new(),
                    detail: pretty_detail(&item),
                }
            }));
        }
        Err(err) => record_failure(err),
    }
    match events {
        Ok(data) => {
            let (items, more) = page_of(&data);
            has_more |= more;
            rows.extend(items.into_iter().map(|item| {
                let id = text_of(&item, &["id"]);
                let kind = text_of(&item, &["event_type", "type", "kind"]);
                let summary = text_of(&item, &["message", "summary", "description"]);
                CatalogRow {
                    kind: CatalogKind::Events,
                    id,
                    name: kind,
                    status: String::new(),
                    summary,
                    detail: pretty_detail(&item),
                }
            }));
        }
        Err(err) => record_failure(err),
    }
    if rows.is_empty() && failures == 4 {
        if let Some(err) = first_err {
            return Err(crate::error::CliError::from(err));
        }
    }
    Ok((ScreenData::Catalog(rows), has_more))
}

async fn fetch_git(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let data: serde_json::Value = client.get_json("/api/v1/git/files").await?;
    let (items, has_more) = page_of(&data);
    let rows = items
        .into_iter()
        .map(|item| {
            let path = text_of(&item, &["path", "file"]);
            let status = text_of(&item, &["status", "state"]);
            GitRow {
                path,
                status,
                detail: pretty_detail(&item),
            }
        })
        .collect();
    Ok((ScreenData::Git(rows), has_more))
}

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

fn truncate_value(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if out.len() < text.len() {
        out.push_str("...");
    }
    out
}

fn cap_detail_lines(mut lines: Vec<String>) -> Vec<String> {
    if lines.len() > DETAIL_LINE_CAP {
        lines.truncate(DETAIL_LINE_CAP);
        lines.push(format!("... truncated to {DETAIL_LINE_CAP} lines"));
    }
    if lines.is_empty() {
        lines.push("(none)".to_string());
    }
    lines
}

/// Snapshot the local config file values shown on the settings screen.
fn load_local_settings() -> LocalSettings {
    let path = crate::app_config::config_file_path().map(|p| p.display().to_string());
    let tool_config = crate::tui::input_classify::load_tool_output_config();
    let copy_target = crate::tui::input_classify::load_copy_target();
    let mouse_capture = crate::tui::input_classify::load_mouse_opt_in();
    let session_scrollbar = crate::tui::input_classify::load_session_scrollbar_opt_in();
    let manager = path.as_ref().and_then(|_| {
        crate::app_config::ConfigManager::load(&crate::app_config::config_file_path().unwrap()).ok()
    });
    let appearance_theme = manager
        .as_ref()
        .and_then(|m| m.config().appearance.theme)
        .map(|v| format!("{v:?}").to_lowercase())
        .unwrap_or_else(|| "auto".to_string());
    let motion_performance = manager
        .as_ref()
        .and_then(|m| m.config().motion.performance)
        .map(|v| format!("{v:?}").to_lowercase())
        .unwrap_or_else(|| "auto".to_string());
    let notification_enabled = manager
        .as_ref()
        .and_then(|m| m.config().notifications.enabled);
    LocalSettings {
        config_path: path.unwrap_or_else(|| "unavailable".to_string()),
        mouse_capture,
        session_scrollbar,
        copy_target: format!("{copy_target:?}").to_lowercase(),
        tool_output_enabled: tool_config.enabled,
        tool_output_level: tool_config.level.label().to_string(),
        preview_max_lines: tool_config.preview_max_lines,
        preview_max_chars: tool_config.preview_max_chars,
        appearance_theme,
        motion_performance,
        notification_enabled,
    }
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
