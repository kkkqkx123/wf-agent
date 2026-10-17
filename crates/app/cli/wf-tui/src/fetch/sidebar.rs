//! Sidebar section fetchers: git working-tree status, execution file history
//! and MCP tool availability.
//!
//! Each section has an independent server source and degrades on its own: a
//! failing endpoint yields `None`, which tells the shell to keep the rows it
//! already has instead of blanking the panel.

use std::collections::HashSet;

use wf_client::WfClient;

use super::common::items_of;
use crate::context_sidebar::{FileLine, GitSection, McpLine};

/// Fetched sidebar sections. `None` marks a source that was unavailable this
/// round; the shell keeps its previously cached rows for that section.
#[derive(Debug, Clone, Default)]
pub struct SidebarFetch {
    pub git: Option<GitSection>,
    pub files: Option<Vec<FileLine>>,
    pub mcp: Option<Vec<McpLine>>,
}

/// Fetch every sidebar source concurrently. The execution-scoped file history
/// is skipped when no execution id is attached.
pub async fn fetch_sidebar(client: &WfClient, execution_id: Option<String>) -> SidebarFetch {
    let (git, files, mcp) = tokio::join!(
        fetch_git_section(client),
        fetch_touched_files(client, execution_id),
        fetch_mcp(client),
    );
    SidebarFetch { git, files, mcp }
}

/// Working-tree changes from `/git/files`, newest server order preserved.
async fn fetch_git_section(client: &WfClient) -> Option<GitSection> {
    let data: serde_json::Value = client.get_json("/api/v1/git/files").await.ok()?;
    let files = items_of(&data)
        .into_iter()
        .filter_map(|item| {
            let path = item.get("path").and_then(|value| value.as_str())?;
            let kind = item
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            Some(FileLine {
                path: path.to_string(),
                status: git_status_letter(kind).to_string(),
            })
        })
        .collect();
    Some(GitSection {
        branch: None,
        files,
    })
}

/// Single-letter porcelain marker for a git file kind.
fn git_status_letter(kind: &str) -> &'static str {
    match kind {
        "untracked" => "?",
        "renamed" => "R",
        "conflicted" => "!",
        "staged" | "unstaged" => "M",
        _ => "",
    }
}

/// Files touched by the execution, newest first, from the file-checkpoint
/// history keyed by the `agent:{execution_id}` actor.
async fn fetch_touched_files(
    client: &WfClient,
    execution_id: Option<String>,
) -> Option<Vec<FileLine>> {
    let execution_id = execution_id?;
    let path = format!("/api/v1/file-checkpoint/changes?actor=agent:{execution_id}&limit=100");
    let data: serde_json::Value = client.get_json(&path).await.ok()?;
    let items = items_of(&data);
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    // The change log is chronological; walk it backwards so the most recently
    // touched path leads, keeping the first (newest) occurrence of each.
    for item in items.iter().rev() {
        let Some(file) = item.get("file").and_then(|value| value.as_str()) else {
            continue;
        };
        if seen.insert(file.to_string()) {
            files.push(FileLine {
                path: file.to_string(),
                status: String::new(),
            });
        }
    }
    Some(files)
}

/// MCP connections aggregated from the tool catalog: one row per server that
/// owns at least one discovered `mcp` tool.
async fn fetch_mcp(client: &WfClient) -> Option<Vec<McpLine>> {
    let data: serde_json::Value = client.get_json("/api/v1/tools").await.ok()?;
    let items = items_of(&data);
    let mut servers: Vec<(String, bool, usize)> = Vec::new();
    for item in items {
        if item.get("tool_type").and_then(|value| value.as_str()) != Some("mcp") {
            continue;
        }
        let Some(server) = item
            .get("config")
            .and_then(|config| config.get("server_name"))
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        let enabled = item
            .get("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        match servers.iter_mut().find(|(name, _, _)| name == server) {
            Some((_, all_enabled, count)) => {
                *all_enabled &= enabled;
                *count += 1;
            }
            None => servers.push((server.to_string(), enabled, 1)),
        }
    }
    Some(
        servers
            .into_iter()
            .map(|(name, enabled, count)| McpLine {
                name,
                status: format!(
                    "{} · {count} tools",
                    if enabled { "enabled" } else { "disabled" }
                ),
            })
            .collect(),
    )
}
