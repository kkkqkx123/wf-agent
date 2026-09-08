//! History cell trait and implementations for the TUI conversation display.
//!
//! [`HistoryCell`] is the fundamental unit of conversation display. Each cell
//! knows how to render itself at a given width, report its height, and provide
//! animation signals for streaming content.
//!
//! Implementations include:
//! - [`PlainCell`] — Simple pre-built lines
//! - [`UserMessageCell`] — User messages with background styling
//! - [`AssistantMessageCell`] — Assistant messages with Markdown rendering
//! - [`ToolCallCell`] — Tool execution with status indicators
//! - [`ErrorCell`] — Error messages
//! - [`StatusCell`] — System status messages

use std::any::Any;
use std::fmt::Debug;

use ratatui::text::Line;

use crate::theme;

/// Trait for conversation display units.
///
/// Each cell type knows how to render itself at a given width, report its
/// height, and provide animation signals for streaming content.
pub trait HistoryCell: Debug + Send + Sync + Any {
    /// Render the cell to display lines at the given width.
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;

    /// Number of viewport rows this cell occupies at `width`.
    fn desired_height(&self, width: u16) -> u16 {
        u16::try_from(self.display_lines(width).len()).unwrap_or(u16::MAX)
    }

    /// Animation tick for streaming content. Returns `None` if the cell
    /// is not animated, or `Some(tick)` where `tick` changes each time
    /// the animation frame should be redrawn.
    fn animation_tick(&self) -> Option<u64> {
        None
    }

    /// Get the source text for clipboard/copy operations.
    fn raw_text(&self, width: u16) -> String {
        self.display_lines(width)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Convert to Any for downcasting.
    fn as_any(&self) -> &dyn Any;
}

/// Simple pre-built lines cell.
#[derive(Debug, Clone)]
pub struct PlainCell {
    lines: Vec<Line<'static>>,
}

impl PlainCell {
    pub fn new(lines: Vec<Line<'static>>) -> Self {
        Self { lines }
    }
}

impl HistoryCell for PlainCell {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        self.lines.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// User message cell with background styling.
#[derive(Debug, Clone)]
pub struct UserMessageCell {
    text: String,
}

impl UserMessageCell {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl HistoryCell for UserMessageCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let w = usize::from(width.max(1));
        let mut out = Vec::new();

        // Apply user message styling (subtle background)
        let style = theme::user_message_style(theme::Rgb::new(0x0F, 0x14, 0x1A)); // dark bg default

        for line in self.text.lines() {
            if line.width() <= w {
                let spans: Vec<ratatui::text::Span<'static>> = vec![
                    ratatui::text::Span::styled(line.to_string(), style),
                ];
                out.push(Line::from(spans));
            } else {
                out.extend(word_wrap_with_style(line, w, style));
            }
        }

        if out.is_empty() {
            let spans: Vec<ratatui::text::Span<'static>> = vec![
                ratatui::text::Span::styled(String::new(), style),
            ];
            out.push(Line::from(spans));
        }

        out
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Assistant message cell with plain text (Markdown rendering can be added later).
#[derive(Debug, Clone)]
pub struct AssistantMessageCell {
    text: String,
}

impl AssistantMessageCell {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl HistoryCell for AssistantMessageCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let w = usize::from(width.max(1));
        let mut out = Vec::new();

        for line in self.text.lines() {
            if line.width() <= w {
                out.push(Line::raw(line.to_string()));
            } else {
                out.extend(word_wrap_plain(line, w));
            }
        }

        if out.is_empty() {
            out.push(Line::raw(""));
        }

        out
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Tool call cell with status indicators.
#[derive(Debug, Clone)]
pub struct ToolCallCell {
    tool_name: String,
    args: Option<String>,
    result: Option<String>,
    success: Option<bool>,
    duration_ms: Option<u64>,
}

impl ToolCallCell {
    pub fn new(tool_name: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            args: None,
            result: None,
            success: None,
            duration_ms: None,
        }
    }

    pub fn with_args(mut self, args: impl Into<String>) -> Self {
        self.args = Some(args.into());
        self
    }

    pub fn with_result(mut self, result: impl Into<String>, success: bool) -> Self {
        self.result = Some(result.into());
        self.success = Some(success);
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }
}

impl HistoryCell for ToolCallCell {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Tool name with indicator
        let indicator_style = match self.success {
            Some(true) => theme::success_style(),
            Some(false) => theme::error_style(),
            None => theme::tool_call_style(), // in progress
        };

        let indicator = match self.success {
            Some(true) => "\u{2713}", // checkmark
            Some(false) => "\u{2717}", // cross
            None => "\u{25B2}",        // triangle up (in progress)
        };

        let duration_str = self
            .duration_ms
            .map(|ms| format!(" ({}ms)", ms))
            .unwrap_or_default();

        lines.push(Line::styled(
            format!("{} {} {}", indicator, self.tool_name, duration_str),
            indicator_style,
        ));

        // Arguments (if present)
        if let Some(args) = &self.args {
            let truncated = if args.len() > 200 {
                format!("{}...", &args[..200])
            } else {
                args.clone()
            };
            lines.push(Line::styled(format!("  {}", truncated), theme::muted_style()));
        }

        // Result (if present)
        if let Some(result) = &self.result {
            let truncated = if result.len() > 500 {
                format!("{}...", &result[..500])
            } else {
                result.clone()
            };
            let result_style = match self.success {
                Some(true) => theme::success_style(),
                Some(false) => theme::error_style(),
                None => theme::muted_style(),
            };
            lines.push(Line::styled(format!("  {}", truncated), result_style));
        }

        lines
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Error cell with error styling.
#[derive(Debug, Clone)]
pub struct ErrorCell {
    message: String,
}

impl ErrorCell {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl HistoryCell for ErrorCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let w = usize::from(width.max(1));
        let style = theme::error_style();

        let mut out = Vec::new();
        for line in self.message.lines() {
            if line.width() <= w {
                out.push(Line::styled(line.to_string(), style));
            } else {
                out.extend(word_wrap_with_style(line, w, style));
            }
        }

        if out.is_empty() {
            out.push(Line::styled("", style));
        }

        out
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Status cell for system messages.
#[derive(Debug, Clone)]
pub struct StatusCell {
    message: String,
}

impl StatusCell {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl HistoryCell for StatusCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let w = usize::from(width.max(1));
        let style = theme::muted_style();

        let mut out = Vec::new();
        for line in self.message.lines() {
            if line.width() <= w {
                out.push(Line::styled(line.to_string(), style));
            } else {
                out.extend(word_wrap_with_style(line, w, style));
            }
        }

        if out.is_empty() {
            out.push(Line::styled("", style));
        }

        out
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ── Helper functions ──────────────────────────────────────────────────

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Word wrap with styling.
fn word_wrap_with_style(
    text: &str,
    width: usize,
    style: ratatui::style::Style,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    let mut cur_w = 0usize;

    for grapheme in text.graphemes(true) {
        let gw = grapheme.width();
        if !cur.is_empty() && cur_w + gw > width {
            out.push(Line::from(cur.clone()));
            cur.clear();
            cur_w = 0;
        }
        cur.push(ratatui::text::Span::styled(grapheme.to_string(), style));
        cur_w += gw;
    }

    if !cur.is_empty() {
        out.push(Line::from(cur));
    }

    if out.is_empty() {
        out.push(Line::styled("", style));
    }

    out
}

/// Word wrap without styling (plain text).
fn word_wrap_plain(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    let mut cur_w = 0usize;

    for grapheme in text.graphemes(true) {
        let gw = grapheme.width();
        if !cur.is_empty() && cur_w + gw > width {
            out.push(Line::from(cur.clone()));
            cur.clear();
            cur_w = 0;
        }
        cur.push(ratatui::text::Span::raw(grapheme.to_string()));
        cur_w += gw;
    }

    if !cur.is_empty() {
        out.push(Line::from(cur));
    }

    if out.is_empty() {
        out.push(Line::raw(""));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_cell_renders_lines() {
        let cell = PlainCell::new(vec![
            Line::raw("line 1"),
            Line::raw("line 2"),
        ]);
        let lines = cell.display_lines(80);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn user_message_cell_applies_style() {
        let cell = UserMessageCell::new("Hello, world!");
        let lines = cell.display_lines(80);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans[0].style.bg.is_some());
    }

    #[test]
    fn tool_call_cell_shows_indicator() {
        let cell = ToolCallCell::new("bash")
            .with_result("success", true)
            .with_duration(100);
        let lines = cell.display_lines(80);
        assert!(lines[0].spans[0].content.contains("\u{2713}"));
    }

    #[test]
    fn error_cell_uses_error_style() {
        let cell = ErrorCell::new("Something went wrong");
        let lines = cell.display_lines(80);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn wrap_respects_width() {
        let cell = UserMessageCell::new("The quick brown fox jumps over the lazy dog");
        let lines = cell.display_lines(20);
        assert!(lines.len() > 1, "long text should wrap");
    }

    #[test]
    fn empty_text_yields_one_line() {
        let cell = UserMessageCell::new("");
        assert_eq!(cell.desired_height(80), 1);
    }
}
