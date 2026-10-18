//! Unresolved merge-conflict fetcher: the files whose feature merge left
//! standard diff3 markers, with the region excerpts pre-rendered for the
//! read-only viewer. Resolution itself is dispatched from the shell's key
//! actions (choose a side per file).

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{ConflictRow, ScreenData};

use super::common::items_of;

/// Build the conflict rows from the unresolved-conflict endpoint.
pub(super) async fn fetch_conflicts(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let data: serde_json::Value = client.get_json("/api/v1/file-checkpoint/conflicts").await?;
    let rows = items_of(&data).iter().map(conflict_row).collect();
    Ok((ScreenData::Conflicts(rows), false))
}

/// Map one conflict record to a display row, pre-rendering its region
/// excerpts for the viewer.
fn conflict_row(conflict: &serde_json::Value) -> ConflictRow {
    let path = conflict
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let partition = conflict
        .get("partition")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let snapshot_id = conflict
        .get("snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let regions = conflict
        .get("conflicts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let binary = regions.is_empty();
    let mut detail = vec![
        format!("path: {path}"),
        format!("partition: {partition}"),
        format!("snapshot: {snapshot_id}"),
        format!("regions: {}", regions.len()),
    ];
    if binary {
        detail.push("binary conflict: no textual markers".to_string());
    }
    for (idx, region) in regions.iter().enumerate() {
        let start = region
            .get("start_line")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        detail.push(format!("--- region {} (line {}) ---", idx + 1, start));
        for side in ["base", "ours", "theirs"] {
            detail.push(format!("{side}:"));
            if let Some(lines) = region.get(side).and_then(|v| v.as_array()) {
                detail.extend(
                    lines
                        .iter()
                        .map(|line| format!("  {}", line.as_str().unwrap_or(""))),
                );
            }
        }
    }
    ConflictRow {
        path,
        partition,
        snapshot_id,
        regions: regions.len(),
        binary,
        detail: detail.join("\n"),
    }
}
