//! Edit-session list fetcher: staged file edit groups, read-only.
//!
//! Sessions are staged edit batches (in-memory on the server until they
//! commit); this screen lists them with their labels and creation times so
//! the operator can see what is pending. Begin and rollback are dispatched
//! from the shell's key actions against the active execution's actor.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{EditSessionRow, ScreenData};

use super::common::{format_ts, page_of};

/// Build the edit-session rows from the paged sessions endpoint.
pub(super) async fn fetch_edit_sessions(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let data: serde_json::Value = client.get_json("/api/v1/file-checkpoint/sessions").await?;
    let (items, has_more) = page_of(&data);
    let rows = items.iter().map(session_row).collect();
    Ok((ScreenData::EditSessions(rows), has_more))
}

/// Map one session record to a display row, pre-rendering its viewer text.
fn session_row(session: &serde_json::Value) -> EditSessionRow {
    let id = session
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let label = session
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let created_at = format_ts(
        session
            .get("created_at")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    );
    let mut detail = vec![format!("id: {id}")];
    if !label.is_empty() {
        detail.push(format!("label: {label}"));
    }
    detail.push(format!("created: {created_at}"));
    EditSessionRow {
        id,
        label,
        created_at,
        detail: detail.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_row_renders_identity_and_optional_label() {
        let row = session_row(&serde_json::json!({
            "id": "sess-1700000000-1-0",
            "label": "format file",
            "created_at": 1_700_000_000_000_i64,
        }));
        assert_eq!(row.id, "sess-1700000000-1-0");
        assert_eq!(row.label, "format file");
        assert!(row.detail.contains("label: format file"));
        assert!(row.detail.contains("id: sess-1700000000-1-0"));
    }

    #[test]
    fn session_row_omits_missing_label_line() {
        let row = session_row(&serde_json::json!({
            "id": "sess-1",
            "created_at": 1_700_000_000_000_i64,
        }));
        assert!(row.label.is_empty());
        assert!(!row.detail.contains("label:"));
    }
}
