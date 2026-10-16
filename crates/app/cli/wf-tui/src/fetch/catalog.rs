//! Catalog, settings, git and search-palette fetchers.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{
    CatalogKind, CatalogRow, GitRow, ProfileRow, ScreenData, SearchRow, SettingsData,
};

use super::common::{items_of, load_local_settings, page_of, pretty_detail, text_of};

/// Fetch one page of global search hits for the search palette overlay.
pub async fn fetch_search_rows(
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

pub(super) async fn fetch_settings(client: &WfClient) -> CliResult<ScreenData> {
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

/// Fetch the unified catalog: the four former screens load in parallel
/// and merge into one row list. Each source degrades independently so a
/// single failing endpoint still renders the other groups.
pub(super) async fn fetch_catalog(client: &WfClient) -> CliResult<(ScreenData, bool)> {
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

pub(super) async fn fetch_git(client: &WfClient) -> CliResult<(ScreenData, bool)> {
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
