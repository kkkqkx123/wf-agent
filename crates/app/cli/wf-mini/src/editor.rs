//! Raw-mode prompt editor: multiline-aware editing with ghost-text suggestions.
//!
//! The editor owns the crossterm event stream for the duration of one
//! prompt, mirrors the zsh-autosuggestions interaction model on a plain
//! terminal, and hands the stream back afterwards:
//!
//! - **ghost text** — the suggestion suffix is drawn dim after the cursor
//!   and never enters the buffer (zsh `POSTDISPLAY`,
//!   `widgets.zsh:103`);
//! - **widget action classes** — buffer modifications refetch the
//!   suggestion, cursor movement clears it, `modify/clear/accept/
//!   partial_accept` from `widgets.zsh:63-93`;
//! - **accept** — Right arrow at end of buffer or Tab accepts the whole
//!   suggestion; Ctrl-Right or Alt-f accepts one word (`partial_accept`,
//!   `widgets.zsh:121-142`);
//! - **history browsing** — Up/Down walk prefix-filtered history, restoring
//!   the typed prefix when browsed past the newest match.
//!
//! Structure: [`EditorCore`] is a pure state machine (unit-testable without
//! a terminal); `run_tty` wraps it with the crossterm event source, raw
//! mode, bracketed paste and the Drop-guarded teardown. Non-TTY sessions
//! never reach this module — the session falls back to the cooked
//! [`crate::input::LineReader`] pipe path.

use std::io::{self, Write};
use std::sync::Arc;

use crossterm::cursor::{Hide, MoveLeft, MoveTo, MoveToColumn, MoveUp, Show};
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyCode, KeyEvent,
    KeyModifiers,
};
use crossterm::style::Color;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use futures::StreamExt;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::suggest::{SuggestBroker, SuggestCtx, SuggestEngine};

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
}

/// Terminal keys the editor reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    /// `bool` = Ctrl modifier (Ctrl-Left/Ctrl-Right word motion).
    Left(bool),
    Right(bool),
    Up,
    Down,
    Home,
    End,
    CtrlA,
    CtrlC,
    CtrlD,
    CtrlE,
    CtrlK,
    CtrlL,
    CtrlU,
    CtrlW,
    AltF,
    AltB,
    AltD,
    AltBackspace,
    AltDot,
    Unknown,
}

/// Translate one crossterm key event into the editor key space. Release
/// events are ignored (`None`) so held keys do not double-apply; every
/// other non-bound key maps to [`Key::Unknown`] (redraw, no state change).
pub fn map_key_event(event: KeyEvent) -> Option<Key> {
    use KeyCode as C;
    if event.kind.is_release() {
        return None;
    }
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let key = match event.code {
        C::Enter => Key::Enter,
        C::Tab | C::BackTab => Key::Tab,
        C::Backspace if alt => Key::AltBackspace,
        C::Backspace => Key::Backspace,
        C::Delete => Key::Delete,
        C::Left if ctrl => Key::Left(true),
        C::Left => Key::Left(false),
        C::Right if ctrl => Key::Right(true),
        C::Right => Key::Right(false),
        C::Up => Key::Up,
        C::Down => Key::Down,
        C::Home => Key::Home,
        C::End => Key::End,
        C::Esc => Key::Unknown,
        C::Char(c) if ctrl && !alt => match c.to_ascii_lowercase() {
            'a' => Key::CtrlA,
            'c' => Key::CtrlC,
            'd' => Key::CtrlD,
            'e' => Key::CtrlE,
            'h' => Key::Backspace,
            'k' => Key::CtrlK,
            'l' => Key::CtrlL,
            'u' => Key::CtrlU,
            'w' => Key::CtrlW,
            _ => Key::Unknown,
        },
        C::Char(c) if alt && !ctrl => match c.to_ascii_lowercase() {
            'f' => Key::AltF,
            'b' => Key::AltB,
            'd' => Key::AltD,
            '.' => Key::AltDot,
            _ => Key::Unknown,
        },
        C::Char(c) if !ctrl && !alt => Key::Char(c),
        _ => Key::Unknown,
    };
    Some(key)
}

/// Current terminal width for prompt layout; zero on failure so callers can
/// fall back to single-row rendering.
pub fn terminal_cols() -> u16 {
    crossterm::terminal::size()
        .map(|(cols, _)| cols)
        .unwrap_or(0)
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
            self.browse = None;
            self.buffer = browse.prefix;
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

/// Redraw bookkeeping across frames: rows used by the last frame and the
/// cursor row inside it, so the next frame can return to the block start.
#[derive(Debug, Default)]
struct RedrawState {
    rows: usize,
    cursor_row: usize,
}

/// Run one prompt on the raw terminal. Takes over the event stream for the
/// duration; enables raw mode (flushing any stale half-typed line first)
/// and bracketed paste, and always restores the terminal — the guard also
/// covers panics/unwinds.
pub async fn run_tty(
    mut stream: EventStream,
    snapshot: EditorSnapshot,
    engine: Arc<SuggestEngine>,
    broker: Arc<SuggestBroker>,
) -> (PromptEvent, EventStream) {
    let mut core = EditorCore::new(snapshot, engine, broker);
    let raw = enable_raw_mode().is_ok();
    if raw {
        // Drop the half-typed line the tty driver may still hold from the
        // previous cooked phase (typed mid-turn); the channel-side drain
        // already dropped the complete lines. Crossterm exposes no flush,
        // so the termios call stays behind a unix gate.
        #[cfg(unix)]
        unsafe {
            libc::tcflush(0, libc::TCIFLUSH);
        }
    }
    let mut err = io::stderr();
    let paste = if raw {
        crossterm::execute!(err, EnableBracketedPaste).is_ok()
    } else {
        false
    };
    let guard = RawGuard { active: raw, paste };
    let event = edit_loop(&mut stream, &mut core).await;
    drop(guard);
    let _ = err.flush();
    (event, stream)
}

async fn edit_loop(stream: &mut EventStream, core: &mut EditorCore) -> PromptEvent {
    let mut out = io::stderr();
    let mut state = RedrawState::default();
    redraw(&mut out, core, &mut state);
    loop {
        let Some(next) = stream.next().await else {
            return PromptEvent::Eof;
        };
        let event = match next {
            Ok(event) => event,
            Err(_) => return PromptEvent::Eof,
        };
        match event {
            Event::Key(key) => {
                let Some(mapped) = map_key_event(key) else {
                    continue;
                };
                match core.handle_key(mapped) {
                    CoreAction::Continue => redraw(&mut out, core, &mut state),
                    CoreAction::ClearScreen => {
                        let _ = crossterm::execute!(out, Clear(ClearType::All), MoveTo(0, 0));
                        state = RedrawState::default();
                        redraw(&mut out, core, &mut state);
                    }
                    terminal => {
                        let _ = out.write_all(b"\r\n");
                        let _ = out.flush();
                        return match terminal {
                            CoreAction::Submit => PromptEvent::Line(core.line()),
                            CoreAction::Eof => PromptEvent::Eof,
                            CoreAction::Interrupt => PromptEvent::Interrupt,
                            CoreAction::Continue | CoreAction::ClearScreen => {
                                unreachable!("non-terminal actions handled above")
                            }
                        };
                    }
                }
            }
            Event::Paste(text) => {
                core.insert_paste(&text);
                redraw(&mut out, core, &mut state);
            }
            Event::Resize(cols, _) => {
                if cols > 0 {
                    core.set_term_cols(cols);
                }
                redraw(&mut out, core, &mut state);
            }
            _ => {}
        }
    }
}

/// Restores cooked mode and bracketed paste on drop, including across
/// panics/unwinds.
struct RawGuard {
    active: bool,
    paste: bool,
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let mut err = io::stderr();
        if self.paste {
            let _ = crossterm::execute!(err, DisableBracketedPaste);
        }
        if self.active {
            let _ = disable_raw_mode();
        }
    }
}

/// Redraw the prompt block: return to the block start, clear down, write
/// prompt plus buffer plus dim ghost, then place the cursor by display
/// width. Unknown widths fall back to the legacy single-row path.
fn redraw(out: &mut impl Write, core: &EditorCore, state: &mut RedrawState) {
    let cols = core.term_cols();
    if cols == 0 {
        redraw_single(out, core);
        state.rows = 1;
        state.cursor_row = 0;
        return;
    }
    let cols = cols as usize;
    let _ = crossterm::queue!(out, Hide);
    if state.rows > 0 {
        if state.cursor_row > 0 {
            let _ = crossterm::queue!(out, MoveUp(state.cursor_row as u16));
        }
        let _ = crossterm::queue!(out, MoveToColumn(0), Clear(ClearType::FromCursorDown));
    } else {
        let _ = crossterm::queue!(out, MoveToColumn(0), Clear(ClearType::UntilNewLine));
    }
    let _ = out.write_all(core.prompt_text().as_bytes());
    let buffer: String = core.buffer().iter().collect();
    let _ = out.write_all(buffer.as_bytes());
    if let Some(suffix) = core.suggestion_suffix() {
        let _ = crossterm::queue!(out, crossterm::style::SetForegroundColor(Color::DarkGrey));
        let _ = out.write_all(suffix.as_bytes());
        let _ = crossterm::queue!(out, crossterm::style::ResetColor);
    }
    let full_w = core.full_line_width();
    let rows = full_w.max(1).div_ceil(cols);
    let cursor_w = core.width_left_of_cursor();
    let cursor_row = cursor_w / cols;
    let cursor_col = cursor_w % cols;
    if rows > 1 || cursor_row > 0 {
        let end_row = rows - 1;
        if end_row > cursor_row {
            let _ = crossterm::queue!(out, MoveUp((end_row - cursor_row) as u16));
        }
        let _ = crossterm::queue!(out, MoveToColumn(cursor_col as u16));
    } else {
        let back = core.width_right_of_cursor();
        if back > 0 {
            let _ = crossterm::queue!(out, MoveLeft(back as u16));
        }
    }
    let _ = crossterm::queue!(out, Show);
    let _ = out.flush();
    state.rows = rows.max(1);
    state.cursor_row = cursor_row;
}

/// Legacy single-row redraw for unknown terminal widths.
fn redraw_single(out: &mut impl Write, core: &EditorCore) {
    let _ = crossterm::queue!(out, Hide, MoveToColumn(0), Clear(ClearType::UntilNewLine));
    let _ = out.write_all(core.prompt_text().as_bytes());
    let buffer: String = core.buffer().iter().collect();
    let _ = out.write_all(buffer.as_bytes());
    if let Some(suffix) = core.suggestion_suffix() {
        let _ = crossterm::queue!(out, crossterm::style::SetForegroundColor(Color::DarkGrey));
        let _ = out.write_all(suffix.as_bytes());
        let _ = crossterm::queue!(out, crossterm::style::ResetColor);
    }
    let back = core.width_right_of_cursor();
    if back > 0 {
        let _ = crossterm::queue!(out, MoveLeft(back as u16));
    }
    let _ = crossterm::queue!(out, Show);
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suggest::SuggestEngine;
    use crossterm::event::KeyEventKind;

    fn core(history: &[&str], prev: Option<&str>) -> EditorCore {
        EditorCore::new(
            EditorSnapshot {
                history: history.iter().map(|s| s.to_string()).collect(),
                prev_prompt: prev.map(|s| s.to_string()),
                prompt: "> ".to_string(),
                term_cols: 80,
            },
            Arc::new(SuggestEngine::new()),
            Arc::new(SuggestBroker::new()),
        )
    }

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, KeyEventKind::Press)
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
    fn map_plain_chars_and_enter() {
        assert_eq!(
            map_key_event(key(KeyCode::Char('a'), KeyModifiers::empty())),
            Some(Key::Char('a'))
        );
        assert_eq!(
            map_key_event(key(KeyCode::Enter, KeyModifiers::empty())),
            Some(Key::Enter)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Tab, KeyModifiers::empty())),
            Some(Key::Tab)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Backspace, KeyModifiers::empty())),
            Some(Key::Backspace)
        );
    }

    #[test]
    fn map_control_keys() {
        assert_eq!(
            map_key_event(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Key::CtrlC)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(Key::CtrlD)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            Some(Key::CtrlA)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Char('h'), KeyModifiers::CONTROL)),
            Some(Key::Backspace)
        );
    }

    #[test]
    fn map_arrows_and_modifiers() {
        assert_eq!(
            map_key_event(key(KeyCode::Up, KeyModifiers::empty())),
            Some(Key::Up)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Down, KeyModifiers::empty())),
            Some(Key::Down)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Right, KeyModifiers::empty())),
            Some(Key::Right(false))
        );
        assert_eq!(
            map_key_event(key(KeyCode::Right, KeyModifiers::CONTROL)),
            Some(Key::Right(true))
        );
        assert_eq!(
            map_key_event(key(KeyCode::Left, KeyModifiers::CONTROL)),
            Some(Key::Left(true))
        );
        assert_eq!(
            map_key_event(key(KeyCode::Home, KeyModifiers::empty())),
            Some(Key::Home)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Delete, KeyModifiers::empty())),
            Some(Key::Delete)
        );
    }

    #[test]
    fn map_alt_family() {
        assert_eq!(
            map_key_event(key(KeyCode::Char('f'), KeyModifiers::ALT)),
            Some(Key::AltF)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Char('b'), KeyModifiers::ALT)),
            Some(Key::AltB)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Char('d'), KeyModifiers::ALT)),
            Some(Key::AltD)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Char('.'), KeyModifiers::ALT)),
            Some(Key::AltDot)
        );
        assert_eq!(
            map_key_event(key(KeyCode::Backspace, KeyModifiers::ALT)),
            Some(Key::AltBackspace)
        );
    }

    #[test]
    fn map_ignores_release_and_reports_unknown() {
        let release = KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::empty(),
            KeyEventKind::Release,
        );
        assert_eq!(map_key_event(release), None);
        assert_eq!(
            map_key_event(key(KeyCode::Esc, KeyModifiers::empty())),
            Some(Key::Unknown)
        );
        assert_eq!(
            map_key_event(key(KeyCode::F(1), KeyModifiers::empty())),
            Some(Key::Unknown)
        );
    }

    #[test]
    fn map_utf8_char() {
        assert_eq!(
            map_key_event(key(KeyCode::Char('你'), KeyModifiers::empty())),
            Some(Key::Char('你'))
        );
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
