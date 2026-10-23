use std::collections::BTreeMap;

use futures::future::join_all;

use crate::ApiContext;
use crate::ApiResult;

use super::types::{SearchOptions, SearchResourceType, SearchResult, SearchResultItem};

pub async fn search(
    ctx: &ApiContext,
    query: &str,
    options: &SearchOptions,
) -> ApiResult<SearchResult> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Ok(SearchResult {
            query,
            items: Vec::new(),
            by_type: BTreeMap::new(),
            total: 0,
            truncated: false,
            next_cursor: None,
        });
    }
    let (types, per_type, total_limit) = options.effective();
    let cursor_offsets = decode_cursor(options.cursor.as_deref());

    let futures = types
        .iter()
        .copied()
        .map(|resource_type| {
            let query_ref = &query;
            let start = cursor_offsets
                .get(resource_type.as_str())
                .copied()
                .unwrap_or(0);
            async move {
                // Fetch enough to skip the cursor offset, then page.
                let fetch = start.saturating_add(per_type);
                let result = search_type(ctx, query_ref, resource_type, fetch).await;
                match result {
                    Ok(items) => items.into_iter().skip(start).take(per_type).collect(),
                    Err(err) => {
                        tracing::warn!(
                            target: "wf_api",
                            resource_type = resource_type.as_str(),
                            error = %err,
                            "resource search failed, degrading to empty results"
                        );
                        Vec::new()
                    }
                }
            }
        })
        .collect::<Vec<_>>();

    let mut results: Vec<SearchResultItem> =
        join_all(futures).await.into_iter().flatten().collect();
    results.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.r#type.cmp(&b.r#type))
            .then_with(|| a.id.cmp(&b.id))
    });

    let truncated = results.len() > total_limit;
    results.truncate(total_limit);

    let mut by_type: BTreeMap<String, Vec<SearchResultItem>> = BTreeMap::new();
    for item in &results {
        by_type
            .entry(item.r#type.clone())
            .or_default()
            .push(item.clone());
    }

    let total = results.len();
    let next_cursor = if truncated {
        let mut next = cursor_offsets.clone();
        for item in &results {
            // Advance each represented source by the items consumed on this
            // page; sources absent from this page keep their offset.
            let entry = next.entry(item.r#type.clone()).or_insert(0);
            *entry = entry.saturating_add(1);
        }
        // Cap cursor growth: only track sources in this query.
        next.retain(|k, _| types.iter().any(|t| t.as_str() == k));
        Some(encode_cursor(&next))
    } else {
        None
    };
    Ok(SearchResult {
        query,
        items: results,
        by_type,
        total,
        truncated,
        next_cursor,
    })
}

/// Encode per-source offsets as an opaque continuation token (hex of JSON).
pub(crate) fn encode_cursor(offsets: &BTreeMap<String, usize>) -> String {
    let json = serde_json::to_string(offsets).unwrap_or_else(|_| "{}".to_string());
    let mut out = String::with_capacity(json.len() * 2);
    for byte in json.as_bytes() {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// Decode an opaque continuation token back to per-source offsets.
/// Unknown or malformed tokens degrade to the start of every source.
pub(crate) fn decode_cursor(cursor: Option<&str>) -> BTreeMap<String, usize> {
    let Some(raw) = cursor else {
        return BTreeMap::new();
    };
    if !raw.len().is_multiple_of(2) {
        return BTreeMap::new();
    }
    let mut bytes = Vec::with_capacity(raw.len() / 2);
    let chars: Vec<char> = raw.chars().collect();
    for pair in chars.chunks(2) {
        let hi = pair[0].to_digit(16);
        let lo = pair[1].to_digit(16);
        match (hi, lo) {
            (Some(hi), Some(lo)) => bytes.push(((hi << 4) | lo) as u8),
            _ => return BTreeMap::new(),
        }
    }
    let json = String::from_utf8(bytes).unwrap_or_default();
    serde_json::from_str(&json).unwrap_or_default()
}

async fn search_type(
    ctx: &ApiContext,
    query: &str,
    resource_type: SearchResourceType,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    match resource_type {
        SearchResourceType::Workflow => super::workflows::search_workflows(ctx, query, limit).await,
        SearchResourceType::Execution => {
            super::executions::search_executions(ctx, query, limit).await
        }
        SearchResourceType::Task => super::tasks::search_tasks(ctx, query, limit).await,
        SearchResourceType::Checkpoint => {
            super::checkpoints::search_checkpoints(ctx, query, limit).await
        }
        SearchResourceType::Event => Ok(super::events::search_events(ctx, query, limit).await),
        SearchResourceType::AgentLoop => {
            super::agent_loops::search_agent_loops(ctx, query, limit).await
        }
        SearchResourceType::Message => super::messages::search_messages(ctx, query, limit).await,
    }
}
