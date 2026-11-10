use std::collections::HashSet;

use base64::Engine as _;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::message::{Message, UserImageInput};

use crate::infra::context::ApiContext;

use super::*;

use wf_common::mention::TURN_MENTION_LIMIT;
/// Listed paths per skill resource kind inside an explicit skill mention.
const SKILL_RESOURCE_LIST_LIMIT: usize = 20;

/// Per-reference resolution receipt of one turn's inline mentions. The server
/// reports it back beside the stored conversation so clients can show which
/// image references actually rode the turn and which were skipped.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MentionReceipt {
    /// Reference as written by the user, without the leading `@`
    /// (for example `file:shots/a.png`).
    pub reference: String,
    /// Reference kind (`file`, `workflow`, ...).
    pub kind: String,
    /// Resolution result: `attached` (image riding the structured payload),
    /// `resolved` (text context joined), `skipped` (image unreadable or
    /// oversized), `unmatched` (nothing found) or `omitted` (over the turn
    /// cap; refine the turn instead of silently dropping the reference).
    pub outcome: String,
}

/// Inline mentions resolved from one turn: supplemental text messages plus
/// image payloads. Image references (`@file:` pointing at an image file)
/// travel as structured inputs so binary bytes never pass text decoding.
pub async fn resolve_mentions(
    ctx: &ApiContext,
    text: &str,
) -> (Vec<Message>, Vec<UserImageInput>, Vec<MentionReceipt>) {
    let Ok(re) = regex::Regex::new(
        r"@(?:(?P<kind>[a-z][a-z0-9_]*):(?P<value>[\w./-]+(?:[#:]#?\d+(?:-\d+)?)?)|(?P<path>[\w./-]*[./][\w./-]*(?:[#:]#?\d+(?:-\d+)?)?))",
    ) else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let mut out = Vec::new();
    let mut images = Vec::new();
    let mut receipts = Vec::new();
    let mut ordered: Vec<(String, String)> = Vec::new();
    let mut seen_keys = HashSet::new();
    for capture in re.captures_iter(text) {
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
        if !seen_keys.insert(key) {
            continue;
        }
        ordered.push((kind.to_string(), id.to_string()));
    }
    let omitted = ordered.len().saturating_sub(TURN_MENTION_LIMIT);
    let active = ordered.len().min(TURN_MENTION_LIMIT);
    for (kind, id) in ordered.into_iter().take(active) {
        let kind = kind.as_str();
        let id = id.as_str();

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
                    let mut snippet: String = text.chars().take(121).collect();
                    if snippet.chars().count() > 120 {
                        snippet = snippet.chars().take(120).collect();
                        snippet.push('…');
                    }
                    format!(
                        "message: {} (execution {})\n{}",
                        record.id, record.execution_id, snippet
                    )
                }),
            "file" => {
                // Image references bypass the text snippet path: the bytes
                // travel as a structured image input instead. Image paths never
                // fall through to text reading so binary bytes cannot enter the
                // prompt and oversized or unreadable images stay visible.
                if wf_types::message::is_image_path(id) {
                    if let Some(image) = resolve_image_mention(ctx, id) {
                        images.push(image);
                        receipts.push(MentionReceipt {
                            reference: format!("file:{id}"),
                            kind: kind.to_string(),
                            outcome: "attached".to_string(),
                        });
                        out.push(crate::dynamic_context_message(format!(
                            "image: {id} (attached as structured image)"
                        )));
                        continue;
                    }
                    receipts.push(MentionReceipt {
                        reference: format!("file:{id}"),
                        kind: kind.to_string(),
                        outcome: "skipped".to_string(),
                    });
                    Some(format!(
                        "image: {id} (skipped: unreadable, outside the workspace, or exceeds the size limit)"
                    ))
                } else {
                    resolve_file_mention(ctx, id)
                }
            }
            "skill" => resolve_skill_mention(ctx, id),
            "tool" => resolve_tool_mention(ctx, id),
            "script" => resolve_script_mention(ctx, id).await,
            "resource" => Some(format!("resource: {id}")),
            _ => None,
        };

        if let Some(summary) = summary {
            receipts.push(MentionReceipt {
                reference: format!("{kind}:{id}"),
                kind: kind.to_string(),
                outcome: "resolved".to_string(),
            });
            out.push(crate::dynamic_context_message(format!(
                "Referenced {kind} context:\n{summary}"
            )));
        } else {
            receipts.push(MentionReceipt {
                reference: format!("{kind}:{id}"),
                kind: kind.to_string(),
                outcome: "unmatched".to_string(),
            });
        }
    }
    if omitted > 0 {
        receipts.push(MentionReceipt {
            reference: "mention:turn-limit".to_string(),
            kind: "mention".to_string(),
            outcome: "omitted".to_string(),
        });
        out.push(crate::dynamic_context_message(format!(
            "Omitted {omitted} mention(s): turn limit is {TURN_MENTION_LIMIT}; keep the turn focused or split references across turns."
        )));
    }
    (out, images, receipts)
}

fn parse_file_mention(raw: &str) -> (String, Option<(u32, u32)>) {
    wf_common::mention::split_file_range(raw)
}

/// Resolve an `@file:` reference pointing at an image file into a
/// structured image input. Line ranges never apply to images, oversized
/// files are skipped (the entry validator would reject the turn), and
/// path traversal outside the workspace root resolves to nothing.
fn resolve_image_mention(ctx: &ApiContext, raw: &str) -> Option<UserImageInput> {
    if wf_common::mention::split_file_range(raw).1.is_some() {
        return None;
    }
    let media = wf_types::message::media_type_for_image_path(raw)?;
    let root = ctx
        .user_git_config()
        .and_then(|config| config.repo_root.clone())
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?;
    let root_canonical = root.canonicalize().ok()?;
    let full = if std::path::Path::new(raw).is_absolute() {
        std::path::PathBuf::from(raw)
    } else {
        root.join(raw)
    };
    let canonical = full.canonicalize().ok()?;
    if !canonical.starts_with(&root_canonical) || !canonical.is_file() {
        return None;
    }
    let bytes = std::fs::read(&canonical).ok()?;
    if bytes.len() > wf_types::message::MAX_USER_IMAGE_BYTES {
        tracing::warn!(
            path = raw,
            bytes = bytes.len(),
            "image mention exceeds the per-image limit and is skipped"
        );
        return None;
    }
    Some(UserImageInput::inline(
        base64::engine::general_purpose::STANDARD.encode(&bytes),
        media,
    ))
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
        // Probe one past the display bound so overflow stays visible.
        let mut entries: Vec<String> = std::fs::read_dir(&canonical)
            .ok()?
            .flatten()
            .take(51)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        entries.sort();
        let folded = entries.len() > 50;
        entries.truncate(50);
        let mut body = entries.join("\n");
        if folded {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str("… and more (narrow the reference to a subdirectory)");
        }
        return Some(format!("directory: {path}\n{body}"));
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
    let loader = ctx.tool_registry.skill_loader()?;
    let meta = loader.get_skill(id)?;
    if !loader.is_skill_enabled(id) {
        return Some(format!(
            "skill: {} (disabled)\n{}\nEnable the skill before use.",
            meta.name, meta.description
        ));
    }
    let Ok(body) = loader.load_content(id) else {
        return Some(format!(
            "skill: {} ({})\n{}",
            meta.name,
            meta.version.as_deref().unwrap_or("unknown"),
            meta.description
        ));
    };
    let mut out = format!(
        "skill: {} ({})\n{}\n\n{}",
        meta.name,
        meta.version.as_deref().unwrap_or("unknown"),
        meta.description,
        body.trim()
    );
    let inventory = skill_resource_inventory(loader.as_ref(), id);
    if !inventory.is_empty() {
        out.push_str("\n\nAvailable resources:\n");
        out.push_str(&inventory);
    }
    Some(out)
}

/// One-line-per-kind inventory of bundled skill files. Binary attachments
/// list paths only and are never inlined.
fn skill_resource_inventory(loader: &wf_tools::skill::SkillLoader, name: &str) -> String {
    let kinds = [
        (wf_types::skill::SkillResourceType::References, "references"),
        (wf_types::skill::SkillResourceType::Examples, "examples"),
        (wf_types::skill::SkillResourceType::Scripts, "scripts"),
        (wf_types::skill::SkillResourceType::Assets, "assets"),
    ];
    let mut lines = Vec::new();
    for (kind, label) in kinds {
        let mut paths = loader.list_skill_resources(name, kind);
        if paths.is_empty() {
            continue;
        }
        paths.sort();
        let total = paths.len();
        let shown: Vec<&str> = paths
            .iter()
            .take(SKILL_RESOURCE_LIST_LIMIT)
            .map(String::as_str)
            .collect();
        if total > shown.len() {
            lines.push(format!(
                "- {} ({}): {} and {} more",
                label,
                total,
                shown.join(", "),
                total - shown.len()
            ));
        } else {
            lines.push(format!("- {} ({}): {}", label, total, shown.join(", ")));
        }
    }
    lines.join("\n")
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
