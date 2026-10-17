//! Pending file-change approvals fetcher: the review lines awaiting a
//! human decision, with the submitted file set pre-rendered for the viewer.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{ApprovalRow, ScreenData};

use super::common::{format_ts, items_of};

/// Build the pending-approval rows. Every listed item is pending by
/// definition, so the row status is constant.
pub(super) async fn fetch_file_approvals(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let data: serde_json::Value = client
        .get_json("/api/v1/file-checkpoint/approvals/pending")
        .await?;
    let rows = items_of(&data).iter().map(approval_row).collect();
    Ok((ScreenData::FileApprovals(rows), false))
}

/// Map one pending-approval record to a display row.
fn approval_row(approval: &serde_json::Value) -> ApprovalRow {
    let actor = approval
        .get("actor")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let snapshot = approval
        .get("snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let submitted = format_ts(
        approval
            .get("submitted_at")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    );
    let changes = approval
        .get("changes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut detail = vec![
        format!("actor: {actor}"),
        format!("snapshot: {snapshot}"),
        format!("submitted: {submitted}"),
        format!("files: {}", changes.len()),
    ];
    if !changes.is_empty() {
        detail.push("--- files ---".to_string());
        detail.extend(changes.iter().map(|change| {
            change
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string()
        }));
    }
    ApprovalRow {
        actor,
        files: changes.len(),
        submitted,
        status: "pending".to_string(),
        detail: detail.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_row_counts_files_and_lists_them() {
        let row = approval_row(&serde_json::json!({
            "actor": "agent:loop-1",
            "snapshot_id": "deadbeef",
            "submitted_at": 1_700_000_000_000_i64,
            "changes": [
                {"file": "src/main.rs"},
                {"file": "src/lib.rs"}
            ],
        }));
        assert_eq!(row.actor, "agent:loop-1");
        assert_eq!(row.files, 2);
        assert_eq!(row.status, "pending");
        assert!(row.detail.contains("snapshot: deadbeef"));
        assert!(row.detail.contains("src/main.rs"));
        assert!(row.detail.contains("src/lib.rs"));
    }

    #[test]
    fn approval_row_without_changes_reports_zero_files() {
        let row = approval_row(&serde_json::json!({
            "actor": "manual",
            "snapshot_id": "abc",
            "submitted_at": 1_700_000_000_000_i64,
            "changes": [],
        }));
        assert_eq!(row.files, 0);
        assert!(row.detail.contains("files: 0"));
        assert!(!row.detail.contains("--- files ---"));
    }
}
