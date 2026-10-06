//! Abstract terminal control plane (injectable for tests) and its
//! recording test double.

use std::fmt;
use std::io;

/// Abstract terminal control plane (injectable for tests).
pub trait TerminalControl: fmt::Debug {
    fn enable_raw(&mut self) -> io::Result<()>;
    fn disable_raw(&mut self) -> io::Result<()>;
    fn enter_alt_screen(&mut self) -> io::Result<()>;
    fn leave_alt_screen(&mut self) -> io::Result<()>;
    fn enable_bracketed_paste(&mut self) -> io::Result<()>;
    fn disable_bracketed_paste(&mut self) -> io::Result<()>;
    fn hide_cursor(&mut self) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
    fn enable_focus_change(&mut self) -> io::Result<()>;
    fn disable_focus_change(&mut self) -> io::Result<()>;
    fn enable_mouse_capture(&mut self) -> io::Result<()>;
    fn disable_mouse_capture(&mut self) -> io::Result<()>;
    fn enable_alternate_scroll(&mut self) -> io::Result<()>;
    fn disable_alternate_scroll(&mut self) -> io::Result<()>;
    /// Push the enhancement flags onto the terminal stack (enter path).
    fn push_keyboard_enhancement(&mut self) -> io::Result<()>;
    /// Pop one enhancement level (restore path).
    fn pop_keyboard_enhancement(&mut self) -> io::Result<()>;
    /// Rewrite the enhancement flags without touching the stack
    /// (mid-run reassert path, keeps push/pop balanced).
    fn set_keyboard_enhancement(&mut self) -> io::Result<()>;
    /// Strong reset for exit paths: pop, then clear any leaked level so the
    /// parent shell never inherits enhanced reporting.
    fn reset_keyboard_enhancement(&mut self) -> io::Result<()>;
}

/// Recording control plane for unit tests: every operation is appended to
/// `ops` and always succeeds.
#[derive(Debug, Default)]
pub struct FakeControl {
    pub ops: Vec<&'static str>,
}

impl TerminalControl for FakeControl {
    fn enable_raw(&mut self) -> io::Result<()> {
        self.ops.push("enable_raw");
        Ok(())
    }
    fn disable_raw(&mut self) -> io::Result<()> {
        self.ops.push("disable_raw");
        Ok(())
    }
    fn enter_alt_screen(&mut self) -> io::Result<()> {
        self.ops.push("enter_alt_screen");
        Ok(())
    }
    fn leave_alt_screen(&mut self) -> io::Result<()> {
        self.ops.push("leave_alt_screen");
        Ok(())
    }
    fn enable_bracketed_paste(&mut self) -> io::Result<()> {
        self.ops.push("enable_bracketed_paste");
        Ok(())
    }
    fn disable_bracketed_paste(&mut self) -> io::Result<()> {
        self.ops.push("disable_bracketed_paste");
        Ok(())
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.ops.push("hide_cursor");
        Ok(())
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.ops.push("show_cursor");
        Ok(())
    }
    fn enable_focus_change(&mut self) -> io::Result<()> {
        self.ops.push("enable_focus_change");
        Ok(())
    }
    fn disable_focus_change(&mut self) -> io::Result<()> {
        self.ops.push("disable_focus_change");
        Ok(())
    }
    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        self.ops.push("enable_mouse_capture");
        Ok(())
    }
    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        self.ops.push("disable_mouse_capture");
        Ok(())
    }
    fn enable_alternate_scroll(&mut self) -> io::Result<()> {
        self.ops.push("enable_alternate_scroll");
        Ok(())
    }
    fn disable_alternate_scroll(&mut self) -> io::Result<()> {
        self.ops.push("disable_alternate_scroll");
        Ok(())
    }
    fn push_keyboard_enhancement(&mut self) -> io::Result<()> {
        self.ops.push("push_keyboard_enhancement");
        Ok(())
    }
    fn pop_keyboard_enhancement(&mut self) -> io::Result<()> {
        self.ops.push("pop_keyboard_enhancement");
        Ok(())
    }
    fn set_keyboard_enhancement(&mut self) -> io::Result<()> {
        self.ops.push("set_keyboard_enhancement");
        Ok(())
    }
    fn reset_keyboard_enhancement(&mut self) -> io::Result<()> {
        self.ops.push("reset_keyboard_enhancement");
        Ok(())
    }
}
