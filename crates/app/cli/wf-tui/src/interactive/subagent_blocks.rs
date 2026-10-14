//! Sub-agent scrollback blocks: start/end row bookkeeping for jump targets.
//!
//! Each dispatched child keeps one [`SubagentBlock`] keyed by child
//! execution id. Like [`super::tool_blocks::ToolBlock`], the scrollback
//! itself stays plain [`HistoryLine`] rows; this table owns the row
//! bookkeeping (start/end sequence numbers) so keyboard and future mouse
//! jumps can translate a child id into a viewport offset.

use crate::transcript::{HistoryLine, Role};

use super::InteractiveController;

/// Lifecycle rows of one dispatched child: the start row is the jump
/// target, the end row (when settled) closes the range. The linkage and
/// outcome fields feed the mouse hit-testing and footer panel stages;
/// the keyboard jump only needs the start sequence.
#[allow(dead_code)]
pub(crate) struct SubagentBlock {
    id: String,
    name: String,
    tool_call_id: Option<String>,
    start_seq: u64,
    end_seq: Option<u64>,
    success: Option<bool>,
    activity: Option<String>,
}

impl InteractiveController {
    /// Record a child start: one muted row plus jump bookkeeping. A
    /// duplicate start for a live id refreshes the label instead of
    /// pushing a second row.
    pub(crate) fn record_subagent_started(
        &mut self,
        id: &str,
        name: &str,
        tool_call_id: Option<&str>,
        preview: Option<&str>,
    ) {
        if let Some(block) = self.subagent_blocks.get_mut(id) {
            block.name = name.to_string();
            if block.end_seq.is_none() {
                return;
            }
        }
        let mut text = format!("◇ subagent started: {name}");
        if let Some(preview) = preview.map(str::trim).filter(|p| !p.is_empty()) {
            text.push_str(&format!(" · {preview}"));
        }
        let mut rows = vec![HistoryLine::new_role(text, Role::Muted)];
        self.assign_seq(&mut rows);
        let start_seq = rows.first().map(|row| row.seq_no()).unwrap_or(0);
        self.subagent_blocks.insert(
            id.to_string(),
            SubagentBlock {
                id: id.to_string(),
                name: name.to_string(),
                tool_call_id: tool_call_id.map(str::to_string),
                start_seq,
                end_seq: None,
                success: None,
                activity: None,
            },
        );
        if !self.subagent_order.contains(&id.to_string()) {
            self.subagent_order.push(id.to_string());
        }
        // A new child restarts the cycle at the most recent start.
        self.subagent_cycle = 0;
        self.pending_scroll.extend(rows);
    }

    /// Record a child finish: one settled row closing the block range.
    /// Unknown ids still render a row (a missed start never loses the
    /// outcome) but gain no jump bookkeeping beyond the new entry.
    pub(crate) fn record_subagent_ended(&mut self, id: &str, name: &str, success: bool) {
        let mark = if success { "✓" } else { "✗" };
        let role = if success { Role::Add } else { Role::Error };
        let mut rows = vec![HistoryLine::new_role(
            format!("{mark} subagent ended: {name}"),
            role,
        )];
        self.assign_seq(&mut rows);
        let end_seq = rows.first().map(|row| row.seq_no()).unwrap_or(0);
        if let Some(block) = self.subagent_blocks.get_mut(id) {
            block.end_seq = Some(end_seq);
            block.success = Some(success);
        } else {
            self.subagent_blocks.insert(
                id.to_string(),
                SubagentBlock {
                    id: id.to_string(),
                    name: name.to_string(),
                    tool_call_id: None,
                    start_seq: 0,
                    end_seq: Some(end_seq),
                    success: Some(success),
                    activity: None,
                },
            );
            if !self.subagent_order.contains(&id.to_string()) {
                self.subagent_order.push(id.to_string());
            }
        }
        self.pending_scroll.extend(rows);
    }

    /// Note the latest milestone of a running child. No scrollback row:
    /// milestones update the footer snapshot (via the reducer) and the
    /// block activity for future panels.
    pub(crate) fn note_subagent_activity(
        &mut self,
        id: &str,
        tool_call_id: Option<&str>,
        activity: Option<&str>,
    ) {
        if let Some(block) = self.subagent_blocks.get_mut(id) {
            if tool_call_id.is_some() {
                block.tool_call_id = tool_call_id.map(str::to_string);
            }
            if activity.is_some() {
                block.activity = activity.map(str::to_string);
            }
        }
    }

    /// Jump the viewport to a sub-agent start row, cycling from the most
    /// recent start toward older ones on repeated presses. Unknown or
    /// trimmed rows drop their bookkeeping and the jump falls through to
    /// the next candidate; with no candidate the viewport stays put.
    pub(crate) fn jump_to_subagent(&mut self) {
        self.settle_scrollback();
        let width = self.last_layout_width.max(1);
        let total: usize = self
            .scrollback
            .iter()
            .map(|line| line.display_lines(width).len())
            .sum();
        let streaming_len = self
            .streaming
            .as_ref()
            .map(|line| line.display_lines(width).len())
            .unwrap_or(0);
        let full_total = total + streaming_len;
        let order: Vec<String> = self.subagent_order.iter().rev().cloned().collect();
        if order.is_empty() {
            return;
        }
        let mut attempts = 0;
        while attempts < order.len() {
            let idx = (self.subagent_cycle + attempts) % order.len();
            let Some(block) = self.subagent_blocks.get(&order[idx]) else {
                attempts += 1;
                continue;
            };
            if block.start_seq == 0 {
                attempts += 1;
                continue;
            }
            let Some(pos) = self
                .scrollback
                .iter()
                .position(|row| row.seq_no() == block.start_seq)
            else {
                // Trimmed away: drop the bookkeeping like tool blocks do.
                self.subagent_blocks.remove(&order[idx]);
                self.subagent_order.retain(|id| id != &order[idx]);
                continue;
            };
            let above: usize = self.scrollback[..pos]
                .iter()
                .map(|line| line.display_lines(width).len())
                .sum();
            // Rows below the target (plus the live tail) become the new
            // offset so the target lands at the top of the viewport.
            self.view_scroll = full_total.saturating_sub(above);
            self.scroll_at_top = false;
            self.subagent_cycle = idx + 1;
            return;
        }
    }
}
