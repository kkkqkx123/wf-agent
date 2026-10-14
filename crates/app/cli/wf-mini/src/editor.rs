//! Raw-mode prompt editor: single-line editing with ghost-text suggestions.
//!
//! The editor borrows the stdin byte channel for the duration of one
//! prompt, mirrors the zsh-autosuggestions interaction model on a plain
//! terminal, and hands the channel back afterwards:
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
//! a terminal); `run_blocking` wraps it with the crossterm renderer, raw
//! mode and the Drop-guarded teardown. Non-TTY sessions never reach this
//! module — the session falls back to the cooked [`crate::input::LineReader`].
//!
//! Known limitation: rendering assumes the edited line fits on one terminal
//! row (long input wraps and redraw degrades), and a lone Escape waits for
//! the next byte instead of timing out — both acceptable for short prompts.

use std::io::{self, Write};
use std::sync::Arc;

use crossterm::cursor::MoveToColumn;
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use unicode_width::UnicodeWidthChar;

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
    Unknown,
}

/// Incremental byte-to-key decoder across chunk boundaries. Escape
/// sequences and multi-byte UTF-8 characters may arrive split; `next_key`
/// returns `None` until enough bytes are buffered.
#[derive(Debug, Default)]
pub struct KeyDecoder {
    buf: Vec<u8>,
}

impl KeyDecoder {
    pub fn feed(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    /// Decode the next complete key, or `None` while more bytes are needed.
    pub fn next_key(&mut self) -> Option<Key> {
        if self.buf.is_empty() {
            return None;
        }
        let lead = self.buf[0];
        match lead {
            0x1b => self.decode_escape(),
            0x0d | 0x0a => {
                self.buf.drain(..1);
                Some(Key::Enter)
            }
            0x09 => {
                self.buf.drain(..1);
                Some(Key::Tab)
            }
            0x7f | 0x08 => {
                self.buf.drain(..1);
                Some(Key::Backspace)
            }
            0x03 => {
                self.buf.drain(..1);
                Some(Key::CtrlC)
            }
            0x04 => {
                self.buf.drain(..1);
                Some(Key::CtrlD)
            }
            0x01 => {
                self.buf.drain(..1);
                Some(Key::CtrlA)
            }
            0x05 => {
                self.buf.drain(..1);
                Some(Key::CtrlE)
            }
            0x0b => {
                self.buf.drain(..1);
                Some(Key::CtrlK)
            }
            0x0c => {
                self.buf.drain(..1);
                Some(Key::CtrlL)
            }
            0x15 => {
                self.buf.drain(..1);
                Some(Key::CtrlU)
            }
            0x17 => {
                self.buf.drain(..1);
                Some(Key::CtrlW)
            }
            0x00..=0x1f => {
                self.buf.drain(..1);
                Some(Key::Unknown)
            }
            0x20..=0x7e => {
                self.buf.drain(..1);
                Some(Key::Char(lead as char))
            }
            _ => self.decode_utf8(),
        }
    }

    /// CSI / SS3 / Alt-prefixed sequences after a lone ESC byte.
    fn decode_escape(&mut self) -> Option<Key> {
        if self.buf.len() < 2 {
            // A lone ESC could still grow into a sequence; wait for it.
            return None;
        }
        match self.buf[1] {
            b'[' => self.decode_csi(),
            b'O' => {
                if self.buf.len() < 3 {
                    return None;
                }
                let key = match self.buf[2] {
                    b'A' => Key::Up,
                    b'B' => Key::Down,
                    b'C' => Key::Right(false),
                    b'D' => Key::Left(false),
                    b'H' => Key::Home,
                    b'F' => Key::End,
                    _ => Key::Unknown,
                };
                self.buf.drain(..3);
                Some(key)
            }
            // Alt-modified keys arrive as ESC + byte; only Alt-f is bound.
            b'f' => {
                self.buf.drain(..2);
                Some(Key::AltF)
            }
            _ => {
                self.buf.drain(..2);
                Some(Key::Unknown)
            }
        }
    }

    /// CSI: `ESC [ <params> <final>` with the final byte in `0x40..=0x7E`.
    fn decode_csi(&mut self) -> Option<Key> {
        let end = self.buf[2..]
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))?
            + 2;
        let params = String::from_utf8_lossy(&self.buf[2..end]).to_string();
        let final_byte = self.buf[end];
        self.buf.drain(..=end);
        // Modifier 5 = Ctrl in xterm parameter encoding (`1;5C`).
        let ctrl = params.split(';').any(|part| part.trim() == "5");
        let key = match final_byte {
            b'A' => Key::Up,
            b'B' => Key::Down,
            b'C' => Key::Right(ctrl),
            b'D' => Key::Left(ctrl),
            b'H' => Key::Home,
            b'F' => Key::End,
            b'~' => match params.as_str() {
                "1" | "7" => Key::Home,
                "4" | "8" => Key::End,
                "3" => Key::Delete,
                _ => Key::Unknown,
            },
            _ => Key::Unknown,
        };
        Some(key)
    }

    /// Multi-byte UTF-8: wait for the full sequence, resync by dropping the
    /// lead byte when it is invalid.
    fn decode_utf8(&mut self) -> Option<Key> {
        let lead = self.buf[0];
        let len = match lead {
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => {
                self.buf.drain(..1);
                return Some(Key::Unknown);
            }
        };
        if self.buf.len() < len {
            return None;
        }
        match std::str::from_utf8(&self.buf[..len]) {
            Ok(text) => {
                let c = text.chars().next().expect("non-empty valid utf8");
                self.buf.drain(..len);
                Some(Key::Char(c))
            }
            Err(_) => {
                self.buf.drain(..1);
                Some(Key::Unknown)
            }
        }
    }
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
    pub fn new(snapshot: EditorSnapshot, engine: Arc<SuggestEngine>, broker: Arc<SuggestBroker>) -> Self {
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
            Key::Left(true) => {
                self.word_left();
                self.clear_suggestion();
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
                prefix
                    .iter()
                    .all(|p| chars.next().is_some_and(|c| c == *p))
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
fn partial_word_bytes(suffix: &str) -> usize {
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

/// Run one prompt on the raw terminal. Takes over the stdin receiver,
/// enables raw mode (flushing any stale half-typed line first), and always
/// restores the terminal — the guard also covers panics/unwinds.
pub fn run_blocking(
    rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    snapshot: EditorSnapshot,
    engine: Arc<SuggestEngine>,
    broker: Arc<SuggestBroker>,
) -> (PromptEvent, tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>) {
    let mut core = EditorCore::new(snapshot, engine, broker);
    let raw = enable_raw_mode().is_ok();
    if raw {
        // Drop the half-typed line the tty driver may still hold from the
        // previous cooked phase (typed mid-turn); the channel-side drain
        // already dropped the complete lines.
        // SAFETY: fd 0 is a valid descriptor for the process lifetime.
        unsafe { libc::tcflush(0, libc::TCIFLUSH) };
    }
    let guard = RawGuard { active: raw };
    let mut rx = Some(rx);
    let event = edit_loop(&mut rx, &mut core, &guard);
    drop(guard);
    (event, rx.expect("receiver returned by edit_loop"))
}

fn edit_loop(
    rx: &mut Option<tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>>,
    core: &mut EditorCore,
    _guard: &RawGuard,
) -> PromptEvent {
    let mut out = io::stderr();
    let mut decoder = KeyDecoder::default();
    redraw(&mut out, core);
    loop {
        let Some(chunk) = rx.as_mut().and_then(|rx| rx.blocking_recv()) else {
            return PromptEvent::Eof;
        };
        decoder.feed(&chunk);
        while let Some(key) = decoder.next_key() {
            match core.handle_key(key) {
                CoreAction::Continue => redraw(&mut out, core),
                CoreAction::ClearScreen => {
                    let _ = crossterm::execute!(
                        out,
                        Clear(ClearType::All),
                        crossterm::cursor::MoveTo(0, 0)
                    );
                    redraw(&mut out, core);
                }
                terminal => {
                    let _ = out.write_all(b"\r\n");
                    let _ = out.flush();
                    return match terminal {
                        CoreAction::Submit => PromptEvent::Line(core.line()),
                        CoreAction::Eof => PromptEvent::Eof,
                        CoreAction::Interrupt => PromptEvent::Interrupt,
                        CoreAction::Continue | CoreAction::ClearScreen => unreachable!(
                            "non-terminal actions handled above"
                        ),
                    };
                }
            }
        }
    }
}

/// Restores cooked mode on drop, including across panics/unwinds.
struct RawGuard {
    active: bool,
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = disable_raw_mode();
        }
    }
}

/// Redraw the whole prompt line: clear, prompt, buffer, dim suggestion,
/// then move the cursor back to its position.
fn redraw(out: &mut impl Write, core: &EditorCore) {
    let _ = crossterm::queue!(out, MoveToColumn(0), Clear(ClearType::UntilNewLine));
    let _ = out.write_all(core.prompt.as_bytes());
    let buffer: String = core.buffer().iter().collect();
    let _ = out.write_all(buffer.as_bytes());
    if let Some(suffix) = core.suggestion_suffix() {
        let _ = crossterm::queue!(out, SetForegroundColor(Color::DarkGrey));
        let _ = out.write_all(suffix.as_bytes());
        let _ = crossterm::queue!(out, ResetColor);
    }
    let back = core.width_right_of_cursor();
    if back > 0 {
        let _ = crossterm::queue!(out, crossterm::cursor::MoveLeft(back as u16));
    }
    let _ = out.flush();
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
    fn decoder_plain_chars_and_enter() {
        let mut decoder = KeyDecoder::default();
        decoder.feed(b"ab\r");
        assert_eq!(decoder.next_key(), Some(Key::Char('a')));
        assert_eq!(decoder.next_key(), Some(Key::Char('b')));
        assert_eq!(decoder.next_key(), Some(Key::Enter));
        assert_eq!(decoder.next_key(), None);
    }

    #[test]
    fn decoder_escape_sequences() {
        let mut decoder = KeyDecoder::default();
        decoder.feed(b"\x1b[A\x1b[B\x1b[C\x1b[D\x1b[H\x1b[F\x1b[3~\x1b[1;5C\x1bOF");
        assert_eq!(decoder.next_key(), Some(Key::Up));
        assert_eq!(decoder.next_key(), Some(Key::Down));
        assert_eq!(decoder.next_key(), Some(Key::Right(false)));
        assert_eq!(decoder.next_key(), Some(Key::Left(false)));
        assert_eq!(decoder.next_key(), Some(Key::Home));
        assert_eq!(decoder.next_key(), Some(Key::End));
        assert_eq!(decoder.next_key(), Some(Key::Delete));
        assert_eq!(decoder.next_key(), Some(Key::Right(true)));
        assert_eq!(decoder.next_key(), Some(Key::End));
    }

    #[test]
    fn decoder_waits_for_split_escape_sequence() {
        let mut decoder = KeyDecoder::default();
        decoder.feed(&[0x1b]);
        assert_eq!(decoder.next_key(), None, "lone ESC waits");
        decoder.feed(&[b'[', b'C']);
        assert_eq!(decoder.next_key(), Some(Key::Right(false)));
    }

    #[test]
    fn decoder_handles_utf8_split_across_feeds() {
        let mut decoder = KeyDecoder::default();
        let text = "你好";
        let bytes = text.as_bytes();
        decoder.feed(&bytes[..2]);
        assert_eq!(decoder.next_key(), None, "incomplete sequence waits");
        decoder.feed(&bytes[2..]);
        assert_eq!(decoder.next_key(), Some(Key::Char('你')));
        assert_eq!(decoder.next_key(), Some(Key::Char('好')));
    }

    #[test]
    fn decoder_invalid_utf8_resyncs() {
        let mut decoder = KeyDecoder::default();
        decoder.feed(&[0xff, b'x']);
        assert_eq!(decoder.next_key(), Some(Key::Unknown));
        assert_eq!(decoder.next_key(), Some(Key::Char('x')));
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
        feed(&mut core, &[
            Key::CtrlA,
            Key::CtrlK,
        ]);
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
}
