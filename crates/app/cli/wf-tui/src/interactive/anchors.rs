//! Unified jump anchors for the session scrollback.
//!
//! One table owns every keyboard jump target: user prompts, todo updates,
//! tool failures, sub-agent starts, checkpoints and human decisions. The
//! scrollback stays plain history rows; this table maps an anchor identity
//! to the absolute sequence number of its row so jumps can translate an
//! anchor into a viewport offset. Trimmed rows drop their bookkeeping on
//! the next jump, matching the tool-block convention.

use wf_types::events::{BaseEvent, EventType};

use super::InteractiveController;
use tui_components::scrollbar;

/// What happened at the anchor row. Names describe the behaviour, not the
/// implementation that produced the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnchorKind {
    UserPrompt,
    TodoUpdate,
    ToolFailure,
    SubAgent,
    Checkpoint,
    Decision,
}

impl AnchorKind {
    /// Short label used in timeline listings and badge hints.
    pub fn label(self) -> &'static str {
        match self {
            AnchorKind::UserPrompt => "prompt",
            AnchorKind::TodoUpdate => "todo",
            AnchorKind::ToolFailure => "failed",
            AnchorKind::SubAgent => "subagent",
            AnchorKind::Checkpoint => "checkpoint",
            AnchorKind::Decision => "decision",
        }
    }

    /// Every anchor kind, for filter-aware listings.
    pub fn all() -> [AnchorKind; 6] {
        [
            AnchorKind::UserPrompt,
            AnchorKind::TodoUpdate,
            AnchorKind::ToolFailure,
            AnchorKind::SubAgent,
            AnchorKind::Checkpoint,
            AnchorKind::Decision,
        ]
    }

    /// Cycle key shown in listings, kept next to the keys that honor it.
    pub fn shortcut_hint(self) -> &'static str {
        match self {
            AnchorKind::UserPrompt => "Ctrl+P",
            AnchorKind::TodoUpdate => "Ctrl+U",
            AnchorKind::ToolFailure => "Ctrl+F",
            AnchorKind::SubAgent => "Ctrl+G",
            AnchorKind::Checkpoint => "Ctrl+N",
            AnchorKind::Decision => "Ctrl+X",
        }
    }
}

/// Identity of one anchor: its kind plus the domain id behind the row
/// (tool call id, checkpoint id, child execution id, or the row sequence
/// for prompt rows without a domain id).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorKey {
    pub kind: AnchorKind,
    pub id: String,
}

/// One jump target: the row sequence plus a one-line label for listings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorEntry {
    pub kind: AnchorKind,
    pub id: String,
    pub label: String,
    pub seq: u64,
}

/// First line of `text`, trimmed to `max` characters for timeline labels.
pub fn first_line_label(text: &str, max: usize) -> String {
    let first = text.lines().next().unwrap_or("").trim();
    if first.len() <= max {
        return first.to_string();
    }
    let mut out = String::new();
    for c in first.chars() {
        if out.len() + c.len_utf8() > max.saturating_sub(1) {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// Display rows kept above a jump target so the landing keeps context.
const JUMP_CONTEXT_ROWS: usize = 1;

impl InteractiveController {
    /// Record or refresh one anchor. A repeated id refreshes its label
    /// instead of duplicating the order entry. A new anchor restarts that
    /// kind's cycle at the most recent entry.
    pub(crate) fn record_anchor(&mut self, kind: AnchorKind, id: &str, label: String, seq: u64) {
        if seq == 0 || id.is_empty() {
            return;
        }
        let key = AnchorKey {
            kind,
            id: id.to_string(),
        };
        if let Some(entry) = self.anchors.get_mut(&key) {
            entry.label = label;
            entry.seq = seq;
            return;
        }
        self.anchors.insert(
            key.clone(),
            AnchorEntry {
                kind,
                id: id.to_string(),
                label,
                seq,
            },
        );
        if !self.anchor_order.contains(&key) {
            self.anchor_order.push(key);
        }
        self.anchor_cycle.insert(kind, 0);
    }

    /// Switch one anchor kind on or off for jumps, badge hints and the
    /// timeline listing. Recording keeps every kind so toggling never
    /// loses data; only navigation reads the switch. No settings surface
    /// wires it yet, so the method waits for that layer like the cached
    /// label helper below.
    #[allow(dead_code)]
    pub(crate) fn set_anchor_kind_enabled(&mut self, kind: AnchorKind, enabled: bool) {
        if enabled {
            self.anchor_disabled.remove(&kind);
        } else {
            self.anchor_disabled.insert(kind);
        }
    }

    /// Whether jumps may land on this kind. All kinds jump by default.
    fn anchor_kind_allowed(&self, kind: AnchorKind) -> bool {
        !self.anchor_disabled.contains(&kind)
    }

    /// Anchors of one kind, newest first.
    fn anchors_of_kind(&self, kind: AnchorKind) -> Vec<AnchorKey> {
        self.anchor_order
            .iter()
            .rev()
            .filter(|key| key.kind == kind)
            .cloned()
            .collect()
    }

    /// Timeline of all anchors, newest first.
    pub(crate) fn anchor_timeline(&self) -> Vec<AnchorEntry> {
        self.anchor_order
            .iter()
            .rev()
            .filter_map(|key| self.anchors.get(key).cloned())
            .collect()
    }

    /// Timeline of enabled anchors, newest first. Every jump target reads
    /// this so a disabled kind disappears from jumps, hints and listings.
    fn enabled_anchor_timeline(&self) -> Vec<AnchorEntry> {
        self.anchor_timeline()
            .into_iter()
            .filter(|entry| self.anchor_kind_allowed(entry.kind))
            .collect()
    }

    /// Sequence number of the row at the viewport top. Shared by the badge
    /// hint and the cross-kind jumps so all three agree on the cursor.
    fn viewport_top_seq(&self) -> u64 {
        let width = self.last_layout_width.max(1);
        let capacity = usize::from(self.last_layout_height.max(1));
        let total: usize = self
            .scrollback
            .iter()
            .map(|line| line.display_lines(width).len())
            .sum();
        let start = scrollbar::top_for_tail(total, capacity, self.view_scroll);
        let mut consumed = 0usize;
        let mut top_seq = 0u64;
        for line in &self.scrollback {
            let height = line.display_lines(width).len();
            if consumed + height > start {
                top_seq = line.seq_no();
                break;
            }
            consumed += height;
            top_seq = line.seq_no();
        }
        top_seq
    }

    /// Jump the viewport so the target sequence lands one row below the
    /// top, keeping a line of context above it like message jumps do.
    /// Targets inside the last viewport page pin to the tail instead.
    pub(crate) fn jump_to_seq(&mut self, seq: u64) {
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
        let Some(pos) = self.scrollback.iter().position(|row| row.seq_no() == seq) else {
            return;
        };
        let above: usize = self.scrollback[..pos]
            .iter()
            .map(|line| line.display_lines(width).len())
            .sum();
        let capacity = usize::from(self.last_layout_height.max(1));
        let max_scroll = full_total.saturating_sub(capacity);
        let desired_top = above.saturating_sub(JUMP_CONTEXT_ROWS);
        self.view_scroll = max_scroll.saturating_sub(desired_top);
        self.scroll_at_top = false;
    }

    /// Cycle through one kind newest-first. Trimmed rows drop their
    /// bookkeeping and the jump falls through to the next candidate; with
    /// no candidate the viewport stays put. Disabled kinds stay put too.
    pub(crate) fn jump_to_anchor_kind(&mut self, kind: AnchorKind) {
        self.settle_scrollback();
        if !self.anchor_kind_allowed(kind) {
            return;
        }
        let order = self.anchors_of_kind(kind);
        if order.is_empty() {
            return;
        }
        let start = self.anchor_cycle.get(&kind).copied().unwrap_or(0);
        let mut attempts = 0;
        while attempts < order.len() {
            let idx = (start + attempts) % order.len();
            let Some(entry) = self.anchors.get(&order[idx]).cloned() else {
                attempts += 1;
                continue;
            };
            if self
                .scrollback
                .iter()
                .position(|row| row.seq_no() == entry.seq)
                .is_none()
            {
                self.anchors.remove(&order[idx]);
                self.anchor_order.retain(|key| key != &order[idx]);
                continue;
            };
            self.jump_to_seq(entry.seq);
            self.anchor_cycle.insert(kind, idx + 1);
            return;
        }
    }

    /// Nearest enabled anchor at or above the current viewport top, for badge hints.
    pub(crate) fn anchor_above_viewport(&self) -> Option<AnchorEntry> {
        if self.scrollback.is_empty() {
            return None;
        }
        let top_seq = self.viewport_top_seq();
        let mut best: Option<&AnchorEntry> = None;
        for key in &self.anchor_order {
            let Some(entry) = self.anchors.get(key) else {
                continue;
            };
            if !self.anchor_kind_allowed(entry.kind) {
                continue;
            }
            if entry.seq > top_seq {
                continue;
            }
            let replace = best.is_none_or(|current: &AnchorEntry| entry.seq >= current.seq);
            if replace {
                best = Some(entry);
            }
        }
        best.cloned()
    }

    /// Drop anchors whose rows were trimmed away.
    pub(crate) fn prune_anchors(&mut self) {
        if self.scrollback.is_empty() {
            return;
        }
        let live: std::collections::HashSet<u64> =
            self.scrollback.iter().map(|row| row.seq_no()).collect();
        self.anchors
            .retain(|_, entry| entry.seq == 0 || live.contains(&entry.seq));
        let still_live: std::collections::HashSet<AnchorKey> =
            self.anchors.keys().cloned().collect();
        self.anchor_order.retain(|key| still_live.contains(key));
    }

    /// Rebuild prompt, todo, failure and sub-agent anchors by scanning
    /// replayed rows. Rows already carry absolute sequence numbers, so the
    /// scan only maps row prefixes to anchor identities.
    pub(crate) fn rebuild_anchors_from_scrollback(&mut self) {
        self.anchors.clear();
        self.anchor_order.clear();
        self.anchor_cycle.clear();
        let snapshot: Vec<(u64, String)> = self
            .scrollback
            .iter()
            .map(|row| (row.seq_no(), row.raw_lines(1000).join(" ")))
            .collect();
        for (seq, text) in snapshot {
            if seq == 0 {
                continue;
            }
            let trimmed = text.trim();
            if let Some(body) = trimmed.strip_prefix("◆ ") {
                if body.is_empty() {
                    continue;
                }
                self.record_anchor(
                    AnchorKind::Decision,
                    &format!("decision-{seq}"),
                    first_line_label(body, 60),
                    seq,
                );
            } else if trimmed.starts_with("❯ ") || trimmed.starts_with("> ") {
                let body = trimmed
                    .strip_prefix("❯ ")
                    .or_else(|| trimmed.strip_prefix("> "))
                    .unwrap_or(trimmed)
                    .trim();
                if body.is_empty() {
                    continue;
                }
                self.record_anchor(
                    AnchorKind::UserPrompt,
                    &format!("prompt-{seq}"),
                    first_line_label(body, 60),
                    seq,
                );
            } else if trimmed.starts_with("◇ subagent started:") {
                let name = trimmed
                    .strip_prefix("◇ subagent started:")
                    .unwrap_or("")
                    .trim();
                self.record_anchor(
                    AnchorKind::SubAgent,
                    &format!("subagent-{seq}"),
                    first_line_label(name, 60),
                    seq,
                );
            } else if trimmed.starts_with("✓ update_todo_list") {
                self.record_anchor(
                    AnchorKind::TodoUpdate,
                    &format!("todo-{seq}"),
                    first_line_label(trimmed, 60),
                    seq,
                );
            } else if trimmed.starts_with("✗ ") {
                self.record_anchor(
                    AnchorKind::ToolFailure,
                    &format!("failed-{seq}"),
                    first_line_label(trimmed, 60),
                    seq,
                );
            } else if trimmed.starts_with("⬢ checkpoint ") {
                self.record_anchor(
                    AnchorKind::Checkpoint,
                    &format!("checkpoint-{seq}"),
                    first_line_label(trimmed, 60),
                    seq,
                );
            }
        }
        // A rebuilt table restarts every cycle at the most recent entry.
        for kind in [
            AnchorKind::UserPrompt,
            AnchorKind::TodoUpdate,
            AnchorKind::ToolFailure,
            AnchorKind::SubAgent,
            AnchorKind::Checkpoint,
            AnchorKind::Decision,
        ] {
            if self.anchors_of_kind(kind).is_empty() {
                continue;
            }
            self.anchor_cycle.insert(kind, 0);
        }
    }

    /// Record a checkpoint engine event as a jump anchor. Only the created
    /// event builds an anchor; failures render an error row without a jump
    /// target and every other engine event stays ignored.
    pub(crate) fn record_engine_anchor(&mut self, event: &BaseEvent) {
        use crate::transcript::{HistoryLine, Role};
        match event.r#type {
            EventType::CheckpointCreated => {
                let checkpoint_id = event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("checkpoint_id"))
                    .and_then(|value| value.as_str())
                    .unwrap_or(event.id.as_str());
                let short = checkpoint_id.chars().take(8).collect::<String>();
                let mut rows = vec![HistoryLine::new_role(
                    format!("⬢ checkpoint {short}"),
                    Role::Muted,
                )];
                self.assign_seq(&mut rows);
                let seq = rows.first().map(|row| row.seq_no()).unwrap_or(0);
                self.record_anchor(
                    AnchorKind::Checkpoint,
                    checkpoint_id,
                    first_line_label(&format!("checkpoint {short}"), 60),
                    seq,
                );
                self.pending_scroll.extend(rows);
            }
            EventType::CheckpointFailed => {
                self.pending_scroll.push(HistoryLine::new_role(
                    "✗ checkpoint creation failed".to_string(),
                    Role::Error,
                ));
            }
            _ => {}
        }
    }

    /// One-line timeline listing, newest first, for the anchor index key.
    /// Disabled kinds stay out of the listing; the header names the keys
    /// that produced the visible rows.
    pub(crate) fn list_anchors(&mut self) {
        use crate::transcript::{HistoryLine, Role};
        self.settle_scrollback();
        let timeline = self.enabled_anchor_timeline();
        if timeline.is_empty() {
            self.pending_scroll.push(HistoryLine::new_role(
                "no anchors yet · prompts, todos, failures, subagents, decisions and checkpoints land here"
                    .to_string(),
                Role::Muted,
            ));
            return;
        }
        let keys = AnchorKind::all()
            .into_iter()
            .filter(|kind| self.anchor_kind_allowed(*kind))
            .map(|kind| format!("{} {}", kind.shortcut_hint(), kind.label()))
            .collect::<Vec<_>>()
            .join(" · ");
        self.pending_scroll.push(HistoryLine::new_role(
            format!("anchors · newest first ({keys})"),
            Role::Muted,
        ));
        for entry in timeline.iter().take(30) {
            self.pending_scroll.push(HistoryLine::new_role(
                format!("{} · {}", entry.kind.label(), entry.label),
                Role::Muted,
            ));
        }
    }

    /// Jump to the next anchor toward the tail relative to the viewport top.
    pub(crate) fn jump_to_next_anchor(&mut self) {
        self.settle_scrollback();
        let top_seq = self.viewport_top_seq();
        let mut newer: Vec<AnchorEntry> = self
            .enabled_anchor_timeline()
            .into_iter()
            .filter(|entry| entry.seq > top_seq)
            .collect();
        newer.sort_by_key(|entry| entry.seq);
        let Some(target) = newer.first() else {
            self.jump_to_latest();
            return;
        };
        self.jump_to_seq(target.seq);
    }

    /// Jump to the previous anchor toward the head relative to the viewport top.
    pub(crate) fn jump_to_prev_anchor(&mut self) {
        self.settle_scrollback();
        let top_seq = self.viewport_top_seq();
        let mut older: Vec<AnchorEntry> = self
            .enabled_anchor_timeline()
            .into_iter()
            .filter(|entry| entry.seq < top_seq)
            .collect();
        older.sort_by_key(|entry| std::cmp::Reverse(entry.seq));
        let Some(target) = older.first() else {
            return;
        };
        self.jump_to_seq(target.seq);
    }
}

/// Cached anchor counts for tests without a controller.
#[allow(dead_code)]
pub fn anchor_kind_label(kind: AnchorKind) -> &'static str {
    kind.label()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn first_line_label_truncates() {
        assert_eq!(first_line_label("hello\nworld", 60), "hello");
        let long = "a".repeat(100);
        let label = first_line_label(&long, 10);
        assert!(label.chars().count() <= 10);
        assert!(label.ends_with('…'));
    }

    #[test]
    fn anchor_kind_labels_describe_behaviour() {
        assert_eq!(AnchorKind::UserPrompt.label(), "prompt");
        assert_eq!(AnchorKind::TodoUpdate.label(), "todo");
        assert_eq!(AnchorKind::ToolFailure.label(), "failed");
        assert_eq!(AnchorKind::SubAgent.label(), "subagent");
        assert_eq!(AnchorKind::Checkpoint.label(), "checkpoint");
        assert_eq!(AnchorKind::Decision.label(), "decision");
    }

    #[test]
    fn anchor_kinds_cover_all_jump_targets_with_distinct_keys() {
        let all = AnchorKind::all();
        assert_eq!(all.len(), 6);
        let mut hints = all
            .iter()
            .map(|kind| kind.shortcut_hint())
            .collect::<Vec<_>>();
        hints.sort_unstable();
        hints.dedup();
        assert_eq!(hints.len(), all.len());
    }

    #[test]
    fn anchor_key_hashes_by_kind_and_id() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(AnchorKey {
            kind: AnchorKind::TodoUpdate,
            id: "t1".to_string(),
        });
        assert!(set.contains(&AnchorKey {
            kind: AnchorKind::TodoUpdate,
            id: "t1".to_string(),
        }));
        assert!(!set.contains(&AnchorKey {
            kind: AnchorKind::ToolFailure,
            id: "t1".to_string(),
        }));
    }

    #[test]
    fn unused_import_guard() {
        let _ = HashMap::<String, String>::new();
    }

    async fn controller_with_rows(count: usize) -> InteractiveController {
        let client = wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
        let mut ctl = InteractiveController::start(client, "exec".into()).await;
        ctl.last_layout_width = 80;
        ctl.last_layout_height = 24;
        let mut rows: Vec<crate::transcript::HistoryLine> = (0..count)
            .map(|index| crate::transcript::HistoryLine::new(format!("row {index:03}")))
            .collect();
        ctl.assign_seq(&mut rows);
        ctl.scrollback = rows;
        ctl
    }

    #[tokio::test]
    async fn jump_lands_target_one_row_below_the_top() {
        let mut ctl = controller_with_rows(100).await;
        ctl.jump_to_seq(50);
        assert_eq!(ctl.view_scroll, 28);
        assert_eq!(ctl.viewport_top_seq(), 49);
    }

    #[tokio::test]
    async fn disabled_kinds_stay_out_of_jumps_but_keep_recording() {
        let mut ctl = controller_with_rows(100).await;
        ctl.record_anchor(AnchorKind::UserPrompt, "prompt-10", "prompt".into(), 10);
        ctl.record_anchor(AnchorKind::TodoUpdate, "todo-20", "todo".into(), 20);
        assert_eq!(ctl.anchor_timeline().len(), 2);
        ctl.set_anchor_kind_enabled(AnchorKind::TodoUpdate, false);
        ctl.jump_to_anchor_kind(AnchorKind::TodoUpdate);
        assert_eq!(ctl.view_scroll, 0);
        ctl.jump_to_anchor_kind(AnchorKind::UserPrompt);
        assert!(ctl.view_scroll > 0);
        assert_eq!(ctl.anchor_above_viewport().map(|entry| entry.kind), None);
        ctl.jump_to_latest();
        assert_eq!(
            ctl.anchor_above_viewport().map(|entry| entry.kind.label()),
            Some("prompt")
        );
        ctl.set_anchor_kind_enabled(AnchorKind::TodoUpdate, true);
        assert_eq!(
            ctl.anchor_above_viewport().map(|entry| entry.kind.label()),
            Some("todo")
        );
    }
}
