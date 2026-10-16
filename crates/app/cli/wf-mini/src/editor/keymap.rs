//! Key event mapping: terminal keys to the editor key space.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, KeyEventKind::Press)
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
}
