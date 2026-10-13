//! Tool output blocks: per-call capped excerpts with collapsible rows.
//!
//! Each finished tool call keeps one [`ToolBlock`] keyed by tool call id.
//! The scrollback itself stays plain [`HistoryLine`] rows: one summary row
//! followed by preview or full body rows. Toggling a block splices its body
//! rows in place and re-prepares the scrollback, so the preparation cache
//! needs no new key inputs.
//!
//! Presentation dispatches on the protocol display payload when the server
//! provides one and falls back to name-based inference for older servers
//! and persisted history. Either way the frontend applies only generic
//! truncation; per-tool presentation knowledge lives in `wf-tools`.

use tui_core::tool_output::{
    cap_block_bytes, collapse_output, collapse_shell_tail, infer_kind, summarize_activity,
    Collapsed, DetailLevel, ToolOutputConfig,
};
use wf_types::tool::{DisplayKind, DisplayPayload};

use crate::transcript::{HistoryLine, Role};

use super::InteractiveController;

/// Capped excerpt of one finished tool call plus its row bookkeeping.
/// `summary_seq` locates the summary row after scrollback trims shift
/// indices; `body_len` is the body row count currently on screen.
pub(crate) struct ToolBlock {
    tool_call_id: String,
    tool_name: String,
    success: bool,
    output: String,
    output_capped: bool,
    error: Option<String>,
    kind: DisplayKind,
    /// Tool-side preview text, when the server provided a display payload.
    display_preview: Option<String>,
    /// Whether the payload reported truncation (body hidden by design).
    display_truncated: bool,
    expandable: bool,
    expanded: bool,
    summary_seq: u64,
    body_len: usize,
}

/// Full-output viewing request drained by the application shell into a
/// read-only viewer modal.
pub(crate) struct ToolViewerRequest {
    pub title: String,
    pub content: String,
    pub is_diff: bool,
}

impl ToolBlock {
    /// Preview source: the error wins on failure, the display preview or
    /// the output otherwise.
    fn preview_source(&self) -> &str {
        if !self.success {
            if let Some(error) = self.error.as_deref() {
                if !error.trim().is_empty() {
                    return error;
                }
            }
        }
        self.display_preview.as_deref().unwrap_or(&self.output)
    }

    fn collapsed_preview(&self, config: &ToolOutputConfig) -> Collapsed {
        let source = self.preview_source();
        let error_first = !self.success
            && self
                .error
                .as_deref()
                .is_some_and(|error| !error.trim().is_empty());
        if error_first {
            return collapse_output(source, config.error_preview_lines, config.preview_max_chars);
        }
        match self.kind {
            DisplayKind::ShellTail => {
                collapse_shell_tail(source, config.shell_preview_lines, config.preview_max_chars)
            }
            DisplayKind::Diff | DisplayKind::Count | DisplayKind::Text => {
                collapse_output(source, config.preview_max_lines, config.preview_max_chars)
            }
        }
    }

    /// Retrieval bodies stay hidden by default: the count summary is the
    /// whole inline story and the full text waits in the viewer. This holds
    /// with or without a protocol display payload so live and replay agree.
    fn hides_body(&self) -> bool {
        self.kind == DisplayKind::Count && self.success
    }
}

/// Row role for one body line. Diff rows reuse the add/remove accent roles
/// so unified diffs read as diffs inline; everything else stays muted.
fn body_row_role(kind: DisplayKind, line: &str) -> Role {
    if kind != DisplayKind::Diff {
        return Role::Muted;
    }
    if let Some(rest) = line.strip_prefix("@@") {
        let _ = rest;
        return Role::Accent;
    }
    if line.starts_with('+') {
        return Role::Add;
    }
    if line.starts_with('-') {
        return Role::Remove;
    }
    Role::Muted
}

/// Body rows for a block in its collapsed or expanded form. Expanded rows
/// show the full capped excerpt; on failure the complete error leads.
fn block_rows(block: &ToolBlock, expanded: bool, config: &ToolOutputConfig) -> Vec<HistoryLine> {
    if expanded {
        let mut rows = Vec::new();
        if block.output_capped {
            rows.push(HistoryLine::new_role(
                "[earlier output omitted]",
                Role::Warning,
            ));
        }
        if !block.success {
            if let Some(error) = block.error.as_deref() {
                for line in error.lines() {
                    rows.push(HistoryLine::new_role(line.to_string(), Role::Error));
                }
            }
        }
        for line in block.output.lines() {
            rows.push(HistoryLine::new_role(
                line.to_string(),
                body_row_role(block.kind, line),
            ));
        }
        rows
    } else if block.hides_body() {
        Vec::new()
    } else {
        block
            .collapsed_preview(config)
            .text
            .lines()
            .map(|line| HistoryLine::new_role(line.to_string(), body_row_role(block.kind, line)))
            .collect()
    }
}

/// Build one block plus its collapsed preview. The preview doubles as the
/// expandable signal: only overflowing previews (or hidden retrieval
/// bodies) can expand.
#[allow(clippy::too_many_arguments)]
fn build_block(
    tool_call_id: &str,
    tool_name: &str,
    success: bool,
    output: String,
    output_capped: bool,
    error: Option<String>,
    kind: DisplayKind,
    display: Option<&DisplayPayload>,
    config: &ToolOutputConfig,
) -> (ToolBlock, Collapsed) {
    let mut block = ToolBlock {
        tool_call_id: tool_call_id.to_string(),
        tool_name: tool_name.to_string(),
        success,
        output,
        output_capped,
        error,
        kind,
        display_preview: display.map(|payload| payload.preview.clone()),
        display_truncated: display.is_some_and(|payload| payload.truncated),
        expandable: false,
        expanded: false,
        summary_seq: 0,
        body_len: 0,
    };
    if block.hides_body() {
        block.expandable = block.display_truncated || !block.output.trim().is_empty();
        let preview = block.collapsed_preview(config);
        return (block, preview);
    }
    let preview = block.collapsed_preview(config);
    block.expandable = preview.overflow;
    (block, preview)
}

impl InteractiveController {
    /// Replace the tool-output thresholds (detail level, preview lengths).
    pub(crate) fn set_tool_config(&mut self, config: ToolOutputConfig) {
        self.tool_config = config;
    }

    /// Flush a pending low-level activity group into one summary row.
    /// No-op unless the level is Low and tools were accumulated.
    pub(crate) fn flush_activity(&mut self) {
        if self.activity.is_empty() {
            return;
        }
        let items = std::mem::take(&mut self.activity);
        let summary = summarize_activity(&items);
        let mut rows = vec![HistoryLine::new_role(format!("▪ {summary}"), Role::Muted)];
        self.assign_seq(&mut rows);
        self.pending_scroll.extend(rows);
    }

    /// Cycle the detail level Low -> Medium -> High, flushing any pending
    /// activity group first so rows stay ordered.
    pub(crate) fn cycle_detail_level(&mut self) {
        self.flush_activity();
        let next = self.tool_config.level.cycle();
        self.tool_config.level = next;
        self.pending_scroll.push(HistoryLine::new_role(
            format!("detail level: {}", next.label()),
            Role::Muted,
        ));
    }

    /// Expand all rendered blocks when any of them is collapsed, otherwise
    /// collapse them all. Grouped (unrendered) blocks stay grouped.
    pub(crate) fn toggle_all_tool_blocks(&mut self) {
        let any_collapsed = self.tool_order.iter().any(|id| {
            self.tool_blocks
                .get(id)
                .is_some_and(|block| block.expandable && !block.expanded && block.summary_seq != 0)
        });
        self.set_all_tool_blocks(any_collapsed);
    }

    /// Record a finished tool call as a collapsible block: one summary row
    /// plus collapsed preview rows. With the master switch off this keeps
    /// the legacy one-line lifecycle row. The parameters mirror the stream
    /// event one-to-one, so they stay positional instead of a new struct.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push_tool_end_block(
        &mut self,
        tool_call_id: &str,
        tool_name: &str,
        success: bool,
        result: &str,
        error: Option<&str>,
        display: Option<&DisplayPayload>,
        elapsed_ms: Option<u64>,
    ) {
        let role = if success { Role::Add } else { Role::Error };
        let summary = match (success, elapsed_ms) {
            (true, Some(elapsed)) => format!("✓ {tool_name} ({elapsed}ms)"),
            (true, None) => format!("✓ {tool_name}"),
            (false, _) => format!("✗ {tool_name}"),
        };
        if !self.tool_config.enabled {
            self.pending_scroll
                .push(HistoryLine::new_role(summary, role));
            return;
        }
        let (output, output_capped) = cap_block_bytes(result, self.tool_config.block_max_bytes);
        let error_text = error
            .map(str::to_string)
            .filter(|text| !text.trim().is_empty());
        let kind = display
            .map(|payload| payload.kind)
            .unwrap_or_else(|| infer_kind(tool_name));
        let (mut block, preview) = build_block(
            tool_call_id,
            tool_name,
            success,
            output,
            output_capped,
            error_text,
            kind,
            display,
            &self.tool_config,
        );
        // Low level accumulates successful tools into one activity group
        // instead of rendering rows. Failures stay visible immediately.
        if self.tool_config.level == DetailLevel::Low && success {
            self.tool_order.push(tool_call_id.to_string());
            self.tool_blocks.insert(tool_call_id.to_string(), block);
            self.activity.push((tool_name.to_string(), success));
            return;
        }
        self.flush_activity();
        // High level renders retrieval blocks expanded by default.
        block.expanded = self.tool_config.level == DetailLevel::High
            && block.kind == DisplayKind::Count
            && success;
        let summary = if block.hides_body() {
            // The count suffix stays even when expanded: it names the body.
            if block.expandable {
                match block.display_preview.as_deref() {
                    Some(preview) if !preview.trim().is_empty() => {
                        format!("{summary} · {preview}")
                    }
                    _ => {
                        let total = block.output.lines().count();
                        if total == 0 {
                            summary
                        } else if total == 1 {
                            format!("{summary} · 1 line")
                        } else {
                            format!("{summary} · {total} lines")
                        }
                    }
                }
            } else {
                summary
            }
        } else if preview.overflow {
            if preview.hidden_lines > 0 {
                format!("{summary} · +{} lines", preview.hidden_lines)
            } else {
                format!("{summary} · +more")
            }
        } else {
            summary
        };
        let mut rows = vec![HistoryLine::new_role(summary, role)];
        if !block.hides_body() || block.expanded {
            rows.extend(block_rows(&block, block.expanded, &self.tool_config));
        }
        block.body_len = rows.len() - 1;
        self.assign_seq(&mut rows);
        block.summary_seq = rows.first().map(|row| row.seq_no()).unwrap_or(0);
        self.tool_order.push(tool_call_id.to_string());
        self.tool_blocks.insert(tool_call_id.to_string(), block);
        self.pending_scroll.extend(rows);
    }

    /// Queue a viewer modal for the most recent expandable block. The
    /// application shell drains the request after key handling.
    pub(crate) fn request_last_tool_viewer(&mut self) {
        let target = self.tool_order.iter().rev().find_map(|id| {
            self.tool_blocks
                .get(id)
                .filter(|block| block.expandable)
                .map(|block| block.tool_call_id.clone())
        });
        let Some(tool_call_id) = target else {
            return;
        };
        let Some(block) = self.tool_blocks.get(&tool_call_id) else {
            return;
        };
        let mut content = String::new();
        if !block.success {
            if let Some(error) = block.error.as_deref() {
                content.push_str(error);
                if !content.ends_with('\n') {
                    content.push('\n');
                }
            }
        }
        if block.output_capped {
            content.push_str("[earlier output omitted]\n");
        }
        content.push_str(&block.output);
        self.viewer_request = Some(ToolViewerRequest {
            title: format!("{} · {}", block.tool_name, block.tool_call_id),
            content,
            is_diff: block.kind == DisplayKind::Diff,
        });
    }

    /// Drain a pending viewer request, if any.
    pub(crate) fn take_tool_viewer(&mut self) -> Option<ToolViewerRequest> {
        self.viewer_request.take()
    }

    /// Toggle the most recent rendered expandable block between its
    /// collapsed preview and its full capped excerpt. Grouped blocks stay
    /// grouped; their output waits in the viewer.
    pub(crate) fn toggle_last_tool_block(&mut self) {
        self.settle_scrollback();
        let target = self.tool_order.iter().rev().find_map(|id| {
            self.tool_blocks
                .get(id)
                .filter(|block| block.expandable && block.summary_seq != 0)
                .map(|block| block.tool_call_id.clone())
        });
        let Some(tool_call_id) = target else {
            return;
        };
        self.toggle_tool_block_to(&tool_call_id, None);
    }

    /// Expand or collapse every rendered expandable block. Used by the
    /// detail-level cycle and the expand-all key. Grouped blocks stay grouped.
    pub(crate) fn set_all_tool_blocks(&mut self, expanded: bool) {
        self.settle_scrollback();
        let targets: Vec<String> = self
            .tool_order
            .iter()
            .filter_map(|id| {
                self.tool_blocks
                    .get(id)
                    .filter(|block| {
                        block.expandable && block.expanded != expanded && block.summary_seq != 0
                    })
                    .map(|block| block.tool_call_id.clone())
            })
            .collect();
        for tool_call_id in targets {
            self.toggle_tool_block_to(&tool_call_id, Some(expanded));
        }
    }

    /// Move one block to its expanded or collapsed form (`None` toggles).
    /// Missing summaries (trimmed away) drop their bookkeeping.
    fn toggle_tool_block_to(&mut self, tool_call_id: &str, expanded: Option<bool>) {
        let config = self.tool_config;
        let Some(block) = self.tool_blocks.get(tool_call_id) else {
            return;
        };
        let target = expanded.unwrap_or(!block.expanded);
        if target == block.expanded {
            return;
        }
        let summary_seq = block.summary_seq;
        let old_len = block.body_len;
        let mut rows = block_rows(block, target, &config);
        self.assign_seq(&mut rows);
        let new_len = rows.len();
        if let Some(block) = self.tool_blocks.get_mut(tool_call_id) {
            block.expanded = target;
            block.body_len = new_len;
        }
        self.splice_block_rows(summary_seq, old_len, rows);
    }

    /// Replace the body rows after the summary row carrying `summary_seq`.
    /// The drain range is clamped so concurrent trims cannot panic; a lost
    /// summary drops its bookkeeping instead.
    fn splice_block_rows(&mut self, summary_seq: u64, old_len: usize, new_rows: Vec<HistoryLine>) {
        let Some(summary) = self
            .scrollback
            .iter()
            .position(|row| row.seq_no() == summary_seq)
        else {
            self.tool_blocks
                .retain(|_, block| block.summary_seq != summary_seq);
            self.tool_order.retain(|id| {
                self.tool_blocks
                    .get(id)
                    .is_some_and(|block| block.summary_seq != summary_seq)
            });
            return;
        };
        let start = summary + 1;
        let end = (start + old_len).min(self.scrollback.len());
        self.scrollback.splice(start..end, new_rows);
        let version = self.bump_version();
        let width = self.last_layout_width;
        let scrollback = std::mem::take(&mut self.scrollback);
        self.prep.sync_replace(&scrollback, width, version);
        self.scrollback = scrollback;
    }
}
