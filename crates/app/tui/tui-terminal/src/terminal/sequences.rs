//! Byte-sequence writers: the single choke points for the enter, restore and
//! reassert paths so tests can assert the exact bytes for each switch
//! combination.

use std::io::{self, Write};

use crossterm::QueueableCommand;

use super::commands::{
    keyboard_enhancement_flags, DisableAlternateScroll, EnableAlternateScroll,
    SetKeyboardEnhancementFlags,
};
use super::modes::TerminalModes;

/// Byte sequences emitted when entering `modes` (raw mode excluded: it is
/// termios state, not output bytes). Single choke point for the enter path
/// so tests can assert the exact bytes for each switch combination.
pub fn write_enter_sequences(writer: &mut impl Write, modes: TerminalModes) -> io::Result<()> {
    use crossterm::event::PushKeyboardEnhancementFlags;
    use crossterm::event::{EnableBracketedPaste, EnableFocusChange, EnableMouseCapture};
    if modes.cursor_hidden {
        writer.queue(crossterm::cursor::Hide)?;
    }
    if modes.alt_screen {
        writer.queue(crossterm::terminal::EnterAlternateScreen)?;
    }
    if modes.bracketed_paste {
        writer.queue(EnableBracketedPaste)?;
    }
    if modes.focus_change {
        writer.queue(EnableFocusChange)?;
    }
    if modes.mouse_capture {
        writer.queue(EnableMouseCapture)?;
    }
    if modes.alternate_scroll {
        writer.queue(EnableAlternateScroll)?;
    }
    if modes.keyboard_enhancement {
        writer.queue(PushKeyboardEnhancementFlags(keyboard_enhancement_flags()))?;
    }
    writer.flush()
}

/// Byte sequences emitted when restoring from `modes` towards all-off, in
/// reverse enter order (raw mode excluded).
pub fn write_restore_sequences(writer: &mut impl Write, modes: TerminalModes) -> io::Result<()> {
    use crossterm::event::PopKeyboardEnhancementFlags;
    use crossterm::event::{DisableBracketedPaste, DisableFocusChange, DisableMouseCapture};
    if modes.keyboard_enhancement {
        writer.queue(PopKeyboardEnhancementFlags)?;
    }
    if modes.alternate_scroll {
        writer.queue(DisableAlternateScroll)?;
    }
    if modes.mouse_capture {
        writer.queue(DisableMouseCapture)?;
    }
    if modes.focus_change {
        writer.queue(DisableFocusChange)?;
    }
    if modes.bracketed_paste {
        writer.queue(DisableBracketedPaste)?;
    }
    if modes.alt_screen {
        writer.queue(crossterm::terminal::LeaveAlternateScreen)?;
    }
    if modes.cursor_hidden {
        writer.queue(crossterm::cursor::Show)?;
    }
    writer.flush()
}

/// Byte sequences for a mid-run reassert (focus regained, suspend resumed,
/// external editor returned): bracketed paste always on, focus / mouse /
/// alternate scroll per the modes, keyboard enhancement rewritten with the
/// set form so the push/pop stack stays balanced. Never disables anything.
pub fn write_reassert_sequences(writer: &mut impl Write, modes: TerminalModes) -> io::Result<()> {
    use crossterm::event::{EnableBracketedPaste, EnableFocusChange, EnableMouseCapture};
    writer.queue(EnableBracketedPaste)?;
    if modes.focus_change {
        writer.queue(EnableFocusChange)?;
    }
    if modes.mouse_capture {
        writer.queue(EnableMouseCapture)?;
    }
    if modes.alternate_scroll {
        writer.queue(EnableAlternateScroll)?;
    }
    if modes.keyboard_enhancement {
        writer.queue(SetKeyboardEnhancementFlags)?;
    }
    writer.flush()
}
