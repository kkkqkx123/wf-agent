//! Visual debug capture plus screenshot and record export hooks.
//!
//! Rect captures are serializable geometry snapshots of one frame: every
//! widget placement the debug overlay may outline. Screenshot and record
//! exports write JSON files under an explicit directory so headless tests
//! exercise the same path without a terminal.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Maximum frame captures retained in memory.
pub const VISUAL_CAPTURE_MAX_FRAMES: usize = 32;

/// Serializable cell rectangle for one captured widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RectCapture {
    /// Left column.
    pub x: u16,
    /// Top row.
    pub y: u16,
    /// Width in columns.
    pub w: u16,
    /// Height in rows.
    pub h: u16,
}

impl RectCapture {
    /// Build a capture from coordinates.
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }

    /// True when the rectangle covers no area.
    pub fn is_empty(self) -> bool {
        self.w == 0 || self.h == 0
    }
}

/// Layout capture: viewport plus footer and input rows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LayoutCapture {
    /// Visible viewport rectangle.
    pub viewport: RectCapture,
    /// Footer rectangle.
    pub footer: RectCapture,
    /// Input rectangle.
    pub input: RectCapture,
}

/// One widget placement: what was drawn where.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WidgetPlacementCapture {
    /// Widget kind label (`transcript`, `footer`, `modal`).
    pub kind: String,
    /// Placement side (`main`, `sidebar`, `overlay`).
    pub side: String,
    /// Placed rectangle.
    pub rect: RectCapture,
}

/// Render timing capture for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RenderTimingCapture {
    /// Preparation time in milliseconds.
    pub prepare_ms: u64,
    /// Draw time in milliseconds.
    pub draw_ms: u64,
    /// Total frame time in milliseconds.
    pub total_ms: u64,
}

/// Image region capture for one inline-image placeholder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImageRegionCapture {
    /// Placeholder rectangle.
    pub rect: RectCapture,
    /// Content hash of the diagram source.
    pub content_hash: u64,
}

/// Message capture for one transcript row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MessageCapture {
    /// Row index in display order.
    pub row: usize,
    /// Plain-text content of the row.
    pub text: String,
}

/// One captured frame: geometry plus timing plus text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FrameCapture {
    /// Frame sequence number.
    pub frame: u64,
    /// Layout rectangles for the frame.
    pub layout: LayoutCapture,
    /// Widget placements for the frame.
    pub placements: Vec<WidgetPlacementCapture>,
    /// Render timing for the frame.
    pub timing: RenderTimingCapture,
    /// Image placeholder regions for the frame.
    pub images: Vec<ImageRegionCapture>,
    /// Transcript messages for the frame.
    pub messages: Vec<MessageCapture>,
    /// Rendered plain text rows for the frame.
    pub text: Vec<String>,
    /// Anomaly label, when the builder flagged this frame.
    pub anomaly: Option<String>,
}

impl FrameCapture {
    /// Normalized form for comparisons: anomaly labels never affect equality.
    pub fn normalized(&self) -> Self {
        let mut copy = self.clone();
        copy.anomaly = None;
        copy
    }
}

/// Builder for one frame capture with anomaly checks.
#[derive(Debug, Clone, Default)]
pub struct FrameCaptureBuilder {
    frame: u64,
    layout: Option<LayoutCapture>,
    placements: Vec<WidgetPlacementCapture>,
    timing: Option<RenderTimingCapture>,
    images: Vec<ImageRegionCapture>,
    messages: Vec<MessageCapture>,
    text: Vec<String>,
    anomaly: Option<String>,
}

impl FrameCaptureBuilder {
    /// Empty builder for `frame`.
    pub fn new(frame: u64) -> Self {
        Self {
            frame,
            ..Self::default()
        }
    }

    /// Record the layout rectangles.
    pub fn layout(&mut self, layout: LayoutCapture) -> &mut Self {
        self.layout = Some(layout);
        self
    }

    /// Record one widget placement.
    pub fn placement(&mut self, placement: WidgetPlacementCapture) -> &mut Self {
        self.placements.push(placement);
        self
    }

    /// Record the render timing.
    pub fn timing(&mut self, timing: RenderTimingCapture) -> &mut Self {
        self.timing = Some(timing);
        self
    }

    /// Record one image placeholder region.
    pub fn image(&mut self, region: ImageRegionCapture) -> &mut Self {
        self.images.push(region);
        self
    }

    /// Record one transcript message.
    pub fn message(&mut self, message: MessageCapture) -> &mut Self {
        self.messages.push(message);
        self
    }

    /// Record the rendered plain-text rows.
    pub fn text(&mut self, rows: Vec<String>) -> &mut Self {
        self.text = rows;
        self
    }

    /// Flag an anomaly label without failing the build.
    pub fn anomaly(&mut self, label: &str) -> &mut Self {
        self.anomaly = Some(label.to_string());
        self
    }

    /// Check an invariant, flagging the anomaly label when it fails.
    pub fn check(&mut self, ok: bool, label: &str) -> &mut Self {
        if !ok {
            self.anomaly(label);
        }
        self
    }

    /// Build the capture, filling missing layout and timing with zeros.
    pub fn build(&self) -> FrameCapture {
        FrameCapture {
            frame: self.frame,
            layout: self.layout.clone().unwrap_or(LayoutCapture {
                viewport: RectCapture::new(0, 0, 0, 0),
                footer: RectCapture::new(0, 0, 0, 0),
                input: RectCapture::new(0, 0, 0, 0),
            }),
            placements: self.placements.clone(),
            timing: self.timing.unwrap_or(RenderTimingCapture {
                prepare_ms: 0,
                draw_ms: 0,
                total_ms: 0,
            }),
            images: self.images.clone(),
            messages: self.messages.clone(),
            text: self.text.clone(),
            anomaly: self.anomaly.clone(),
        }
    }
}

/// In-memory visual debug recorder with file export.
#[derive(Debug, Clone, Default)]
pub struct VisualDebugCapture {
    enabled: bool,
    overlay: bool,
    frames: VecDeque<FrameCapture>,
}

impl VisualDebugCapture {
    /// Disabled recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable recording.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable recording and drop retained frames.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.frames.clear();
    }

    /// True when recording.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable the on-screen rectangle overlay.
    pub fn set_overlay(&mut self, overlay: bool) {
        self.overlay = overlay;
    }

    /// True when the overlay should draw.
    pub fn overlay(&self) -> bool {
        self.overlay
    }

    /// Record one frame when enabled.
    pub fn record_frame(&mut self, frame: FrameCapture) {
        if !self.enabled {
            return;
        }
        self.frames.push_back(frame);
        while self.frames.len() > VISUAL_CAPTURE_MAX_FRAMES {
            self.frames.pop_front();
        }
    }

    /// Latest retained frame, if any.
    pub fn latest_frame(&self) -> Option<&FrameCapture> {
        self.frames.back()
    }

    /// Latest frame serialized to JSON, if any.
    pub fn latest_frame_json(&self) -> Option<String> {
        self.latest_frame()
            .and_then(|frame| serde_json::to_string(frame).ok())
    }

    /// Latest frame normalized (anomaly cleared) serialized to JSON, if any.
    pub fn latest_frame_json_normalized(&self) -> Option<String> {
        self.latest_frame()
            .and_then(|frame| serde_json::to_string(&frame.normalized()).ok())
    }

    /// Retained frame count.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// True when no frame was retained.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Dump the latest frame JSON to `dir`/`name`, creating directories.
    /// Returns the written path on success. Headless-safe: no terminal needed.
    pub fn dump_to_file(&self, dir: &Path, name: &str) -> Option<PathBuf> {
        let json = self.latest_frame_json()?;
        std::fs::create_dir_all(dir).ok()?;
        let path = dir.join(name);
        std::fs::write(&path, json).ok()?;
        Some(path)
    }
}

/// Normalized frame equality ignoring anomaly labels.
pub fn frames_equal_normalized(left: &FrameCapture, right: &FrameCapture) -> bool {
    left.normalized() == right.normalized()
}

/// Screenshot export state: writes ready JSON files under a runtime directory.
/// Headless-safe: only file writes, never terminal queries.
#[derive(Debug, Clone, Default)]
pub struct ScreenshotState {
    enabled: bool,
    runtime_dir: Option<PathBuf>,
    last_ready: Option<String>,
}

impl ScreenshotState {
    /// Disabled screenshot state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable exports under `dir`.
    pub fn enable(&mut self, dir: impl Into<PathBuf>) {
        self.enabled = true;
        self.runtime_dir = Some(dir.into());
    }

    /// Disable exports and clear the last payload.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.last_ready = None;
    }

    /// True when exports are enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Signal a ready screenshot with `state_json`: writes
    /// `runtime_dir/jcode-screenshots/ready.json` when enabled. Returns the
    /// written path on success; disabled state drops the payload.
    pub fn signal_ready(&mut self, state_json: &str) -> Option<PathBuf> {
        if !self.enabled {
            return None;
        }
        let dir = self.runtime_dir.clone().unwrap_or_else(std::env::temp_dir);
        let target = dir.join("jcode-screenshots");
        std::fs::create_dir_all(&target).ok()?;
        let path = target.join("ready.json");
        std::fs::write(&path, state_json).ok()?;
        self.last_ready = Some(state_json.to_string());
        Some(path)
    }

    /// Clear the last payload without disabling.
    pub fn clear(&mut self) {
        self.last_ready = None;
    }

    /// Last signaled payload, if any.
    pub fn last_ready(&self) -> Option<&str> {
        self.last_ready.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame(frame: u64) -> FrameCapture {
        FrameCaptureBuilder::new(frame)
            .layout(LayoutCapture {
                viewport: RectCapture::new(0, 0, 80, 19),
                footer: RectCapture::new(0, 19, 80, 4),
                input: RectCapture::new(0, 23, 80, 1),
            })
            .timing(RenderTimingCapture {
                prepare_ms: 2,
                draw_ms: 3,
                total_ms: 5,
            })
            .text(vec!["hello".to_string()])
            .build()
    }

    #[test]
    fn builder_fills_missing_sections_with_zeros() {
        let frame = FrameCaptureBuilder::new(1).build();
        assert_eq!(frame.frame, 1);
        assert!(frame.placements.is_empty());
        assert_eq!(frame.timing.total_ms, 0);
    }

    #[test]
    fn anomaly_does_not_affect_normalized_equality() {
        let mut left = sample_frame(2);
        let mut right = sample_frame(2);
        left.anomaly = Some("flash".to_string());
        right.anomaly = Some("other".to_string());
        assert_ne!(left, right);
        assert!(frames_equal_normalized(&left, &right));
        assert_eq!(left.normalized().anomaly, None);
    }

    #[test]
    fn capture_ring_records_and_exports_json_headlessly() {
        let mut capture = VisualDebugCapture::new();
        assert!(!capture.is_enabled());
        capture.record_frame(sample_frame(1));
        assert!(capture.is_empty());
        capture.enable();
        capture.set_overlay(true);
        assert!(capture.overlay());
        capture.record_frame(sample_frame(1));
        capture.record_frame(sample_frame(2));
        assert_eq!(capture.len(), 2);
        let json = capture.latest_frame_json().expect("latest serializes");
        assert!(json.contains("\"frame\":2"));
        let normalized = capture
            .latest_frame_json_normalized()
            .expect("normalized serializes");
        assert!(normalized.contains("\"frame\":2"));
        let dir = std::env::temp_dir().join("wf-visual-capture-test");
        let path = capture
            .dump_to_file(&dir, "frame.json")
            .expect("dump headlessly");
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
        capture.disable();
        assert!(capture.is_empty());
    }

    #[test]
    fn rect_capture_reports_empty_area() {
        assert!(RectCapture::new(0, 0, 0, 5).is_empty());
        assert!(!RectCapture::new(0, 0, 4, 1).is_empty());
    }

    #[test]
    fn screenshot_state_writes_ready_json_headlessly() {
        let mut state = ScreenshotState::new();
        assert!(!state.is_enabled());
        assert_eq!(state.signal_ready("{}"), None);
        let dir = std::env::temp_dir().join("wf-screenshot-test");
        state.enable(dir.clone());
        let path = state.signal_ready("{\"frame\":1}").expect("writes file");
        assert!(path.exists());
        assert_eq!(state.last_ready(), Some("{\"frame\":1}"));
        state.clear();
        assert_eq!(state.last_ready(), None);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(dir.join("jcode-screenshots"));
        state.disable();
        assert!(!state.is_enabled());
    }
}
