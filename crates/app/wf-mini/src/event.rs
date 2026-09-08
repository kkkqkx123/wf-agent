//! Key mapping for the mini TUI.
//!
//! Converts crossterm key events into a compact [`Key`] struct (modifier-free
//! code + modifier flags) so the rest of the mini TUI never touches
//! crossterm types directly.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Physical key code (modifier-free).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CKey {
    Char(char),
    Enter,
    Esc,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
}

/// A key chord: code plus optional modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: CKey,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Convert a crossterm key event to our internal [`Key`].
pub fn map_key(key: KeyEvent) -> Option<Key> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let code = match key.code {
        KeyCode::Char(c) => CKey::Char(c),
        KeyCode::Enter => CKey::Enter,
        KeyCode::Esc => CKey::Esc,
        KeyCode::Backspace => CKey::Backspace,
        KeyCode::Delete => CKey::Delete,
        KeyCode::Up => CKey::Up,
        KeyCode::Down => CKey::Down,
        KeyCode::Left => CKey::Left,
        KeyCode::Right => CKey::Right,
        KeyCode::Tab => CKey::Tab,
        KeyCode::BackTab => CKey::Tab,
        KeyCode::Home => CKey::Home,
        KeyCode::End => CKey::End,
        KeyCode::PageUp => CKey::PageUp,
        KeyCode::PageDown => CKey::PageDown,
        _ => return None,
    };
    Some(Key {
        code,
        ctrl,
        alt,
        shift,
    })
}
