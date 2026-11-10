//! Mention data sources: candidate search and normalization for `@mention`.

use serde::Serialize;

use crate::analysis::search::{search as search_resources, SearchOptions};
use crate::ApiContext;
use crate::{ApiError, ApiResult};
use wf_storage::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Serialize)]
pub struct MentionCandidate {
    pub kind: String,
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub value: String,
    pub keywords: Vec<String>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct MentionSearchOptions {
    pub query: String,
    pub kinds: Option<Vec<String>>,
    pub limit: usize,
}

/// Search mention candidates across all registered data sources.
pub async fn search(
    ctx: &ApiContext,
    dir: Option<&std::path::Path>,
    options: &MentionSearchOptions,
) -> ApiResult<Vec<MentionCandidate>> {
    let query = options.query.trim();
    let kinds = options.kinds.clone();
    let wants = |kind: &str| {
        kinds
            .as_ref()
            .map(|list| {
                list.iter().any(|k| {
                    k == kind
                        || wf_common::mention::normalize_mention_kind(k) == kind
                        || kind == "workflow" && k == "wf"
                        || kind == "agent_loop" && k == "agent"
                })
            })
            .unwrap_or(true)
    };
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let limit = options.limit.clamp(1, 100);

    let mut out: Vec<MentionCandidate> = Vec::new();

    // Storage-backed kinds via the unified search API.
    if !query.is_empty()
        && (wants("workflow")
            || wants("execution")
            || wants("task")
            || wants("checkpoint")
            || wants("event")
            || wants("agent_loop")
            || wants("message"))
    {
        let storage_types = kinds
            .as_ref()
            .map(|list| {
                list.iter()
                    .filter_map(
                        |kind| match wf_common::mention::normalize_mention_kind(kind) {
                            "workflow" => {
                                Some(crate::analysis::search::SearchResourceType::Workflow)
                            }
                            "execution" => {
                                Some(crate::analysis::search::SearchResourceType::Execution)
                            }
                            "task" => Some(crate::analysis::search::SearchResourceType::Task),
                            "checkpoint" => {
                                Some(crate::analysis::search::SearchResourceType::Checkpoint)
                            }
                            "event" => Some(crate::analysis::search::SearchResourceType::Event),
                            "agent" => Some(crate::analysis::search::SearchResourceType::AgentLoop),
                            "message" => Some(crate::analysis::search::SearchResourceType::Message),
                            _ => None,
                        },
                    )
                    .collect::<Vec<_>>()
            })
            .filter(|types| !types.is_empty());
        let opts = SearchOptions {
            types: storage_types,
            limit_per_type: Some(limit),
            limit_total: Some(limit * 8),
            cursor: None,
        };
        if let Ok(result) = search_resources(ctx, query, &opts).await {
            for item in result.items {
                out.push(MentionCandidate {
                    kind: item.r#type.clone(),
                    id: item.id.clone(),
                    label: item.label.clone(),
                    description: item.description.clone(),
                    value: item.id.clone(),
                    keywords: item.matches.clone(),
                    data: serde_json::json!({}),
                });
            }
        }
    }

    // Skills.
    if wants("skill") {
        for skill in crate::entity::skill::list_skills(ctx).unwrap_or_default() {
            if contains_query(&skill.name, query) || contains_query(&skill.description, query) {
                out.push(MentionCandidate {
                    kind: "skill".to_string(),
                    id: skill.name.clone(),
                    label: skill.name.clone(),
                    description: Some(skill.description.clone()),
                    value: skill.name.clone(),
                    keywords: vec![skill.name.clone(), skill.description.clone()],
                    data: serde_json::json!({ "scope": "skill" }),
                });
                if out.len() >= limit * 8 {
                    break;
                }
            }
        }
    }

    // Tools.
    if wants("tool") {
        for tool in ctx.tool_registry.search(query) {
            out.push(MentionCandidate {
                kind: "tool".to_string(),
                id: tool.id.clone(),
                label: tool.name.clone(),
                description: Some(tool.description.clone()),
                value: tool.id.clone(),
                keywords: vec![tool.name.clone(), tool.id.clone(), tool.description.clone()],
                data: serde_json::json!({}),
            });
        }
    }

    // Scripts.
    if wants("script") {
        if let Ok(scripts) = ctx.storage.script.list(None).await {
            for script in scripts {
                if contains_query(&script.name, query)
                    || script
                        .description
                        .as_deref()
                        .map(|d| contains_query(d, query))
                        .unwrap_or(false)
                {
                    out.push(MentionCandidate {
                        kind: "script".to_string(),
                        id: script.id.to_string(),
                        label: script.name.clone(),
                        description: script.description.clone(),
                        value: script.id.to_string(),
                        keywords: vec![script.name.clone()],
                        data: serde_json::json!({ "language": script.language }),
                    });
                }
            }
        }
    }

    // Files (best-effort local scan) anchor on the request directory.
    // A missing directory is a request error, never a silent fallback:
    // there is no server-side default workspace.
    if wants("file") {
        let Some(dir) = dir else {
            return Err(ApiError::Validation(
                "request directory is required".to_string(),
            ));
        };
        let root = dir.to_path_buf();
        let mut files = wf_common::mention::scan_files(
            &root,
            if query.is_empty() { None } else { Some(query) },
        );
        files.truncate(limit);
        for rel in files {
            let is_dir = rel.ends_with('/');
            let clean = rel.trim_end_matches('/').to_string();
            let label = std::path::Path::new(&clean)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&clean)
                .to_string();
            out.push(MentionCandidate {
                kind: "file".to_string(),
                id: rel.clone(),
                label,
                description: Some(clean.clone()),
                value: rel.clone(),
                keywords: vec![clean.clone()],
                data: serde_json::json!({ "kind": if is_dir { "directory" } else { "file" } }),
            });
        }
    }

    // MCP resources.
    if wants("resource") {
        if let Some(manager) = ctx.tool_registry.mcp_manager() {
            for server in manager.registry().list() {
                for resource in server.resources {
                    if contains_query(&resource.name, query) || contains_query(&resource.uri, query)
                    {
                        out.push(MentionCandidate {
                            kind: "resource".to_string(),
                            id: resource.uri.clone(),
                            label: resource.name.clone(),
                            description: resource.description.clone(),
                            value: resource.uri.clone(),
                            keywords: vec![resource.name.clone(), resource.uri.clone()],
                            data: serde_json::json!({
                                "server": server.name,
                                "mimeType": resource.mime_type,
                            }),
                        });
                    }
                }
            }
        }
    }

    if let Some(kinds) = &kinds {
        out.retain(|item| {
            kinds.iter().any(|kind| {
                kind == &item.kind || wf_common::mention::normalize_mention_kind(kind) == item.kind
            })
        });
    }
    out.truncate(limit * 8);
    Ok(out)
}

fn contains_query(text: &str, query: &str) -> bool {
    text.to_lowercase().contains(&query.to_lowercase())
}
