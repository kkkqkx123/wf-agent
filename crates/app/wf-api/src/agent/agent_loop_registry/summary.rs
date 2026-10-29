//! Summary and lifecycle queries over agent loop executions.

use std::collections::BTreeMap;

use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::execution::ExecutionType;
use wf_types::ExecutionStatus;

use crate::agent::agent_loop_registry::types::{
    AgentLoopFilter, AgentLoopStatistics, AgentLoopSummary,
};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::workflow::execution_state::{parse_status, status_str};

/// Get agent loop summaries, optionally filtered.
pub async fn summaries(
    ctx: &ApiContext,
    filter: Option<&AgentLoopFilter>,
) -> ApiResult<Vec<AgentLoopSummary>> {
    let mut summaries = all_summaries(ctx).await;
    if let Some(filter) = filter {
        summaries.retain(|s| filter_matches(s, filter));
    }
    Ok(summaries)
}

/// Get a single agent loop summary by id.
pub async fn summary(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Option<AgentLoopSummary>> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let mut summary = live_summary(&entity).await;
        if let Ok(Some(meta)) = ctx.storage.agent_loop.load(agent_loop_id).await {
            if let Some(title) = normalized_stored_title(meta.title.as_deref()) {
                summary.title = Some(title);
            }
        }
        return Ok(Some(summary));
    }
    if let Some(record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        let mut summary = persisted_summary(&record);
        if let Ok(Some(meta)) = ctx.storage.agent_loop.load(agent_loop_id).await {
            if let Some(title) = normalized_stored_title(meta.title.as_deref()) {
                summary.title = Some(title);
            }
        }
        return Ok(Some(summary));
    }
    if let Some(meta) = ctx.storage.agent_loop.load(agent_loop_id).await? {
        let Some(summary) = meta_summary(&meta) else {
            return Err(ApiError::Conflict(format!(
                "record [{}] carries unknown status [{}]",
                meta.id, meta.status
            )));
        };
        return Ok(Some(summary));
    }
    Ok(None)
}

/// Longest stored session title, in characters. Longer input is truncated
/// rather than rejected so rename stays total.
const MAX_TITLE_CHARS: usize = 80;

/// Normalize a stored title: trim, collapse to one line and drop empties.
/// Returns `None` for missing or blank titles so callers fall back to the id.
fn normalized_stored_title(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let single: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    if single.is_empty() {
        return None;
    }
    let truncated: String = single.chars().take(MAX_TITLE_CHARS).collect();
    Some(truncated)
}

/// Validate and normalize a rename payload. Blank titles are rejected;
/// longer titles are truncated to [`MAX_TITLE_CHARS`] characters.
pub fn normalize_title(raw: &str) -> ApiResult<String> {
    let Some(title) = normalized_stored_title(Some(raw)) else {
        return Err(ApiError::Validation(
            "title must not be blank".to_string(),
        ));
    };
    Ok(title)
}

/// Derive a display title from a conversation: the first non-empty user
/// message, single-lined and truncated. Assistant-only or empty histories
/// yield `None` so the id stays the fallback label.
pub fn derive_title_from_messages(messages: &[wf_types::message::Message]) -> Option<String> {
    for message in messages {
        if message.role != wf_types::message::MessageRole::User {
            continue;
        }
        let text = match &message.content {
            wf_types::message::MessageContentValue::Text(text) => text.clone(),
            wf_types::message::MessageContentValue::Rich(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    wf_types::message::MessageContent::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" "),
        };
        let Some(title) = normalized_stored_title(Some(&text)) else {
            continue;
        };
        // First-message titles read better short: keep one preview line.
        let preview: String = title.chars().take(60).collect();
        return Some(preview);
    }
    None
}

/// Record the derived title for `agent_loop_id` when no explicit title
/// exists yet. Called after conversation persist so summaries can show a
/// human label without per-summary message reads. Never overwrites a rename.
pub async fn ensure_loop_title(
    ctx: &ApiContext,
    agent_loop_id: &str,
    messages: &[wf_types::message::Message],
) {
    let Ok(existing) = ctx.storage.agent_loop.load(agent_loop_id).await else {
        return;
    };
    if let Some(meta) = existing {
        if normalized_stored_title(meta.title.as_deref()).is_some() {
            return;
        }
        let Some(title) = derive_title_from_messages(messages) else {
            return;
        };
        let mut updated = meta;
        updated.title = Some(title);
        updated.updated_at = wf_common::now();
        if let Err(err) = ctx.storage.agent_loop.save(&updated).await {
            tracing::warn!(
                target: "wf_api",
                agent_loop_id = %agent_loop_id,
                error = %err,
                "failed to persist derived session title"
            );
        }
        return;
    }
    let Some(title) = derive_title_from_messages(messages) else {
        return;
    };
    let now = wf_common::now();
    let status = match ctx.storage.agent_execution.load(agent_loop_id).await {
        Ok(Some(record)) => crate::workflow::execution_state::status_str(&record.status).to_string(),
        _ => "completed".to_string(),
    };
    let meta = wf_types::AgentLoopStorageMetadata {
        id: wf_types::Id::from(agent_loop_id.to_string()),
        definition_id: wf_types::Id::from(String::new()),
        status,
        current_iteration: 0,
        started_at: messages.first().map(|m| m.timestamp).unwrap_or(now),
        updated_at: now,
        title: Some(title),
    };
    if let Err(err) = ctx.storage.agent_loop.save(&meta).await {
        tracing::warn!(
            target: "wf_api",
            agent_loop_id = %agent_loop_id,
            error = %err,
            "failed to persist derived session title"
        );
    }
}

/// Rename a session: validate the title, then upsert the loop metadata.
/// Unknown loops surface as not-found so clients can fallback to local state.
pub async fn update_title(ctx: &ApiContext, agent_loop_id: &str, raw: &str) -> ApiResult<()> {
    let title = normalize_title(raw)?;
    if let Some(mut meta) = ctx.storage.agent_loop.load(agent_loop_id).await? {
        meta.title = Some(title);
        meta.updated_at = wf_common::now();
        ctx.storage.agent_loop.save(&meta).await?;
        return Ok(());
    }
    if let Some(record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        let meta = wf_types::AgentLoopStorageMetadata {
            id: wf_types::Id::from(agent_loop_id.to_string()),
            definition_id: record.definition_id.clone(),
            status: crate::workflow::execution_state::status_str(&record.status).to_string(),
            current_iteration: record.current_iteration,
            started_at: record.started_at,
            updated_at: wf_common::now(),
            title: Some(title),
        };
        ctx.storage.agent_loop.save(&meta).await?;
        return Ok(());
    }
    // A live-only loop has no persisted record yet: create a metadata shell
    // so the rename survives the next persist without touching the entity.
    if ctx.agent_loop(agent_loop_id).is_some() {
        let now = wf_common::now();
        let meta = wf_types::AgentLoopStorageMetadata {
            id: wf_types::Id::from(agent_loop_id.to_string()),
            definition_id: wf_types::Id::from(String::new()),
            status: "running".to_string(),
            current_iteration: 0,
            started_at: now,
            updated_at: now,
            title: Some(title),
        };
        ctx.storage.agent_loop.save(&meta).await?;
        return Ok(());
    }
    Err(ApiError::execution_not_found(agent_loop_id))
}

/// List agent loops matching a status.
pub async fn list_by_status(
    ctx: &ApiContext,
    status: ExecutionStatus,
) -> ApiResult<Vec<AgentLoopSummary>> {
    summaries(
        ctx,
        Some(&AgentLoopFilter {
            status: Some(status),
            ..AgentLoopFilter::default()
        }),
    )
    .await
}

/// Update the status of an agent loop through the entity's lifecycle
/// transitions when the loop is live (pause / resume / stop); otherwise
/// rewrites the persisted `AgentLoopStorageMetadata` status.
pub async fn update_status(
    ctx: &ApiContext,
    agent_loop_id: &str,
    status: ExecutionStatus,
) -> ApiResult<()> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        match status {
            ExecutionStatus::Running => entity.resume().await?,
            ExecutionStatus::Paused => entity.pause().await?,
            ExecutionStatus::Cancelled | ExecutionStatus::Stopped => entity.stop().await?,
            _ => {
                // Direct status set is not expressible through the entity
                // state machine; fall back to the persisted record.
                update_persisted_status(ctx, agent_loop_id, status).await?;
            }
        }
        return Ok(());
    }
    update_persisted_status(ctx, agent_loop_id, status).await
}

/// Get the status of an agent loop, or `None` when it does not exist.
pub async fn get_status(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<ExecutionStatus>> {
    Ok(summary(ctx, agent_loop_id).await?.map(|s| s.status))
}

/// Agent loop statistics: total count and per-status breakdown.
pub async fn statistics(ctx: &ApiContext) -> ApiResult<AgentLoopStatistics> {
    let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0;
    for summary in all_summaries(ctx).await {
        total += 1;
        *by_status
            .entry(status_str(&summary.status).to_string())
            .or_insert(0) += 1;
    }
    Ok(AgentLoopStatistics { total, by_status })
}

/// Whether an agent loop exists (live or persisted).
pub async fn has(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<bool> {
    Ok(summary(ctx, agent_loop_id).await?.is_some())
}

/// Number of known agent loops (live + persisted).
pub async fn count(ctx: &ApiContext) -> ApiResult<usize> {
    Ok(all_summaries(ctx).await.len())
}

pub async fn all_summaries(ctx: &ApiContext) -> Vec<AgentLoopSummary> {
    let mut by_id: BTreeMap<String, AgentLoopSummary> = BTreeMap::new();

    for id in ctx.agent_loops.get_all_ids() {
        if let Some(entity) = ctx.agent_loop(&id.to_string()) {
            by_id.insert(id.to_string(), live_summary(&entity).await);
        }
    }

    if let Ok(records) = ctx.storage.agent_execution.list(None).await {
        for record in records {
            by_id
                .entry(record.id.to_string())
                .or_insert_with(|| persisted_summary(&record));
        }
    }

    let mut titles: BTreeMap<String, String> = BTreeMap::new();
    let mut meta_only: Vec<wf_types::AgentLoopStorageMetadata> = Vec::new();
    if let Ok(metas) = ctx.storage.agent_loop.list(None).await {
        for meta in metas {
            if let Some(title) = normalized_stored_title(meta.title.as_deref()) {
                titles.insert(meta.id.to_string(), title);
            }
            let id = meta.id.to_string();
            if by_id.contains_key(&id) {
                continue;
            }
            meta_only.push(meta);
        }
    }
    for meta in meta_only {
        let id = meta.id.to_string();
        if let Some(summary) = meta_summary(&meta) {
            by_id.insert(id, summary);
        }
    }
    for (id, title) in titles {
        if let Some(summary) = by_id.get_mut(&id) {
            if summary.title.is_none() {
                summary.title = Some(title);
            }
        }
    }

    by_id.into_values().collect()
}

async fn update_persisted_status(
    ctx: &ApiContext,
    agent_loop_id: &str,
    status: ExecutionStatus,
) -> ApiResult<()> {
    if let Some(mut meta) = ctx.storage.agent_loop.load(agent_loop_id).await? {
        meta.status = status_str(&status).to_string();
        ctx.storage.agent_loop.save(&meta).await?;
        return Ok(());
    }
    if let Some(mut record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        record.status = status;
        ctx.storage.agent_execution.save(&record).await?;
        return Ok(());
    }
    Err(ApiError::execution_not_found(agent_loop_id))
}

fn filter_matches(summary: &AgentLoopSummary, filter: &AgentLoopFilter) -> bool {
    if let Some(ids) = &filter.ids {
        if !ids.iter().any(|id| id == &summary.id) {
            return false;
        }
    }
    if let Some(status) = &filter.status {
        if &summary.status != status {
            return false;
        }
    }
    if let Some(profile_id) = &filter.profile_id {
        if summary.profile_id.as_deref() != Some(profile_id.as_str()) {
            return false;
        }
    }
    if let Some(range) = filter.created_at_range {
        let Some(start_time) = summary.start_time else {
            return false;
        };
        if let Some(start) = range.0 {
            if start_time < start {
                return false;
            }
        }
        if let Some(end) = range.1 {
            if start_time > end {
                return false;
            }
        }
    }
    if let Some(query) = filter.query.as_deref() {
        let needle = query.trim().to_lowercase();
        if !needle.is_empty() {
            let id_hit = summary.id.to_lowercase().contains(&needle);
            let title_hit = summary
                .title
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&needle);
            let status_hit = format!("{:?}", summary.status)
                .to_lowercase()
                .contains(&needle);
            if !(id_hit || title_hit || status_hit) {
                return false;
            }
        }
    }
    true
}

pub async fn live_summary(entity: &wf_agent::entity::AgentLoopEntity) -> AgentLoopSummary {
    let snapshot = entity.state.read().await;
    let status: ExecutionStatus = snapshot.status().into();
    let start_time = snapshot.start_time();
    let end_time = snapshot.end_time();
    AgentLoopSummary {
        id: entity.id().to_string(),
        execution_type: ExecutionType::Agent,
        status,
        current_iteration: snapshot.current_iteration(),
        tool_call_count: snapshot.tool_call_count(),
        start_time: Some(start_time),
        end_time,
        execution_time: match (start_time, end_time) {
            (start, Some(end)) => Some(end - start),
            _ => None,
        },
        profile_id: Some(entity.model().to_string()),
        parent_execution_id: entity.parent_execution_id().map(|p| p.to_string()),
        title: None,
    }
}

fn persisted_summary(record: &wf_types::AgentExecution) -> AgentLoopSummary {
    let start_time = Some(record.started_at);
    let end_time = record.completed_at;
    AgentLoopSummary {
        id: record.id.to_string(),
        execution_type: record.execution_type,
        status: record.status.clone(),
        current_iteration: record.current_iteration,
        tool_call_count: record.tool_call_count,
        start_time,
        end_time,
        execution_time: match (start_time, end_time) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        },
        profile_id: record.context.as_ref().and_then(|c| c.profile_id.clone()),
        parent_execution_id: record
            .hierarchy
            .as_ref()
            .and_then(|h| h.parent_execution_id()),
        title: None,
    }
}

fn meta_summary(meta: &wf_types::AgentLoopStorageMetadata) -> Option<AgentLoopSummary> {
    let status = parse_status(&meta.status)?;
    let start_time = meta.started_at;
    Some(AgentLoopSummary {
        id: meta.id.to_string(),
        execution_type: ExecutionType::Agent,
        status,
        current_iteration: meta.current_iteration,
        tool_call_count: 0,
        start_time: Some(start_time),
        end_time: None,
        execution_time: None,
        profile_id: None,
        parent_execution_id: None,
        title: normalized_stored_title(meta.title.as_deref()),
    })
}
