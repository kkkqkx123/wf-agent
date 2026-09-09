//! Simple grapheme-aware text editor for the mini TUI input line.
//!
//! The editor owns a [`String`] buffer and a byte-offset cursor that always
//! sits on a grapheme boundary. It supports insert, backspace, delete,
//! home/end, left/right and a bounded command history (↑/↓).

use std::collections::VecDeque;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Maximum history entries retained.
const HISTORY_LIMIT: usize = 200;

/// Single-line prompt editor with history.
#[derive(Debug, Clone)]
pub struct TextEditor {
    buf: String,
    /// Byte offset of the cursor into `buf`; always on a grapheme boundary.
    cursor: usize,
    history: VecDeque<String>,
    /// Draft saved while navigating history (↑), restored at the end (↓).
    stash: Option<String>,
}

impl Default for TextEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEditor {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            cursor: 0,
            history: VecDeque::new(),
            stash: None,
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

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
    }

    /// Submit the current buffer: push to history, return the text, clear.
    pub fn submit(&mut self) -> Option<String> {
        let text = std::mem::take(&mut self.buf);
        self.cursor = 0;
        self.stash = None;
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return None;
        }
        // Skip consecutive duplicates.
        if self.history.back().map(|s| s.as_str()) != Some(&trimmed) {
            self.history.push_back(trimmed.clone());
            if self.history.len() > HISTORY_LIMIT {
                self.history.pop_front();
            }
        }
        Some(trimmed)
    }

    /// Insert a character at the cursor.
    pub fn insert_char(&mut self, ch: char) {
        let width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width == 0 {
            return;
        }
        let idx = self.cursor;
        self.buf.insert(idx, ch);
        self.cursor = self
            .buf
            .grapheme_indices(true)
            .find(|&(i, _)| i >= idx + ch.len_utf8())
            .map(|(i, _)| i)
            .unwrap_or(self.buf.len());
    }

    /// Delete the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self
            .buf
            .grapheme_indices(true)
            .take_while(|&(i, _)| i < self.cursor)
            .last()
            .map(|(i, g)| (i, g.len()))
            .unwrap();
        self.buf.drain(prev.0..prev.0 + prev.1);
        self.cursor = prev.0;
    }

    /// Delete the character after the cursor (forward delete).
    pub fn delete_forward(&mut self) {
        if self.cursor >= self.buf.len() {
            return;
        }
        let grapheme_len = self
            .buf
            .grapheme_indices(true)
            .find(|&(i, _)| i >= self.cursor)
            .map(|(_, g)| g.len())
            .unwrap_or(0);
        self.buf.drain(self.cursor..self.cursor + grapheme_len);
    }

    /// Move cursor left by one grapheme.
    pub fn move_left(&mut self) {
        if let Some((i, _)) = self
            .buf
            .grapheme_indices(true)
            .take_while(|&(idx, _)| idx < self.cursor)
            .last()
        {
            self.cursor = i;
        }
    }

    /// Move cursor right by one grapheme.
    pub fn move_right(&mut self) {
        if let Some((i, _)) = self
            .buf
            .grapheme_indices(true)
            .find(|&(idx, _)| idx > self.cursor)
        {
            self.cursor = i;
        } else {
            self.cursor = self.buf.len();
        }
    }

    /// Move cursor to the start of the line.
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to the end of the line.
    pub fn end(&mut self) {
        self.cursor = self.buf.len();
    }

    /// Move to the previous history entry (↑).
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.stash.is_none() {
            self.stash = Some(self.buf.clone());
        }
        if let Some(back) = self.history.back() {
            let target = back.clone();
            self.buf = target;
            self.cursor = self.buf.len();
            self.history.pop_back();
        }
    }

    /// Move to the next history entry (↓).
    pub fn history_down(&mut self) {
        if let Some(stash) = self.stash.take() {
            self.buf = stash;
            self.cursor = self.buf.len();
        }
    }

    /// Visible width of the current buffer content (for scrolling).
    pub fn content_width(&self) -> usize {
        UnicodeWidthStr::width(self.buf.as_str())
    }

    /// Cursor column offset (for rendering).
    pub fn cursor_col(&self) -> usize {
        let before = &self.buf[..self.cursor];
        UnicodeWidthStr::width(before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_backspace() {
        let mut e = TextEditor::new();
        e.insert_char('a');
        e.insert_char('b');
        e.insert_char('c');
        assert_eq!(e.content(), "abc");
        e.backspace();
        assert_eq!(e.content(), "ab");
        e.move_left();
        e.backspace();
        assert_eq!(e.content(), "b");
    }

    #[test]
    fn history_dedup() {
        let mut e = TextEditor::new();
        e.insert_char('h');
        e.submit();
        e.insert_char('h');
        e.submit();
        assert_eq!(e.history.len(), 1);
    }

    #[test]
    fn history_up_down() {
        let mut e = TextEditor::new();
        e.insert_char('a');
        e.submit();
        e.insert_char('b');
        e.submit();
        // After submit, buf is empty. history_up stashes the current draft.
        e.history_up();
        assert_eq!(e.content(), "b");
        e.history_up();
        assert_eq!(e.content(), "a");
        // history_down restores the stashed draft (empty string after submit).
        e.history_down();
        assert_eq!(e.content(), "");
    }

    #[test]
    fn home_end() {
        let mut e = TextEditor::new();
        e.insert_char('a');
        e.insert_char('b');
        e.insert_char('c');
        e.home();
        assert_eq!(e.cursor, 0);
        e.end();
        assert_eq!(e.cursor, 3);
    }
}
