//! Pure prompt state machine: buffer, cursor, suggestion, history browsing.

use std::sync::Arc;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::suggest::{SuggestBroker, SuggestCtx, SuggestEngine};

use super::keymap::Key;

/// Maximum number of characters of a suggestion's ghost text that is
/// painted on the terminal before an ellipsis. Longer suggestions are still
/// accepted in full; only their on-screen preview is capped so a single long
/// history entry cannot occupy the whole prompt area while typing.
pub const SUGGESTION_DISPLAY_LIMIT: usize = 30;

/// What one prompt read produced for the session loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptEvent {
    /// A submitted line (the typed buffer; a visible suggestion is NOT
    /// included — accepting it requires Right/Tab, like zsh).
    Line(String),
    /// Ctrl-D on an empty buffer, or stdin closed.
    Eof,
    /// Ctrl-C: abandon the current line (the session keeps its quit logic).
    Interrupt,
}

/// Per-prompt inputs, snapshotted by the session when the prompt starts.
pub struct EditorSnapshot {
    /// Prompt history, oldest first.
    pub history: Vec<String>,
    /// The previously submitted prompt (backs `match_prev_cmd`).
    pub prev_prompt: Option<String>,
    /// Prompt string, e.g. `"> "`.
    pub prompt: String,
    /// Terminal width in columns at prompt start; resized live on resize
    /// events. Zero means unknown and falls back to single-row rendering.
    pub term_cols: u16,
    /// Skill names available for `@skill:` mention completion.
    pub skills: Vec<String>,
    /// Workflow ids available for `@workflow:` mention completion.
    pub workflows: Vec<String>,
    /// Project root used for `@file:` mention scanning.
    pub project_root: Option<std::path::PathBuf>,
    /// Maximum number of characters of a ghost suggestion painted before an
    /// ellipsis. `0` falls back to the built-in default.
    pub suggestion_display_limit: usize,
}

/// What the session loop should do after one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreAction {
    /// Keep editing (the caller redraws).
    Continue,
    /// Ctrl-L: clear the screen, then redraw.
    ClearScreen,
    Submit,
    Eof,
    Interrupt,
}

/// Pure prompt state machine: buffer, cursor, suggestion, history
/// browsing. No terminal access, so every behavior is unit-testable.
pub struct EditorCore {
    prompt: String,
    buffer: Vec<char>,
    cursor: usize,
    /// Suggestion suffix drawn after the cursor (never part of the buffer
    /// until accepted).
    suggestion: Option<String>,
    /// Active Up/Down browsing state.
    browse: Option<Browse>,
    snapshot: EditorSnapshot,
    engine: Arc<SuggestEngine>,
    broker: Arc<SuggestBroker>,
}

#[derive(Clone)]
struct Browse {
    /// The buffer as it was when browsing started.
    prefix: Vec<char>,
    /// Indices into `snapshot.history` matching the prefix, ascending.
    matches: Vec<usize>,
    /// Position within `matches`; Up moves older (smaller), Down newer.
    pos: usize,
}

impl EditorCore {
    pub fn new(
        snapshot: EditorSnapshot,
        engine: Arc<SuggestEngine>,
        broker: Arc<SuggestBroker>,
    ) -> Self {
        let mut core = Self {
            prompt: snapshot.prompt.clone(),
            buffer: Vec::new(),
            cursor: 0,
            suggestion: None,
            browse: None,
            snapshot,
            engine,
            broker,
        };
        core.refetch();
        core
    }

    pub fn buffer(&self) -> &[char] {
        &self.buffer
    }

    pub fn prompt_text(&self) -> &str {
        &self.prompt
    }

    pub fn term_cols(&self) -> u16 {
        self.snapshot.term_cols
    }

    pub fn set_term_cols(&mut self, cols: u16) {
        self.snapshot.term_cols = cols;
    }

    /// The submitted line: the typed buffer only.
    pub fn line(&self) -> String {
        self.buffer.iter().collect()
    }

    /// Pre-send hint when the buffer carries more references than one turn
    /// resolves. Pure and non-blocking; the server stays authoritative.
    pub fn mention_hint(&self) -> Option<String> {
        wf_common::mention::mention_turn_hint(&self.buffer.iter().collect::<String>())
    }

    /// Suffix to draw dim after the cursor; only meaningful at end of
    /// buffer, which is where every suggestion-producing action leaves the
    /// cursor anyway. Returns the complete suggestion so callers that need
    /// the full text (accept, diagnostics) keep working.
    pub fn suggestion_suffix(&self) -> Option<&str> {
        self.suggestion.as_deref()
    }

    /// Ghost text actually painted on the terminal: the suggestion capped
    /// to the configured display limit characters followed by an ellipsis
    /// when it is longer. The complete suggestion is still inserted on
    /// accept, so this only affects display.
    pub fn suggestion_display_suffix(&self) -> Option<String> {
        self.suggestion.as_deref().map(|suffix| {
            truncate_suggestion_with_limit(suffix, self.snapshot.suggestion_display_limit)
        })
    }

    /// Display width of the buffer tail right of the cursor (for cursor
    /// repositioning after a full-line redraw).
    pub fn width_right_of_cursor(&self) -> usize {
        self.buffer[self.cursor..]
            .iter()
            .map(|c| c.width().unwrap_or(0))
            .sum()
    }

    /// Display width of the prompt plus the buffer head left of the cursor.
    pub fn width_left_of_cursor(&self) -> usize {
        let prompt_w = UnicodeWidthStr::width(self.prompt.as_str());
        let head_w: usize = self.buffer[..self.cursor]
            .iter()
            .map(|c| c.width().unwrap_or(0))
            .sum();
        prompt_w + head_w
    }

    /// Display width of the whole visible line (prompt, buffer, ghost).
    ///
    /// The ghost is rendered truncated (see [`SUGGESTION_DISPLAY_LIMIT`])
    /// so an over-long history entry cannot force the prompt to wrap while
    /// the user is still typing, which is what made the view jump. The
    /// underlying suggestion stays complete and is inserted in full on
    /// accept.
    pub fn full_line_width(&self) -> usize {
        let prompt_w = UnicodeWidthStr::width(self.prompt.as_str());
        let buf_w: usize = self.buffer.iter().map(|c| c.width().unwrap_or(0)).sum();
        let ghost_w = self
            .suggestion
            .as_deref()
            .map(|suffix| {
                truncate_suggestion_with_limit(suffix, self.snapshot.suggestion_display_limit)
            })
            .map(|s| UnicodeWidthStr::width(s.as_str()))
            .unwrap_or(0);
        prompt_w + buf_w + ghost_w
    }

    /// Insert a bracketed-paste payload as one edit: newlines flatten to
    /// spaces (history convention), cursor lands after the insert, one
    /// refetch at the end instead of one per character.
    pub fn insert_paste(&mut self, text: &str) {
        let flat = text.replace("\r\n", " ").replace(['\n', '\r'], " ");
        if flat.is_empty() {
            return;
        }
        for c in flat.chars() {
            self.buffer.insert(self.cursor, c);
            self.cursor += 1;
        }
        self.browse = None;
        self.refetch();
    }

    pub fn handle_key(&mut self, key: Key) -> CoreAction {
        match key {
            Key::Char(c) => {
                self.buffer.insert(self.cursor, c);
                self.cursor += 1;
                self.browse = None;
                self.refetch();
                CoreAction::Continue
            }
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                    self.browse = None;
                    self.refetch();
                }
                CoreAction::Continue
            }
            Key::Delete => {
                if self.cursor < self.buffer.len() {
                    self.buffer.remove(self.cursor);
                    self.browse = None;
                    self.refetch();
                }
                CoreAction::Continue
            }
            Key::Left(false) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.clear_suggestion();
                }
                CoreAction::Continue
            }
            Key::Right(false) => {
                if self.cursor < self.buffer.len() {
                    self.cursor += 1;
                    self.clear_suggestion();
                } else if self.suggestion.is_some() {
                    // Right at end of buffer accepts the whole suggestion.
                    self.accept_full();
                }
                CoreAction::Continue
            }
            Key::Right(true) | Key::AltF => {
                if self.cursor == self.buffer.len() && self.suggestion.is_some() {
                    // Partial accept: one word of the suggestion.
                    self.accept_word();
                } else {
                    self.word_right();
                    self.clear_suggestion();
                }
                CoreAction::Continue
            }
            Key::Left(true) | Key::AltB => {
                self.word_left();
                self.clear_suggestion();
                CoreAction::Continue
            }
            Key::AltD => {
                self.delete_word_forward();
                CoreAction::Continue
            }
            Key::AltBackspace => {
                let end = self.cursor;
                self.word_left();
                self.buffer.drain(self.cursor..end);
                self.browse = None;
                self.refetch();
                CoreAction::Continue
            }
            Key::AltDot => {
                self.insert_history_tail_word();
                CoreAction::Continue
            }
            Key::Up => {
                self.browse_up();
                CoreAction::Continue
            }
            Key::Down => {
                self.browse_down();
                CoreAction::Continue
            }
            Key::Home | Key::CtrlA => {
                self.cursor = 0;
                self.clear_suggestion();
                CoreAction::Continue
            }
            Key::End | Key::CtrlE => {
                self.cursor = self.buffer.len();
                self.clear_suggestion();
                CoreAction::Continue
            }
            Key::CtrlK => {
                self.buffer.truncate(self.cursor);
                self.browse = None;
                self.refetch();
                CoreAction::Continue
            }
            Key::CtrlU => {
                self.buffer.drain(..self.cursor);
                self.cursor = 0;
                self.browse = None;
                self.refetch();
                CoreAction::Continue
            }
            Key::CtrlW => {
                let end = self.cursor;
                self.word_left();
                self.buffer.drain(self.cursor..end);
                self.browse = None;
                self.refetch();
                CoreAction::Continue
            }
            Key::Tab => {
                if self.cursor == self.buffer.len() && self.suggestion.is_some() {
                    self.accept_full();
                }
                CoreAction::Continue
            }
            Key::Enter => CoreAction::Submit,
            Key::CtrlC => CoreAction::Interrupt,
            Key::CtrlD => {
                if self.buffer.is_empty() {
                    CoreAction::Eof
                } else if self.cursor < self.buffer.len() {
                    self.buffer.remove(self.cursor);
                    self.browse = None;
                    self.refetch();
                    CoreAction::Continue
                } else {
                    CoreAction::Continue
                }
            }
            Key::CtrlL => CoreAction::ClearScreen,
            Key::Unknown => CoreAction::Continue,
        }
    }

    /// Cursor movement must not leave a stale ghost (zsh `clear` widgets).
    fn clear_suggestion(&mut self) {
        self.suggestion = None;
    }

    /// Re-run the strategy pipeline after any buffer modification. The
    /// broker token implements cancel-pending: nothing computed under an
    /// older token survives a newer keystroke.
    fn refetch(&mut self) {
        let token = self.broker.begin();
        let line: String = self.buffer.iter().collect();
        let ctx = SuggestCtx {
            history: &self.snapshot.history,
            prev_prompt: self.snapshot.prev_prompt.as_deref(),
            skills: &self.snapshot.skills,
            workflows: &self.snapshot.workflows,
            project_root: self.snapshot.project_root.as_deref(),
        };
        let fetched = self.engine.suggest(&line, &ctx);
        self.suggestion = if self.broker.is_current(token) {
            fetched.map(|full| full[line.len()..].to_string())
        } else {
            None
        };
    }

    /// Accept the whole suggestion into the buffer.
    fn accept_full(&mut self) {
        let Some(suffix) = self.suggestion.take() else {
            return;
        };
        self.buffer.extend(suffix.chars());
        self.cursor = self.buffer.len();
        self.browse = None;
    }

    /// Accept the suggestion up to the end of its next word, keeping the
    /// remainder as the new suggestion (zsh partial-accept keeps the
    /// postdisplay tail, `widgets.zsh:47-66`).
    fn accept_word(&mut self) {
        let Some(suffix) = self.suggestion.clone() else {
            return;
        };
        let take = partial_word_bytes(&suffix);
        let rest = suffix[take..].to_string();
        self.buffer.extend(suffix[..take].chars());
        self.cursor = self.buffer.len();
        self.browse = None;
        self.suggestion = if rest.is_empty() { None } else { Some(rest) };
    }

    fn word_left(&mut self) {
        let mut i = self.cursor;
        while i > 0 && self.buffer[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !self.buffer[i - 1].is_whitespace() {
            i -= 1;
        }
        self.cursor = i;
    }

    fn word_right(&mut self) {
        let len = self.buffer.len();
        let mut i = self.cursor;
        while i < len && self.buffer[i].is_whitespace() {
            i += 1;
        }
        while i < len && !self.buffer[i].is_whitespace() {
            i += 1;
        }
        self.cursor = i;
    }

    fn delete_word_forward(&mut self) {
        let len = self.buffer.len();
        let mut end = self.cursor;
        while end < len && self.buffer[end].is_whitespace() {
            end += 1;
        }
        while end < len && !self.buffer[end].is_whitespace() {
            end += 1;
        }
        if end > self.cursor {
            self.buffer.drain(self.cursor..end);
            self.browse = None;
            self.refetch();
        }
    }

    /// Insert the last whitespace-separated word of the newest history
    /// entry (zsh Alt-.): a pure key semantic, no mode change.
    fn insert_history_tail_word(&mut self) {
        let word = self
            .snapshot
            .prev_prompt
            .as_deref()
            .and_then(|prev| prev.split_whitespace().next_back().map(str::to_string))
            .or_else(|| {
                self.snapshot
                    .history
                    .last()
                    .and_then(|entry| entry.split_whitespace().next_back().map(str::to_string))
            });
        let Some(word) = word else {
            return;
        };
        for c in word.chars() {
            self.buffer.insert(self.cursor, c);
            self.cursor += 1;
        }
        self.browse = None;
        self.refetch();
    }

    /// Walk to the previous (older) prefix match; the first Up starts
    /// browsing from the current buffer as the prefix.
    fn browse_up(&mut self) {
        let matches = self.prefix_matches();
        if matches.is_empty() {
            return;
        }
        let next_pos = match &self.browse {
            Some(browse) => {
                if browse.pos == 0 {
                    return; // already at the oldest match
                }
                browse.pos - 1
            }
            None => matches.len() - 1, // start at the newest match
        };
        self.recall(&matches, next_pos);
    }

    /// Walk to the next (newer) prefix match; past the newest restores the
    /// typed prefix.
    fn browse_down(&mut self) {
        let Some(browse) = self.browse.clone() else {
            return;
        };
        if browse.pos + 1 >= browse.matches.len() {
            // Past the newest match: restore the typed prefix.
            let prefix = browse.prefix.clone();
            self.browse = None;
            self.buffer = prefix;
            self.cursor = self.buffer.len();
            self.refetch();
            return;
        }
        self.recall(&browse.matches, browse.pos + 1);
    }

    fn prefix_matches(&self) -> Vec<usize> {
        let prefix: Vec<char> = match &self.browse {
            Some(browse) => browse.prefix.clone(),
            None => self.buffer.clone(),
        };
        self.snapshot
            .history
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let mut chars = entry.chars();
                prefix.iter().all(|p| chars.next().is_some_and(|c| c == *p))
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn recall(&mut self, matches: &[usize], pos: usize) {
        let prefix = match &self.browse {
            Some(browse) => browse.prefix.clone(),
            None => self.buffer.clone(),
        };
        let entry = &self.snapshot.history[matches[pos]];
        self.browse = Some(Browse {
            prefix,
            matches: matches.to_vec(),
            pos,
        });
        self.buffer = entry.chars().collect();
        self.cursor = self.buffer.len();
        self.refetch();
    }
}

/// Cap a suggestion's on-screen preview to [`SUGGESTION_DISPLAY_LIMIT`]
/// characters, appending an ellipsis when truncation occurred. Char-aware so
/// multibyte input is never split.
pub fn truncate_suggestion(suffix: &str) -> String {
    truncate_suggestion_with_limit(suffix, SUGGESTION_DISPLAY_LIMIT)
}

/// Like [`truncate_suggestion`] but with an explicit character limit. A limit
/// of zero falls back to [`SUGGESTION_DISPLAY_LIMIT`].
pub fn truncate_suggestion_with_limit(suffix: &str, limit: usize) -> String {
    let limit = if limit == 0 {
        SUGGESTION_DISPLAY_LIMIT
    } else {
        limit
    };
    let count = suffix.chars().count();
    if count <= limit {
        return suffix.to_string();
    }
    let head: String = suffix.chars().take(limit).collect();
    format!("{head}…")
}

/// Byte length of the next accept chunk of `suffix`: the leading
/// whitespace run, or one word plus its trailing whitespace.
pub fn partial_word_bytes(suffix: &str) -> usize {
    let chars: Vec<(usize, char)> = suffix.char_indices().collect();
    if chars.is_empty() {
        return suffix.len();
    }
    let mut i = 0;
    if chars[0].1.is_whitespace() {
        while i < chars.len() && chars[i].1.is_whitespace() {
            i += 1;
        }
    } else {
        while i < chars.len() && !chars[i].1.is_whitespace() {
            i += 1;
        }
        while i < chars.len() && chars[i].1.is_whitespace() {
            i += 1;
        }
    }
    chars.get(i).map(|(byte, _)| *byte).unwrap_or(suffix.len())
}

#[cfg(test)]
mod test;
