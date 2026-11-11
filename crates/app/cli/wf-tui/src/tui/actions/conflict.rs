//! Conflict resolution and rebase helpers.

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, ApprovalRow, CheckpointRow, ConflictRow, ScreenData, ScreenKind};
use wf_client::WfClient;

use super::*;
use crate::tui::data_cache::DataResult;
use crate::tui::TuiApp;

impl TuiApp {
    /// Currently selected conflict row, or `None` off the conflicts screen.
    pub(crate) fn selected_conflict_row(&self) -> Option<ConflictRow> {
        let ScreenData::Conflicts(rows) = self.display_data() else {
            return None;
        };
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(rows.len().saturating_sub(1));
        rows.get(idx).cloned()
    }

    /// Resolve the selected conflict file by accepting one whole side. Binary
    /// files are refused; main-line conflicts prompt for the feature name.
    pub(crate) fn resolve_selected_conflict_side(&mut self, accept_ours: bool) {
        let Some(row) = self.selected_conflict_row() else {
            self.app.notice.set("No conflict selected.".to_string());
            return;
        };
        if row.binary {
            self.app.notice.set(
                "Binary conflicts cannot be resolved in the TUI; use the CLI or web app."
                    .to_string(),
            );
            return;
        }
        match row.partition.strip_prefix("refs/wf/feat/") {
            Some(feature) => {
                let feature = feature.to_string();
                self.spawn_conflict_resolution(row, feature, accept_ours);
            }
            None => {
                let rx =
                    self.modals
                        .push_with_result(Box::new(crate::prompt_modal::PromptModal::new(
                            "Resolve on main",
                            "Feature name",
                        )));
                let tx = self.feedback_tx.clone();
                let client = self.client.clone();
                self.tasks.push(tokio::spawn(async move {
                    let feature = match rx.await {
                        Ok(ModalResult::Value(value)) if !value.trim().is_empty() => value,
                        _ => {
                            let _ = tx.send(Feedback::Notice("Resolve cancelled".to_string()));
                            return;
                        }
                    };
                    let outcome = resolve_conflict_side(&client, &row, &feature, accept_ours).await;
                    let _ = tx.send(Feedback::Refresh(ScreenKind::Conflicts));
                    let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
                    let _ = tx.send(Feedback::Notice(outcome));
                }));
            }
        }
    }

    /// Spawn the side-choice resolution for a feature-ref conflict.
    pub(crate) fn spawn_conflict_resolution(
        &mut self,
        row: ConflictRow,
        feature: String,
        accept_ours: bool,
    ) {
        let tx = self.feedback_tx.clone();
        let client = self.client.clone();
        self.tasks.push(tokio::spawn(async move {
            let outcome = resolve_conflict_side(&client, &row, &feature, accept_ours).await;
            let _ = tx.send(Feedback::Refresh(ScreenKind::Conflicts));
            let _ = tx.send(Feedback::Refresh(ScreenKind::FileWorkspace));
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }
}

/// Read the conflicted bytes at the merge commit, rebuild the whole file
/// with one side per region, and submit the resolution. Returns the user
/// notice text; never panics on a missing payload.
pub(crate) async fn resolve_conflict_side(
    client: &WfClient,
    row: &ConflictRow,
    feature: &str,
    accept_ours: bool,
) -> String {
    let actor = "manual";
    let content_path = format!(
        "/api/v1/file-checkpoint/content?actor={}&path={}&version={}",
        crate::fetch::common::encode_segment(actor),
        crate::fetch::common::encode_segment(&row.path),
        crate::fetch::common::encode_segment(&row.snapshot_id),
    );
    let content: serde_json::Value = match client.get_json(&content_path).await {
        Ok(value) => value,
        Err(err) => return format!("Conflict read failed: {err}"),
    };
    if content
        .get("is_binary")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return "Binary conflict: resolve it in the CLI or web app.".to_string();
    }
    if content
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return "Conflicted file is truncated (over 1 MiB); resolve it in the CLI or web app."
            .to_string();
    }
    let Some(text) = content.get("content").and_then(|v| v.as_str()) else {
        return "Conflicted content is empty; resolve it in the CLI or web app.".to_string();
    };
    let Some(resolved) = rebuild_conflict_content(text, accept_ours) else {
        return "No conflict markers found in the merged file; nothing to resolve.".to_string();
    };
    let body = serde_json::json!({
        "actor": actor,
        "feature": feature,
        "resolutions": { row.path.clone(): resolved },
    });
    match client
        .post_json::<_, serde_json::Value>("/api/v1/file-checkpoint/conflicts/resolve", &body)
        .await
    {
        Ok(value) => {
            let remaining = value.get("remaining").and_then(|v| v.as_u64()).unwrap_or(0);
            if remaining == 0 {
                format!("Resolved {}; no conflicts remain", row.path)
            } else {
                format!(
                    "Resolved {}; {remaining} file(s) still conflicted",
                    row.path
                )
            }
        }
        Err(err) => format!("Resolution failed: {err}"),
    }
}

/// Rebuild a whole-file resolution from diff3 markers, keeping one side per
/// region and every non-region line verbatim. Returns `None` when the bytes
/// carry no markers (already resolved or binary text).
pub(crate) fn rebuild_conflict_content(content: &str, accept_ours: bool) -> Option<String> {
    // 0 = normal text, 1 = ours, 2 = theirs, 3 = base (diff3).
    let mut state = 0u8;
    let mut saw_marker = false;
    let mut out: Vec<&str> = Vec::new();
    for line in content.lines() {
        match (state, line) {
            (0, "<<<<<<< ours") => {
                state = 1;
                saw_marker = true;
            }
            (1, l) if l.starts_with("|||||||") => state = 3,
            (1, "=======") => state = 2,
            (3, "=======") => state = 2,
            (2, l) if l.starts_with(">>>>>>>") => state = 0,
            (1, l) => {
                if accept_ours {
                    out.push(l);
                }
            }
            (3, _) => {}
            (2, l) => {
                if !accept_ours {
                    out.push(l);
                }
            }
            _ => out.push(line),
        }
    }
    if !saw_marker || state != 0 {
        return None;
    }
    let mut resolved = out.join("\n");
    if content.ends_with('\n') {
        resolved.push('\n');
    }
    Some(resolved)
}

pub(crate) async fn toggle_execution(
    client: &WfClient,
    id: &str,
    paused: bool,
) -> Result<(), wf_client::ClientError> {
    let action = if paused { "resume" } else { "pause" };
    if client
        .post_json::<_, serde_json::Value>(
            &format!("/api/v1/executions/{id}/{action}"),
            &serde_json::json!({}),
        )
        .await
        .is_ok()
    {
        return Ok(());
    }
    client
        .post_json::<_, serde_json::Value>(
            &format!("/api/v1/agent-loops/{id}/{action}"),
            &serde_json::json!({}),
        )
        .await
        .map(|_| ())
}

pub(crate) async fn cancel_execution(
    client: &WfClient,
    id: &str,
) -> Result<(), wf_client::ClientError> {
    if client
        .post_json::<_, serde_json::Value>(
            &format!("/api/v1/executions/{id}/cancel"),
            &serde_json::json!({}),
        )
        .await
        .is_ok()
    {
        return Ok(());
    }
    client
        .post_json::<_, serde_json::Value>(
            &format!("/api/v1/agent-loops/{id}/cancel"),
            &serde_json::json!({}),
        )
        .await
        .map(|_| ())
}

/// Parsed baseline selector for a sync rebase: every field is optional and
/// serializes as null when absent, mirroring the CLI flags.
pub(crate) struct RebaseParams {
    pub(crate) branch: Option<String>,
    pub(crate) commit_prefix: Option<String>,
    pub(crate) behind: Option<u64>,
    pub(crate) mode: String,
}

impl RebaseParams {
    pub(crate) fn describe(&self) -> String {
        format!(
            "branch={} prefix={} behind={} mode={}",
            self.branch.as_deref().unwrap_or("-"),
            self.commit_prefix.as_deref().unwrap_or("-"),
            self.behind
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string()),
            self.mode
        )
    }
}

/// Parse the single-line rebase selector the prompt collects: a bare token
/// is the branch, `prefix=` / `behind=` / `mode=` set the named fields.
/// Commit prefix and behind stay mutually exclusive, like the server rule.
pub(crate) fn parse_rebase_selector(selector: &str) -> Result<RebaseParams, String> {
    let mut branch: Option<String> = None;
    let mut commit_prefix: Option<String> = None;
    let mut behind: Option<u64> = None;
    let mut mode = "borrow".to_string();
    for token in selector.split_whitespace() {
        if let Some(prefix) = token.strip_prefix("prefix=") {
            if prefix.is_empty() {
                return Err("Rebase prefix is empty.".to_string());
            }
            commit_prefix = Some(prefix.to_string());
        } else if let Some(n) = token.strip_prefix("behind=") {
            let parsed: u64 = n
                .parse()
                .map_err(|_| format!("Rebase behind is not a number: {n}"))?;
            behind = Some(parsed);
        } else if let Some(name) = token.strip_prefix("mode=") {
            if name != "borrow" && name != "copy" {
                return Err(format!("Rebase mode must be borrow or copy: {name}"));
            }
            mode = name.to_string();
        } else {
            if branch.is_some() {
                return Err(format!("Rebase takes one branch: {token}"));
            }
            branch = Some(token.to_string());
        }
    }
    if commit_prefix.is_some() && behind.is_some() {
        return Err("Commit prefix and behind are mutually exclusive.".to_string());
    }
    Ok(RebaseParams {
        branch,
        commit_prefix,
        behind,
        mode,
    })
}

/// Map a sync write failure onto a user notice: conflicts ask for a later
/// retry without auto-retrying, missing state points at the file-checkpoint
/// state, and validation failures keep the server reason verbatim.
pub(crate) fn sync_failure_notice(label: &str, err: &wf_client::ClientError) -> String {
    let text = err.to_string();
    if text.contains("not found") || text.contains("NotFound") {
        format!("{label}: not found — check the file-checkpoint state ({text})")
    } else if text.contains("CONFLICT")
        || text.contains("Conflict")
        || text.contains("conflict")
        || text.contains("already running")
    {
        format!("{label}: conflict — retry later ({text})")
    } else {
        format!("{label} failed: {text}")
    }
}

/// Step `current` up or down the `steps` ladder, clamping at the ends.
pub(crate) fn step_option(current: usize, steps: &[usize], up: bool) -> usize {
    if up {
        steps
            .iter()
            .find(|step| **step > current)
            .copied()
            .unwrap_or(current)
    } else {
        steps
            .iter()
            .rev()
            .find(|step| **step < current)
            .copied()
            .unwrap_or(current)
    }
}
