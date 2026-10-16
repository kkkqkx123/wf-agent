//! Raw-terminal prompt loop: event source, raw mode, bracketed paste and
//! redraw.

use std::io::{self, Write};
use std::sync::Arc;

use crossterm::cursor::{Hide, MoveLeft, MoveTo, MoveToColumn, MoveUp, Show};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste, Event, EventStream};
use crossterm::style::Color;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use futures::StreamExt;

use crate::suggest::{SuggestBroker, SuggestEngine};

use super::core::{CoreAction, EditorCore, EditorSnapshot, PromptEvent};
use super::keymap::{map_key_event, Key};

/// Current terminal width for prompt layout; zero on failure so callers can
/// fall back to single-row rendering.
pub fn terminal_cols() -> u16 {
    crossterm::terminal::size()
        .map(|(cols, _)| cols)
        .unwrap_or(0)
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
            Event::Mouse(mouse) => {
                // No mouse capture is enabled in mini: button presses stay
                // native. Wheel notches (when the terminal forwards them)
                // browse history like Up/Down without touching selection.
                use crossterm::event::MouseEventKind;
                let key = match mouse.kind {
                    MouseEventKind::ScrollUp => Some(Key::Up),
                    MouseEventKind::ScrollDown => Some(Key::Down),
                    _ => None,
                };
                if let Some(key) = key {
                    match core.handle_key(key) {
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
