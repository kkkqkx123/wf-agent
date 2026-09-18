//! Read-only render view, test doubles and headless frame rendering.
//!
//! [`RenderView`] is the wide read-only interface between state and
//! drawing: preparation and drawing only see this trait, never concrete
//! controllers. [`TestRenderModel`] is the test double (fixed history plus
//! streaming text stepped by the injectable clock), [`render_headless`]
//! runs the scrollback layout without a terminal, and
//! [`draw_recovered_frame`] renders the panic fallback so a failing widget
//! degrades to a visible frame instead of killing the loop. Event
//! recording hooks ([`EventRecorder`] / [`EventPlayer`]) serialize an
//! interaction sequence for replay; full capture arrives later.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use serde::{Deserialize, Serialize};

use crate::frame_metrics::{FrameMetric, FrameMetrics};
use crate::width::{ColumnWidth, WidthMeasure};
use tui_style::theme_mode::ThemeMode;

/// Read-only rendering source. Every method has a default so test doubles
/// only override the groups they care about; version accessors are the
/// single invalidation signal for caches.
pub trait RenderView {
    /// Monotonic scrollback content version.
    fn content_version(&self) -> u64 {
        0
    }
    /// Visible streaming prefix text.
    fn streaming_text(&self) -> &str {
        ""
    }
    /// Layout width in columns.
    fn width(&self) -> u16 {
        80
    }
    /// Viewport height in rows.
    fn height(&self) -> u16 {
        24
    }
    /// Whether an overlay, modal or picker is active (forces full frames).
    fn overlay_active(&self) -> bool {
        false
    }
    /// Viewport scroll offset in display rows (0 is tail-follow).
    fn view_scroll(&self) -> usize {
        0
    }
    /// True when automatic tail-follow is paused for reading history.
    fn auto_scroll_paused(&self) -> bool {
        false
    }
    /// Current composer input text.
    fn input_text(&self) -> &str {
        ""
    }
    /// Composer cursor position in characters.
    fn input_cursor(&self) -> usize {
        0
    }
    /// Whether an agent turn is streaming.
    fn is_processing(&self) -> bool {
        false
    }
    /// Queued follow-up prompt count.
    fn queued_count(&self) -> usize {
        0
    }
    /// Active theme mode.
    fn theme_mode(&self) -> ThemeMode {
        ThemeMode::Dark
    }
    /// Discretized animation tick (see [`tui_clock::clock::anim_bucket`]).
    fn anim_tick(&self) -> u64 {
        0
    }
    /// Digest of the footer state.
    fn footer_digest(&self) -> u64 {
        0
    }
    /// Active performance tier marker.
    fn perf_marker(&self) -> &str {
        "perf:full"
    }
    /// Side panel visibility.
    fn side_panel_visible(&self) -> bool {
        false
    }
    /// Diagram pane visibility.
    fn diagram_visible(&self) -> bool {
        false
    }
    /// Image collection signature for cache identity.
    fn image_signature(&self) -> u64 {
        0
    }
    /// Number of committed history rows.
    fn history_count(&self) -> usize {
        0
    }
    /// Plain text of history row `index`, oldest first.
    fn history_line(&self, index: usize) -> Option<&str> {
        let _ = index;
        None
    }
}

/// Per-domain read views converging state access: rendering and key handling
/// depend only on the domains they need, never on a concrete controller.
/// Each trait stays under ten methods by design.
pub trait TranscriptView {
    fn content_version(&self) -> u64;
    fn history_count(&self) -> usize;
    fn history_line(&self, index: usize) -> Option<&str>;
    fn streaming_text(&self) -> &str;
    fn image_signature(&self) -> u64;
}

pub trait InputView {
    fn input_text(&self) -> &str;
    fn input_cursor(&self) -> usize;
    fn is_processing(&self) -> bool;
    fn queued_count(&self) -> usize;
}

pub trait ScrollView {
    fn view_scroll(&self) -> usize;
    fn auto_scroll_paused(&self) -> bool;
}

pub trait LayoutView {
    fn width(&self) -> u16;
    fn height(&self) -> u16;
    fn overlay_active(&self) -> bool;
    fn side_panel_visible(&self) -> bool;
    fn diagram_visible(&self) -> bool;
}

pub trait ThemeView {
    fn theme_mode(&self) -> ThemeMode;
}

pub trait PerfView {
    fn anim_tick(&self) -> u64;
    fn footer_digest(&self) -> u64;
    fn perf_marker(&self) -> &str;
}

/// Fixed-content test double stepping through [`RenderView`] with the
/// injectable clock.
#[derive(Debug, Clone, Default)]
pub struct TestRenderModel {
    /// Committed history rows, oldest first.
    pub history: Vec<String>,
    /// In-flight streaming text.
    pub streaming: String,
    /// Content version bumped by the test on each mutation.
    pub version: u64,
    /// Layout width in columns.
    pub width: u16,
    /// Viewport height in rows.
    pub height: u16,
    /// Whether an overlay is active in the recorded state.
    pub overlay: bool,
    /// Viewport scroll offset in display rows.
    pub scroll: usize,
    /// Whether tail-follow is paused.
    pub paused: bool,
    /// Composer input text.
    pub input: String,
    /// Composer cursor position.
    pub cursor: usize,
    /// Whether a turn is streaming.
    pub processing: bool,
    /// Queued prompt count.
    pub queued: usize,
    /// Active theme mode.
    pub mode: ThemeMode,
    /// Clock value (ms) driving the animation tick.
    pub now_ms: u64,
    /// Footer digest.
    pub footer: u64,
    /// Performance tier marker.
    pub perf: String,
    /// Side panel visibility.
    pub side_panel: bool,
    /// Diagram visibility.
    pub diagram: bool,
    /// Image collection signature.
    pub images: u64,
}

impl TestRenderModel {
    /// Empty model at `width` columns.
    pub fn new(width: u16) -> Self {
        Self {
            width,
            height: 24,
            perf: "perf:full".to_string(),
            ..Self::default()
        }
    }

    /// Append a history row and bump the version.
    pub fn push_history(&mut self, line: impl Into<String>) {
        self.history.push(line.into());
        self.version = self.version.wrapping_add(1);
    }

    /// Replace the streaming prefix.
    pub fn set_streaming(&mut self, text: impl Into<String>) {
        self.streaming = text.into();
    }

    /// Advance the model's clock; animation ticks follow on demand.
    pub fn advance(&mut self, delta_ms: u64) {
        self.now_ms = self.now_ms.saturating_add(delta_ms);
    }
}

impl TranscriptView for TestRenderModel {
    fn content_version(&self) -> u64 {
        self.version
    }
    fn history_count(&self) -> usize {
        self.history.len()
    }
    fn history_line(&self, index: usize) -> Option<&str> {
        self.history.get(index).map(String::as_str)
    }
    fn streaming_text(&self) -> &str {
        &self.streaming
    }
    fn image_signature(&self) -> u64 {
        self.images
    }
}

impl InputView for TestRenderModel {
    fn input_text(&self) -> &str {
        &self.input
    }
    fn input_cursor(&self) -> usize {
        self.cursor
    }
    fn is_processing(&self) -> bool {
        self.processing
    }
    fn queued_count(&self) -> usize {
        self.queued
    }
}

impl ScrollView for TestRenderModel {
    fn view_scroll(&self) -> usize {
        self.scroll
    }
    fn auto_scroll_paused(&self) -> bool {
        self.paused
    }
}

impl LayoutView for TestRenderModel {
    fn width(&self) -> u16 {
        self.width
    }
    fn height(&self) -> u16 {
        self.height
    }
    fn overlay_active(&self) -> bool {
        self.overlay
    }
    fn side_panel_visible(&self) -> bool {
        self.side_panel
    }
    fn diagram_visible(&self) -> bool {
        self.diagram
    }
}

impl ThemeView for TestRenderModel {
    fn theme_mode(&self) -> ThemeMode {
        self.mode
    }
}

impl PerfView for TestRenderModel {
    fn anim_tick(&self) -> u64 {
        tui_clock::clock::anim_bucket(self.now_ms)
    }
    fn footer_digest(&self) -> u64 {
        self.footer
    }
    fn perf_marker(&self) -> &str {
        &self.perf
    }
}

impl RenderView for TestRenderModel {
    fn content_version(&self) -> u64 {
        self.version
    }
    fn streaming_text(&self) -> &str {
        &self.streaming
    }
    fn width(&self) -> u16 {
        self.width
    }
    fn height(&self) -> u16 {
        self.height
    }
    fn overlay_active(&self) -> bool {
        self.overlay
    }
    fn view_scroll(&self) -> usize {
        self.scroll
    }
    fn auto_scroll_paused(&self) -> bool {
        self.paused
    }
    fn input_text(&self) -> &str {
        &self.input
    }
    fn input_cursor(&self) -> usize {
        self.cursor
    }
    fn is_processing(&self) -> bool {
        self.processing
    }
    fn queued_count(&self) -> usize {
        self.queued
    }
    fn theme_mode(&self) -> ThemeMode {
        self.mode
    }
    fn anim_tick(&self) -> u64 {
        tui_clock::clock::anim_bucket(self.now_ms)
    }
    fn footer_digest(&self) -> u64 {
        self.footer
    }
    fn perf_marker(&self) -> &str {
        &self.perf
    }
    fn side_panel_visible(&self) -> bool {
        self.side_panel
    }
    fn diagram_visible(&self) -> bool {
        self.diagram
    }
    fn image_signature(&self) -> u64 {
        self.images
    }
    fn history_count(&self) -> usize {
        self.history.len()
    }
    fn history_line(&self, index: usize) -> Option<&str> {
        self.history.get(index).map(String::as_str)
    }
}

/// Viewport geometry probe: the visible display-row window for a scrollback
/// of `total_rows` with `scroll` rows held above the tail.
pub fn viewport_window(total_rows: usize, height: usize, scroll: usize) -> (usize, usize) {
    let height = height.max(1);
    let max_scroll = total_rows.saturating_sub(height);
    let scroll = scroll.min(max_scroll);
    let start = max_scroll.saturating_sub(scroll);
    (start, start.saturating_add(height).min(total_rows))
}

/// Lay out a view without a terminal: wrap history plus the streaming tail
/// to `width` and return the visible rows for a `height`-tall viewport.
/// Wrapping is grapheme-safe and mirrors the scrollback tail-follow rule.
///
/// This is a layout probe, not the event-summary kernel in
/// [`crate::headless::HeadlessRenderer`]: that kernel turns execution events
/// into stdout/diag text, while this function only wraps committed rows for
/// geometry and budget assertions.
pub fn render_headless(model: &impl RenderView, width: u16, height: usize) -> Vec<String> {
    let w = usize::from(width.max(1));
    let mut rows: Vec<String> = Vec::new();
    for index in 0..model.history_count() {
        if let Some(line) = model.history_line(index) {
            rows.extend(wrap_plain(line, w));
        }
    }
    let streaming = model.streaming_text();
    if !streaming.is_empty() {
        rows.extend(wrap_plain(streaming, w));
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    let (start, end) = viewport_window(rows.len(), height, model.view_scroll());
    rows[start..end].to_vec()
}

/// Lay out a view without a terminal through the neutral document model:
/// each history row is parsed to a document, flattened to plain text, then
/// wrapped. The plain-text view stays the ground truth, so this path must
/// agree with [`render_headless`] on the same input.
pub fn render_headless_document(model: &impl RenderView, width: u16, height: usize) -> Vec<String> {
    let w = usize::from(width.max(1));
    let mut rows: Vec<String> = Vec::new();
    for index in 0..model.history_count() {
        if let Some(line) = model.history_line(index) {
            let doc = tui_markdown::markdown::document::parse_document(line);
            let plain = tui_markdown::markdown::document::document_plain_text(&doc);
            let source = if plain.is_empty() {
                line.to_string()
            } else {
                plain
            };
            for part in source.split('\n') {
                rows.extend(wrap_plain(part, w));
            }
        }
    }
    let streaming = model.streaming_text();
    if !streaming.is_empty() {
        rows.extend(wrap_plain(streaming, w));
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    let (start, end) = viewport_window(rows.len(), height, model.view_scroll());
    rows[start..end].to_vec()
}

/// Replay a fixed model sequence headlessly and collect per-frame metrics.
/// Each frame lays out the visible rows and samples the streaming bytes as
/// the parse signal, so the returned report is a deterministic baseline for
/// optimization comparisons: rerun the same sequence after a change and
/// diff the reports.
pub fn replay_baseline(models: &[TestRenderModel], width: u16, height: usize) -> FrameMetrics {
    let mut metrics = FrameMetrics::new();
    for (frame, model) in models.iter().enumerate() {
        let rows = render_headless(model, width, height);
        let parsed = RenderView::streaming_text(model).len()
            + model.history.iter().map(|line| line.len()).sum::<usize>();
        metrics.record(FrameMetric::new(frame as u64, 0, 0, 0, rows.len(), parsed));
    }
    metrics
}

/// Fixed representative frame sequence for baseline reports: empty, two
/// streaming-growth frames, a commit, a scrolled frame and a settled tail.
/// Rerun [`replay_baseline`] over this sequence before and after an
/// optimization and diff the reports.
pub fn fixed_baseline_sequence(width: u16) -> Vec<TestRenderModel> {
    let mut empty = TestRenderModel::new(width);
    empty.now_ms = 0;
    let mut growing = TestRenderModel::new(width);
    growing.push_history("alpha line one");
    growing.set_streaming("live tail one");
    growing.now_ms = 100;
    let mut grown = growing.clone();
    grown.set_streaming("live tail one plus more text arriving");
    grown.now_ms = 200;
    let mut committed = TestRenderModel::new(width);
    committed.push_history("alpha line one");
    committed.push_history("live tail one plus more text arriving");
    committed.now_ms = 300;
    let mut scrolled = committed.clone();
    scrolled.push_history("third line settles");
    scrolled.scroll = 1;
    scrolled.now_ms = 400;
    let mut settled = scrolled.clone();
    settled.scroll = 0;
    settled.now_ms = 500;
    vec![empty, growing, grown, committed, scrolled, settled]
}

/// Key rectangles a headless test can assert without pixel comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameGeometry {
    /// Full frame area.
    pub frame: Rect,
    /// Scrollback viewport area.
    pub viewport: Rect,
    /// Optional animation bounding box.
    pub anim_bounds: Option<Rect>,
}

impl FrameGeometry {
    /// Split a frame into scrollback viewport plus footer/input rows.
    pub fn split(frame: Rect, footer_height: u16, input_height: u16) -> Self {
        let viewport_height = frame
            .height
            .saturating_sub(footer_height + input_height + 1);
        let viewport = Rect {
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: viewport_height,
        };
        Self {
            frame,
            viewport,
            anim_bounds: None,
        }
    }

    /// Attach the recorded animation bounds so animation-only frames can
    /// assert the exact cells they may touch.
    pub fn with_anim_bounds(mut self, area: crate::redraw::AnimationArea) -> Self {
        self.anim_bounds = area.bounds();
        self
    }
}

/// Render the panic fallback into `buf`: a recognizable frame proving the
/// loop survived a failing widget.
pub fn draw_recovered_frame(buf: &mut Buffer, area: Rect) {
    let msg = "render recovered after panic";
    buf.set_string(area.x, area.y, msg, Style::default());
}

/// Recorded interaction event for session replay with timestamps and bus
/// coverage. The version field keeps serialized sequences comparable across
/// recorder changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecordedEvent {
    /// Logical key press identified by its debug label at `at_ms`.
    Key { label: String, at_ms: u64 },
    /// Terminal resize to columns × rows at `at_ms`.
    Resize { width: u16, height: u16, at_ms: u64 },
    /// Streamed text delta arrival at `at_ms`.
    Delta { text: String, at_ms: u64 },
    /// Background bus notification at `at_ms`.
    Bus { topic: String, at_ms: u64 },
    /// Frame tick at the given clock value.
    Tick(u64),
}

impl RecordedEvent {
    /// Clock value carried by the event, if any.
    pub fn at_ms(&self) -> Option<u64> {
        match self {
            RecordedEvent::Key { at_ms, .. }
            | RecordedEvent::Resize { at_ms, .. }
            | RecordedEvent::Delta { at_ms, .. }
            | RecordedEvent::Bus { at_ms, .. } => Some(*at_ms),
            RecordedEvent::Tick(value) => Some(*value),
        }
    }
}

/// Backwards-compatible constructors for the logical event shapes.
impl RecordedEvent {
    pub fn key(label: impl Into<String>) -> Self {
        Self::Key {
            label: label.into(),
            at_ms: 0,
        }
    }

    pub fn resize(width: u16, height: u16) -> Self {
        Self::Resize {
            width,
            height,
            at_ms: 0,
        }
    }

    pub fn delta(text: impl Into<String>) -> Self {
        Self::Delta {
            text: text.into(),
            at_ms: 0,
        }
    }
}

/// Collects a JSON-serializable interaction sequence.
#[derive(Debug, Default)]
pub struct EventRecorder {
    events: Vec<RecordedEvent>,
}

impl EventRecorder {
    /// Empty recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event.
    pub fn push(&mut self, event: RecordedEvent) {
        self.events.push(event);
    }

    /// Recorded events in order.
    pub fn events(&self) -> &[RecordedEvent] {
        &self.events
    }

    /// Serialize the sequence to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.events).unwrap_or_else(|_| "[]".to_string())
    }
}

/// Replays a recorded sequence frame by frame.
#[derive(Debug, Default)]
pub struct EventPlayer {
    events: Vec<RecordedEvent>,
    cursor: usize,
}

impl EventPlayer {
    /// Build a player from a recorded JSON sequence; malformed input yields
    /// an empty player rather than failing.
    pub fn from_json(raw: &str) -> Self {
        let events: Vec<RecordedEvent> = serde_json::from_str(raw).unwrap_or_default();
        Self { events, cursor: 0 }
    }

    /// Next event, if any.
    pub fn next_event(&mut self) -> Option<&RecordedEvent> {
        let event = self.events.get(self.cursor)?;
        self.cursor += 1;
        Some(event)
    }

    /// Whether the whole sequence was consumed.
    pub fn exhausted(&self) -> bool {
        self.cursor >= self.events.len()
    }
}

/// Failure evidence bundle: everything needed to reproduce a failed frame
/// assertion without rerunning the live session. Built only on failure, so
/// the success path pays nothing.
#[derive(Debug, Default)]
pub struct TestEvidence {
    /// Bundle name (usually the failing test name).
    pub name: String,
    /// Interaction sequence leading to the failure.
    pub events: Vec<RecordedEvent>,
    /// Frame snapshots (one entry per frame, headless rows joined).
    pub frames: Vec<String>,
    /// Assertion messages that failed.
    pub assertions: Vec<String>,
    /// Captured log lines.
    pub logs: Vec<String>,
    /// Captured standard output lines.
    pub stdout: Vec<String>,
    /// Captured standard error lines.
    pub stderr: Vec<String>,
    /// Sealed bundles reject further mutation; `seal` freezes the bundle
    /// for handoff to CI artifacts.
    sealed: bool,
}

impl TestEvidence {
    /// New empty bundle.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Record an event. Ignored once sealed.
    pub fn add_event(&mut self, event: RecordedEvent) {
        if !self.sealed {
            self.events.push(event);
        }
    }

    /// Record a frame snapshot. Ignored once sealed.
    pub fn add_frame(&mut self, frame: impl Into<String>) {
        if !self.sealed {
            self.frames.push(frame.into());
        }
    }

    /// Record a failed assertion. Ignored once sealed.
    pub fn add_assertion(&mut self, assertion: impl Into<String>) {
        if !self.sealed {
            self.assertions.push(assertion.into());
        }
    }

    /// Record a log line. Ignored once sealed.
    pub fn add_log(&mut self, log: impl Into<String>) {
        if !self.sealed {
            self.logs.push(log.into());
        }
    }

    /// Record a standard output line. Ignored once sealed.
    pub fn add_stdout(&mut self, line: impl Into<String>) {
        if !self.sealed {
            self.stdout.push(line.into());
        }
    }

    /// Record a standard error line. Ignored once sealed.
    pub fn add_stderr(&mut self, line: impl Into<String>) {
        if !self.sealed {
            self.stderr.push(line.into());
        }
    }

    /// True when at least one assertion failed.
    pub fn failed(&self) -> bool {
        !self.assertions.is_empty()
    }

    /// One-line summary for test output.
    pub fn summary(&self) -> String {
        format!(
            "{}: events={} frames={} assertions={} logs={} stdout={} stderr={}",
            self.name,
            self.events.len(),
            self.frames.len(),
            self.assertions.len(),
            self.logs.len(),
            self.stdout.len(),
            self.stderr.len(),
        )
    }

    /// Seal the bundle: further `add_*` calls are ignored. Returns the
    /// summary for CI logs.
    pub fn seal(&mut self) -> String {
        self.sealed = true;
        self.summary()
    }

    /// Whether the bundle is sealed.
    pub fn is_sealed(&self) -> bool {
        self.sealed
    }
}

/// Key layout rectangles a headless test asserts without pixel comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutProbe {
    /// Scrollback viewport area.
    pub viewport: Rect,
    /// Footer/status area.
    pub footer: Rect,
    /// Input/composer area.
    pub input: Rect,
}

/// Split a frame into viewport, footer and input rectangles for the given
/// row heights. The probe carries no pixels, only geometry.
pub fn probe_layout(frame: Rect, footer_height: u16, input_height: u16) -> LayoutProbe {
    let viewport_height = frame.height.saturating_sub(footer_height + input_height);
    let footer_y = frame.y + viewport_height;
    LayoutProbe {
        viewport: Rect {
            height: viewport_height,
            ..frame
        },
        footer: Rect {
            y: footer_y,
            height: footer_height.min(frame.height.saturating_sub(viewport_height)),
            ..frame
        },
        input: Rect {
            y: footer_y + footer_height.min(frame.height.saturating_sub(viewport_height)),
            height: input_height.min(frame.height.saturating_sub(viewport_height + footer_height)),
            ..frame
        },
    }
}

/// Unified geometry snapshot: viewport, footer, input, diagram and sidebar
/// rectangles plus the decision inputs that produced them. Regression tests
/// assert this struct instead of pixels; production fills it from the same
/// layout entry points the draw path uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnifiedGeometryProbe {
    pub viewport: Rect,
    pub footer: Rect,
    pub input: Rect,
    pub diagram: Option<Rect>,
    pub sidebar: Option<Rect>,
    pub aspect_bucket: u8,
    pub diagram_requested: u16,
    pub max_ratio: u32,
    pub focused: bool,
    pub overlay_active: bool,
}

/// Probe unified geometry without drawing.
pub fn probe_unified_geometry(
    frame: Rect,
    footer_height: u16,
    input_height: u16,
    diagram_requested: u16,
    sidebar_active: bool,
    focused: bool,
    overlay_active: bool,
) -> UnifiedGeometryProbe {
    let layout = probe_layout(frame, footer_height, input_height);
    let aspect_bucket = {
        let h = frame.height.max(1);
        ((f32::from(frame.width) / f32::from(h) * 10.0).round() as u8).min(9)
    };
    let sidebar = sidebar_active.then(|| Rect {
        x: frame.x,
        y: frame.y,
        width: (f32::from(frame.width) * 0.3) as u16,
        height: frame.height.saturating_sub(1),
    });
    let diagram = if diagram_requested == 0 {
        None
    } else {
        let max_width = ((f32::from(frame.width) * 0.4).round() as u16).max(20);
        let w = diagram_requested.clamp(20, max_width);
        if frame.width <= 50 || w >= frame.width {
            None
        } else {
            Some(Rect {
                x: frame.x + frame.width - w,
                width: w,
                ..frame
            })
        }
    };
    UnifiedGeometryProbe {
        viewport: layout.viewport,
        footer: layout.footer,
        input: layout.input,
        diagram,
        sidebar,
        aspect_bucket,
        diagram_requested,
        max_ratio: 40,
        focused,
        overlay_active,
    }
}

/// Draw one component in isolation: a panic degrades to the recovered
/// placeholder for that area and records a metric instead of killing the
/// frame. Returns the elapsed metric for budget accounting.
pub fn isolated_component_draw(
    frame_no: u64,
    buf: &mut Buffer,
    area: Rect,
    draw: impl FnOnce(&mut Buffer, Rect),
) -> FrameMetric {
    let start = std::time::Instant::now();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        draw(buf, area);
    }));
    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    if outcome.is_err() {
        draw_recovered_frame(buf, area);
        tracing::warn!("tui: isolated component panicked, recovered placeholder drawn");
    }
    FrameMetric::new(
        frame_no,
        area.width as usize * area.height as usize,
        area.width as usize * area.height as usize * 8,
        elapsed_ms,
        0,
        0,
    )
}

/// Transport boundary of the event loop: the single submit point behind
/// `Terminal::draw` in production and a recording double in tests. Replay
/// drives the full loop through this trait without a real terminal.
pub trait FrameTransport {
    /// Submit one frame of `scope`; returns whether a frame was emitted.
    fn submit(&mut self, scope: crate::redraw::RedrawScope) -> bool;
}

/// Recording transport double: replays submit decisions headlessly.
#[derive(Debug, Default)]
pub struct TestTransport {
    submitted: Vec<crate::redraw::RedrawScope>,
}

impl TestTransport {
    /// Empty double.
    pub fn new() -> Self {
        Self::default()
    }

    /// Submitted scopes in order.
    pub fn submitted(&self) -> &[crate::redraw::RedrawScope] {
        &self.submitted
    }
}

impl FrameTransport for TestTransport {
    fn submit(&mut self, scope: crate::redraw::RedrawScope) -> bool {
        if scope == crate::redraw::RedrawScope::None {
            return false;
        }
        self.submitted.push(scope);
        true
    }
}

/// Wrap one plain-text line to `width` columns on grapheme-cluster
/// boundaries so ZWJ sequences and flags never split.
fn wrap_plain(text: &str, width: usize) -> Vec<String> {
    wrap_plain_with(&ColumnWidth, text, width)
}

/// Wrap one plain-text line to `width` columns measured by `measure`, so a
/// second frontend can reuse the algorithm with different font metrics.
fn wrap_plain_with(measure: &impl WidthMeasure, text: &str, width: usize) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut rows = Vec::new();
    for source in text.split('\n') {
        let mut current = String::new();
        let mut current_width = 0usize;
        for grapheme in source.graphemes(true) {
            let cw = grapheme
                .chars()
                .map(|ch| measure.cell_width(ch))
                .max()
                .unwrap_or(0);
            if current_width + cw > width && !current.is_empty() {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
            }
            current.push_str(grapheme);
            current_width += cw;
        }
        rows.push(current);
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::{
        draw_recovered_frame, fixed_baseline_sequence, probe_layout, probe_unified_geometry,
        render_headless, render_headless_document, replay_baseline, viewport_window, EventPlayer,
        EventRecorder, FrameGeometry, FrameTransport, RecordedEvent, RenderView, TestEvidence,
        TestRenderModel, TestTransport,
    };

    #[test]
    fn headless_render_follows_tail_and_scroll() {
        let mut model = TestRenderModel::new(10);
        for line in ["one", "two", "three", "four"] {
            model.push_history(line);
        }
        let tail = render_headless(&model, 10, 2);
        assert_eq!(tail, vec!["three".to_string(), "four".to_string()]);
        model.scroll = 2;
        let scrolled = render_headless(&model, 10, 2);
        assert_eq!(scrolled, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn headless_render_includes_streaming_tail() {
        let mut model = TestRenderModel::new(80);
        model.push_history("done");
        model.set_streaming("live…");
        let rows = render_headless(&model, 80, 5);
        assert_eq!(rows, vec!["done".to_string(), "live…".to_string()]);
    }

    #[test]
    fn anim_tick_uses_discrete_buckets() {
        let mut model = TestRenderModel::new(80);
        model.now_ms = 50;
        let first = model.anim_tick();
        model.now_ms = 99;
        assert_eq!(model.anim_tick(), first);
        model.now_ms = 100;
        assert_ne!(model.anim_tick(), first);
    }

    #[test]
    fn viewport_window_clamps_scroll() {
        assert_eq!(viewport_window(10, 4, 0), (6, 10));
        assert_eq!(viewport_window(10, 4, 2), (4, 8));
        assert_eq!(viewport_window(10, 4, 99), (0, 4));
        assert_eq!(viewport_window(2, 4, 0), (0, 2));
    }

    #[test]
    fn geometry_carries_animation_bounds() {
        use crate::redraw::AnimationArea;
        let area = AnimationArea::new(Some(Rect::new(0, 7, 1, 1)), None);
        let geometry = FrameGeometry::split(Rect::new(0, 0, 80, 24), 4, 1).with_anim_bounds(area);
        assert_eq!(geometry.anim_bounds, Some(Rect::new(0, 7, 1, 1)));
        let empty = FrameGeometry::split(Rect::new(0, 0, 80, 24), 4, 1)
            .with_anim_bounds(AnimationArea::default());
        assert_eq!(empty.anim_bounds, None);
    }

    #[test]
    fn recovered_frame_marks_the_buffer() {
        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        draw_recovered_frame(&mut buf, area);
        let text: String = buf.content.iter().map(|c| c.symbol().to_string()).collect();
        assert!(text.contains("recovered"));
    }

    #[test]
    fn event_recorder_round_trips_through_json() {
        let mut recorder = EventRecorder::new();
        recorder.push(RecordedEvent::key("enter"));
        recorder.push(RecordedEvent::delta("hi"));
        recorder.push(RecordedEvent::Tick(100));
        let mut player = EventPlayer::from_json(&recorder.to_json());
        assert_eq!(player.next_event(), Some(&RecordedEvent::key("enter")));
        assert_eq!(player.next_event(), Some(&RecordedEvent::delta("hi")));
        assert_eq!(player.next_event(), Some(&RecordedEvent::Tick(100)));
        assert!(player.exhausted());
    }

    #[test]
    fn baseline_replay_is_deterministic() {
        let mut first = TestRenderModel::new(20);
        first.push_history("alpha line one");
        first.push_history("beta line two");
        let mut second = first.clone();
        second.set_streaming("live tail");
        let mut third = second.clone();
        third.scroll = 1;
        let models = [first, second, third];
        let once = replay_baseline(&models, 20, 4);
        let twice = replay_baseline(&models, 20, 4);
        assert_eq!(once.len(), 3);
        assert_eq!(once.report(), twice.report());
        assert!(once.report().contains("frames=3"));
        assert!(once.steady_layout_within(4));
    }

    #[test]
    fn fixed_baseline_sequence_is_deterministic() {
        let seq = fixed_baseline_sequence(20);
        assert_eq!(seq.len(), 6);
        let once = replay_baseline(&seq, 20, 4);
        let twice = replay_baseline(&fixed_baseline_sequence(20), 20, 4);
        assert_eq!(once.len(), 6);
        assert_eq!(once.report(), twice.report());
        assert!(once.total_parsed_bytes() > 0);
    }

    #[test]
    fn long_history_baseline_reports_linear_growth() {
        let mut model = TestRenderModel::new(40);
        for i in 0..200 {
            model.push_history(format!("history line {i} with some words"));
        }
        let models = [model];
        let metrics = replay_baseline(&models, 40, 10);
        assert_eq!(metrics.len(), 1);
        assert!(metrics.total_parsed_bytes() > 200 * 10);
        assert!(metrics.report().contains("frames=1"));
    }

    #[test]
    fn view_carries_size_and_overlay_state() {
        let mut model = TestRenderModel::new(80);
        assert_eq!(model.height(), 24);
        assert!(!model.overlay_active());
        model.height = 30;
        model.overlay = true;
        assert_eq!(model.height(), 30);
        assert!(model.overlay_active());
    }

    #[test]
    fn evidence_bundle_summarizes_failures() {
        let mut evidence = TestEvidence::new("case");
        assert!(!evidence.failed());
        evidence.add_event(RecordedEvent::Tick(7));
        evidence.add_frame("row one");
        evidence.add_assertion("viewport mismatch");
        evidence.add_log("draw took 3ms");
        assert!(evidence.failed());
        let summary = evidence.summary();
        assert!(summary.contains("case"));
        assert!(summary.contains("events=1"));
        assert!(summary.contains("frames=1"));
    }

    #[test]
    fn evidence_seal_freezes_mutation() {
        let mut evidence = TestEvidence::new("sealed");
        evidence.add_frame("one");
        let summary = evidence.seal();
        assert!(evidence.is_sealed());
        assert!(summary.contains("sealed"));
        evidence.add_frame("two");
        assert_eq!(evidence.frames.len(), 1);
    }

    #[test]
    fn unified_probe_carries_diagram_and_sidebar() {
        let frame = Rect::new(0, 0, 100, 30);
        let probe = probe_unified_geometry(frame, 4, 1, 30, true, true, false);
        assert!(probe.diagram.is_some());
        assert!(probe.sidebar.is_some());
        assert!(probe.focused);
        assert!(!probe.overlay_active);
        assert_eq!(probe.diagram_requested, 30);
        let none = probe_unified_geometry(frame, 4, 1, 0, false, true, false);
        assert_eq!(none.diagram, None);
        assert_eq!(none.sidebar, None);
    }

    #[test]
    fn layout_probe_splits_frame_geometry() {
        let frame = Rect::new(0, 0, 80, 24);
        let probe = probe_layout(frame, 4, 1);
        assert_eq!(probe.viewport.height, 19);
        assert_eq!(probe.footer.y, 19);
        assert_eq!(probe.footer.height, 4);
        assert_eq!(probe.input.y, 23);
        assert_eq!(probe.input.height, 1);
        assert_eq!(
            probe.viewport.height + probe.footer.height + probe.input.height,
            24
        );
    }

    #[test]
    fn test_transport_records_only_real_frames() {
        let mut transport = TestTransport::new();
        assert!(!transport.submit(crate::redraw::RedrawScope::None));
        assert!(transport.submit(crate::redraw::RedrawScope::AnimationOnly));
        assert!(transport.submit(crate::redraw::RedrawScope::Full));
        assert_eq!(
            transport.submitted(),
            &[
                crate::redraw::RedrawScope::AnimationOnly,
                crate::redraw::RedrawScope::Full,
            ]
        );
    }

    #[test]
    fn document_headless_agrees_with_plain_ground_truth() {
        let mut model = TestRenderModel::new(40);
        model.push_history("hello world");
        model.push_history("second line here");
        model.set_streaming("live tail");
        assert_eq!(
            render_headless(&model, 40, 10),
            render_headless_document(&model, 40, 10)
        );
    }

    #[test]
    fn widened_view_groups_default_cleanly() {
        let model = TestRenderModel::new(80);
        assert!(!model.auto_scroll_paused());
        assert_eq!(model.input_text(), "");
        assert_eq!(model.input_cursor(), 0);
        assert!(!model.is_processing());
        assert_eq!(model.queued_count(), 0);
        assert_eq!(model.perf_marker(), "perf:full");
        assert!(!model.side_panel_visible());
        assert!(!model.diagram_visible());
        assert_eq!(model.image_signature(), 0);
    }

    #[test]
    fn recorded_events_carry_timestamps() {
        let key = RecordedEvent::Key {
            label: "enter".to_string(),
            at_ms: 12,
        };
        assert_eq!(key.at_ms(), Some(12));
        let bus = RecordedEvent::Bus {
            topic: "fetch".to_string(),
            at_ms: 34,
        };
        assert_eq!(bus.at_ms(), Some(34));
    }

    #[test]
    fn anchor_stability_scores_scrolled_frames() {
        use crate::anchor::{AnchorFrame, AnchorStabilityRecorder};
        let mut recorder = AnchorStabilityRecorder::new();
        recorder.record(AnchorFrame::from_rows(&["a".to_string(), "b".to_string()]));
        assert!(recorder.record(AnchorFrame::from_rows(&["a".to_string(), "b".to_string()])));
        assert_eq!(recorder.stability(), 1.0);
    }

    #[test]
    fn wrapped_map_supports_selection_lookup() {
        use crate::anchor::WrappedLineMap;
        let map = WrappedLineMap::build(&[2, 1]);
        assert_eq!(map.logical_for_wrapped(0), Some(0));
        assert_eq!(map.logical_for_wrapped(2), Some(1));
    }
}
