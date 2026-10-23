//! Pending file-change approvals fetcher: the review lines awaiting a
//! human decision, with the submitted file set pre-rendered for the viewer.

use wf_client::models;
use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{ApprovalRow, ScreenData};

use super::common::format_ts;

/// Build the pending-approval rows. Every listed item is pending by
/// definition, so the row status is constant.
pub(super) async fn fetch_file_approvals(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let envelope = client
        .generated()
        .handle_list_pending_approvals()
        .await
        .map_err(wf_client::error::map_typed_error)?;
    let approvals: Vec<models::PendingApprovalDoc> =
        wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
    let rows = approvals.iter().map(approval_row).collect();
    Ok((ScreenData::FileApprovals(rows), false))
}

/// Map one pending-approval record to a display row.
fn approval_row(approval: &models::PendingApprovalDoc) -> ApprovalRow {
    let actor = approval.actor.clone();
    let snapshot = approval.snapshot_id.clone();
    let submitted = format_ts(approval.submitted_at);
    let changes = &approval.changes;
    let mut detail = vec![
        format!("actor: {actor}"),
        format!("snapshot: {snapshot}"),
        format!("submitted: {submitted}"),
        format!("files: {}", changes.len()),
    ];
    if !changes.is_empty() {
        detail.push("--- files ---".to_string());
        detail.extend(changes.iter().map(|change| change.file.clone()));
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
        let row = approval_row(&models::PendingApprovalDoc {
            actor: "agent:loop-1".to_string(),
            snapshot_id: "deadbeef".to_string(),
            submitted_at: 1_700_000_000_000_i64,
            changes: vec![
                models::DeltaSummaryDoc {
                    additions: 0,
                    deletions: 0,
                    file: "src/main.rs".to_string(),
                    hash: String::new(),
                    kind: String::new(),
                    message: None,
                    snapshot_id: String::new(),
                    source: String::new(),
                    timestamp: 0,
                },
                models::DeltaSummaryDoc {
                    additions: 0,
                    deletions: 0,
                    file: "src/lib.rs".to_string(),
                    hash: String::new(),
                    kind: String::new(),
                    message: None,
                    snapshot_id: String::new(),
                    source: String::new(),
                    timestamp: 0,
                },
            ],
        });
        assert_eq!(row.actor, "agent:loop-1");
        assert_eq!(row.files, 2);
        assert_eq!(row.status, "pending");
        assert!(row.detail.contains("snapshot: deadbeef"));
        assert!(row.detail.contains("src/main.rs"));
        assert!(row.detail.contains("src/lib.rs"));
    }

    #[test]
    fn approval_row_without_changes_reports_zero_files() {
        let row = approval_row(&models::PendingApprovalDoc {
            actor: "manual".to_string(),
            snapshot_id: "abc".to_string(),
            submitted_at: 1_700_000_000_000_i64,
            changes: vec![],
        });
        assert_eq!(row.files, 0);
        assert!(row.detail.contains("files: 0"));
        assert!(!row.detail.contains("--- files ---"));
    }
}
