//! Mention data sources: candidate search and normalization for `@mention`.

use serde::Serialize;

use crate::analysis::search::{search as search_resources, SearchOptions};
use crate::ApiContext;
use crate::ApiResult;
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
    options: &MentionSearchOptions,
) -> ApiResult<Vec<MentionCandidate>> {
    let query = options.query.trim();
    let kinds = options.kinds.clone();
    let wants = |kind: &str| {
        kinds
            .as_ref()
            .map(|list| list.iter().any(|k| k == kind))
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
                    .filter_map(|kind| match kind.as_str() {
                        "workflow" => Some(crate::analysis::search::SearchResourceType::Workflow),
                        "execution" => Some(crate::analysis::search::SearchResourceType::Execution),
                        "task" => Some(crate::analysis::search::SearchResourceType::Task),
                        "checkpoint" => {
                            Some(crate::analysis::search::SearchResourceType::Checkpoint)
                        }
                        "event" => Some(crate::analysis::search::SearchResourceType::Event),
                        "agent_loop" => {
                            Some(crate::analysis::search::SearchResourceType::AgentLoop)
                        }
                        "message" => Some(crate::analysis::search::SearchResourceType::Message),
                        _ => None,
                    })
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

    // Files (best-effort local scan).
    if wants("file") {
        let root = std::env::current_dir().unwrap_or_default();
        let mut files = scan_files(&root, if query.is_empty() { None } else { Some(query) });
        files.truncate(limit);
        for rel in files {
            let label = std::path::Path::new(&rel)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&rel)
                .to_string();
            out.push(MentionCandidate {
                kind: "file".to_string(),
                id: rel.clone(),
                label,
                description: Some(rel.clone()),
                value: rel.clone(),
                keywords: vec![rel.clone()],
                data: serde_json::json!({ "kind": "file" }),
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
        out.retain(|item| kinds.iter().any(|kind| kind == &item.kind));
    }
    out.truncate(limit * 8);
    Ok(out)
}

fn contains_query(text: &str, query: &str) -> bool {
    text.to_lowercase().contains(&query.to_lowercase())
}

fn scan_files(root: &std::path::Path, query: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    let limit = 2000usize;
    let max_depth = 8usize;
    let mut stack: Vec<(std::path::PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let needle = query.map(|q| q.to_ascii_lowercase());
    let ignore_re = regex::Regex::new(r"(?:\.git|node_modules|target|\.next|dist)(?:/|$)").ok();
    while let Some((dir, depth)) = stack.pop() {
        if depth > max_depth || out.len() >= limit {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
            else {
                continue;
            };
            if let Some(re) = &ignore_re {
                if re.is_match(&rel) {
                    continue;
                }
            }
            if path.is_dir() {
                stack.push((path, depth + 1));
            } else if path.is_file() {
                if let Some(q) = &needle {
                    if !rel.to_ascii_lowercase().contains(q.as_str()) {
                        continue;
                    }
                }
                out.push(rel);
                if out.len() >= limit {
                    break;
                }
            }
        }
    }
    out.sort();
    out
}
