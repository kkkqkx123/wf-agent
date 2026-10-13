//! Single-line prompt composer.
//!
//! [`Composer`] is the TUI footer prompt view's text entry (inline form,
//! unrelated to the `wf-mini` binary): a one-line buffer with a
//! grapheme-aware cursor, horizontal scrolling so the cursor stays visible,
//! a bounded history (200 entries, oldest-first) with linear recall (Up stops
//! at the oldest entry, Down past the newest restores the stashed draft), a
//! history ghost suffix for non-empty prefixes (newest match wins, suppressed
//! after deletions and while browsing or reverse-searching), an inline
//! `Ctrl+R` reverse search (case-insensitive substring, newest first,
//! Enter accepts without sending, Esc restores), a placeholder for the empty
//! state and character-level editing (insert / backspace / delete / home /
//! end / left / right).
//!
//! The composer owns no terminal state: `render` draws into a caller-provided
//! ratatui [`Buffer`] with a caller-provided style. History seeding (file
//! tail at startup, replay user prompts later) arrives through [`Composer::seed_older`];
//! every index into the list (browse position, search matches) shifts with it.

use std::cell::Cell;
use std::collections::{HashSet, VecDeque};
use std::ops::Range;
use std::path::{Path, PathBuf};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::mention::{parse_mentions, Mention, MentionKind};

/// Bound of the prompt history ring (aligned with the opencode 200-entry
/// cap; consecutive duplicates are skipped in `submit`).
pub const HISTORY_LIMIT: usize = 200;

/// Placeholder shown while the buffer is empty.
pub const PLACEHOLDER: &str = "> Type a message…";

/// A one-line prompt buffer.
#[derive(Debug, Clone)]
pub struct Composer {
    buf: String,
    /// Byte offset of the cursor into `buf`; always on a grapheme boundary.
    cursor: usize,
    /// Horizontal scroll offset in columns (kept so the cursor is visible).
    scroll_x: Cell<u16>,
    history: VecDeque<String>,
    /// Browse position into `history` while recalling (`None` means the
    /// buffer holds the user's own draft).
    browse: Option<usize>,
    /// Draft saved on the first recall step, restored past the newest entry.
    stash: Option<String>,
    /// Deletion holdback: set by deletions, cleared by insertions, paste,
    /// submit and clear. While set, no ghost suffix is shown.
    suppressed: bool,
    /// Active `Ctrl+R` reverse search; the buffer holds the query while set.
    search: Option<ReverseSearch>,
    /// Optional backing file for cross-process history (best-effort append).
    history_file: Option<PathBuf>,
    placeholder: &'static str,
}

/// An inline `Ctrl+R` reverse search.
#[derive(Debug, Clone)]
struct ReverseSearch {
    /// Buffer saved when the search opened, restored by Esc.
    draft: String,
    /// Browse position saved when the search opened, restored by Esc.
    draft_browse: Option<usize>,
    /// Matching history indices, newest first.
    matches: Vec<usize>,
    /// Which match is lit (`0` is the newest).
    sel: usize,
}

/// Newest-first prefix suffix: the rest of the newest history entry that
/// starts with `prefix` and is strictly longer. Pure over the history list.
pub fn history_ghost(prefix: &str, history: &VecDeque<String>) -> Option<String> {
    if prefix.is_empty() {
        return None;
    }
    history
        .iter()
        .rev()
        .find(|entry| entry.starts_with(prefix) && entry.len() > prefix.len())
        .map(|entry| entry[prefix.len()..].to_string())
}

/// Case-insensitive substring matches, newest first, each distinct text once
/// (kept where it was said last). Empty query matches nothing.
pub fn history_search_matches(history: &VecDeque<String>, query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle = query.to_lowercase();
    let mut seen = HashSet::new();
    history
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, entry)| entry.to_lowercase().contains(&needle))
        .filter(|(_, entry)| seen.insert(entry.as_str()))
        .map(|(i, _)| i)
        .collect()
}

/// Default backing file for cross-process prompt history.
pub fn default_history_path() -> Option<PathBuf> {
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".local").join("state"))
        })?;
    Some(base.join("wf").join("tui_history"))
}

/// Best-effort tail of a plain-text history file (one prompt per line).
/// A missing or unreadable file reads as empty.
pub fn read_history_tail(path: &Path, limit: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = lines.len().saturating_sub(limit);
    lines[tail..].iter().map(|s| s.to_string()).collect()
}

/// Best-effort append of one history line (creates parent dirs on first use).
fn append_history_line(path: &Path, line: &str) {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write as _;
        let _ = writeln!(file, "{line}");
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self {
            buf: String::new(),
            cursor: 0,
            scroll_x: Cell::new(0),
            history: VecDeque::new(),
            browse: None,
            stash: None,
            suppressed: false,
            search: None,
            history_file: None,
            placeholder: PLACEHOLDER,
        }
    }
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

    /// Replace the whole buffer (cursor moves to the end). Counts as fresh
    /// typing: leaves browsing and search, clears the deletion holdback.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buf = text.into();
        self.cursor = self.buf.len();
        self.scroll_x.set(0);
        self.browse = None;
        self.stash = None;
        self.suppressed = false;
        self.search = None;
    }

    /// Insert a single character at the cursor. Typing leaves browsing,
    /// clears the deletion holdback; while searching it extends the query.
    pub fn insert_char(&mut self, c: char) {
        if self.search.is_some() {
            self.search_push_char(c);
            return;
        }
        self.leave_browsing();
        self.suppressed = false;
        self.buf.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Insert a string at the cursor (bracketed paste / mention insert).
    /// Paste counts as typing: leaves browsing, clears the holdback; while
    /// searching it extends the query.
    pub fn insert_text(&mut self, text: &str) {
        if self.search.is_some() {
            self.search_push_text(text);
            return;
        }
        self.leave_browsing();
        self.suppressed = false;
        self.buf.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    /// Delete the grapheme before the cursor. Deletions hold the ghost back
    /// until the next insertion; typing position leaves browsing.
    pub fn backspace(&mut self) {
        if self.search.is_some() {
            self.search_backspace();
            return;
        }
        self.leave_browsing();
        self.suppressed = true;
        if let Some((len, _)) = self.grapheme_before() {
            self.buf.drain(self.cursor - len..self.cursor);
            self.cursor -= len;
        }
    }

    /// Delete the grapheme at the cursor (same holdback as backspace).
    pub fn delete_forward(&mut self) {
        if self.search.is_some() {
            return;
        }
        self.leave_browsing();
        self.suppressed = true;
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

    /// Retained history, oldest first.
    pub fn history_entries(&self) -> Vec<String> {
        self.history.iter().cloned().collect()
    }

    /// Whether a recall session is showing a history entry.
    pub fn browsing(&self) -> bool {
        self.browse.is_some()
    }

    /// Whether the cursor sits at the end of the buffer.
    pub fn is_at_end(&self) -> bool {
        self.cursor == self.buf.len()
    }

    /// The ghost suffix for the current buffer: the rest of the newest
    /// history entry with the buffer as prefix. Empty while suppressed,
    /// while browsing, while searching, for empty input, exact repeats and
    /// misses. The single question both drawing and keys ask.
    pub fn ghost(&self) -> Option<String> {
        if self.suppressed || self.search.is_some() || self.browse.is_some() {
            return None;
        }
        history_ghost(&self.buf, &self.history)
    }

    /// Take the ghost into the buffer when the cursor is at the end.
    /// Returns whether anything was taken (the caller's cue that Right
    /// meant "accept" rather than "move").
    pub fn accept_ghost(&mut self) -> bool {
        if !self.is_at_end() {
            return false;
        }
        let Some(rest) = self.ghost() else {
            return false;
        };
        self.buf.push_str(&rest);
        self.cursor = self.buf.len();
        true
    }

    /// Whether a reverse search is open (the buffer holds the query).
    pub fn search_active(&self) -> bool {
        self.search.is_some()
    }

    /// Number of search matches (0 when closed).
    pub fn search_match_count(&self) -> usize {
        self.search.as_ref().map(|s| s.matches.len()).unwrap_or(0)
    }

    /// Lit search match text, if any.
    pub fn search_selected_text(&self) -> Option<String> {
        let search = self.search.as_ref()?;
        let index = *search.matches.get(search.sel)?;
        self.history.get(index).cloned()
    }

    /// `(selected_position_1_based, match_count)` for the shoulder counter.
    pub fn search_position(&self) -> Option<(usize, usize)> {
        let search = self.search.as_ref()?;
        if search.matches.is_empty() {
            return None;
        }
        Some((search.sel + 1, search.matches.len()))
    }

    /// Open a reverse search, setting the draft aside. Nothing matches until
    /// something is typed; the buffer becomes the query.
    pub fn begin_search(&mut self) {
        if self.search.is_some() {
            return;
        }
        let mut search = ReverseSearch {
            draft: std::mem::take(&mut self.buf),
            draft_browse: self.browse.take(),
            matches: Vec::new(),
            sel: 0,
        };
        self.stash = None;
        self.rematch_search(&mut search);
        self.search = Some(search);
    }

    /// Extend the query by one character (newest match lit).
    pub fn search_push_char(&mut self, c: char) {
        if let Some(mut search) = self.search.take() {
            search_query_push(&mut self.buf, &mut self.cursor, c);
            self.rematch_search(&mut search);
            self.search = Some(search);
        }
    }

    /// Extend the query by pasted text (newest match lit).
    pub fn search_push_text(&mut self, text: &str) {
        if let Some(mut search) = self.search.take() {
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            self.buf.push_str(&normalized);
            self.cursor = self.buf.len();
            self.rematch_search(&mut search);
            self.search = Some(search);
        }
    }

    /// Shorten the query by one character and match again.
    pub fn search_backspace(&mut self) {
        if let Some(mut search) = self.search.take() {
            search_query_pop(&mut self.buf, &mut self.cursor);
            self.rematch_search(&mut search);
            self.search = Some(search);
        }
    }

    /// Light the next older match (stays at the oldest, never wraps).
    pub fn search_older(&mut self) {
        if let Some(search) = self.search.as_mut() {
            search.sel = (search.sel + 1).min(search.matches.len().saturating_sub(1));
        }
    }

    /// Light the next newer match (stays at the newest).
    pub fn search_newer(&mut self) {
        if let Some(search) = self.search.as_mut() {
            search.sel = search.sel.saturating_sub(1);
        }
    }

    /// Accept the lit match into the buffer (caret at the end) and close.
    /// With nothing lit the query stays as typed. Returns whether a match
    /// was taken.
    pub fn search_accept(&mut self) -> bool {
        let Some(search) = self.search.take() else {
            return false;
        };
        if let Some(index) = search.matches.get(search.sel).copied() {
            if let Some(entry) = self.history.get(index).cloned() {
                self.buf = entry;
                self.cursor = self.buf.len();
                self.scroll_x.set(0);
                return true;
            }
        }
        false
    }

    /// Close the search and give back the draft saved on open.
    pub fn search_cancel(&mut self) {
        let Some(search) = self.search.take() else {
            return;
        };
        self.buf = search.draft;
        self.cursor = self.buf.len();
        self.browse = search.draft_browse;
        self.scroll_x.set(0);
    }

    /// Prepend older entries (oldest first) for seeding: file tails at
    /// startup, replay user prompts later. Lines already present are
    /// dropped, the cap is enforced from the front, and every index into
    /// the list (browse position, search matches) shifts. Returns whether
    /// anything was added.
    pub fn seed_older(&mut self, older: Vec<String>) -> bool {
        let already: HashSet<&str> = self.history.iter().map(String::as_str).collect();
        let fresh: Vec<String> = older
            .into_iter()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty() && !already.contains(line.as_str()))
            .collect();
        if fresh.is_empty() {
            return false;
        }
        let grew = fresh.len();
        for line in fresh.into_iter().rev() {
            self.history.push_front(line);
        }
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        if let Some(at) = self.browse.as_mut() {
            *at = (*at + grew).min(self.history.len().saturating_sub(1));
        }
        if let Some(search) = self.search.as_mut() {
            for at in search.matches.iter_mut() {
                *at += grew;
            }
            if let Some(at) = search.draft_browse.as_mut() {
                *at += grew;
            }
            search.matches.retain(|at| *at < self.history.len());
            search.sel = search.sel.min(search.matches.len().saturating_sub(1));
        }
        true
    }

    /// Point the composer at a backing history file: the tail is loaded
    /// immediately (best-effort) and later submits append to it.
    pub fn set_history_file(&mut self, path: PathBuf) {
        let tail = read_history_tail(&path, HISTORY_LIMIT);
        if !tail.is_empty() {
            self.seed_older(tail);
        }
        self.history_file = Some(path);
    }

    /// Back one entry in the history, stashing the draft on the way in.
    /// Stops at the oldest entry (linear, never wraps).
    pub fn history_prev(&mut self) {
        if self.search.is_some() || self.history.is_empty() {
            return;
        }
        let at = match self.browse {
            None => {
                self.stash = Some(std::mem::take(&mut self.buf));
                self.history.len() - 1
            }
            Some(0) => return,
            Some(i) => i - 1,
        };
        self.browse = Some(at);
        self.show_entry(at);
    }

    /// Forward one entry, and out the far side to the stashed draft.
    pub fn history_next(&mut self) {
        if self.search.is_some() {
            return;
        }
        let Some(at) = self.browse else {
            return;
        };
        if at + 1 < self.history.len() {
            self.browse = Some(at + 1);
            self.show_entry(at + 1);
        } else {
            self.browse = None;
            let draft = self.stash.take().unwrap_or_default();
            self.buf = draft;
            self.cursor = self.buf.len();
            self.scroll_x.set(0);
        }
    }

    /// Commit the buffer: record it in the history (dedup against the newest
    /// entry, empty inputs are skipped), clear the buffer and return the
    /// submitted text. While searching, accept instead of sending so Enter
    /// never fires the query as a prompt.
    pub fn submit(&mut self) -> Option<String> {
        if self.search.is_some() {
            self.search_accept();
            return None;
        }
        let text = self.buf.trim().to_string();
        if !text.is_empty() && self.history.back().map(String::as_str) != Some(text.as_str()) {
            if self.history.len() == HISTORY_LIMIT {
                self.history.pop_front();
            }
            self.history.push_back(text.clone());
            if let Some(path) = self.history_file.clone() {
                append_history_line(&path, &text);
            }
        }
        self.clear();
        (!text.is_empty()).then_some(text)
    }

    /// Clear the buffer and reset the cursor / scroll.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
        self.scroll_x.set(0);
        self.browse = None;
        self.stash = None;
        self.suppressed = false;
        self.search = None;
    }

    /// Column (0-based) at which the cursor is drawn for the given area
    /// width, after re-anchoring the horizontal scroll.
    pub fn cursor_col(&self, width: u16) -> u16 {
        self.update_scroll(width);
        let before = self.buf[..self.cursor].width() as u16;
        before.saturating_sub(self.scroll_x.get())
    }

    /// Typing takes ownership of the buffer: leave any recall session and
    /// drop the stashed draft. There is only one thing being composed.
    fn leave_browsing(&mut self) {
        self.browse = None;
        self.stash = None;
    }

    /// Show history entry `at` in the buffer (recall navigation only).
    fn show_entry(&mut self, at: usize) {
        if let Some(entry) = self.history.get(at).cloned() {
            self.buf = entry;
            self.cursor = self.buf.len();
            self.scroll_x.set(0);
        }
    }

    /// Re-match the open search for the buffer as it now stands: newest lit,
    /// buffer kept as the query.
    fn rematch_search(&mut self, search: &mut ReverseSearch) {
        search.matches = history_search_matches(&self.history, &self.buf);
        search.sel = 0;
        self.cursor = self.buf.len();
        self.scroll_x.set(0);
    }

    /// All mention intervals in the current buffer, as byte ranges.
    pub fn mention_ranges(&self) -> Vec<(Range<usize>, MentionKind)> {
        parse_mentions(&self.buf)
            .into_iter()
            .map(|m| (m.range, m.kind))
            .collect()
    }

    /// Parsed mentions in the current buffer.
    pub fn mentions(&self) -> Vec<Mention> {
        parse_mentions(&self.buf)
    }

    /// Query for the mention under the cursor, if any (text after `@` up to
    /// cursor). Used to drive the `@` filter popup.
    pub fn mention_query(&self) -> Option<String> {
        crate::mention::mention_query_at_cursor(&self.buf, self.cursor)
    }

    /// Replace the mention query at the cursor with `candidate` (inserted as
    /// `@candidate ` with a trailing space).
    pub fn apply_mention_completion(&mut self, candidate: &str) {
        if self.mention_query().is_none() {
            return;
        }
        let cursor = self.cursor;
        let at_pos = self.buf[..cursor].rfind('@').unwrap_or(cursor);
        let insert = format!("@{candidate} ");
        self.buf.replace_range(at_pos..cursor, &insert);
        self.cursor = at_pos + insert.len();
        self.scroll_x.set(0);
    }

    /// Render the single line into `area`, applying the horizontal scroll.
    /// Mention intervals are highlighted with a reversed style so `@file`
    /// ranges are visually distinct from plain text.
    pub fn render(&self, area: Rect, buf: &mut Buffer, style: Style) {
        let width = area.width.max(1);
        buf.set_string(area.x, area.y, " ".repeat(width.into()), Style::default());
        if self.buf.is_empty() {
            let ph = truncate_graphemes(self.placeholder, width);
            buf.set_string(area.x, area.y, &ph, style.add_modifier(Modifier::DIM));
            return;
        }
        self.update_scroll(width);
        let mention_ranges = self.mention_ranges();
        let mut col = 0u16;
        let mut skip = self.scroll_x.get();
        let mut byte_offset = 0usize;
        for grapheme in self.buf.graphemes(true) {
            let w = grapheme.width() as u16;
            let in_mention = mention_ranges
                .iter()
                .any(|(r, _)| byte_offset >= r.start && byte_offset < r.end);
            let g_style = if in_mention {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            };
            if skip > 0 {
                skip = skip.saturating_sub(w);
                byte_offset += grapheme.len();
                continue;
            }
            if col + w > width {
                break;
            }
            buf.set_string(area.x + col, area.y, grapheme, g_style);
            col += w;
            byte_offset += grapheme.len();
        }
        // Ghost suffix in dim style after the typed text (never wraps).
        if let Some(ghost) = self.ghost() {
            let dim = style.add_modifier(Modifier::DIM);
            for grapheme in ghost.graphemes(true) {
                let w = grapheme.width() as u16;
                if col + w > width {
                    break;
                }
                buf.set_string(area.x + col, area.y, grapheme, dim);
                col += w;
            }
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
    fn update_scroll(&self, width: u16) {
        if width == 0 {
            return;
        }
        let cursor_col = self.buf[..self.cursor].width() as u16;
        let right_edge = self.scroll_x.get().saturating_add(width);
        if cursor_col >= right_edge {
            self.scroll_x
                .set(cursor_col.saturating_add(1).saturating_sub(width));
        } else if cursor_col < self.scroll_x.get() {
            self.scroll_x.set(cursor_col);
        }
    }
}

impl tui_core::renderable::Renderable for Composer {
    fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        // Default style; callers needing a custom style should use the
        // concrete `render(area, buf, style)` method directly.
        self.render(area, buf, Style::default());
    }

    fn desired_height(&self, _width: u16) -> u16 {
        1
    }

    fn cursor_pos(&self, area: ratatui::layout::Rect) -> Option<(u16, u16)> {
        let col = self.cursor_col(area.width);
        Some((col, 0))
    }
}

/// Push one character onto a search query held in `buf`.
fn search_query_push(buf: &mut String, cursor: &mut usize, c: char) {
    buf.push(c);
    *cursor = buf.len();
}

/// Pop one character off a search query held in `buf`.
fn search_query_pop(buf: &mut String, cursor: &mut usize) {
    buf.pop();
    *cursor = buf.len();
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
    fn history_recall_is_linear_and_restores_the_stash() {
        let mut c = Composer::new();
        c.set_text("first");
        assert_eq!(c.submit(), Some("first".to_string()));
        c.set_text("second");
        assert_eq!(c.submit(), Some("second".to_string()));
        assert_eq!(c.history_len(), 2);

        // Fresh draft; Up stashes it and shows the newest entry.
        c.set_text("draft");
        c.history_prev();
        assert_eq!(c.content(), "second");
        assert!(c.browsing());
        c.history_prev();
        assert_eq!(c.content(), "first");
        // At the oldest entry Up stops (linear, never wraps).
        c.history_prev();
        assert_eq!(c.content(), "first");
        // Down moves forward, then out to the stashed draft.
        c.history_next();
        assert_eq!(c.content(), "second");
        c.history_next();
        assert_eq!(c.content(), "draft");
        assert!(!c.browsing());
        // Past the draft Down is a no-op.
        c.history_next();
        assert_eq!(c.content(), "draft");
    }

    #[test]
    fn ghost_prefers_the_newest_prefix_match() {
        let mut history = VecDeque::new();
        history.push_back("git status".to_string());
        history.push_back("cargo test".to_string());
        history.push_back("cargo nextest run".to_string());
        assert_eq!(
            history_ghost("cargo ", &history),
            Some("nextest run".to_string())
        );
        assert_eq!(history_ghost("git ", &history), Some("status".to_string()));
    }

    #[test]
    fn ghost_stays_empty_for_edge_inputs() {
        let mut history = VecDeque::new();
        history.push_back("git status".to_string());
        assert_eq!(history_ghost("", &history), None);
        assert_eq!(history_ghost("git status", &history), None);
        assert_eq!(history_ghost("make", &history), None);
    }

    #[test]
    fn ghost_hides_while_browsing_searching_or_suppressed() {
        let mut c = Composer::new();
        c.set_text("git status");
        assert_eq!(c.submit(), Some("git status".to_string()));
        c.set_text("git ");
        assert_eq!(c.ghost(), Some("status".to_string()));
        // Browsing shows an entry already: no ghost.
        c.history_prev();
        assert_eq!(c.content(), "git status");
        assert_eq!(c.ghost(), None);
        c.history_next();
        // Deleting holds the ghost back until typing resumes.
        c.set_text("git st");
        c.backspace();
        assert_eq!(c.content(), "git s");
        assert_eq!(c.ghost(), None);
        c.insert_char('t');
        assert_eq!(c.ghost(), Some("atus".to_string()));
        // Searching shows the query, never a ghost.
        c.begin_search();
        assert_eq!(c.ghost(), None);
        c.search_cancel();
    }

    #[test]
    fn ghost_accepts_only_at_the_end_of_line() {
        let mut c = Composer::new();
        c.set_text("git status");
        assert_eq!(c.submit(), Some("git status".to_string()));
        c.set_text("git ");
        assert!(c.is_at_end());
        assert!(c.accept_ghost());
        assert_eq!(c.content(), "git status");
        // Mid-line Right must move, not accept.
        c.set_text("git ");
        c.move_left();
        assert!(!c.is_at_end());
        assert!(!c.accept_ghost());
        assert_eq!(c.content(), "git ");
    }

    #[test]
    fn typing_leaves_browsing_and_drops_the_draft() {
        let mut c = Composer::new();
        c.set_text("first");
        c.submit();
        c.set_text("second");
        c.submit();
        c.set_text("draft");
        c.history_prev();
        assert_eq!(c.content(), "second");
        c.insert_char('!');
        assert!(!c.browsing());
        assert_eq!(c.content(), "second!");
        // The old draft is gone: recall starts from the edited line.
        c.history_prev();
        assert_eq!(c.content(), "second");
    }

    #[test]
    fn reverse_search_matches_newest_first_and_accepts_without_sending() {
        let mut c = Composer::new();
        for entry in ["cargo build", "git status", "cargo fmt --all"] {
            c.set_text(entry);
            c.submit();
        }
        c.set_text("half a thought");
        c.begin_search();
        assert!(c.search_active());
        assert_eq!(c.content(), "");
        c.search_push_text("cargo");
        assert_eq!(c.search_match_count(), 2);
        assert_eq!(
            c.search_selected_text(),
            Some("cargo fmt --all".to_string())
        );
        c.search_older();
        assert_eq!(c.search_selected_text(), Some("cargo build".to_string()));
        // Older stays at the oldest; newer walks back.
        c.search_older();
        assert_eq!(c.search_selected_text(), Some("cargo build".to_string()));
        c.search_newer();
        assert_eq!(
            c.search_selected_text(),
            Some("cargo fmt --all".to_string())
        );
        // Enter (accept) closes without submitting.
        assert!(c.search_accept());
        assert!(!c.search_active());
        assert_eq!(c.content(), "cargo fmt --all");
    }

    #[test]
    fn reverse_search_cancel_restores_the_draft() {
        let mut c = Composer::new();
        c.set_text("git status");
        c.submit();
        c.set_text("half a thought");
        c.begin_search();
        c.search_push_text("git");
        c.search_cancel();
        assert!(!c.search_active());
        assert_eq!(c.content(), "half a thought");
    }

    #[test]
    fn seeding_prepends_without_duplicates_and_shifts_browsing() {
        let mut c = Composer::new();
        c.set_text("mine one");
        c.submit();
        c.set_text("mine two");
        c.submit();
        c.set_text("draft");
        c.history_prev();
        assert_eq!(c.content(), "mine two");
        assert!(c.seed_older(vec!["older one".to_string(), "mine one".to_string()]));
        assert_eq!(
            c.history_entries(),
            vec![
                "older one".to_string(),
                "mine one".to_string(),
                "mine two".to_string()
            ]
        );
        assert_eq!(c.content(), "mine two");
        assert!(!c.seed_older(vec!["mine two".to_string()]));
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
        let c = Composer::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        c.render(Rect::new(0, 0, 10, 1), &mut buf, Style::default());
        let first = buf[(0, 0)].symbol();
        assert_eq!(first, ">", "placeholder starts with the prompt mark");
        // Empty input renders no cursor offset past the placeholder text.
        assert_eq!(c.cursor_col(10), 0);
    }
}
