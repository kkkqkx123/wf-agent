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
    /// Copy text to the system clipboard through OSC 52. The caller owns
    /// policy (capability gate, TTY check, sanitization); this only emits
    /// the byte sequence through the owned writer.
    fn copy_to_clipboard(&mut self, text: &str) -> io::Result<()>;
}

/// Sanitize a copy payload: strip NUL bytes (terminals reject the whole
/// sequence on embedded NULs) and reject empty or whitespace-only input so
/// meaningless writes never produce bytes. Returns the text to emit.
pub fn sanitize_copy_text(text: &str) -> Option<String> {
    let stripped: String = text.chars().filter(|c| *c != '\0').collect();
    if stripped.trim().is_empty() {
        return None;
    }
    Some(stripped)
}

/// Recording control plane for unit tests: every operation is appended to
/// `ops` and always succeeds. Copied payloads are kept in `copied` in
/// emission order; `ops` only records the `"copy_to_clipboard"` marker so
/// the relative order against enter / exit ops stays assertable.
#[derive(Debug, Default)]
pub struct FakeControl {
    pub ops: Vec<&'static str>,
    pub copied: Vec<String>,
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
    fn copy_to_clipboard(&mut self, text: &str) -> io::Result<()> {
        self.ops.push("copy_to_clipboard");
        self.copied.push(text.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_empty_and_whitespace() {
        assert_eq!(sanitize_copy_text(""), None);
        assert_eq!(sanitize_copy_text("   \n\t  "), None);
        assert_eq!(sanitize_copy_text("hello"), Some("hello".to_string()));
        assert_eq!(
            sanitize_copy_text("  hello  "),
            Some("  hello  ".to_string())
        );
    }

    #[test]
    fn sanitize_strips_nul_bytes() {
        assert_eq!(sanitize_copy_text("a\0b\0c"), Some("abc".to_string()));
        assert_eq!(sanitize_copy_text("\0\0"), None);
    }

    #[test]
    fn fake_records_copy_payloads_in_order() {
        let mut fake = FakeControl::default();
        fake.enable_raw().expect("fake always succeeds");
        fake.copy_to_clipboard("first")
            .expect("fake always succeeds");
        fake.copy_to_clipboard("second")
            .expect("fake always succeeds");
        fake.disable_raw().expect("fake always succeeds");
        assert_eq!(
            fake.ops,
            vec![
                "enable_raw",
                "copy_to_clipboard",
                "copy_to_clipboard",
                "disable_raw",
            ]
        );
        assert_eq!(fake.copied, vec!["first".to_string(), "second".to_string()]);
    }
}
