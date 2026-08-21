//! Single-line prompt composer.
//!
//! [`Composer`] is the mini footer's text entry: a one-line buffer with a
//! grapheme-aware cursor, horizontal scrolling so the cursor stays visible,
//! a bounded history (100 entries) with a stash slot for in-progress edits
//! (↑ stashes the draft, ↓ past the newest entry restores it), a placeholder
//! for the empty state and character-level editing (insert / backspace /
//! delete / home / end / left / right).
//!
//! The composer owns no terminal state: `render` draws into a caller-provided
//! ratatui [`Buffer`] with a caller-provided style.

use std::collections::VecDeque;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Bound of the prompt history ring.
pub const HISTORY_LIMIT: usize = 100;

/// Placeholder shown while the buffer is empty.
pub const PLACEHOLDER: &str = "> Type a message…";

/// A one-line prompt buffer.
#[derive(Debug, Clone, Default)]
pub struct Composer {
    buf: String,
    /// Byte offset of the cursor into `buf`; always on a grapheme boundary.
    cursor: usize,
    /// Horizontal scroll offset in columns (kept so the cursor is visible).
    scroll_x: u16,
    history: VecDeque<String>,
    /// Draft saved while navigating history (↑), restored at the end (↓).
    stash: Option<String>,
    placeholder: &'static str,
}

impl Composer {
    /// New empty composer with the default placeholder.
    pub fn new() -> Self {
        Self {
            placeholder: PLACEHOLDER,
            ..Self::default()
        }
    }

    /// The current buffer content.
    pub fn content(&self) -> &str {
        &self.buf
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Replace the whole buffer (cursor moves to the end).
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buf = text.into();
        self.cursor = self.buf.len();
        self.scroll_x = 0;
    }

    /// Insert a single character at the cursor.
    pub fn insert_char(&mut self, c: char) {
        self.buf.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Insert a string at the cursor (bracketed paste / mention insert).
    pub fn insert_text(&mut self, text: &str) {
        self.buf.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    /// Delete the grapheme before the cursor.
    pub fn backspace(&mut self) {
        if let Some((len, _)) = self.grapheme_before() {
            self.buf.drain(self.cursor - len..self.cursor);
            self.cursor -= len;
        }
    }

    /// Delete the grapheme at the cursor.
    pub fn delete_forward(&mut self) {
        if let Some((len, _)) = self.grapheme_after() {
            self.buf.drain(self.cursor..self.cursor + len);
        }
    }

    /// Move one grapheme left.
    pub fn move_left(&mut self) {
        if let Some((len, _)) = self.grapheme_before() {
            self.cursor -= len;
        }
    }

    /// Move one grapheme right.
    pub fn move_right(&mut self) {
        if let Some((len, _)) = self.grapheme_after() {
            self.cursor += len;
        }
    }

    /// Move to the start of the buffer.
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// Move to the end of the buffer.
    pub fn end(&mut self) {
        self.cursor = self.buf.len();
    }

    /// Number of history entries retained.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Navigate to the previous history entry; the current draft is stashed
    /// on the first step and restored after walking past the oldest entry
    /// (ring semantics: stash ↔ newest ↔ … ↔ oldest ↔ stash).
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.stash.is_none() {
            self.stash = Some(std::mem::take(&mut self.buf));
            self.cursor = 0;
            let newest = self.history.back().expect("non-empty history");
            self.set_text(newest.clone());
            return;
        }
        let cur = self.buf.as_str();
        if self.stash.as_deref() == Some(cur) {
            // One step past the stash goes to the newest entry.
            let newest = self.history.back().expect("non-empty history");
            self.set_text(newest.clone());
            return;
        }
        match self.history.iter().rposition(|h| h == cur) {
            Some(i) if i > 0 => {
                let target = self.history[i - 1].clone();
                self.set_text(target);
            }
            // Oldest entry: wrap back to the stash.
            _ => {
                if let Some(stash) = self.stash.clone() {
                    self.set_text(stash);
                }
            }
        }
    }

    /// Navigate to the next history entry (restores the stash past the
    /// newest entry; ring semantics as [`Composer::history_prev`]).
    pub fn history_next(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.stash.is_none() {
            self.stash = Some(self.buf.clone());
            let oldest = self.history.front().expect("non-empty history");
            self.set_text(oldest.clone());
            return;
        }
        let cur = self.buf.as_str();
        if self.stash.as_deref() == Some(cur) {
            // One step past the stash goes to the oldest entry.
            let oldest = self.history.front().expect("non-empty history");
            self.set_text(oldest.clone());
            return;
        }
        match self.history.iter().position(|h| h == cur) {
            Some(i) if i + 1 < self.history.len() => {
                let target = self.history[i + 1].clone();
                self.set_text(target);
            }
            // Newest entry: wrap back to the stash.
            _ => {
                if let Some(stash) = self.stash.clone() {
                    self.set_text(stash);
                }
            }
        }
    }

    /// Commit the buffer: record it in the history (dedup against the newest
    /// entry, empty inputs are skipped), clear the buffer and return the
    /// submitted text.
    pub fn submit(&mut self) -> Option<String> {
        let text = self.buf.trim().to_string();
        if !text.is_empty() && self.history.back().map(String::as_str) != Some(text.as_str()) {
            if self.history.len() == HISTORY_LIMIT {
                self.history.pop_front();
            }
            self.history.push_back(text.clone());
        }
        self.clear();
        (!text.is_empty()).then_some(text)
    }

    /// Clear the buffer and reset the cursor / scroll.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
        self.scroll_x = 0;
        self.stash = None;
    }

    /// Column (0-based) at which the cursor is drawn for the given area
    /// width, after re-anchoring the horizontal scroll.
    pub fn cursor_col(&mut self, width: u16) -> u16 {
        self.update_scroll(width);
        let before = self.buf[..self.cursor].width() as u16;
        before.saturating_sub(self.scroll_x)
    }

    /// Render the single line into `area`, applying the horizontal scroll.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer, style: Style) {
        let width = area.width.max(1);
        buf.set_string(area.x, area.y, &" ".repeat(width.into()), Style::default());
        if self.buf.is_empty() {
            let ph = truncate_graphemes(self.placeholder, width);
            buf.set_string(
                area.x,
                area.y,
                &ph,
                style.add_modifier(Modifier::DIM),
            );
            return;
        }
        self.update_scroll(width);
        let mut col = 0u16;
        let mut skip = self.scroll_x;
        for grapheme in self.buf.graphemes(true) {
            let w = grapheme.width() as u16;
            if skip > 0 {
                skip = skip.saturating_sub(w);
                continue;
            }
            if col + w > width {
                break;
            }
            buf.set_string(area.x + col, area.y, grapheme, style);
            col += w;
        }
    }

    /// Grapheme immediately before the cursor: `(byte_len, display_width)`.
    fn grapheme_before(&self) -> Option<(usize, usize)> {
        self.buf[..self.cursor]
            .graphemes(true)
            .next_back()
            .map(|g| (g.len(), g.width()))
    }

    /// Grapheme immediately after the cursor: `(byte_len, display_width)`.
    fn grapheme_after(&self) -> Option<(usize, usize)> {
        self.buf[self.cursor..]
            .graphemes(true)
            .next()
            .map(|g| (g.len(), g.width()))
    }

    /// Keep the cursor visible: adjust `scroll_x` so the cursor column falls
    /// inside `[scroll_x, scroll_x + width)`.
    fn update_scroll(&mut self, width: u16) {
        if width == 0 {
            return;
        }
        let cursor_col = self.buf[..self.cursor].width() as u16;
        let right_edge = self.scroll_x.saturating_add(width);
        if cursor_col >= right_edge {
            self.scroll_x = cursor_col.saturating_add(1).saturating_sub(width);
        } else if cursor_col < self.scroll_x {
            self.scroll_x = cursor_col;
        }
    }
}

/// Truncate `text` to at most `width` columns on grapheme boundaries.
fn truncate_graphemes(text: &str, width: u16) -> String {
    let mut out = String::new();
    let mut w = 0u16;
    for g in text.graphemes(true) {
        let gw = g.width() as u16;
        if w + gw > width {
            break;
        }
        out.push_str(g);
        w += gw;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_characters_at_the_cursor() {
        let mut c = Composer::new();
        c.insert_char('h');
        c.insert_char('i');
        c.move_left();
        c.insert_char('e');
        assert_eq!(c.content(), "hei");
        assert_eq!(c.cursor, 2);
    }

    #[test]
    fn backspace_removes_the_previous_grapheme() {
        let mut c = Composer::new();
        c.set_text("ab");
        c.end();
        c.backspace();
        assert_eq!(c.content(), "a");
        c.home();
        c.backspace();
        assert_eq!(c.content(), "a");
    }

    #[test]
    fn delete_forward_removes_at_the_cursor() {
        let mut c = Composer::new();
        c.set_text("abc");
        c.home();
        c.delete_forward();
        assert_eq!(c.content(), "bc");
    }

    #[test]
    fn movement_is_grapheme_aware_for_wide_glyphs() {
        let mut c = Composer::new();
        c.set_text("你好");
        // "你" is two columns wide; a single left step lands on it whole.
        c.end();
        assert_eq!(c.cursor, "你好".len());
        c.move_left();
        assert_eq!(c.cursor, "你".len());
        c.move_left();
        assert_eq!(c.cursor, 0);
        c.move_left(); // clamped at the start
        assert_eq!(c.cursor, 0);
    }

    #[test]
    fn home_and_end_jump_to_the_edges() {
        let mut c = Composer::new();
        c.set_text("hello world");
        c.home();
        assert_eq!(c.cursor, 0);
        c.end();
        assert_eq!(c.cursor, "hello world".len());
    }

    #[test]
    fn history_walks_older_entries_and_restores_the_stash() {
        let mut c = Composer::new();
        c.set_text("first");
        assert_eq!(c.submit(), Some("first".to_string()));
        c.set_text("second");
        assert_eq!(c.submit(), Some("second".to_string()));
        assert_eq!(c.history_len(), 2);

        // Fresh draft; ↑ stashes it and shows the newest entry.
        c.set_text("draft");
        c.history_prev();
        assert_eq!(c.content(), "second");
        c.history_prev();
        assert_eq!(c.content(), "first");
        // Walking past the front wraps back to the stash.
        c.history_prev();
        assert_eq!(c.content(), "draft");
        // ↓ moves forward again.
        c.history_next();
        assert_eq!(c.content(), "first");
        c.history_next();
        assert_eq!(c.content(), "second");
    }

    #[test]
    fn submit_records_history_dedups_and_clears() {
        let mut c = Composer::new();
        assert_eq!(c.submit(), None);
        c.set_text("hello");
        assert_eq!(c.submit(), Some("hello".to_string()));
        c.set_text("hello");
        assert_eq!(c.submit(), Some("hello".to_string()), "duplicate skipped");
        assert_eq!(c.history_len(), 1);
        assert!(c.is_empty());
    }

    #[test]
    fn history_is_bounded() {
        let mut c = Composer::new();
        for i in 0..HISTORY_LIMIT + 10 {
            c.set_text(format!("msg-{i}"));
            c.submit();
        }
        assert_eq!(c.history_len(), HISTORY_LIMIT);
        assert!(c.content().is_empty());
    }

    #[test]
    fn cursor_column_tracks_scroll_for_wide_content() {
        let mut c = Composer::new();
        c.set_text("a".repeat(80));
        c.end();
        // The area is 20 columns; the cursor sits at the far end and the
        // scroll anchors it inside the visible window.
        let col = c.cursor_col(20);
        assert!(col < 20, "cursor stays visible, got col {col}");
        c.home();
        assert_eq!(c.cursor_col(20), 0);
    }

    #[test]
    fn renders_placeholder_when_empty() {
        let mut c = Composer::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        c.render(Rect::new(0, 0, 10, 1), &mut buf, Style::default());
        let first = buf[(0, 0)].symbol();
        assert_eq!(first, ">", "placeholder starts with the prompt mark");
        // Empty input renders no cursor offset past the placeholder text.
        assert_eq!(c.cursor_col(10), 0);
    }
}
