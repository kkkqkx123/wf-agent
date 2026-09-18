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
    /// Viewport height in rows (frame identity only; body layout is
    /// width-driven, but overlay and viewport geometry follow height).
    pub height: u16,
    /// Render mode bits (`crate::prep_keys::RENDER_MODE_*`).
    pub render_mode: u8,
    /// Viewport scroll offset in display rows.
    pub view_scroll: usize,
    /// Discretized animation tick (spinner frame index).
    pub anim_tick: u64,
    /// Image collection signature: inline image changes force a full frame.
    pub image_signature: u64,
    /// Expanded image level version.
    pub expanded_version: u64,
    /// Diagram aspect bucket from the unified layout entry point.
    pub aspect_bucket: u8,
    /// Overlay, modal or notice-row visibility changed; forces full.
    pub force_full: bool,
}

/// Event source feeding a redraw request, ordered by interaction priority:
/// input preempts animation, animation preempts periodic polling, and
/// background refreshes never preempt anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum EventSource {
    /// Background data refresh (fetch completion, passive liveness).
    #[default]
    Background,
    /// Periodic poll tick.
    Periodic,
    /// Animation-tick advance.
    Animation,
    /// User input (key, mouse, paste, resize).
    Input,
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
        || prev.height != next.height
        || prev.render_mode != next.render_mode
        || prev.content_version != next.content_version
        || prev.view_scroll != next.view_scroll
        || prev.streaming_len != next.streaming_len
        || prev.streaming_hash != next.streaming_hash
        || prev.image_signature != next.image_signature
        || prev.expanded_version != next.expanded_version
        || prev.aspect_bucket != next.aspect_bucket
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
    /// Highest-priority source among the merged requests.
    source: EventSource,
}

impl PendingScope {
    /// Empty request.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge a new request, keeping the most severe scope.
    pub fn request(&mut self, scope: RedrawScope) {
        self.request_from(scope, EventSource::Background);
    }

    /// Merge a new request from `source`: severity still decides the scope,
    /// while the highest-priority source is retained for scheduling.
    pub fn request_from(&mut self, scope: RedrawScope, source: EventSource) {
        self.scope = self.scope.merge(scope);
        self.note_source(source);
    }

    /// Retain `source` without changing the scope. Input and animation paths
    /// call this when they mark state dirty outside grading, so the submit
    /// point knows the highest-priority cause behind the pending frame.
    /// `take` resets the record, so a retained source always postdates the
    /// last submitted frame.
    pub fn note_source(&mut self, source: EventSource) {
        if source > self.source {
            self.source = source;
        }
    }

    /// Current pending scope.
    pub fn scope(&self) -> RedrawScope {
        self.scope
    }

    /// Highest-priority source among the merged requests.
    pub fn source(&self) -> EventSource {
        self.source
    }

    /// Take and clear the pending scope.
    pub fn take(&mut self) -> RedrawScope {
        let scope = std::mem::replace(&mut self.scope, RedrawScope::None);
        self.source = EventSource::Background;
        scope
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

/// Packed rectangle in a single `u64` for loop-held animation geometry.
/// Layout: `x:16 | y:16 | w:16 | h:16` from high to low bits. Zero means
/// empty. Packing keeps the animation record copyable through the event loop
/// without allocating or cloning `Rect`s per frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackedRect(pub u64);

impl PackedRect {
    /// Empty record.
    pub const EMPTY: Self = Self(0);

    /// Pack a `Rect` (saturating to 16 bits per lane).
    pub fn pack(rect: Rect) -> Self {
        let x = u64::from(rect.x);
        let y = u64::from(rect.y);
        let w = u64::from(rect.width);
        let h = u64::from(rect.height);
        Self((x << 48) | (y << 32) | (w << 16) | h)
    }

    /// Unpack to a `Rect` (`None` when empty).
    pub fn unpack(self) -> Option<Rect> {
        if self.0 == 0 {
            return None;
        }
        Some(Rect {
            x: (self.0 >> 48) as u16,
            y: (self.0 >> 32) as u16,
            width: (self.0 >> 16) as u16,
            height: self.0 as u16,
        })
    }

    /// Whether the record holds a rectangle.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Recorded animation geometry: the only cells an animation-only frame may
/// touch (streaming indicator cell plus statusline spinner cell). Stored as
/// loop-held packed rectangles so the submit point copies a single `u64`
/// per cell instead of cloning `Rect`s.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnimationArea {
    packed_streaming: u64,
    packed_status: u64,
}

impl AnimationArea {
    /// Record from live rectangles.
    pub fn new(streaming_cell: Option<Rect>, status_cell: Option<Rect>) -> Self {
        Self {
            packed_streaming: streaming_cell
                .map(PackedRect::pack)
                .map(|p| p.0)
                .unwrap_or(0),
            packed_status: status_cell.map(PackedRect::pack).map(|p| p.0).unwrap_or(0),
        }
    }

    /// Bounding cell of the streaming-line indicator, if streaming.
    pub fn streaming_cell(&self) -> Option<Rect> {
        PackedRect(self.packed_streaming).unpack()
    }

    /// Bounding cell of the statusline spinner, when busy.
    pub fn status_cell(&self) -> Option<Rect> {
        PackedRect(self.packed_status).unpack()
    }

    /// Packed streaming cell for loop-held copies (0 when empty).
    pub fn packed_streaming(&self) -> u64 {
        self.packed_streaming
    }

    /// Packed status cell for loop-held copies (0 when empty).
    pub fn packed_status(&self) -> u64 {
        self.packed_status
    }

    /// Union bounding box of the recorded cells, if any.
    pub fn bounds(&self) -> Option<Rect> {
        match (self.streaming_cell(), self.status_cell()) {
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

/// Row-level record of the last drawn animation cells. While
/// [`AnimationArea`] keeps the two bounding cells, this set lists every
/// display row an animation-only frame repainted, so tests and the submit
/// path can assert that animation frames never touch other rows. A missing
/// or empty record falls back to a full frame.
#[derive(Debug, Clone, Default)]
pub struct AnimRowRecord {
    rows: Vec<u16>,
}

impl AnimRowRecord {
    /// Empty record.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the rows covered by `area` (union bounds, row granularity).
    pub fn record(&mut self, area: AnimationArea) {
        self.rows.clear();
        if let Some(bounds) = area.bounds() {
            let end = bounds.y.saturating_add(bounds.height);
            self.rows.extend(bounds.y..end);
        }
    }

    /// Recorded rows, in ascending order.
    pub fn rows(&self) -> &[u16] {
        &self.rows
    }

    /// True when `row` may be repainted by an animation-only frame.
    pub fn covers(&self, row: u16) -> bool {
        self.rows.contains(&row)
    }

    /// True when no row was recorded.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
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
            height: 24,
            render_mode: 0,
            view_scroll: 0,
            anim_tick: 3,
            image_signature: 0,
            expanded_version: 0,
            aspect_bucket: 0,
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
    fn height_or_render_mode_change_is_full() {
        let prev = snapshot();
        let mut resized = prev;
        resized.height = 30;
        assert_eq!(decide_scope(Some(prev), resized), RedrawScope::Full);
        let mut overlay = prev;
        overlay.render_mode = crate::prep_keys::RENDER_MODE_OVERLAY;
        assert_eq!(decide_scope(Some(prev), overlay), RedrawScope::Full);
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
    fn media_signals_force_full() {
        let prev = snapshot();
        let mut image = prev;
        image.image_signature = 9;
        assert_eq!(decide_scope(Some(prev), image), RedrawScope::Full);
        let mut aspect = prev;
        aspect.aspect_bucket = 2;
        assert_eq!(decide_scope(Some(prev), aspect), RedrawScope::Full);
        let mut expanded = prev;
        expanded.expanded_version = 1;
        assert_eq!(decide_scope(Some(prev), expanded), RedrawScope::Full);
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
    fn pending_scope_tracks_highest_priority_source() {
        let mut pending = PendingScope::new();
        pending.request_from(RedrawScope::AnimationOnly, EventSource::Background);
        assert_eq!(pending.source(), EventSource::Background);
        pending.request_from(RedrawScope::AnimationOnly, EventSource::Input);
        assert_eq!(pending.source(), EventSource::Input);
        assert!(EventSource::Input > EventSource::Animation);
        assert!(EventSource::Animation > EventSource::Periodic);
        assert!(EventSource::Periodic > EventSource::Background);
        assert_eq!(pending.take(), RedrawScope::AnimationOnly);
        assert_eq!(pending.source(), EventSource::Background);
    }

    #[test]
    fn bare_source_notes_survive_until_take() {
        let mut pending = PendingScope::new();
        pending.note_source(EventSource::Input);
        assert_eq!(pending.source(), EventSource::Input);
        pending.note_source(EventSource::Background);
        assert_eq!(pending.source(), EventSource::Input);
        pending.request(RedrawScope::BottomOnly);
        assert_eq!(pending.source(), EventSource::Input);
    }

    #[test]
    fn anim_row_record_covers_only_recorded_rows() {
        let mut record = AnimRowRecord::new();
        assert!(record.is_empty());
        record.record(AnimationArea::new(
            Some(Rect::new(0, 7, 4, 1)),
            Some(Rect::new(0, 20, 2, 2)),
        ));
        assert!(record.covers(7));
        assert!(record.covers(20));
        assert!(record.covers(21));
        // Union bounds span rows 7..22 at row granularity.
        assert!(record.covers(8));
        assert!(!record.covers(22));
        assert!(!record.covers(0));
        record.record(AnimationArea::default());
        assert!(record.is_empty());
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
    fn packed_rect_round_trips() {
        let rect = Rect::new(3, 7, 4, 1);
        let packed = PackedRect::pack(rect);
        assert!(!packed.is_empty());
        assert_eq!(packed.unpack(), Some(rect));
        assert_eq!(PackedRect::EMPTY.unpack(), None);
    }

    #[test]
    fn animation_area_bounds_unions_cells() {
        let area = AnimationArea::new(Some(Rect::new(0, 0, 2, 1)), Some(Rect::new(5, 3, 1, 1)));
        let bounds = area.bounds().expect("union exists");
        assert_eq!(bounds.x, 0);
        assert_eq!(bounds.y, 0);
        assert_eq!(bounds.width, 6);
        assert_eq!(bounds.height, 4);
    }
}
