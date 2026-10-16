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
    /// Hash of the composer input text. Typing must force a full frame so a
    /// single-cell spinner never paints over a stale input row.
    pub input_hash: u64,
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
        || prev.input_hash != next.input_hash
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

/// Invalidation severity for a redraw request: soft replays the frame from
/// the retained preparation while hard also drops the terminal surface.
///
/// Scrolling, reflow and viewport changes only need a soft invalidation (the
/// next frame recomposes fully without an intermediate clear, so image and
/// diagram placeholders never flash). Only a real desynchronization — a draw
/// panic, a suspend/resume cycle or a geometry reset — escalates to hard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvalidationKind {
    /// No invalidation pending.
    #[default]
    None,
    /// Replay the next frame fully from retained state, no terminal clear.
    Soft,
    /// Clear the terminal surface, then replay fully.
    Hard,
}

impl InvalidationKind {
    /// Severity rank used for merging; larger wins.
    pub fn severity(self) -> u8 {
        match self {
            InvalidationKind::None => 0,
            InvalidationKind::Soft => 1,
            InvalidationKind::Hard => 2,
        }
    }

    /// Merge two requests, keeping the more severe one.
    pub fn merge(self, other: InvalidationKind) -> InvalidationKind {
        if other.severity() > self.severity() {
            other
        } else {
            self
        }
    }

    /// True when the terminal surface must be cleared before recomposing.
    pub fn needs_hard_clear(self) -> bool {
        matches!(self, InvalidationKind::Hard)
    }
}

/// Pending invalidation merged by severity (`Hard > Soft > None`).
#[derive(Debug, Clone, Copy, Default)]
pub struct InvalidationSentinel {
    kind: InvalidationKind,
}

impl InvalidationSentinel {
    /// Empty sentinel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge a new request, keeping the most severe kind.
    pub fn request(&mut self, kind: InvalidationKind) {
        self.kind = self.kind.merge(kind);
    }

    /// Current pending kind.
    pub fn kind(&self) -> InvalidationKind {
        self.kind
    }

    /// Take and clear the pending kind.
    pub fn take(&mut self) -> InvalidationKind {
        std::mem::replace(&mut self.kind, InvalidationKind::None)
    }

    /// True when an invalidation is pending.
    pub fn is_pending(&self) -> bool {
        self.kind != InvalidationKind::None
    }
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

/// How long after the last keystroke typing still holds decorative animation
/// back. Covers a continuous typing burst without pinning a stale draft.
pub const COMPOSING_BACKOFF_MS: u64 = 600;

/// Frame rate cap for purely decorative animation. Functional motion keeps
/// the configured animation rate; decoration costs scale linearly with rate
/// while perceived smoothness saturates early.
pub const DECORATIVE_FPS_CAP: u64 = 30;

/// Why a tick needs a full frame beyond decorative animation. Names stay
/// stable so diagnostics can report the exact predicate keeping the loop on
/// the expensive path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullFrameReason {
    Processing,
    Streaming,
    TailCatchup,
    StatusNotice,
    MouseScroll,
    CopyAutoscroll,
    Overscroll,
    Notification,
    RateLimitCountdown,
    RemoteStartup,
    StatusAnimation,
}

impl FullFrameReason {
    /// Stable diagnostic name for the reason.
    pub fn as_str(self) -> &'static str {
        match self {
            FullFrameReason::Processing => "processing",
            FullFrameReason::Streaming => "streaming",
            FullFrameReason::TailCatchup => "tail_catchup",
            FullFrameReason::StatusNotice => "status_notice",
            FullFrameReason::MouseScroll => "mouse_scroll_animation",
            FullFrameReason::CopyAutoscroll => "copy_autoscroll",
            FullFrameReason::Overscroll => "chat_overscroll",
            FullFrameReason::Notification => "notification",
            FullFrameReason::RateLimitCountdown => "rate_limit_countdown",
            FullFrameReason::RemoteStartup => "remote_startup",
            FullFrameReason::StatusAnimation => "status_animation",
        }
    }

    /// Reason from its diagnostic name, if known.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "processing" => Some(FullFrameReason::Processing),
            "streaming" => Some(FullFrameReason::Streaming),
            "tail_catchup" => Some(FullFrameReason::TailCatchup),
            "status_notice" => Some(FullFrameReason::StatusNotice),
            "mouse_scroll_animation" => Some(FullFrameReason::MouseScroll),
            "copy_autoscroll" => Some(FullFrameReason::CopyAutoscroll),
            "chat_overscroll" => Some(FullFrameReason::Overscroll),
            "notification" => Some(FullFrameReason::Notification),
            "rate_limit_countdown" => Some(FullFrameReason::RateLimitCountdown),
            "remote_startup" => Some(FullFrameReason::RemoteStartup),
            "status_animation" => Some(FullFrameReason::StatusAnimation),
            _ => None,
        }
    }
}

/// Fixed diagnostic table backing the atomic reason slot. Every reason
/// recorded through `record_full_frame_reason` must appear here so a rename
/// cannot silently blind the diagnostics.
pub const FULL_FRAME_REDRAW_REASONS: &[&str] = &[
    "processing",
    "streaming",
    "tail_catchup",
    "status_notice",
    "mouse_scroll_animation",
    "copy_autoscroll",
    "chat_overscroll",
    "notification",
    "rate_limit_countdown",
    "remote_startup",
    "status_animation",
];

static LAST_FULL_FRAME_REDRAW_REASON: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);

/// Record the last reason a tick demanded a full frame. Lock-free so the
/// redraw hot path never blocks on diagnostics.
pub fn record_full_frame_reason(reason: FullFrameReason) {
    if let Some(idx) = FULL_FRAME_REDRAW_REASONS
        .iter()
        .position(|name| *name == reason.as_str())
    {
        LAST_FULL_FRAME_REDRAW_REASON.store(idx, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Last recorded full-frame reason, if any.
pub fn last_full_frame_reason() -> Option<FullFrameReason> {
    FULL_FRAME_REDRAW_REASONS
        .get(LAST_FULL_FRAME_REDRAW_REASON.load(std::sync::atomic::Ordering::Relaxed))
        .copied()
        .and_then(FullFrameReason::from_name)
}

/// Inputs driving the redraw cadence. All time is caller-supplied
/// milliseconds so tests advance deterministically without sleeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleInputs {
    /// Tail-follow catch-up still sliding toward the live tail.
    pub tail_catchup: bool,
    /// Elastic overscroll countdown still depleting.
    pub overscroll: bool,
    /// Terminal window currently focused.
    pub focused: bool,
    /// Turn actively processing.
    pub processing: bool,
    /// True when no streaming text is visible.
    pub streaming_empty: bool,
    /// Mouse scroll animation in flight.
    pub mouse_scroll: bool,
    /// Copy selection edge autoscroll active.
    pub copy_autoscroll: bool,
    /// Persistent notification line present.
    pub notification: bool,
    /// Rate-limit countdown inside its final minute.
    pub rate_limit: bool,
    /// Remote startup phase active.
    pub remote_startup: bool,
    /// Transient status notice visible.
    pub status_notice: bool,
    /// Full-line status animation visible (countdown, build progress).
    pub status_animation: bool,
    /// Decorative animation allowed by policy and configuration.
    pub decorative_enabled: bool,
    /// Renderer published animated rows on its last frame.
    pub animation_on_screen: bool,
    /// User actively typing within the backoff window.
    pub composing: bool,
    /// Current clock for dormancy math.
    pub now_ms: u64,
    /// Last real activity (frames, stream events) in clock millis.
    pub last_activity_ms: Option<u64>,
    /// Last user interaction (key, mouse, paste) in clock millis.
    pub last_interaction_ms: Option<u64>,
}

impl Default for ScheduleInputs {
    fn default() -> Self {
        Self {
            tail_catchup: false,
            overscroll: false,
            focused: true,
            processing: false,
            streaming_empty: true,
            mouse_scroll: false,
            copy_autoscroll: false,
            notification: false,
            rate_limit: false,
            remote_startup: false,
            status_notice: false,
            status_animation: false,
            decorative_enabled: false,
            animation_on_screen: false,
            composing: false,
            now_ms: 0,
            last_activity_ms: None,
            last_interaction_ms: None,
        }
    }
}

/// Whether the session counts as dormant: no stream activity and no user
/// interaction for the deep-idle window. A fresh client with no interaction
/// recorded falls back to the activity clock alone.
pub fn deep_idle_dormant(
    now_ms: u64,
    last_activity_ms: Option<u64>,
    last_interaction_ms: Option<u64>,
) -> bool {
    let stream_dormant = last_activity_ms
        .map(|last| now_ms.saturating_sub(last) >= REDRAW_DEEP_IDLE_AFTER_MS)
        .unwrap_or(false);
    let user_dormant = last_interaction_ms
        .map(|last| now_ms.saturating_sub(last) >= REDRAW_DEEP_IDLE_AFTER_MS)
        .unwrap_or(true);
    stream_dormant && user_dormant
}

/// Whether decorative animation paces redraws: actually on screen, allowed,
/// focused, and nothing live competes with it.
pub fn decorative_paces(inputs: &ScheduleInputs) -> bool {
    inputs.animation_on_screen
        && inputs.decorative_enabled
        && inputs.focused
        && !inputs.processing
        && inputs.streaming_empty
        && !inputs.remote_startup
        && !deep_idle_dormant(
            inputs.now_ms,
            inputs.last_activity_ms,
            inputs.last_interaction_ms,
        )
}

/// Why a tick needs a full frame beyond decorative animation, if anything.
/// Named instead of a bare boolean chain so diagnostics report the exact
/// predicate keeping the loop on the expensive path.
pub fn live_activity_reason(inputs: &ScheduleInputs) -> Option<FullFrameReason> {
    if inputs.processing {
        return Some(FullFrameReason::Processing);
    }
    if !inputs.streaming_empty {
        return Some(FullFrameReason::Streaming);
    }
    if inputs.tail_catchup {
        return Some(FullFrameReason::TailCatchup);
    }
    if inputs.status_notice {
        return Some(FullFrameReason::StatusNotice);
    }
    if inputs.mouse_scroll {
        return Some(FullFrameReason::MouseScroll);
    }
    if inputs.copy_autoscroll {
        return Some(FullFrameReason::CopyAutoscroll);
    }
    if inputs.overscroll {
        return Some(FullFrameReason::Overscroll);
    }
    if inputs.notification {
        return Some(FullFrameReason::Notification);
    }
    if inputs.rate_limit {
        return Some(FullFrameReason::RateLimitCountdown);
    }
    if inputs.remote_startup {
        return Some(FullFrameReason::RemoteStartup);
    }
    None
}

/// Reason a full frame is required right now, checking status animation
/// before live activity. Returns `None` when nothing is live.
pub fn current_full_frame_reason(inputs: &ScheduleInputs) -> Option<FullFrameReason> {
    if inputs.status_animation {
        return Some(FullFrameReason::StatusAnimation);
    }
    live_activity_reason(inputs)
}

/// Tick cadence for the current state. Returns a symbolic cadence so the
/// capability policy maps it to milliseconds without a crate cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedrawCadence {
    TailCatchup,
    Overscroll,
    UnfocusedDeepIdle,
    DeepIdle,
    Decorative,
    DecorativeComposing,
    StatusAnimation,
    PassiveLiveness,
    LiveFast,
    StaticChrome,
    RemoteStartup,
    Idle,
}

/// Cadence for `inputs` in priority order: catch-up and overscroll first,
/// then unfocused and deep-idle short-circuits, then decorative, then status
/// animation, then passive liveness, then live output, then static chrome.
pub fn redraw_cadence(inputs: &ScheduleInputs) -> RedrawCadence {
    if inputs.tail_catchup {
        return RedrawCadence::TailCatchup;
    }
    if inputs.overscroll {
        return RedrawCadence::Overscroll;
    }
    if !inputs.focused
        && !inputs.processing
        && inputs.streaming_empty
        && !inputs.mouse_scroll
        && !inputs.copy_autoscroll
        && !inputs.remote_startup
        && !inputs.rate_limit
    {
        return RedrawCadence::UnfocusedDeepIdle;
    }
    let deep_idle = deep_idle_dormant(
        inputs.now_ms,
        inputs.last_activity_ms,
        inputs.last_interaction_ms,
    );
    if deep_idle
        && !inputs.processing
        && inputs.streaming_empty
        && !inputs.mouse_scroll
        && !inputs.copy_autoscroll
        && !inputs.remote_startup
        && !inputs.rate_limit
    {
        return RedrawCadence::DeepIdle;
    }
    if decorative_paces(inputs) {
        if inputs.composing {
            return RedrawCadence::DecorativeComposing;
        }
        return RedrawCadence::Decorative;
    }
    if inputs.status_animation {
        return RedrawCadence::StatusAnimation;
    }
    if !inputs.mouse_scroll && inputs.streaming_empty && (inputs.processing || inputs.rate_limit) {
        return RedrawCadence::PassiveLiveness;
    }
    if inputs.processing
        || !inputs.streaming_empty
        || inputs.mouse_scroll
        || inputs.copy_autoscroll
        || inputs.rate_limit
    {
        return RedrawCadence::LiveFast;
    }
    if inputs.status_notice {
        return RedrawCadence::StaticChrome;
    }
    if inputs.remote_startup {
        return RedrawCadence::RemoteStartup;
    }
    if deep_idle {
        return RedrawCadence::DeepIdle;
    }
    RedrawCadence::Idle
}

/// Whether a periodic tick needs a full frame, ignoring decorative-only
/// ticks. Records the reason so diagnostics stay queryable. Deep-idle quiet
/// sessions need nothing.
pub fn periodic_needs_full_excluding_decorative(inputs: &ScheduleInputs) -> bool {
    let deep_idle = deep_idle_dormant(
        inputs.now_ms,
        inputs.last_activity_ms,
        inputs.last_interaction_ms,
    );
    if deep_idle
        && !inputs.processing
        && inputs.streaming_empty
        && !inputs.mouse_scroll
        && !inputs.copy_autoscroll
        && !inputs.overscroll
        && !inputs.remote_startup
        && !inputs.rate_limit
    {
        return false;
    }
    if inputs.status_animation {
        record_full_frame_reason(FullFrameReason::StatusAnimation);
        return true;
    }
    if let Some(reason) = live_activity_reason(inputs) {
        record_full_frame_reason(reason);
        return true;
    }
    false
}

/// Inputs gating the single-cell spinner fast path. All conditions must hold;
/// anything else upgrades to a wider scope so a cheap tick never leaves stale
/// cells behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SingleCellInputs {
    /// Turn actively processing.
    pub processing: bool,
    /// Essential spinner kept on by the capability policy.
    pub primary_spinner: bool,
    /// No streaming text visible.
    pub streaming_empty: bool,
    /// Composer centered (spinner cell not at a stable address).
    pub centered: bool,
    /// Mouse scroll animation in flight.
    pub mouse_scroll: bool,
    /// Remote startup phase active.
    pub remote_startup: bool,
    /// Overlay, modal, or approval forcing a full frame.
    pub overlay_full: bool,
    /// Composer input changed since the last frame.
    pub input_changed: bool,
    /// Recorded animation cells exist.
    pub animation_cells: bool,
}

/// Whether the cheap single-cell tick may run. The ratatui diff already emits
/// cursor-addressed single-cell output, so this gates eligibility for the
/// seeded animation-only composition instead of bypassing the backend (which
/// would desynchronize the retained backend buffers).
pub fn single_cell_spinner_available(inputs: &SingleCellInputs) -> bool {
    inputs.processing
        && inputs.primary_spinner
        && inputs.streaming_empty
        && !inputs.centered
        && !inputs.mouse_scroll
        && !inputs.remote_startup
        && !inputs.overlay_full
        && !inputs.input_changed
        && inputs.animation_cells
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

/// Previous composed frame retained for partial redraws.
///
/// The renderer composes into a fresh buffer each frame, so a partial scope
/// must restore the untouched cells itself. [`FrameCache::seed`] copies the
/// stored frame into the working buffer before a partial composition and
/// [`FrameCache::capture`] stores the composed frame afterwards. The stored
/// frame is kept before theme adaptation so adaptation runs once per frame
/// and seeded cells are never flipped twice.
#[derive(Debug, Default)]
pub struct FrameCache {
    frame: Option<Buffer>,
}

impl FrameCache {
    /// Empty cache (first frame has nothing to seed).
    pub fn new() -> Self {
        Self { frame: None }
    }

    /// Whether a stored frame can seed a composition at `area`.
    pub fn can_seed(&self, area: Rect) -> bool {
        self.frame.as_ref().is_some_and(|prev| prev.area == area)
    }

    /// Seed `buf` from the stored frame. Callers must check
    /// [`Self::can_seed`] first; the copy reuses the working buffer's
    /// allocation.
    pub fn seed(&self, buf: &mut Buffer) {
        if let Some(prev) = self.frame.as_ref() {
            if prev.area == *buf.area() {
                buf.content.clear();
                buf.content.extend_from_slice(&prev.content);
            }
        }
    }

    /// Store the composed frame for the next partial composition, reusing the
    /// existing allocation.
    pub fn capture(&mut self, buf: &Buffer) {
        match self.frame.as_mut() {
            Some(prev) => {
                prev.area = buf.area;
                prev.content.clear();
                prev.content.extend_from_slice(&buf.content);
            }
            None => self.frame = Some(buf.clone()),
        }
    }

    /// Drop the stored frame, forcing the next partial scope to compose a
    /// full frame.
    pub fn clear(&mut self) {
        self.frame = None;
    }

    /// Apply an invalidation: soft and hard both drop the stored frame so the
    /// next partial scope upgrades to full; the terminal clear behind a hard
    /// invalidation stays with the shell, which owns the terminal.
    pub fn invalidate(&mut self, kind: InvalidationKind) {
        if kind != InvalidationKind::None {
            self.clear();
        }
    }

    /// Patch one stored cell in place so a single-cell tick keeps the seed in
    /// sync with what the backend painted. Out-of-bounds writes are ignored.
    /// Returns true when a cell was patched.
    pub fn patch_cell(
        &mut self,
        x: u16,
        y: u16,
        symbol: &str,
        style: ratatui::style::Style,
    ) -> bool {
        let Some(prev) = self.frame.as_mut() else {
            return false;
        };
        if x >= prev.area.width || y >= prev.area.height {
            return false;
        }
        prev[(x, y)].set_symbol(symbol);
        prev[(x, y)].set_style(style);
        true
    }

    /// Fill the stored frame with a sentinel symbol so the next composition
    /// treats every cell as dirty without clearing the terminal surface.
    /// Used as the soft-path equivalent of a hard clear for image and diagram
    /// placeholders that must never flash through an intermediate clear.
    pub fn fill_sentinel(&mut self, area: Rect, symbol: &str) {
        let mut sentinel = Buffer::filled(area, ratatui::buffer::Cell::EMPTY);
        for cell in sentinel.content.iter_mut() {
            cell.set_symbol(symbol);
        }
        self.frame = Some(sentinel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Cell;

    fn snapshot() -> RedrawSnapshot {
        RedrawSnapshot {
            content_version: 1,
            streaming_len: 0,
            streaming_hash: 0,
            footer_digest: 7,
            input_hash: 0,
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
    fn invalidation_sentinel_merges_hard_over_soft() {
        let mut sentinel = InvalidationSentinel::new();
        assert!(!sentinel.is_pending());
        sentinel.request(InvalidationKind::Soft);
        assert_eq!(sentinel.kind(), InvalidationKind::Soft);
        sentinel.request(InvalidationKind::None);
        assert_eq!(sentinel.kind(), InvalidationKind::Soft);
        sentinel.request(InvalidationKind::Hard);
        assert_eq!(sentinel.kind(), InvalidationKind::Hard);
        assert!(sentinel.kind().needs_hard_clear());
        assert!(!InvalidationKind::Soft.needs_hard_clear());
        assert_eq!(sentinel.take(), InvalidationKind::Hard);
        assert!(!sentinel.is_pending());
    }

    #[test]
    fn invalidate_drops_seed_without_touching_none() {
        use ratatui::layout::Rect;
        let area = Rect::new(0, 0, 4, 1);
        let mut cache = FrameCache::new();
        cache.capture(&Buffer::filled(area, Cell::EMPTY));
        cache.invalidate(InvalidationKind::None);
        assert!(cache.can_seed(area));
        cache.invalidate(InvalidationKind::Soft);
        assert!(!cache.can_seed(area));
        cache.capture(&Buffer::filled(area, Cell::EMPTY));
        cache.invalidate(InvalidationKind::Hard);
        assert!(!cache.can_seed(area));
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

    #[test]
    fn empty_frame_cache_cannot_seed() {
        let cache = FrameCache::new();
        assert!(!cache.can_seed(Rect::new(0, 0, 4, 1)));
        let mut buf = Buffer::filled(Rect::new(0, 0, 4, 1), Cell::EMPTY);
        cache.seed(&mut buf);
        assert_eq!(
            buf.content,
            Buffer::filled(Rect::new(0, 0, 4, 1), Cell::EMPTY).content
        );
    }

    #[test]
    fn captured_frame_seeds_only_matching_area() {
        let area = Rect::new(0, 0, 8, 2);
        let mut composed = Buffer::filled(area, Cell::EMPTY);
        composed.set_string(0, 0, "HELLO", ratatui::style::Style::default());
        let mut cache = FrameCache::new();
        cache.capture(&composed);

        assert!(cache.can_seed(area));
        assert!(!cache.can_seed(Rect::new(0, 0, 8, 3)));

        let mut blank = Buffer::filled(area, Cell::EMPTY);
        cache.seed(&mut blank);
        assert_eq!(blank.content, composed.content);

        // A mismatched area is left untouched so a stale frame never lands.
        let mut other = Buffer::filled(Rect::new(0, 0, 8, 3), Cell::EMPTY);
        let before = other.content.clone();
        cache.seed(&mut other);
        assert_eq!(other.content, before);
    }

    #[test]
    fn cleared_frame_cache_forces_full_composition() {
        let area = Rect::new(0, 0, 4, 1);
        let mut cache = FrameCache::new();
        cache.capture(&Buffer::filled(area, Cell::EMPTY));
        assert!(cache.can_seed(area));
        cache.clear();
        assert!(!cache.can_seed(area));
    }

    #[test]
    fn recapture_replaces_previous_frame() {
        let area = Rect::new(0, 0, 4, 1);
        let mut first = Buffer::filled(area, Cell::EMPTY);
        first.set_string(0, 0, "AAAA", ratatui::style::Style::default());
        let mut second = Buffer::filled(area, Cell::EMPTY);
        second.set_string(0, 0, "BBBB", ratatui::style::Style::default());

        let mut cache = FrameCache::new();
        cache.capture(&first);
        cache.capture(&second);
        let mut seeded = Buffer::filled(area, Cell::EMPTY);
        cache.seed(&mut seeded);
        assert_eq!(seeded.content, second.content);
    }

    #[test]
    fn seeding_preserves_untouched_cells_across_frames() {
        use ratatui::backend::TestBackend;
        use ratatui::style::Style;
        use ratatui::Terminal;

        let backend = TestBackend::new(10, 2);
        let mut terminal = Terminal::new(backend).expect("backend");
        let mut cache = FrameCache::new();

        // First frame paints a full surface.
        terminal
            .draw(|frame| {
                let area = frame.area();
                if cache.can_seed(area) {
                    cache.seed(frame.buffer_mut());
                }
                frame
                    .buffer_mut()
                    .set_string(0, 0, "HELLO", Style::default());
                cache.capture(frame.buffer_mut());
            })
            .expect("first draw");

        // Second frame is animation-only: it touches a single cell. The
        // terminal resets the working buffer each frame, so without seeding
        // the rest of the screen would be wiped.
        terminal
            .draw(|frame| {
                let area = frame.area();
                if cache.can_seed(area) {
                    cache.seed(frame.buffer_mut());
                }
                frame.buffer_mut().set_string(0, 1, "x", Style::default());
                cache.capture(frame.buffer_mut());
            })
            .expect("second draw");

        let buffer = terminal.backend().buffer();
        let row0: String = buffer
            .content
            .iter()
            .take(10)
            .map(|cell| cell.symbol())
            .collect();
        assert!(row0.starts_with("HELLO"), "row0 was {row0:?}");
        assert_eq!(
            buffer.cell((0, 1)).map(|cell| cell.symbol().to_string()),
            Some("x".to_string())
        );
    }

    #[test]
    fn deep_idle_needs_both_clocks() {
        assert!(deep_idle_dormant(60_000, Some(0), Some(0)));
        assert!(!deep_idle_dormant(60_000, Some(0), Some(59_000)));
        assert!(!deep_idle_dormant(10_000, Some(0), Some(0)));
        assert!(!deep_idle_dormant(60_000, None, Some(0)));
        assert!(deep_idle_dormant(60_000, Some(0), None));
    }

    #[test]
    fn decorative_paces_only_when_on_screen_and_allowed() {
        let base = ScheduleInputs {
            decorative_enabled: true,
            animation_on_screen: true,
            now_ms: 1_000,
            last_activity_ms: Some(900),
            last_interaction_ms: Some(900),
            ..ScheduleInputs::default()
        };
        assert!(decorative_paces(&base));
        assert!(!decorative_paces(&ScheduleInputs {
            animation_on_screen: false,
            ..base
        }));
        assert!(!decorative_paces(&ScheduleInputs {
            decorative_enabled: false,
            ..base
        }));
        assert!(!decorative_paces(&ScheduleInputs {
            focused: false,
            ..base
        }));
        assert!(!decorative_paces(&ScheduleInputs {
            processing: true,
            ..base
        }));
    }

    #[test]
    fn cadence_priority_orders_catchup_first() {
        let catchup = ScheduleInputs {
            tail_catchup: true,
            processing: true,
            ..ScheduleInputs::default()
        };
        assert_eq!(redraw_cadence(&catchup), RedrawCadence::TailCatchup);
        let overscroll = ScheduleInputs {
            overscroll: true,
            processing: true,
            ..ScheduleInputs::default()
        };
        assert_eq!(redraw_cadence(&overscroll), RedrawCadence::Overscroll);
    }

    #[test]
    fn unfocused_idle_falls_to_deep_idle_cadence() {
        let inputs = ScheduleInputs {
            focused: false,
            now_ms: 1_000,
            ..ScheduleInputs::default()
        };
        assert_eq!(redraw_cadence(&inputs), RedrawCadence::UnfocusedDeepIdle);
    }

    #[test]
    fn composing_backs_decorative_off_fast_path() {
        let base = ScheduleInputs {
            decorative_enabled: true,
            animation_on_screen: true,
            now_ms: 1_000,
            last_activity_ms: Some(900),
            last_interaction_ms: Some(900),
            ..ScheduleInputs::default()
        };
        assert_eq!(redraw_cadence(&base), RedrawCadence::Decorative);
        assert_eq!(
            redraw_cadence(&ScheduleInputs {
                composing: true,
                ..base
            }),
            RedrawCadence::DecorativeComposing
        );
    }

    #[test]
    fn live_activity_reason_names_each_predicate() {
        assert_eq!(
            live_activity_reason(&ScheduleInputs {
                processing: true,
                ..ScheduleInputs::default()
            }),
            Some(FullFrameReason::Processing)
        );
        assert_eq!(
            live_activity_reason(&ScheduleInputs {
                streaming_empty: false,
                ..ScheduleInputs::default()
            }),
            Some(FullFrameReason::Streaming)
        );
        assert_eq!(
            live_activity_reason(&ScheduleInputs {
                tail_catchup: true,
                ..ScheduleInputs::default()
            }),
            Some(FullFrameReason::TailCatchup)
        );
        assert_eq!(live_activity_reason(&ScheduleInputs::default()), None);
        assert_eq!(
            current_full_frame_reason(&ScheduleInputs {
                status_animation: true,
                processing: true,
                ..ScheduleInputs::default()
            }),
            Some(FullFrameReason::StatusAnimation)
        );
    }

    #[test]
    fn reason_table_round_trips() {
        for reason in [
            FullFrameReason::Processing,
            FullFrameReason::Streaming,
            FullFrameReason::TailCatchup,
            FullFrameReason::StatusNotice,
            FullFrameReason::MouseScroll,
            FullFrameReason::CopyAutoscroll,
            FullFrameReason::Overscroll,
            FullFrameReason::Notification,
            FullFrameReason::RateLimitCountdown,
            FullFrameReason::RemoteStartup,
            FullFrameReason::StatusAnimation,
        ] {
            assert!(FULL_FRAME_REDRAW_REASONS.contains(&reason.as_str()));
            assert_eq!(FullFrameReason::from_name(reason.as_str()), Some(reason));
            record_full_frame_reason(reason);
            assert_eq!(last_full_frame_reason(), Some(reason));
        }
        assert_eq!(FullFrameReason::from_name("not-a-reason"), None);
    }

    #[test]
    fn periodic_excluding_decorative_skips_quiet_deep_idle() {
        let quiet = ScheduleInputs {
            now_ms: 120_000,
            last_activity_ms: Some(0),
            last_interaction_ms: Some(0),
            ..ScheduleInputs::default()
        };
        assert!(!periodic_needs_full_excluding_decorative(&quiet));
        let live = ScheduleInputs {
            processing: true,
            now_ms: 1_000,
            last_activity_ms: Some(900),
            last_interaction_ms: Some(900),
            ..ScheduleInputs::default()
        };
        assert!(periodic_needs_full_excluding_decorative(&live));
        let decorative_only = ScheduleInputs {
            decorative_enabled: true,
            animation_on_screen: true,
            now_ms: 1_000,
            last_activity_ms: Some(900),
            last_interaction_ms: Some(900),
            ..ScheduleInputs::default()
        };
        assert!(!periodic_needs_full_excluding_decorative(&decorative_only));
    }

    #[test]
    fn input_change_forces_full() {
        let prev = snapshot();
        let mut next = prev;
        next.input_hash = 0x9e37;
        assert_eq!(decide_scope(Some(prev), next), RedrawScope::Full);
    }

    #[test]
    fn single_cell_needs_every_gate() {
        let base = SingleCellInputs {
            processing: true,
            primary_spinner: true,
            streaming_empty: true,
            animation_cells: true,
            ..SingleCellInputs::default()
        };
        assert!(single_cell_spinner_available(&base));
        for blocked in [
            SingleCellInputs {
                processing: false,
                ..base
            },
            SingleCellInputs {
                primary_spinner: false,
                ..base
            },
            SingleCellInputs {
                streaming_empty: false,
                ..base
            },
            SingleCellInputs {
                centered: true,
                ..base
            },
            SingleCellInputs {
                mouse_scroll: true,
                ..base
            },
            SingleCellInputs {
                remote_startup: true,
                ..base
            },
            SingleCellInputs {
                overlay_full: true,
                ..base
            },
            SingleCellInputs {
                input_changed: true,
                ..base
            },
            SingleCellInputs {
                animation_cells: false,
                ..base
            },
        ] {
            assert!(
                !single_cell_spinner_available(&blocked),
                "gate must hold: {blocked:?}"
            );
        }
    }

    #[test]
    fn patch_cell_keeps_seed_in_sync() {
        use ratatui::style::{Color, Modifier, Style};
        let area = Rect::new(0, 0, 4, 1);
        let mut cache = FrameCache::new();
        assert!(!cache.patch_cell(0, 0, "x", Style::default()));
        cache.capture(&Buffer::filled(area, Cell::EMPTY));
        let style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        assert!(cache.patch_cell(1, 0, "x", style));
        assert!(!cache.patch_cell(9, 0, "x", style));
        let mut seeded = Buffer::filled(area, Cell::EMPTY);
        cache.seed(&mut seeded);
        assert_eq!(seeded[(1, 0)].symbol(), "x");
        assert_eq!(seeded[(1, 0)].style().fg, Some(Color::Cyan));
        assert!(seeded[(1, 0)].style().add_modifier.contains(Modifier::BOLD));
        assert_eq!(seeded[(0, 0)].symbol(), " ");
    }

    #[test]
    fn sentinel_fill_forces_repaint_without_clear() {
        let area = Rect::new(0, 0, 4, 1);
        let mut cache = FrameCache::new();
        cache.capture(&Buffer::filled(area, Cell::EMPTY));
        cache.fill_sentinel(area, "\u{0}");
        assert!(cache.can_seed(area));
        let mut seeded = Buffer::filled(area, Cell::EMPTY);
        cache.seed(&mut seeded);
        assert!(seeded.content.iter().all(|cell| cell.symbol() == "\u{0}"));
    }
}
