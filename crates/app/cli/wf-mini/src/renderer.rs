//! Printf-style SGR renderer for the mini TUI.
//!
//! Writes directly to stdout using escape sequences, avoiding any
//! cell-buffer or ratatui dependency. The layout is:
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │ History (scrollback)                │ ← fills upper space
//! ├─────────────────────────────────────┤
//! │ model-name | tokens | status        │ ← status line (fixed)
//! │ > Input (input line)               │ ← input line (fixed)
//! └─────────────────────────────────────┘
//! ```

use std::io::{self, Write};

use crate::scrollback::Role;

// ── SGR color constants ───────────────────────────────────────────────

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const FG_DEFAULT: &str = "\x1b[39m";
const FG_RED: &str = "\x1b[31m";
const FG_GREEN: &str = "\x1b[32m";
const FG_YELLOW: &str = "\x1b[33m";
const FG_CYAN: &str = "\x1b[36m";
const FG_BRIGHT_BLACK: &str = "\x1b[90m";

/// Map a semantic role to an SGR foreground color sequence.
pub fn role_to_sgr(role: Role) -> &'static str {
    match role {
        Role::Default => FG_DEFAULT,
        Role::Muted => FG_BRIGHT_BLACK,
        Role::Accent => FG_CYAN,
        Role::Add => FG_GREEN,
        Role::Remove => FG_RED,
        Role::Warning => FG_YELLOW,
        Role::Error => FG_RED,
    }
}

// ── Terminal writer ───────────────────────────────────────────────────

/// Low-level terminal writer that manages cursor and clear operations.
pub struct TerminalWriter {
    stdout: io::Stdout,
}

impl TerminalWriter {
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
        }
    }

    /// Move cursor to (col, row) (0-indexed).
    pub fn move_to(&mut self, col: u16, row: u16) -> io::Result<()> {
        write!(self.stdout, "\x1b[{};{}H", row + 1, col + 1)
    }

    /// Clear the entire screen.
    pub fn clear_screen(&mut self) -> io::Result<()> {
        write!(self.stdout, "\x1b[2J")
    }

    /// Clear from cursor to end of line.
    pub fn clear_to_eol(&mut self) -> io::Result<()> {
        write!(self.stdout, "\x1b[K")
    }

    /// Hide the cursor.
    pub fn hide_cursor(&mut self) -> io::Result<()> {
        write!(self.stdout, "\x1b[?25l")
    }

    /// Show the cursor.
    pub fn show_cursor(&mut self) -> io::Result<()> {
        write!(self.stdout, "\x1b[?25h")
    }

    /// Flush stdout.
    pub fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }

    /// Write a raw string.
    pub fn write_str(&mut self, s: &str) -> io::Result<()> {
        self.stdout.write_all(s.as_bytes())
    }

    /// Write a byte.
    pub fn write_byte(&mut self, b: u8) -> io::Result<()> {
        self.stdout.write_all(&[b])
    }
}

// ── Renderer ──────────────────────────────────────────────────────────

/// Mini TUI renderer: draws the scrollback, status line and input line.
pub struct Renderer {
    writer: TerminalWriter,
    width: u16,
    height: u16,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            writer: TerminalWriter::new(),
            width: 80,
            height: 24,
        }
    }

    /// Update the cached terminal size.
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    /// The total terminal width.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// The total terminal height.
    pub fn height(&self) -> u16 {
        self.height
    }

    /// Number of rows available for the scrollback (above status + input).
    pub fn scrollback_rows(&self) -> usize {
        self.height.saturating_sub(2) as usize // status + input = 2 rows
    }

    /// Perform a full redraw: clear screen, draw all regions, position
    /// cursor on the input line.
    pub fn draw_full(
        &mut self,
        scrollback_visible: &[(Role, String)],
        status_text: &str,
        input_text: &str,
        cursor_col: u16,
    ) -> io::Result<()> {
        self.writer.clear_screen()?;

        let w = self.width as usize;

        // Draw scrollback lines.
        for (i, (role, text)) in scrollback_visible.iter().enumerate() {
            self.writer.move_to(0, i as u16)?;
            self.writer.write_str(role_to_sgr(*role))?;
            // Truncate to terminal width.
            let truncated: String = text.chars().take(w).collect();
            self.writer.write_str(&truncated)?;
            self.writer.write_str(RESET)?;
            self.writer.clear_to_eol()?;
        }

        // Draw status line (second-to-last row).
        let status_row = self.height.saturating_sub(2);
        self.writer.move_to(0, status_row)?;
        self.writer.write_str(DIM)?;
        self.writer.write_str(status_text)?;
        self.writer.write_str(RESET)?;
        self.writer.clear_to_eol()?;

        // Draw input line (last row).
        let input_row = self.height.saturating_sub(1);
        self.writer.move_to(0, input_row)?;
        self.writer.write_str(BOLD)?;
        self.writer.write_str("> ")?;
        self.writer.write_str(RESET)?;
        self.writer.write_str(input_text)?;
        self.writer.clear_to_eol()?;

        // Position the cursor on the input line.
        let cursor_x = 2 + cursor_col.min((w as u16).saturating_sub(2));
        self.writer.move_to(cursor_x, input_row)?;
        self.writer.show_cursor()?;

        self.writer.flush()
    }

    /// Draw only the streaming tail line (append to scrollback region).
    /// Used during streaming to avoid full redraws.
    pub fn draw_streaming_tail(
        &mut self,
        role: Role,
        text: &str,
        row_offset: u16,
    ) -> io::Result<()> {
        let w = self.width as usize;
        self.writer.move_to(0, row_offset)?;
        self.writer.write_str(role_to_sgr(role))?;
        let truncated: String = text.chars().take(w).collect();
        self.writer.write_str(&truncated)?;
        self.writer.write_str(RESET)?;
        self.writer.clear_to_eol()?;
        self.writer.flush()
    }

    /// Erase the streaming tail line (restore to blank).
    pub fn clear_row(&mut self, row: u16) -> io::Result<()> {
        self.writer.move_to(0, row)?;
        self.writer.clear_to_eol()?;
        self.writer.flush()
    }

    /// Hide the cursor.
    pub fn hide_cursor(&mut self) -> io::Result<()> {
        self.writer.hide_cursor()
    }

    /// Show the cursor.
    pub fn show_cursor(&mut self) -> io::Result<()> {
        self.writer.show_cursor()
    }

    /// Reset the terminal to a clean state.
    pub fn restore(&mut self) -> io::Result<()> {
        self.writer.show_cursor()?;
        self.writer.write_str(RESET)?;
        self.writer.flush()
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_to_sgr_returns_valid_seqs() {
        assert_eq!(role_to_sgr(Role::Default), FG_DEFAULT);
        assert_eq!(role_to_sgr(Role::Error), FG_RED);
        assert_eq!(role_to_sgr(Role::Add), FG_GREEN);
        assert_eq!(role_to_sgr(Role::Muted), FG_BRIGHT_BLACK);
    }

    #[test]
    fn scrollback_rows_excludes_status_and_input() {
        let mut r = Renderer::new();
        r.set_size(80, 24);
        assert_eq!(r.scrollback_rows(), 22);
    }
}
