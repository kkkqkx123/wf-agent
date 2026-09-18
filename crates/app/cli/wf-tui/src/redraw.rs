//! Redraw scope grading and frame scheduling helpers.
//!
//! The event loop used to rebuild the whole screen on every frame. This
//! module grades each frame into one of three scopes so animation and footer
//! changes skip the scrollback layout work:
//!
//! * [`RedrawScope::Full`] — scrollback or layout changed; rebuild everything.
//! * [`RedrawScope::BottomOnly`] — only footer state changed; reuse the
//!   cached scrollback rows and repaint the footer/input rows.
//! * [`RedrawScope::AnimationOnly`] — only the animation tick advanced;
//!   reuse everything except the recorded animation cells.
//!
//! [`decide_scope`] consumes two [`RedrawSnapshot`]s and reports the scope.
//! [`PendingScope`] merges concurrent requests by severity
//! (`Full > BottomOnly > AnimationOnly`). [`idle_poll_interval`] maps idle
//! time to a poll period so idle frames stop spinning at the full rate.

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::status_line::FooterState;

/// Scope of a single frame submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RedrawScope {
    /// Nothing changed; skip the frame.
    #[default]
    None,
    /// Only the animation tick advanced.
    AnimationOnly,
    /// Only footer/input state changed.
    BottomOnly,
    /// Scrollback, streaming text, layout or overlay changed.
    Full,
}

impl RedrawScope {
    /// Severity rank used for merging; larger wins.
    pub fn severity(self) -> u8 {
        match self {
            RedrawScope::None => 0,
            RedrawScope::AnimationOnly => 1,
            RedrawScope::BottomOnly => 2,
            RedrawScope::Full => 3,
        }
    }

    /// Merge two requests, keeping the more severe one.
    pub fn merge(self, other: RedrawScope) -> RedrawScope {
        if other.severity() > self.severity() {
            other
        } else {
            self
        }
    }

    /// True when the frame may reuse the cached scrollback rows.
    pub fn reuses_scrollback(self) -> bool {
        matches!(self, RedrawScope::AnimationOnly | RedrawScope::BottomOnly)
    }
}

/// Point-in-time version snapshot feeding [`decide_scope`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedrawSnapshot {
    /// Private scrollback content version (see `prep_cache`).
    pub content_version: u64,
    /// Visible streaming prefix length.
    pub streaming_len: usize,
    /// Hash of the visible streaming prefix.
    pub streaming_hash: u64,
    /// Digest of the footer state (status text, phase, notice).
    pub footer_digest: u64,
    /// Layout width in columns.
    pub width: u16,
    /// Viewport scroll offset in display rows.
    pub view_scroll: usize,
    /// Discretized animation tick (spinner frame index).
    pub anim_tick: u64,
    /// Overlay, modal or notice-row visibility changed; forces full.
    pub force_full: bool,
}

/// Grade the transition from `prev` to `next` into a redraw scope.
///
/// Content and layout differences yield [`RedrawScope::Full`], a lone footer
/// difference yields [`RedrawScope::BottomOnly`], a lone animation tick
/// yields [`RedrawScope::AnimationOnly`], otherwise no frame is needed.
pub fn decide_scope(prev: Option<RedrawSnapshot>, next: RedrawSnapshot) -> RedrawScope {
    let Some(prev) = prev else {
        return RedrawScope::Full;
    };
    if next.force_full || prev.force_full != next.force_full {
        return RedrawScope::Full;
    }
    if prev.width != next.width
        || prev.content_version != next.content_version
        || prev.view_scroll != next.view_scroll
        || prev.streaming_len != next.streaming_len
        || prev.streaming_hash != next.streaming_hash
    {
        return RedrawScope::Full;
    }
    if prev.footer_digest != next.footer_digest {
        return RedrawScope::BottomOnly;
    }
    if prev.anim_tick != next.anim_tick {
        return RedrawScope::AnimationOnly;
    }
    RedrawScope::None
}

/// Pending redraw request merged by severity.
#[derive(Debug, Clone, Copy, Default)]
pub struct PendingScope {
    scope: RedrawScope,
}

impl PendingScope {
    /// Empty request.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge a new request, keeping the most severe scope.
    pub fn request(&mut self, scope: RedrawScope) {
        self.scope = self.scope.merge(scope);
    }

    /// Current pending scope.
    pub fn scope(&self) -> RedrawScope {
        self.scope
    }

    /// Take and clear the pending scope.
    pub fn take(&mut self) -> RedrawScope {
        std::mem::replace(&mut self.scope, RedrawScope::None)
    }

    /// True when a frame is pending.
    pub fn is_pending(&self) -> bool {
        self.scope != RedrawScope::None
    }
}

/// Animation frame period (10 FPS): animation-only requests must not fire
/// faster than this even though full frames may run up to 120 FPS.
pub const ANIMATION_FRAME_INTERVAL_MS: u64 = 100;

/// Idle poll periods mirroring the graded redraw rhythm: active frames poll
/// fast, sustained idle backs off so empty loops stop spinning.
pub const REDRAW_ACTIVE_MS: u64 = 100;
pub const REDRAW_IDLE_MS: u64 = 250;
pub const REDRAW_PASSIVE_MS: u64 = 1_000;
pub const REDRAW_DEEP_IDLE_MS: u64 = 5_000;
pub const REDRAW_DEEP_IDLE_AFTER_MS: u64 = 30_000;

/// Map idle duration to a poll interval.
pub fn idle_poll_interval(idle_ms: u64) -> Duration {
    if idle_ms >= REDRAW_DEEP_IDLE_AFTER_MS {
        Duration::from_millis(REDRAW_DEEP_IDLE_MS)
    } else if idle_ms >= REDRAW_PASSIVE_MS {
        Duration::from_millis(REDRAW_PASSIVE_MS)
    } else if idle_ms >= REDRAW_IDLE_MS {
        Duration::from_millis(REDRAW_IDLE_MS)
    } else {
        Duration::from_millis(REDRAW_ACTIVE_MS)
    }
}

/// True when an animation-only frame is due under the animation frame rate.
pub fn animation_frame_due(now_ms: u64, last_anim_ms: Option<u64>) -> bool {
    animation_frame_due_with(now_ms, last_anim_ms, ANIMATION_FRAME_INTERVAL_MS)
}

/// True when an animation-only frame is due under an explicit interval so a
/// capability policy can slow animation frames on constrained terminals.
pub fn animation_frame_due_with(now_ms: u64, last_anim_ms: Option<u64>, interval_ms: u64) -> bool {
    match last_anim_ms {
        None => true,
        Some(last) => now_ms.saturating_sub(last) >= interval_ms.max(1),
    }
}

/// Count cells whose symbol or style differs between two buffers.
pub fn changed_cells(prev: &Buffer, next: &Buffer) -> usize {
    if prev.area != next.area {
        return next.content.len();
    }
    prev.content
        .iter()
        .zip(next.content.iter())
        .filter(|(a, b)| a.symbol() != b.symbol() || a.style() != b.style())
        .count()
}

/// Approximate diff payload: changed cells times average cell bytes.
pub fn diff_bytes(prev: &Buffer, next: &Buffer) -> usize {
    changed_cells(prev, next).saturating_mul(8)
}

/// Digest of the footer fields that affect the bottom rows.
pub fn footer_digest(state: &FooterState) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    mix(&[phase_tag(state.phase)]);
    mix(&state.iteration.to_le_bytes());
    mix(&state.message_count.to_le_bytes());
    mix(&state.subagent_count.to_le_bytes());
    mix(&state.duration_ms.to_le_bytes());
    for tool in &state.active_tools {
        mix(tool.as_bytes());
        mix(&[0xff]);
    }
    if let Some(model) = &state.model {
        mix(model.as_bytes());
    }
    mix(&[0xfe]);
    if let Some(exec) = &state.execution_id {
        mix(exec.as_bytes());
    }
    mix(&[0xfd]);
    if let Some(err) = &state.last_error {
        mix(err.as_bytes());
    }
    mix(&[0xfc]);
    if let Some((notice, _)) = &state.notice {
        mix(notice.as_bytes());
    }
    hash
}

fn phase_tag(phase: crate::reducer::Phase) -> u8 {
    match phase {
        crate::reducer::Phase::Idle => 0,
        crate::reducer::Phase::Streaming => 1,
    }
}

/// Recorded animation geometry: the only cells an animation-only frame may
/// touch (streaming indicator cell plus statusline spinner cell).
#[derive(Debug, Clone, Copy, Default)]
pub struct AnimationArea {
    /// Bounding cell of the streaming-line indicator, if streaming.
    pub streaming_cell: Option<Rect>,
    /// Bounding cell of the statusline spinner, when busy.
    pub status_cell: Option<Rect>,
}

impl AnimationArea {
    /// Union bounding box of the recorded cells, if any.
    pub fn bounds(&self) -> Option<Rect> {
        match (self.streaming_cell, self.status_cell) {
            (Some(a), Some(b)) => Some(union_rect(a, b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> RedrawSnapshot {
        RedrawSnapshot {
            content_version: 1,
            streaming_len: 0,
            streaming_hash: 0,
            footer_digest: 7,
            width: 80,
            view_scroll: 0,
            anim_tick: 3,
            force_full: false,
        }
    }

    #[test]
    fn first_frame_is_full() {
        assert_eq!(decide_scope(None, snapshot()), RedrawScope::Full);
    }

    #[test]
    fn identical_snapshots_need_no_frame() {
        let snap = snapshot();
        assert_eq!(decide_scope(Some(snap), snap), RedrawScope::None);
    }

    #[test]
    fn content_change_is_full() {
        let prev = snapshot();
        let mut next = prev;
        next.content_version += 1;
        assert_eq!(decide_scope(Some(prev), next), RedrawScope::Full);
    }

    #[test]
    fn streaming_change_is_full() {
        let prev = snapshot();
        let mut next = prev;
        next.streaming_len += 5;
        next.streaming_hash ^= 0x9e37;
        assert_eq!(decide_scope(Some(prev), next), RedrawScope::Full);
    }

    #[test]
    fn width_or_scroll_change_is_full() {
        let prev = snapshot();
        let mut next = prev;
        next.width = 100;
        assert_eq!(decide_scope(Some(prev), next), RedrawScope::Full);
        let mut scrolled = prev;
        scrolled.view_scroll = 4;
        assert_eq!(decide_scope(Some(prev), scrolled), RedrawScope::Full);
    }

    #[test]
    fn footer_change_is_bottom_only() {
        let prev = snapshot();
        let mut next = prev;
        next.footer_digest ^= 0x1234;
        assert_eq!(decide_scope(Some(prev), next), RedrawScope::BottomOnly);
    }

    #[test]
    fn animation_tick_alone_is_animation_only() {
        let prev = snapshot();
        let mut next = prev;
        next.anim_tick += 1;
        assert_eq!(decide_scope(Some(prev), next), RedrawScope::AnimationOnly);
    }

    #[test]
    fn force_full_wins_over_bottom_and_animation() {
        let prev = snapshot();
        let mut next = prev;
        next.force_full = true;
        next.footer_digest ^= 1;
        next.anim_tick += 1;
        assert_eq!(decide_scope(Some(prev), next), RedrawScope::Full);
    }

    #[test]
    fn pending_scope_merges_by_severity() {
        let mut pending = PendingScope::new();
        pending.request(RedrawScope::AnimationOnly);
        pending.request(RedrawScope::BottomOnly);
        assert_eq!(pending.scope(), RedrawScope::BottomOnly);
        pending.request(RedrawScope::AnimationOnly);
        assert_eq!(pending.scope(), RedrawScope::BottomOnly);
        pending.request(RedrawScope::Full);
        assert_eq!(pending.take(), RedrawScope::Full);
        assert!(!pending.is_pending());
    }

    #[test]
    fn animation_frames_follow_their_own_rate() {
        assert!(animation_frame_due(0, None));
        assert!(!animation_frame_due(50, Some(0)));
        assert!(animation_frame_due(100, Some(0)));
        assert!(animation_frame_due(250, Some(100)));
    }

    #[test]
    fn idle_schedule_backs_off() {
        assert_eq!(idle_poll_interval(0).as_millis(), 100);
        assert_eq!(idle_poll_interval(300).as_millis(), 250);
        assert_eq!(idle_poll_interval(2_000).as_millis(), 1_000);
        assert_eq!(idle_poll_interval(60_000).as_millis(), 5_000);
    }

    #[test]
    fn changed_cells_counts_only_differences() {
        use ratatui::buffer::Cell;
        let area = Rect::new(0, 0, 4, 1);
        let prev = Buffer::filled(area, Cell::EMPTY);
        let mut next = prev.clone();
        next.set_string(0, 0, "ab", ratatui::style::Style::default());
        assert_eq!(changed_cells(&prev, &next), 2);
        assert_eq!(changed_cells(&prev, &prev), 0);
    }

    #[test]
    fn animation_area_bounds_unions_cells() {
        let area = AnimationArea {
            streaming_cell: Some(Rect::new(0, 0, 2, 1)),
            status_cell: Some(Rect::new(5, 3, 1, 1)),
        };
        let bounds = area.bounds().expect("union exists");
        assert_eq!(bounds.x, 0);
        assert_eq!(bounds.y, 0);
        assert_eq!(bounds.width, 6);
        assert_eq!(bounds.height, 4);
    }
}
