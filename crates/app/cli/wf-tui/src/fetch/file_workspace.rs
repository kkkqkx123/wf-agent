//! File-workspace fetcher: the files an execution touched, joined with the
//! unified diff against main so the read-only viewer needs no extra fetch.
//!
//! The actor partition is `agent:{execution_id}`, the same key the sidebar
//! file history uses. Without an active execution the screen is empty and the
//! title says so.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{FileChangeRow, ScreenData};

use super::common::items_of;

/// Render one per-file diff view as unified-diff text: a one-line header
/// plus every hunk's header and raw lines. Binary files carry no hunks.
fn render_diff(diff: &serde_json::Value) -> String {
    let path = diff.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = diff.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
    let additions = diff.get("additions").and_then(|v| v.as_u64()).unwrap_or(0);
    let deletions = diff.get("deletions").and_then(|v| v.as_u64()).unwrap_or(0);
    let binary = diff.get("binary").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut out = vec![format!("diff --{kind} {path} (+{additions} -{deletions})")];
    if binary {
        out.push("(binary file: no textual diff)".to_string());
        return out.join("\n");
    }
    if let Some(hunks) = diff.get("hunks").and_then(|v| v.as_array()) {
        for hunk in hunks {
            if let Some(header) = hunk.get("header").and_then(|v| v.as_str()) {
                out.push(header.to_string());
            }
            if let Some(lines) = hunk.get("lines").and_then(|v| v.as_array()) {
                for line in lines.iter().filter_map(|line| line.as_str()) {
                    out.push(line.to_string());
                }
            }
        }
    }
    if out.len() == 1 {
        out.push("(no textual changes)".to_string());
    }
    out.join("\n")
}

/// Build the file-workspace rows for the active execution: one row per file
/// the execution changed against main, each carrying its rendered diff.
pub(super) async fn fetch_file_workspace(
    client: &WfClient,
    execution_id: Option<&str>,
) -> CliResult<(ScreenData, bool)> {
    let Some(execution_id) = execution_id.filter(|id| !id.is_empty()) else {
        return Ok((ScreenData::FileWorkspace(Vec::new()), false));
    };
    let actor = format!("agent:{execution_id}");
    let data: serde_json::Value = client
        .get_json(&format!("/api/v1/file-checkpoint/diff/main/{actor}"))
        .await?;
    let rows = items_of(&data)
        .iter()
        .map(|diff| FileChangeRow {
            path: diff
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            actor: actor.clone(),
            change_type: diff
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            detail: render_diff(diff),
        })
        .collect();
    // A per-actor diff is returned whole; there is no server-side paging.
    Ok((ScreenData::FileWorkspace(rows), false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_diff_includes_header_and_lines() {
        let diff = serde_json::json!({
            "path": "src/main.rs",
            "kind": "modified",
            "additions": 2,
            "deletions": 1,
            "binary": false,
            "hunks": [
                {"header": "@@ -1,3 +1,4 @@", "lines": [" ctx", "-old", "+new", "+extra"]}
            ],
        });
        let text = render_diff(&diff);
        assert!(text.contains("diff --modified src/main.rs (+2 -1)"));
        assert!(text.contains("@@ -1,3 +1,4 @@"));
        assert!(text.contains("+new"));
    }

    #[test]
    fn render_diff_marks_binary_and_empty() {
        let binary = serde_json::json!({
            "path": "logo.png", "kind": "added", "additions": 0, "deletions": 0,
            "binary": true, "hunks": [],
        });
        assert!(render_diff(&binary).contains("(binary file: no textual diff)"));

        let empty = serde_json::json!({
            "path": "added.rs", "kind": "added", "additions": 0, "deletions": 0,
            "binary": false, "hunks": [],
        });
        assert!(render_diff(&empty).contains("(no textual changes)"));
    }
}
