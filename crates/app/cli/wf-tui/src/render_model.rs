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

use crate::theme_mode::ThemeMode;

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
    /// Viewport scroll offset in display rows (0 is tail-follow).
    fn view_scroll(&self) -> usize {
        0
    }
    /// Active theme mode.
    fn theme_mode(&self) -> ThemeMode {
        ThemeMode::Dark
    }
    /// Discretized animation tick (see [`crate::clock::anim_bucket`]).
    fn anim_tick(&self) -> u64 {
        0
    }
    /// Digest of the footer state.
    fn footer_digest(&self) -> u64 {
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
    /// Viewport scroll offset in display rows.
    pub scroll: usize,
    /// Active theme mode.
    pub mode: ThemeMode,
    /// Clock value (ms) driving the animation tick.
    pub now_ms: u64,
    /// Footer digest.
    pub footer: u64,
}

impl TestRenderModel {
    /// Empty model at `width` columns.
    pub fn new(width: u16) -> Self {
        Self {
            width,
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
    fn view_scroll(&self) -> usize {
        self.scroll
    }
    fn theme_mode(&self) -> ThemeMode {
        self.mode
    }
    fn anim_tick(&self) -> u64 {
        crate::clock::anim_bucket(self.now_ms)
    }
    fn footer_digest(&self) -> u64 {
        self.footer
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
}

/// Render the panic fallback into `buf`: a recognizable frame proving the
/// loop survived a failing widget.
pub fn draw_recovered_frame(buf: &mut Buffer, area: Rect) {
    let msg = "render recovered after panic";
    buf.set_string(area.x, area.y, msg, Style::default());
}

/// Recorded interaction event placeholder for session replay. Full capture
/// (keys, resizes, bus events with timestamps) arrives after the view and
/// clock land; the shape is fixed now so producers can start emitting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecordedEvent {
    /// Logical key press identified by its debug label.
    Key(String),
    /// Terminal resize to columns × rows.
    Resize(u16, u16),
    /// Streamed text delta arrival.
    Delta(String),
    /// Frame tick at the given clock value.
    Tick(u64),
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

/// Wrap one plain-text line to `width` columns on character boundaries.
fn wrap_plain(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    for source in text.split('\n') {
        let mut current = String::new();
        let mut current_width = 0usize;
        for ch in source.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(ch)
                .unwrap_or(1)
                .max(1);
            if current_width + cw > width && !current.is_empty() {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
            }
            current.push(ch);
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
    use super::*;

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
        recorder.push(RecordedEvent::Key("enter".to_string()));
        recorder.push(RecordedEvent::Delta("hi".to_string()));
        recorder.push(RecordedEvent::Tick(100));
        let mut player = EventPlayer::from_json(&recorder.to_json());
        assert_eq!(
            player.next_event(),
            Some(&RecordedEvent::Key("enter".to_string()))
        );
        assert_eq!(
            player.next_event(),
            Some(&RecordedEvent::Delta("hi".to_string()))
        );
        assert_eq!(player.next_event(), Some(&RecordedEvent::Tick(100)));
        assert!(player.exhausted());
    }
}
