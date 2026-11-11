//! Interactive transcript selection: regions, autoscroll and hover.

use super::*;
use crate::animation::AnimationController;
use crate::approval_overlay::ApprovalRemembered;
use crate::context_sidebar::{ContextSnapshot, SidebarSnapshot, TodoLine};
use crate::footer::Footer;
use crate::pager::ReplayPager;
use crate::prep_cache::PreparedScrollback;
use crate::reducer::Phase;
use crate::reducer::SessionReducer;
use crate::stream_pacer::SegmentedPacer;
use crate::terminal::{DoublePressTracker, ESC_DOUBLE_PRESS_WINDOW, SIGINT_DOUBLE_PRESS_WINDOW};
use crate::transcript::HistoryLine;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use wf_client::WfClient;
use wf_types::events::ExecutionStreamEvent;
use wf_types::interaction::tool_approval::ToolApprovalRequestData;

impl InteractiveController {
    /// Current transcript drag selection, if a content drag is in progress.
    pub fn transcript_selection(&self) -> Option<TranscriptSelection> {
        self.transcript_selection
    }

    /// Set the transcript drag selection for highlight and release copy.
    pub fn set_transcript_selection(&mut self, selection: TranscriptSelection) {
        self.transcript_selection = Some(selection);
    }

    /// Move the active selection into the committed regions so an additive
    /// drag can start another disjoint range without losing the old one.
    pub fn begin_additive_transcript_selection(&mut self) {
        if let Some(selection) = self.transcript_selection.take() {
            if selection.anchor != selection.head {
                self.transcript_regions.push(selection);
            }
        }
    }

    /// Whether any transcript range (active or committed) exists.
    pub fn has_transcript_selection(&self) -> bool {
        self.transcript_selection.is_some() || !self.transcript_regions.is_empty()
    }

    /// Clear every transcript range on release, Escape or a fresh press.
    pub fn clear_transcript_selection(&mut self) {
        self.transcript_selection = None;
        self.transcript_regions.clear();
        self.transcript_autoscroll = None;
    }

    /// Combined text of every transcript range plus an optional drag range
    /// that has not been published yet. Ranges are ordered by logical
    /// position and overlapping or touching ranges are merged.
    pub fn selected_transcript_text_all(
        &self,
        active: Option<TranscriptSelection>,
    ) -> Option<String> {
        let mut ranges: Vec<_> = self
            .transcript_regions
            .iter()
            .copied()
            .chain(self.transcript_selection)
            .chain(active)
            .map(TranscriptSelection::ordered)
            .filter(|(start, end)| start != end)
            .collect();
        ranges.sort();
        let mut merged: Vec<(
            tui_core::panel_select::TextPos,
            tui_core::panel_select::TextPos,
        )> = Vec::with_capacity(ranges.len());
        for (start, end) in ranges {
            match merged.last_mut() {
                Some((_, last_end)) if start <= *last_end => {
                    if end > *last_end {
                        *last_end = end;
                    }
                }
                _ => merged.push((start, end)),
            }
        }
        let parts: Vec<String> = merged
            .iter()
            .filter_map(|(start, end)| {
                self.selected_transcript_text(TranscriptSelection {
                    anchor: *start,
                    head: *end,
                })
            })
            .collect();
        let joined = parts.join("\n");
        (!joined.trim().is_empty()).then_some(joined)
    }

    /// Take the combined selection text and clear every range afterwards.
    pub fn take_transcript_selection_text(
        &mut self,
        active: Option<TranscriptSelection>,
    ) -> Option<String> {
        let text = self.selected_transcript_text_all(active);
        self.clear_transcript_selection();
        text
    }

    /// Queue the current selection on the controller's pending copy slot so
    /// the keyboard path shares the shell's clipboard flush. `clear_after`
    /// overrides the controller's stored preference for this call.
    pub fn copy_transcript_selection_with_policy(&mut self, clear_after: bool) -> bool {
        let Some(text) = self.selected_transcript_text_all(None) else {
            return false;
        };
        let Some(clean) = crate::terminal::sanitize_copy_text(&text) else {
            return false;
        };
        self.pending_copy = Some(crate::copy::PendingCopy::new(clean, "selection"));
        if clear_after {
            self.clear_transcript_selection();
        }
        true
    }

    /// Queue the current selection on the controller's pending copy slot so
    /// the keyboard path shares the shell's clipboard flush.
    pub fn copy_transcript_selection(&mut self) -> bool {
        self.copy_transcript_selection_with_policy(self.clear_selection_after_copy)
    }

    /// Pending drag auto-scroll direction: `-1` scrolls up, `+1` scrolls
    /// down. Set while a content drag extends past the viewport edge.
    pub fn transcript_autoscroll(&self) -> Option<i8> {
        self.transcript_autoscroll
    }

    /// Arm drag auto-scroll in one direction.
    pub fn start_transcript_autoscroll(&mut self, direction: i8) {
        let direction = direction.signum();
        if direction != 0 {
            self.transcript_autoscroll = Some(direction);
        }
    }

    /// Advance one auto-scroll step: move the viewport one notch in the
    /// armed direction and extend the logical head to the newly exposed
    /// edge row. The anchor never moves, so scrolling cannot drift the
    /// selected start. Returns whether a step ran.
    pub fn tick_transcript_autoscroll(&mut self) -> bool {
        let Some(direction) = self.transcript_autoscroll else {
            return false;
        };
        let Some(selection) = self.transcript_selection else {
            self.transcript_autoscroll = None;
            return false;
        };
        if direction < 0 {
            self.scroll_history_up();
        } else {
            self.scroll_history_down();
        }
        let capacity = self.last_layout_height.max(1);
        let (edge_row, edge_col) = if direction < 0 {
            (0, 0)
        } else {
            (capacity.saturating_sub(1), usize::MAX)
        };
        if let Some(head) = self.transcript_pos_at_display_row(usize::from(edge_row), edge_col) {
            self.transcript_selection = Some(TranscriptSelection {
                anchor: selection.anchor,
                head,
            });
        }
        true
    }

    /// Disarm drag auto-scroll without touching the selection.
    pub fn clear_transcript_autoscroll(&mut self) {
        self.transcript_autoscroll = None;
    }

    /// Record one hover cell for affordance highlights. Never requests a
    /// frame; cleared alongside the hit snapshot.
    pub fn set_hover_cell(&mut self, col: u16, row: u16) {
        self.hover_cell = Some((col, row));
    }

    /// Last hovered cell, if any. The shell reads this to dedupe motion
    /// events before deciding whether a frame is due.
    pub(crate) fn hover_cell(&self) -> Option<(u16, u16)> {
        self.hover_cell
    }

    /// Clear the hover cell after scroll, resize or button activity.
    pub fn clear_hover_cell(&mut self) {
        self.hover_cell = None;
    }
}
