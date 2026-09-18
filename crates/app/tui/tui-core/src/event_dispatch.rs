//! Concrete event types for the TUI.
//!
//! Production input flows through the shell's explicit `match` on [`EventType`];
//! no handler registry or middleware chain exists here by design, so the only
//! dynamic dispatch in the TUI remains the modal stack (see
//! `tui-components::modal` for the written rationale).

use crate::keymap::Key;

/// Event types routed to the shell.
pub enum EventType {
    /// A terminal key press.
    Key(Key),
    /// Terminal resize.
    Resize(u16, u16),
}

/// Result of event handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    /// Event was consumed; stop propagation.
    Consumed,
    /// Event was not handled; continue to next handler.
    Ignored,
    /// A redraw is needed after this event.
    NeedRedraw,
    /// The application should exit.
    Exit,
}

/// Label for debug logging without allocating in production paths.
pub fn event_label(event: &EventType) -> String {
    match event {
        EventType::Key(k) => format!("key({:?})", k.code),
        EventType::Resize(w, h) => format!("resize({w}x{h})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::CKey;

    fn test_key() -> EventType {
        EventType::Key(Key {
            code: CKey::Char('a'),
            ctrl: false,
            alt: false,
            shift: false,
        })
    }

    #[test]
    fn event_label_covers_key_and_resize() {
        assert!(event_label(&test_key()).starts_with("key("));
        assert_eq!(event_label(&EventType::Resize(80, 24)), "resize(80x24)");
    }
}
