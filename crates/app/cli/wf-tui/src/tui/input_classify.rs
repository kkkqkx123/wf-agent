//! Pure input classification for the TUI shell: key mapping, paste routing,
//! mouse event classification and focus state transitions.

use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};

use crate::keymap::{normalize_key, CKey, Key};
use crate::screens::{ExecStatusFilter, ScreenKind};

/// Focus input driving the focus state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FocusInput {
    /// Terminal focus-gained report.
    Gained,
    /// Terminal focus-lost report.
    Lost,
    /// Any key / mouse (non-move) / paste delivery, which proves the
    /// window is focused right now.
    Stream,
}

/// Pure focus transition returning the focused state after the input and
/// whether a catch-up frame is due. Gained always repaints (differential
/// catch-up, no backend clear); lost never paints; a stream event repaints
/// only when it flips a stuck-unfocused window back.
pub(super) fn focus_transition(focused: bool, input: FocusInput) -> (bool, bool) {
    match input {
        FocusInput::Gained => (true, true),
        FocusInput::Lost => (false, false),
        FocusInput::Stream => {
            if focused {
                (true, false)
            } else {
                (true, true)
            }
        }
    }
}

/// Where a bracketed-paste body lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PasteTarget {
    /// An open modal swallows the paste.
    Swallowed,
    /// The session screen composer.
    Session,
    /// The search screen draft.
    Search,
    /// No text entry on this screen; the paste is dropped.
    Ignored,
}

/// Route a paste body: modals swallow everything, the session screen feeds
/// the composer, the search screen feeds the draft, other screens drop it.
pub(super) fn classify_paste(modal_open: bool, screen: ScreenKind) -> PasteTarget {
    if modal_open {
        return PasteTarget::Swallowed;
    }
    match screen {
        ScreenKind::Interactive => PasteTarget::Session,
        ScreenKind::Search => PasteTarget::Search,
        _ => PasteTarget::Ignored,
    }
}

/// How a mouse event kind is consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MouseAction {
    /// Wheel up: scroll the content up.
    ScrollUp,
    /// Wheel down: scroll the content down.
    ScrollDown,
    /// Press, release and drag: ignored so native selection keeps working.
    IgnoredButton,
}

/// Classify a mouse event kind. Motion never reaches this function: the
/// event loop filters it out before any focus mark or frame request.
pub(super) fn classify_mouse(kind: MouseEventKind) -> MouseAction {
    match kind {
        MouseEventKind::ScrollUp => MouseAction::ScrollUp,
        MouseEventKind::ScrollDown => MouseAction::ScrollDown,
        MouseEventKind::Down(_)
        | MouseEventKind::Up(_)
        | MouseEventKind::Drag(_)
        | MouseEventKind::Moved
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => MouseAction::IgnoredButton,
    }
}

/// Best-effort mouse opt-in from the user config file. A missing or
/// unreadable config means off: mouse capture stays disabled and the
/// terminal keeps native text selection.
pub(super) fn load_mouse_opt_in() -> bool {
    let path = match crate::app_config::config_file_path() {
        Some(path) => path,
        None => return false,
    };
    match crate::app_config::ConfigManager::load(&path) {
        Ok(manager) => manager.config().behavior.mouse_capture,
        Err(_) => false,
    }
}

pub(super) fn map_key(key: crossterm::event::KeyEvent) -> Key {
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
        _ => CKey::Char('?'),
    };
    normalize_key(
        Key {
            code,
            ctrl,
            alt,
            shift,
        },
        cfg!(target_os = "macos"),
    )
}

pub(super) fn digit_to_screen(c: char) -> Option<ScreenKind> {
    match c {
        '1' => Some(ScreenKind::Workflow),
        '2' => Some(ScreenKind::Executions),
        '3' => Some(ScreenKind::AgentLoops),
        '4' => Some(ScreenKind::Insights),
        '5' => Some(ScreenKind::Interactive),
        '6' => Some(ScreenKind::Checkpoints),
        '7' => Some(ScreenKind::Search),
        '8' => Some(ScreenKind::Settings),
        '9' => Some(ScreenKind::Dashboard),
        '0' => Some(ScreenKind::Help),
        _ => None,
    }
}

/// Cycle the executions screen status filter.
pub(super) fn next_filter(current: ExecStatusFilter) -> ExecStatusFilter {
    let all = ExecStatusFilter::ALL;
    let idx = all
        .iter()
        .position(|f| *f == current)
        .map(|i| (i + 1) % all.len())
        .unwrap_or(0);
    all[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::ScreenKind;
    use crossterm::event::MouseButton;

    #[test]
    fn digit_to_screen_maps_all_screens() {
        let mapped: Vec<_> = ('1'..='9')
            .chain(std::iter::once('0'))
            .filter_map(digit_to_screen)
            .collect();
        assert_eq!(mapped.len(), 10);
        assert_eq!(mapped[0], ScreenKind::Workflow);
        assert_eq!(mapped[7], ScreenKind::Settings);
        assert_eq!(mapped[9], ScreenKind::Help);
        assert!(digit_to_screen('z').is_none());
    }

    #[test]
    fn filter_cycles_through_every_value() {
        let mut filter = ExecStatusFilter::All;
        let mut seen = vec![filter];
        for _ in 0..ExecStatusFilter::ALL.len() - 1 {
            filter = next_filter(filter);
            seen.push(filter);
        }
        assert_eq!(seen.len(), ExecStatusFilter::ALL.len());
        // A full cycle returns to the start.
        assert_eq!(next_filter(filter), ExecStatusFilter::All);
    }

    #[test]
    fn paste_routes_to_session_search_or_swallow() {
        // Modals swallow every paste regardless of the screen.
        for screen in ScreenKind::all() {
            assert_eq!(classify_paste(true, *screen), PasteTarget::Swallowed);
        }
        assert_eq!(
            classify_paste(false, ScreenKind::Interactive),
            PasteTarget::Session
        );
        assert_eq!(
            classify_paste(false, ScreenKind::Search),
            PasteTarget::Search
        );
        // Screens without text entry drop the paste.
        for screen in [
            ScreenKind::Dashboard,
            ScreenKind::Workflow,
            ScreenKind::Executions,
            ScreenKind::Checkpoints,
            ScreenKind::Settings,
            ScreenKind::Help,
        ] {
            assert_eq!(classify_paste(false, screen), PasteTarget::Ignored);
        }
    }

    #[test]
    fn mouse_kinds_classify_to_scroll_or_ignore() {
        assert_eq!(
            classify_mouse(MouseEventKind::ScrollUp),
            MouseAction::ScrollUp
        );
        assert_eq!(
            classify_mouse(MouseEventKind::ScrollDown),
            MouseAction::ScrollDown
        );
        // Buttons never drive frames or selection in this version.
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::ScrollLeft,
            MouseEventKind::ScrollRight,
        ] {
            assert_eq!(classify_mouse(kind), MouseAction::IgnoredButton);
        }
    }

    #[test]
    fn focus_truth_table_covers_gained_lost_and_compensation() {
        // Gained always marks focused with a catch-up frame; lost only
        // records the state; any input stream re-asserts focus.
        let gained = focus_transition(true, FocusInput::Gained);
        assert_eq!(gained, (true, true));
        let regained = focus_transition(false, FocusInput::Gained);
        assert_eq!(regained, (true, true));
        let lost = focus_transition(true, FocusInput::Lost);
        assert_eq!(lost, (false, false));
        let already_lost = focus_transition(false, FocusInput::Lost);
        assert_eq!(already_lost, (false, false));
        let compensated = focus_transition(false, FocusInput::Stream);
        assert_eq!(compensated, (true, true));
        let steady = focus_transition(true, FocusInput::Stream);
        assert_eq!(steady, (true, false));
    }
}
