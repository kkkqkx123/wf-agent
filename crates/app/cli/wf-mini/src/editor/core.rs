//! Pure prompt state machine: buffer, cursor, suggestion, history browsing.

use std::sync::Arc;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::suggest::{SuggestBroker, SuggestCtx, SuggestEngine};

use super::keymap::Key;

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

    /// Suffix to draw dim after the cursor; only meaningful at end of
    /// buffer, which is where every suggestion-producing action leaves the
    /// cursor anyway.
    pub fn suggestion_suffix(&self) -> Option<&str> {
        self.suggestion.as_deref()
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
    pub fn full_line_width(&self) -> usize {
        let prompt_w = UnicodeWidthStr::width(self.prompt.as_str());
        let buf_w: usize = self.buffer.iter().map(|c| c.width().unwrap_or(0)).sum();
        let ghost_w = self
            .suggestion
            .as_deref()
            .map(UnicodeWidthStr::width)
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
mod tests {
    use super::*;
    use crate::suggest::SuggestEngine;

    fn core(history: &[&str], prev: Option<&str>) -> EditorCore {
        EditorCore::new(
            EditorSnapshot {
                history: history.iter().map(|s| s.to_string()).collect(),
                prev_prompt: prev.map(|s| s.to_string()),
                prompt: "> ".to_string(),
                term_cols: 80,
                skills: Vec::new(),
                workflows: Vec::new(),
                project_root: None,
            },
            Arc::new(SuggestEngine::new()),
            Arc::new(SuggestBroker::new()),
        )
    }

    fn feed(core: &mut EditorCore, keys: &[Key]) {
        for key in keys {
            core.handle_key(key.clone());
        }
    }

    fn type_str(core: &mut EditorCore, text: &str) {
        feed(core, &text.chars().map(Key::Char).collect::<Vec<_>>());
    }

    fn buffer_str(core: &EditorCore) -> String {
        core.line()
    }

    #[test]
    fn typing_refetches_suggestion_from_history() {
        let mut core = core(&["cargo nextest run"], None);
        type_str(&mut core, "cargo ");
        assert_eq!(core.suggestion_suffix(), Some("nextest run"));
        type_str(&mut core, "n");
        assert_eq!(core.suggestion_suffix(), Some("extest run"));
    }

    #[test]
    fn enter_submits_buffer_without_suggestion() {
        let mut core = core(&["cargo nextest run"], None);
        type_str(&mut core, "cargo ");
        assert_eq!(core.handle_key(Key::Enter), CoreAction::Submit);
        assert_eq!(buffer_str(&core), "cargo ");
    }

    #[test]
    fn cursor_movement_clears_suggestion() {
        let mut core = core(&["cargo nextest run"], None);
        type_str(&mut core, "cargo ");
        assert!(core.suggestion_suffix().is_some());
        assert_eq!(core.handle_key(Key::Left(false)), CoreAction::Continue);
        assert_eq!(core.suggestion_suffix(), None, "clear on cursor move");
    }

    #[test]
    fn right_at_end_accepts_full_suggestion() {
        let mut core = core(&["cargo nextest run"], None);
        type_str(&mut core, "cargo ");
        assert_eq!(core.handle_key(Key::Right(false)), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "cargo nextest run");
        assert_eq!(core.suggestion_suffix(), None);
    }

    #[test]
    fn tab_accepts_full_suggestion() {
        let mut core = core(&["cargo nextest run"], None);
        type_str(&mut core, "cargo");
        assert_eq!(core.handle_key(Key::Tab), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "cargo nextest run");
    }

    #[test]
    fn ctrl_right_accepts_one_word_at_a_time() {
        let mut core = core(&["cargo nextest run"], None);
        type_str(&mut core, "cargo ");
        // "nextest " accepted first…
        assert_eq!(core.handle_key(Key::Right(true)), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "cargo nextest ");
        assert_eq!(core.suggestion_suffix(), Some("run"));
        // …then "run".
        assert_eq!(core.handle_key(Key::AltF), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "cargo nextest run");
        assert_eq!(core.suggestion_suffix(), None);
    }

    #[test]
    fn partial_word_accepts_leading_whitespace_run() {
        assert_eq!(partial_word_bytes(" nextest run"), " ".len());
        assert_eq!(partial_word_bytes("nextest run"), "nextest ".len());
        assert_eq!(partial_word_bytes("nextest"), "nextest".len());
        assert_eq!(partial_word_bytes(""), 0);
    }

    #[test]
    fn up_browses_prefix_matches_newest_first() {
        let mut core = core(&["cargo test", "git status", "cargo nextest run"], None);
        type_str(&mut core, "cargo");
        assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "cargo nextest run");
        assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "cargo test");
        assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "cargo test", "oldest stop");
    }

    #[test]
    fn down_restores_typed_prefix_after_newest() {
        let mut core = core(&["cargo test", "cargo nextest run"], None);
        type_str(&mut core, "cargo");
        feed(&mut core, &[Key::Up, Key::Down, Key::Down]);
        assert_eq!(buffer_str(&core), "cargo", "prefix restored");
    }

    #[test]
    fn up_with_empty_buffer_walks_all_history() {
        let mut core = core(&["one", "two"], None);
        assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "two");
        assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "one");
    }

    #[test]
    fn editing_keys_modify_buffer() {
        let mut core = core(&[], None);
        type_str(&mut core, "hello world");
        feed(&mut core, &[Key::CtrlA, Key::CtrlK]);
        assert_eq!(buffer_str(&core), "");
        type_str(&mut core, "abc def");
        assert_eq!(core.handle_key(Key::CtrlW), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "abc ");
        assert_eq!(core.handle_key(Key::CtrlU), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "");
    }

    #[test]
    fn ctrl_d_semantics() {
        let mut core = core(&[], None);
        assert_eq!(core.handle_key(Key::CtrlD), CoreAction::Eof);
        type_str(&mut core, "ab");
        feed(&mut core, &[Key::Left(false), Key::Left(false)]);
        assert_eq!(core.handle_key(Key::CtrlD), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "b");
    }

    #[test]
    fn ctrl_c_interrupts_and_ctrl_l_clears() {
        let mut core = core(&[], None);
        assert_eq!(core.handle_key(Key::CtrlC), CoreAction::Interrupt);
        assert_eq!(core.handle_key(Key::CtrlL), CoreAction::ClearScreen);
    }

    #[test]
    fn suggestion_lifecycle_through_word_kill() {
        let mut core = core(&["cargo nextest run"], None);
        type_str(&mut core, "cargo nextest run extra");
        // Delete the last word with Ctrl-W; the prefix suggests again.
        assert_eq!(core.handle_key(Key::CtrlW), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "cargo nextest run ");
        assert_eq!(core.suggestion_suffix(), None);
    }

    #[test]
    fn cursor_reposition_width_counts_wide_chars() {
        let mut core = core(&[], None);
        type_str(&mut core, "你好");
        feed(&mut core, &[Key::Left(false), Key::Left(false)]);
        assert_eq!(core.width_right_of_cursor(), 4, "CJK chars are width 2");
    }

    #[test]
    fn paste_inserts_whole_and_flattens_newlines() {
        let mut core = core(&[], None);
        core.insert_paste("hello\nworld\r\ntail");
        assert_eq!(buffer_str(&core), "hello world tail");
        assert_eq!(core.cursor, "hello world tail".chars().count());
    }

    #[test]
    fn alt_word_keys_edit_by_word() {
        let mut core = core(&[], None);
        type_str(&mut core, "hello world");
        assert_eq!(core.handle_key(Key::AltB), CoreAction::Continue);
        assert_eq!(core.cursor, 6);
        assert_eq!(core.handle_key(Key::AltD), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "hello ");
    }

    #[test]
    fn alt_backspace_kills_word_backward() {
        let mut core = core(&[], None);
        type_str(&mut core, "hello world");
        assert_eq!(core.handle_key(Key::AltBackspace), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "hello ");
    }

    #[test]
    fn alt_dot_inserts_history_tail_word() {
        let mut core = core(&["git commit -m msg"], Some("git status"));
        assert_eq!(core.handle_key(Key::AltDot), CoreAction::Continue);
        assert_eq!(buffer_str(&core), "status");
    }

    #[test]
    fn wrap_layout_counts_rows_by_width() {
        let mut core = core(&[], None);
        core.set_term_cols(10);
        type_str(&mut core, "123456789012345");
        assert!(core.full_line_width() > 10);
        assert_eq!(core.width_left_of_cursor(), 2 + 15);
    }
}
