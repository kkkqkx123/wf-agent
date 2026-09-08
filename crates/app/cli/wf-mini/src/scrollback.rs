//! Scrollback buffer for the mini TUI.
//!
//! Holds the conversation history as a sequence of [`ScrollLine`] entries.
//! Each entry carries the raw source text plus a semantic role for color
//! mapping. The renderer soft-wraps lines to the current terminal width.

/// Semantic role of a scrollback line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    /// Normal assistant text.
    #[default]
    Default,
    /// Muted/dimmed secondary text.
    Muted,
    /// User prompt / accent emphasis.
    Accent,
    /// Additions (diff +, success).
    Add,
    /// Removals (diff -).
    Remove,
    /// Warnings.
    Warning,
    /// Errors.
    Error,
}

/// One line in the scrollback buffer.
#[derive(Debug, Clone)]
pub struct ScrollLine {
    pub role: Role,
    pub text: String,
}

impl ScrollLine {
    pub fn new(text: impl Into<String>, role: Role) -> Self {
        Self {
            role,
            text: text.into(),
        }
    }

    /// Soft-wrap this line to `width` columns. Always returns at least one
    /// segment even for empty content.
    pub fn wrapped(&self, width: usize) -> Vec<&str> {
        if width == 0 || self.text.is_empty() {
            return vec![""];
        }
        let mut lines = Vec::new();
        let mut start = 0;
        let bytes = self.text.as_bytes();
        let len = bytes.len();
        while start < len {
            let end = wrap_end(&self.text, start, width);
            lines.push(&self.text[start..end]);
            // Skip newline if present at the boundary.
            start = if end < len && bytes[end] == b'\n' {
                end + 1
            } else {
                end
            };
        }
        if lines.is_empty() {
            lines.push("");
        }
        lines
    }
}

/// Find the byte offset where a soft-wrap should occur for text[start..]
/// fitting within `width` display columns.
fn wrap_end(text: &str, start: usize, width: usize) -> usize {
    let slice = &text[start..];
    let mut cols = 0;
    let mut last_break = start;
    for (i, ch) in slice.char_indices() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
        if cols + w > width {
            return if last_break > start { last_break } else { start + i };
        }
        cols += w;
        let byte_idx = start + i + ch.len_utf8();
        if ch == '\n' || ch == ' ' || ch == '\t' {
            last_break = byte_idx;
        }
    }
    text.len()
}

/// Scrollback buffer with a maximum capacity.
pub struct Scrollback {
    lines: Vec<ScrollLine>,
    /// Viewport scroll offset from the tail (0 = tail-follow).
    scroll: usize,
    max_lines: usize,
}

impl Scrollback {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: Vec::new(),
            scroll: 0,
            max_lines,
        }
    }

    /// Push a new line at the tail.
    pub fn push(&mut self, line: ScrollLine) {
        self.lines.push(line);
        if self.lines.len() > self.max_lines {
            let drop = self.lines.len() - self.max_lines;
            self.lines.drain(0..drop);
        }
    }

    /// Append multiple lines.
    pub fn extend(&mut self, iter: impl IntoIterator<Item = ScrollLine>) {
        for line in iter {
            self.push(line);
        }
    }

    /// Clear all lines and reset scroll.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll = 0;
    }

    /// Scroll up (older content).
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_add(amount);
    }

    /// Scroll down (toward tail).
    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    /// Jump to the tail (tail-follow).
    pub fn scroll_to_tail(&mut self) {
        self.scroll = 0;
    }

    /// Whether the viewport is at the tail.
    pub fn is_at_tail(&self) -> bool {
        self.scroll == 0
    }

    /// All lines in the buffer (for rendering).
    pub fn lines(&self) -> &[ScrollLine] {
        &self.lines
    }

    /// Number of lines.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Compute the visible line range for a viewport of `height` rows and
    /// `width` columns. Returns `(visible_lines, scroll_info)` where each
    /// visible line is a `(role, text)` pair (already soft-wrapped).
    pub fn visible(
        &self,
        height: usize,
        width: usize,
    ) -> Vec<(Role, String)> {
        // First, expand all lines into wrapped display lines.
        let mut display: Vec<(Role, &str)> = Vec::new();
        for line in &self.lines {
            for wrapped in line.wrapped(width) {
                display.push((line.role, wrapped));
            }
        }

        let total = display.len();
        if total == 0 {
            return Vec::new();
        }

        let max_scroll = total.saturating_sub(height);
        let scroll = self.scroll.min(max_scroll);
        let start = total - scroll - height.min(total);
        let end = (start + height).min(total);

        display[start..end]
            .iter()
            .map(|(role, text)| (*role, text.to_string()))
            .collect()
    }
}

impl Default for Scrollback {
    fn default() -> Self {
        Self::new(10_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollline_wraps_long_text() {
        let line = ScrollLine::new("hello world", Role::Default);
        let wrapped = line.wrapped(6);
        assert_eq!(wrapped, vec!["hello ", "world"]);
    }

    #[test]
    fn scrollback_tail_follow() {
        let mut sb = Scrollback::new(100);
        for i in 0..20 {
            sb.push(ScrollLine::new(format!("line {i}"), Role::Default));
        }
        assert!(sb.is_at_tail());
        let vis = sb.visible(5, 80);
        assert_eq!(vis.len(), 5);
        assert_eq!(vis[4].1, "line 19");
    }

    #[test]
    fn scrollback_scroll_up() {
        let mut sb = Scrollback::new(100);
        for i in 0..20 {
            sb.push(ScrollLine::new(format!("line {i}"), Role::Default));
        }
        sb.scroll_up(5);
        let vis = sb.visible(5, 80);
        assert_eq!(vis[0].1, "line 10");
        assert_eq!(vis[4].1, "line 14");
    }
}
