//! Fixed-content test double over the read-only render view.

use tui_style::theme_mode::ThemeMode;

use super::view::{
    InputView, LayoutView, PerfView, RenderView, ScrollView, ThemeView, TranscriptView,
};

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
