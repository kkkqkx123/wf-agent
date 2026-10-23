//! Git-sync fetcher: the reconcile point, the external-update hint and the
//! sync defaults, each rendered as one fixed navigation row with
//! pre-rendered detail text for the read-only viewer.
//!
//! All three endpoints return either a payload or null; null maps to the
//! normal empty states (never exported, aligned, unconfigured). Any single
//! request failing fails the whole fetch — there is no partial fallback.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{FileSyncData, FileSyncRow, ScreenData};

use super::common::format_ts;

fn text(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Build the three sync rows. `hint`, `reconcile` and `config` are the raw
/// endpoint payloads (`Value::Null` when the server reports nothing).
fn sync_rows(
    hint: &serde_json::Value,
    reconcile: &serde_json::Value,
    config: &serde_json::Value,
) -> Vec<FileSyncRow> {
    let mut rows = Vec::with_capacity(3);

    let (reconcile_summary, reconcile_detail) = if reconcile.is_null() {
        (
            "not exported yet".to_string(),
            "reconcile point: none (no export has run yet)".to_string(),
        )
    } else {
        let internal_main = text(reconcile, "internal_main");
        let external_commit = text(reconcile, "external_commit");
        let external_repo = text(reconcile, "external_repo");
        let updated = format_ts(
            reconcile
                .get("updated_at_ms")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
        );
        (
            format!(
                "{} <- {}",
                crate::screens::short_id(&internal_main),
                crate::screens::short_id(&external_commit)
            ),
            [
                format!("internal main: {internal_main}"),
                format!("external commit: {external_commit}"),
                format!("external repo: {external_repo}"),
                format!("updated: {updated}"),
            ]
            .join("\n"),
        )
    };
    rows.push(FileSyncRow {
        kind: "reconcile".to_string(),
        title: "Reconcile point".to_string(),
        summary: reconcile_summary,
        detail: reconcile_detail,
    });

    let (hint_summary, hint_detail) = if hint.is_null() {
        (
            "external head aligned".to_string(),
            "external update: none (the external head matches the reconcile point)".to_string(),
        )
    } else {
        let external_head = text(hint, "external_head");
        let file_count = hint.get("file_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let inner = hint.get("reconcile").cloned().unwrap_or_default();
        let detail = [
            format!("external head: {external_head}"),
            format!("files: {file_count}"),
            format!("internal main: {}", text(&inner, "internal_main")),
            format!("external commit: {}", text(&inner, "external_commit")),
        ]
        .join("\n");
        (
            format!(
                "{} file(s) ahead at {}",
                file_count,
                crate::screens::short_id(&external_head)
            ),
            detail,
        )
    };
    rows.push(FileSyncRow {
        kind: "hint".to_string(),
        title: "External update".to_string(),
        summary: hint_summary,
        detail: hint_detail,
    });

    let (defaults_summary, defaults_detail) = if config.is_null() {
        (
            "sync unconfigured".to_string(),
            "sync defaults: none (no sync section configured)".to_string(),
        )
    } else {
        let auto_commit = config
            .get("auto_commit")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let auto_import = config
            .get("auto_import")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let on_off = |on: bool| if on { "on" } else { "off" };
        (
            format!(
                "auto commit {} · auto import {}",
                on_off(auto_commit),
                on_off(auto_import)
            ),
            [
                format!("auto commit: {}", on_off(auto_commit)),
                format!("auto import: {}", on_off(auto_import)),
            ]
            .join("\n"),
        )
    };
    rows.push(FileSyncRow {
        kind: "defaults".to_string(),
        title: "Sync defaults".to_string(),
        summary: defaults_summary,
        detail: defaults_detail,
    });
    rows
}

/// Fetch the three read-only sync endpoints over plain HTTP and assemble the
/// fixed sync rows. No local fallback: every request goes to `wf-server`.
pub(super) async fn fetch_file_sync(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let hint: serde_json::Value = client.get_json("/api/v1/file-checkpoint/sync/hint").await?;
    let reconcile: serde_json::Value = client
        .get_json("/api/v1/file-checkpoint/sync/reconcile")
        .await?;
    let config: serde_json::Value = client
        .get_json("/api/v1/file-checkpoint/sync/config")
        .await?;
    let auto_commit = config
        .get("auto_commit")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let rows = sync_rows(&hint, &reconcile, &config);
    Ok((
        ScreenData::FileSync(FileSyncData { auto_commit, rows }),
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_payloads_render_as_empty_states() {
        let rows = sync_rows(
            &serde_json::Value::Null,
            &serde_json::Value::Null,
            &serde_json::Value::Null,
        );
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].kind, "reconcile");
        assert!(rows[0].summary.contains("not exported yet"));
        assert!(rows[1].summary.contains("aligned"));
        assert!(rows[2].summary.contains("unconfigured"));
    }

    #[test]
    fn payloads_render_ids_and_counts() {
        let rows = sync_rows(
            &serde_json::json!({
                "external_head": "abcdef123456",
                "file_count": 3,
                "reconcile": {"internal_main": "11111111", "external_commit": "22222222"}
            }),
            &serde_json::json!({
                "internal_main": "11111111",
                "external_commit": "22222222",
                "external_repo": "/srv/repo",
                "updated_at_ms": 1_700_000_000_000_i64
            }),
            &serde_json::json!({"auto_commit": true, "auto_import": false}),
        );
        assert!(rows[0].detail.contains("/srv/repo"));
        assert!(rows[1].summary.contains("3 file(s) ahead"));
        assert!(rows[1].detail.contains("abcdef123456"));
        assert!(rows[2].summary.contains("auto commit on"));
    }
}
