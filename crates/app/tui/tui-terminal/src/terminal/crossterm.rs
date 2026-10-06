//! Production control plane backed by crossterm (any writer; stdout in
//! production, `Vec<u8>` in tests).

use std::fmt;
use std::io::{self, Write};

use crossterm::ExecutableCommand;

use super::commands::{
    keyboard_enhancement_flags, DisableAlternateScroll, EnableAlternateScroll,
    ResetKeyboardEnhancementFlags,
};
use super::control::TerminalControl;

/// Production control plane backed by crossterm (any writer; stdout in
/// production, `Vec<u8>` in tests).
#[derive(Debug)]
pub struct CrosstermControl<W: Write + fmt::Debug> {
    writer: W,
}

impl<W: Write + fmt::Debug> CrosstermControl<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Borrow the writer (e.g. for additional escape sequences).
    pub fn writer(&mut self) -> &mut W {
        &mut self.writer
    }
}

impl<W: Write + fmt::Debug> TerminalControl for CrosstermControl<W> {
    fn enable_raw(&mut self) -> io::Result<()> {
        crossterm::terminal::enable_raw_mode()
    }
    fn disable_raw(&mut self) -> io::Result<()> {
        crossterm::terminal::disable_raw_mode()
    }
    fn enter_alt_screen(&mut self) -> io::Result<()> {
        self.writer.execute(crossterm::terminal::EnterAlternateScreen)?;
        Ok(())
    }
    fn leave_alt_screen(&mut self) -> io::Result<()> {
        self.writer.execute(crossterm::terminal::LeaveAlternateScreen)?;
        Ok(())
    }
    fn enable_bracketed_paste(&mut self) -> io::Result<()> {
        crossterm::execute!(self.writer, crossterm::event::EnableBracketedPaste)
    }
    fn disable_bracketed_paste(&mut self) -> io::Result<()> {
        crossterm::execute!(self.writer, crossterm::event::DisableBracketedPaste)
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.writer.execute(crossterm::cursor::Hide)?;
        Ok(())
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.writer.execute(crossterm::cursor::Show)?;
        Ok(())
    }
    fn enable_focus_change(&mut self) -> io::Result<()> {
        crossterm::execute!(self.writer, crossterm::event::EnableFocusChange)
    }
    fn disable_focus_change(&mut self) -> io::Result<()> {
        crossterm::execute!(self.writer, crossterm::event::DisableFocusChange)
    }
    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        crossterm::execute!(self.writer, crossterm::event::EnableMouseCapture)
    }
    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        crossterm::execute!(self.writer, crossterm::event::DisableMouseCapture)
    }
    fn enable_alternate_scroll(&mut self) -> io::Result<()> {
        crossterm::execute!(self.writer, EnableAlternateScroll)
    }
    fn disable_alternate_scroll(&mut self) -> io::Result<()> {
        crossterm::execute!(self.writer, DisableAlternateScroll)
    }
    fn push_keyboard_enhancement(&mut self) -> io::Result<()> {
        crossterm::execute!(
            self.writer,
            crossterm::event::PushKeyboardEnhancementFlags(keyboard_enhancement_flags())
        )
    }
    fn pop_keyboard_enhancement(&mut self) -> io::Result<()> {
        crossterm::execute!(self.writer, crossterm::event::PopKeyboardEnhancementFlags)
    }
    fn set_keyboard_enhancement(&mut self) -> io::Result<()> {
        crossterm::execute!(
            self.writer,
            super::commands::SetKeyboardEnhancementFlags
        )
    }
    fn reset_keyboard_enhancement(&mut self) -> io::Result<()> {
        crossterm::execute!(
            self.writer,
            crossterm::event::PopKeyboardEnhancementFlags,
            ResetKeyboardEnhancementFlags
        )
    }
}
