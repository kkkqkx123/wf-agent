//! Interactive view state: redraw, animation and hit frames.

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
    /// Advance streaming catch-up for a `viewport`-row window. Returns the
    /// effective rows-above-tail offset for this frame and closes the lag by
    /// one bounded step; static rendering snaps instantly. Detached windows
    /// (`view_scroll > 0`) keep user control and never advance the lag.
    pub(crate) fn advance_tail_catchup(&mut self, viewport: usize) -> usize {
        if self.view_scroll > 0 {
            return self.view_scroll;
        }
        let animated = !self.simplified_render;
        let step = crate::tail::catchup_step(self.tail_lag, viewport, animated);
        self.tail_lag = self.tail_lag.saturating_sub(step.min(self.tail_lag));
        self.view_scroll.saturating_add(self.tail_lag)
    }

    /// Drop the retained preparation key so the next partial scope replays
    /// fully. Scroll and reflow paths use this soft invalidation: no terminal
    /// clear ever runs here, so placeholders never flash.
    pub(crate) fn soft_invalidate(&mut self) {
        self.scroll_cache_key = None;
    }

    /// Current redraw snapshot for scope grading. The animation tick is
    /// bucketed so spinner progress alone grades animation-only and never
    /// invalidates the preparation cache inside a bucket.
    pub fn redraw_snapshot(&self) -> crate::redraw::RedrawSnapshot {
        let now_ms = self.now_ms();
        let streaming_text = self.stream.streaming_text();
        let render_mode = if self.diagram_requested > 0 {
            crate::prep_keys::RENDER_MODE_DIAGRAM
        } else {
            0
        };
        crate::redraw::RedrawSnapshot {
            content_version: self.content_version,
            streaming_len: streaming_text.len(),
            streaming_hash: crate::prep_keys::hash_prefix(streaming_text),
            footer_digest: crate::redraw::footer_digest(&self.footer.state),
            footer_route: self.footer.route.id(),
            input_hash: crate::prep_keys::hash_prefix(self.footer.composer.content()),
            width: self.last_layout_width,
            height: self.last_layout_height,
            render_mode,
            view_scroll: self.view_scroll.saturating_add(self.tail_lag),
            anim_tick: crate::clock::anim_bucket(now_ms),
            image_signature: self.image_signature,
            expanded_version: self.expanded_version,
            aspect_bucket: crate::layout::aspect_bucket(
                self.last_layout_width,
                self.last_layout_height,
            ),
            force_full: self.footer.approval.is_some(),
        }
    }

    /// Grade the current state against the previous frame.
    pub fn grade_redraw(&self) -> crate::redraw::RedrawScope {
        crate::redraw::decide_scope(self.last_snapshot, self.redraw_snapshot())
    }

    /// Snapshot graded on the previous frame.
    pub fn last_snapshot(&self) -> Option<crate::redraw::RedrawSnapshot> {
        self.last_snapshot
    }

    /// Record the snapshot a frame was submitted for.
    pub(crate) fn set_last_snapshot(&mut self, snapshot: crate::redraw::RedrawSnapshot) {
        self.last_snapshot = Some(snapshot);
    }

    /// Recorded animation geometry for animation-only frames.
    pub fn anim_area(&self) -> crate::redraw::AnimationArea {
        self.anim_area
    }

    /// Row-level record of the last drawn animation cells.
    pub fn anim_rows(&self) -> &crate::redraw::AnimRowRecord {
        &self.anim_rows
    }

    /// Last millisecond an animation-only frame was emitted.
    pub fn last_anim_ms(&self) -> Option<u64> {
        self.last_anim_ms
    }

    /// Whether the optional session scrollbar is enabled. Visibility still
    /// requires overflow plus the shared minimum viewport and width guards.
    pub fn session_scrollbar_enabled(&self) -> bool {
        self.show_session_scrollbar
    }

    /// Set the optional session scrollbar switch. Takes effect on the next
    /// frame: the text-width change grades full so no restart is needed.
    pub fn set_session_scrollbar(&mut self, enabled: bool) {
        self.show_session_scrollbar = enabled;
    }

    /// Mouse hit snapshot generation for press and release matching.
    pub fn hit_generation(&self) -> u64 {
        self.hit_generation
    }

    /// Badge rectangle published on the last full or bottom frame.
    pub fn last_badge_rect(&self) -> Option<ratatui::layout::Rect> {
        self.last_badge_rect
    }

    /// Scrollback viewport published on the last frame.
    pub fn last_scroll_area(&self) -> Option<ratatui::layout::Rect> {
        self.last_scroll_area
    }

    /// Composer input viewport published on the last frame.
    pub fn last_input_area(&self) -> Option<ratatui::layout::Rect> {
        self.last_input_area
    }

    /// Publish one hit snapshot: the badge rectangle plus the viewports that
    /// produced it. Bumps the generation so stale presses never land.
    pub(crate) fn record_hit_frame(
        &mut self,
        scroll_area: ratatui::layout::Rect,
        badge: Option<ratatui::layout::Rect>,
        input_area: ratatui::layout::Rect,
    ) {
        self.sync_transcript_layout();
        self.hit_generation = self.hit_generation.wrapping_add(1);
        self.last_scroll_area = Some(scroll_area);
        self.last_badge_rect = badge;
        self.last_input_area = Some(input_area);
    }

    /// Drop any published hit snapshot (resize or overlay switch): presses
    /// recorded against the old geometry can no longer land. The logical
    /// selection survives because its position does not depend on the
    /// viewport.
    pub(crate) fn clear_hit_frame(&mut self) {
        self.last_badge_rect = None;
        self.last_scroll_area = None;
        self.last_input_area = None;
        self.hover_cell = None;
    }
}
