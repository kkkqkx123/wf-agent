//! Background data fetching for the TUI screens.
//!
//! Each screen kind maps to an async fetch function that produces a
//! [`ScreenData`] model. The fetch runs on a background task; the draw
//! path only sees the cached result.

use std::str::FromStr;

use wf_api::agent::agent_execution_registry::AgentExecutionFilter;
use wf_api::ApiContext;

use crate::error::CliResult;
use crate::screens::{
    short_id, CheckpointRow, DashboardData, ExecRow, ExecStatusFilter, InsightsData, InsightTable,
    LoopRow, ProfileRow, ScreenData, ScreenKind, SearchData, SearchRow, SettingsData, WorkflowRow,
};

/// Fetch data for the given screen kind.
pub async fn fetch_for(
    ctx: &ApiContext,
    kind: ScreenKind,
    query: &str,
    filter: ExecStatusFilter,
) -> CliResult<ScreenData> {
    match kind {
        ScreenKind::Dashboard => fetch_dashboard(ctx).await,
        ScreenKind::Workflow => fetch_workflows(ctx).await,
        ScreenKind::Executions => fetch_executions(ctx, filter).await,
        ScreenKind::AgentLoops => fetch_agent_loops(ctx).await,
        ScreenKind::Insights => fetch_insights(ctx).await,
        ScreenKind::Checkpoints => fetch_checkpoints(ctx).await,
        ScreenKind::Search => fetch_search(ctx, query).await,
        ScreenKind::Settings => fetch_settings(ctx).await,
        ScreenKind::Interactive | ScreenKind::Help => Ok(ScreenData::None),
    }
}

async fn fetch_dashboard(ctx: &ApiContext) -> CliResult<ScreenData> {
    let workflows = wf_api::workflow::summary::workflow_summaries(ctx, None).await?;
    let executions = wf_api::agent::agent_execution_registry::summaries(ctx, None).await?;
    let sessions = wf_api::agent::agent_loop_registry::summaries(ctx, None).await?;
    let checkpoints = wf_api::checkpoint::record::list_checkpoints(&ctx.storage, None).await?;

    let workflow_count = workflows.len();
    let execution_count = executions.len() + sessions.len();
    let running_count = executions
        .iter()
        .filter(|e| e.status.as_str().eq_ignore_ascii_case("running"))
        .count()
        + sessions
            .iter()
            .filter(|s| s.status.as_str().eq_ignore_ascii_case("running"))
            .count();
    let checkpoint_count = checkpoints.len();

    let mut recent: Vec<String> = executions
        .iter()
        .map(|e| {
            format!(
                "exec {} · {} · iter {}",
                short_id(&e.execution_id),
                e.status.as_str(),
                e.current_iteration
            )
        })
        .collect();
    recent.extend(sessions.iter().map(|s| {
        format!(
            "session {} · {} · iter {}",
            short_id(&s.id),
            s.status.as_str(),
            s.current_iteration
        )
    }));
    recent.truncate(5);

    Ok(ScreenData::Dashboard(DashboardData {
        workflow_count,
        execution_count,
        running_count,
        checkpoint_count,
        recent,
    }))
}

async fn fetch_workflows(ctx: &ApiContext) -> CliResult<ScreenData> {
    let workflows = wf_api::workflow::summary::workflow_summaries(ctx, None).await?;
    let rows = workflows
        .into_iter()
        .map(|w| WorkflowRow {
            id: w.id,
            name: w.name,
            description: w.description,
            node_count: w.node_count,
        })
        .collect();
    Ok(ScreenData::Workflow(rows))
}

async fn fetch_executions(ctx: &ApiContext, filter: ExecStatusFilter) -> CliResult<ScreenData> {
    let status = match filter {
        ExecStatusFilter::All => None,
        other => wf_types::ExecutionStatus::from_str(other.label()).ok(),
    };
    let query = AgentExecutionFilter {
        status,
        agent_id: None,
        parent_execution_id: None,
    };
    let mut rows: Vec<ExecRow> =
        wf_api::agent::agent_execution_registry::summaries(ctx, Some(&query))
            .await?
            .into_iter()
            .map(|e| ExecRow {
                id: e.execution_id,
                status: e.status.as_str().to_string(),
                iteration: e.current_iteration,
                tool_calls: e.tool_call_count,
                started: format_ts(e.start_time),
            })
            .filter(|row| filter.matches(&row.status))
            .collect();

    // Agent-loop (session) executions live in a second registry; merge them so
    // the screen shows one unified timeline.
    let sessions = wf_api::agent::agent_loop_registry::summaries(ctx, None).await?;
    rows.extend(sessions.into_iter().filter_map(|s| {
        let status = s.status.as_str().to_string();
        if !filter.matches(&status) {
            return None;
        }
        Some(ExecRow {
            id: s.id,
            status,
            iteration: s.current_iteration,
            tool_calls: s.tool_call_count,
            started: format_ts(s.start_time.unwrap_or(0)),
        })
    }));

    Ok(ScreenData::Executions(rows))
}

async fn fetch_agent_loops(ctx: &ApiContext) -> CliResult<ScreenData> {
    let sessions = wf_api::agent::agent_loop_registry::summaries(ctx, None).await?;
    let rows = sessions
        .into_iter()
        .map(|s| LoopRow {
            id: s.id,
            status: s.status.as_str().to_string(),
            iteration: s.current_iteration,
            tool_calls: s.tool_call_count,
            started: format_ts(s.start_time.unwrap_or(0)),
        })
        .collect();
    Ok(ScreenData::AgentLoops(rows))
}

async fn fetch_insights(ctx: &ApiContext) -> CliResult<ScreenData> {
    let workflows = wf_api::workflow::summary::workflow_summaries(ctx, None).await?;
    let executions = wf_api::agent::agent_execution_registry::summaries(ctx, None).await?;
    let sessions = wf_api::agent::agent_loop_registry::summaries(ctx, None).await?;

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
    for e in &executions {
        record(e.status.as_str());
        total_tool_calls += u64::from(e.tool_call_count);
        total_iterations += u64::from(e.current_iteration);
    }
    for s in &sessions {
        record(s.status.as_str());
        total_tool_calls += u64::from(s.tool_call_count);
        total_iterations += u64::from(s.current_iteration);
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

async fn fetch_checkpoints(ctx: &ApiContext) -> CliResult<ScreenData> {
    let rows = wf_api::checkpoint::record::list_checkpoints(&ctx.storage, None)
        .await?
        .into_iter()
        .map(|c| CheckpointRow {
            id: c.id,
            entity: c.entity_id,
            timestamp: format_ts(c.timestamp),
        })
        .collect();
    Ok(ScreenData::Checkpoints(rows))
}

async fn fetch_search(ctx: &ApiContext, query: &str) -> CliResult<ScreenData> {
    let options = wf_api::analysis::search::SearchOptions {
        types: None,
        limit_per_type: Some(20),
        limit_total: Some(100),
        cursor: None,
    };
    let result = wf_api::analysis::search::search(ctx, query, &options).await?;
    let rows = result
        .items
        .into_iter()
        .map(|item| SearchRow {
            id: item.id,
            kind: item.r#type,
            label: item.label,
            score: item.score,
        })
        .collect();
    Ok(ScreenData::Search(SearchData {
        query: query.to_string(),
        results: rows,
        total: result.total,
        truncated: result.truncated,
        running: false,
    }))
}

async fn fetch_settings(ctx: &ApiContext) -> CliResult<ScreenData> {
    let profiles = wf_api::llm::llm_profile::list(ctx).await?;
    let default_profile = wf_api::llm::llm_profile::get_default_id(ctx).await?;
    let theme = crate::theme::load_theme_cache()
        .map(|t| format!("{:?}", t.kind))
        .unwrap_or_else(|| "unknown (not probed)".to_string());
    let rows = profiles
        .into_iter()
        .map(|p| ProfileRow {
            id: p.id,
            name: p.name,
            model: p.model,
        })
        .collect();
    Ok(ScreenData::Settings(SettingsData {
        profiles: rows,
        default_profile,
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
}
