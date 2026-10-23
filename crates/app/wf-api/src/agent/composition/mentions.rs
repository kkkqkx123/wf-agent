use std::collections::HashSet;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::message::Message;

use crate::infra::context::ApiContext;

use super::*;

pub async fn resolve_mentions(ctx: &ApiContext, text: &str) -> Vec<Message> {
    let Ok(re) = regex::Regex::new(
        r"@(?:(?P<kind>[a-z][a-z0-9_]*):(?P<value>[\w./-]+(?:#\d+(?:-\d+)?)?)|(?P<path>[\w./-]*[./][\w./-]*(?:#\d+(?:-\d+)?)?))",
    ) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for capture in re.captures_iter(text) {
        if out.len() >= 8 {
            break;
        }
        let (kind, id) = if let Some(kind_match) = capture.name("kind") {
            let Some(value_match) = capture.name("value") else {
                continue;
            };
            (kind_match.as_str(), value_match.as_str())
        } else {
            let Some(path_match) = capture.name("path") else {
                continue;
            };
            ("file", path_match.as_str())
        };
        let key = format!("{kind}:{id}");
        if !seen.insert(key) {
            continue;
        }

        let summary = match kind {
            "workflow" | "wf" => resolve_workflow_context(&ctx.registries, id).ok(),
            "agent" | "agent_loop" => resolve_agent_loop_mention(ctx, id).await,
            "execution" => ctx
                .storage
                .workflow_execution
                .load(id)
                .await
                .ok()
                .flatten()
                .map(|entity| {
                    format!(
                        "execution: {} (workflow {})\nstatus: {}",
                        entity.id,
                        entity.workflow_id,
                        entity.status.as_str()
                    )
                }),
            "task" => ctx.storage.task.load(id).await.ok().flatten().map(|meta| {
                format!(
                    "task: {} ({})\nstatus: {}",
                    meta.id, meta.task_type, meta.status
                )
            }),
            "checkpoint" => ctx
                .storage
                .checkpoint
                .load(id)
                .await
                .ok()
                .flatten()
                .map(|meta| {
                    format!(
                        "checkpoint: {} (entity {} / {})",
                        meta.id, meta.entity_type, meta.entity_id
                    )
                }),
            "event" => ctx
                .event_bus
                .recent_events()
                .into_iter()
                .find(|event| event.id == id)
                .map(|event| format!("event: {} ({})", event.id, event.r#type.as_str())),
            "message" => crate::entity::message::get(ctx, id)
                .await
                .ok()
                .map(|record| {
                    let text = crate::entity::message::message_text(&record.message);
                    let snippet: String = text.chars().take(120).collect();
                    format!(
                        "message: {} (execution {})\n{}",
                        record.id, record.execution_id, snippet
                    )
                }),
            "file" => resolve_file_mention(ctx, id),
            "skill" => resolve_skill_mention(ctx, id),
            "tool" => resolve_tool_mention(ctx, id),
            "script" => resolve_script_mention(ctx, id).await,
            "resource" => Some(format!("resource: {id}")),
            _ => None,
        };

        if let Some(summary) = summary {
            let trimmed: String = summary.chars().take(1000).collect();
            out.push(crate::dynamic_context_message(format!(
                "Referenced {kind} context:\n{trimmed}"
            )));
        }
    }
    out
}

fn parse_file_mention(raw: &str) -> (String, Option<(u32, u32)>) {
    if let Some((path, range)) = raw.split_once('#') {
        let (start, end) = if let Some((a, b)) = range.split_once('-') {
            (a.parse::<u32>().ok(), b.parse::<u32>().ok())
        } else {
            (range.parse::<u32>().ok(), None)
        };
        let start = start.unwrap_or(1).max(1);
        let end = end.unwrap_or(start).max(start);
        (path.to_string(), Some((start, end)))
    } else {
        (raw.to_string(), None)
    }
}

fn resolve_file_mention(ctx: &ApiContext, raw: &str) -> Option<String> {
    let (path, range) = parse_file_mention(raw);
    let root = ctx
        .user_git_config()
        .and_then(|config| config.repo_root.clone())
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?;
    let root_canonical = root.canonicalize().ok()?;
    let full = if std::path::Path::new(&path).is_absolute() {
        std::path::PathBuf::from(&path)
    } else {
        root.join(&path)
    };
    let canonical = full.canonicalize().ok()?;
    if !canonical.starts_with(&root_canonical) {
        return None;
    }
    if canonical.is_dir() {
        let mut entries: Vec<String> = std::fs::read_dir(&canonical)
            .ok()?
            .flatten()
            .take(50)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        entries.sort();
        return Some(format!(
            "directory: {path}
{}",
            entries.join(
                "
"
            )
        ));
    }
    let content = std::fs::read_to_string(&canonical).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Some(format!("file: {path}"));
    }
    let (start, end) = match range {
        Some((s, e)) => (s as usize, e as usize),
        None => (1usize, lines.len().min(200)),
    };
    let start = start.max(1).min(lines.len());
    let end = end.max(start).min(lines.len()).min(start + 199);
    let selected: Vec<String> = lines[start - 1..end]
        .iter()
        .map(|line| (*line).to_string())
        .collect();
    Some(format!(
        "file: {path} (lines {start}-{end})\n{}",
        selected.join("\n")
    ))
}

fn resolve_skill_mention(ctx: &ApiContext, id: &str) -> Option<String> {
    let meta = crate::entity::skill::get_skill(ctx, id).ok()?;
    Some(format!(
        "skill: {} ({})\n{}",
        meta.name,
        meta.version.as_deref().unwrap_or("unknown"),
        meta.description
    ))
}

fn resolve_tool_mention(ctx: &ApiContext, id: &str) -> Option<String> {
    let tool = ctx.tool_registry.get_tool(id)?;
    Some(format!(
        "tool: {} ({})\n{}",
        tool.name, tool.id, tool.description
    ))
}

async fn resolve_script_mention(ctx: &ApiContext, id: &str) -> Option<String> {
    let meta = ctx.storage.script.load(id).await.ok().flatten()?;
    Some(format!(
        "script: {} ({})\n{}",
        meta.name,
        meta.id,
        meta.description.as_deref().unwrap_or("")
    ))
}

async fn resolve_agent_loop_mention(ctx: &ApiContext, id: &str) -> Option<String> {
    ctx.storage
        .agent_loop
        .load(id)
        .await
        .ok()
        .flatten()
        .map(|meta| {
            format!(
                "agent_loop: {} (definition {})\nstatus: {}",
                meta.id, meta.definition_id, meta.status
            )
        })
}

/// Truncated projection of a workflow template: identity plus the node list
/// (bounded) and edge count, so the loop can reference the workflow without
/// pulling the full graph into the prompt.
pub fn workflow_summary(template: &wf_types::workflow::WorkflowTemplate) -> String {
    const MAX_NODES: usize = 20;
    let nodes = &template.definition.nodes;
    let mut lines = Vec::with_capacity(nodes.len().min(MAX_NODES) + 4);
    lines.push(format!("workflow: {} ({})", template.name, template.id));
    if !template.description.is_empty() {
        lines.push(format!("description: {}", template.description));
    }
    lines.push(format!("nodes ({}):", nodes.len()));
    for node in nodes.iter().take(MAX_NODES) {
        let name = node.name.as_deref().unwrap_or(&node.id);
        lines.push(format!("- {} ({:?})", name, node.node_type));
    }
    if nodes.len() > MAX_NODES {
        lines.push(format!("- … {} more nodes", nodes.len() - MAX_NODES));
    }
    lines.push(format!("edges: {}", template.definition.edges.len()));
    lines.join("\n")
}
