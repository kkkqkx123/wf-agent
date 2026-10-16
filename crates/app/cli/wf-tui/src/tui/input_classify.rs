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
    /// No text entry on this screen; the paste is dropped.
    Ignored,
}

/// Route a paste body: modals swallow everything, the session screen feeds
/// the composer, other screens drop it. The search palette is handled by the
/// shell before this classification runs.
pub(super) fn classify_paste(modal_open: bool, screen: ScreenKind) -> PasteTarget {
    if modal_open {
        return PasteTarget::Swallowed;
    }
    match screen {
        ScreenKind::Interactive => PasteTarget::Session,
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

/// Whether a mouse event bypasses capture and keeps native selection.
/// Shift plus any button or drag always returns to the terminal so the
/// user keeps one native copy path while capture is on.
pub(super) fn mouse_bypasses_capture(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::SHIFT)
}

/// Multi-click kind for selection compensation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClickKind {
    Single,
    Double,
    Triple,
}

/// Click kind for a consecutive count (capped at triple).
pub(super) fn click_kind_for_count(count: u8) -> ClickKind {
    match count {
        2 => ClickKind::Double,
        3.. => ClickKind::Triple,
        _ => ClickKind::Single,
    }
}

/// Multi-click window in milliseconds.
pub(super) const MULTI_CLICK_WINDOW_MS: u64 = 500;

/// Next consecutive click count: same cell inside the window continues the
/// chain (capped at triple), otherwise a new single starts. Scroll and
/// resize clear the chain through the caller dropping the stored state.
pub(super) fn next_click_count(
    prev_count: u8,
    prev_ms: Option<u64>,
    prev_col: Option<u16>,
    prev_row: Option<u16>,
    now_ms: u64,
    col: u16,
    row: u16,
) -> u8 {
    let (Some(prev_ms), Some(prev_col), Some(prev_row)) = (prev_ms, prev_col, prev_row) else {
        return 1;
    };
    if col != prev_col || row != prev_row {
        return 1;
    }
    if now_ms.saturating_sub(prev_ms) > MULTI_CLICK_WINDOW_MS {
        return 1;
    }
    (prev_count.saturating_add(1)).clamp(1, 3)
}

/// Whether a mouse event kind carries the left button.
pub(super) fn mouse_is_left_button(kind: MouseEventKind) -> bool {
    use crossterm::event::MouseButton;
    matches!(
        kind,
        MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Up(MouseButton::Left)
            | MouseEventKind::Drag(MouseButton::Left)
    )
}

/// Best-effort tool-output thresholds from the user config file. A missing
/// or unreadable config means built-in defaults; individual keys fall back
/// independently so a partial `[tool_output]` section still applies.
pub(crate) fn load_tool_output_config() -> crate::tool_output::ToolOutputConfig {
    use crate::app_config::ToolDetailLevelName;
    use crate::tool_output::{DetailLevel, ToolOutputConfig};
    let mut config = ToolOutputConfig::default();
    let path = match crate::app_config::config_file_path() {
        Some(path) => path,
        None => return config,
    };
    let manager = match crate::app_config::ConfigManager::load(&path) {
        Ok(manager) => manager,
        Err(_) => return config,
    };
    let file = &manager.config().tool_output;
    if let Some(enabled) = file.enabled {
        config.enabled = enabled;
    }
    if let Some(lines) = file.preview_max_lines {
        config.preview_max_lines = lines;
    }
    if let Some(chars) = file.preview_max_chars {
        config.preview_max_chars = chars;
    }
    if let Some(lines) = file.shell_preview_lines {
        config.shell_preview_lines = lines;
    }
    if let Some(lines) = file.error_preview_lines {
        config.error_preview_lines = lines;
    }
    if let Some(bytes) = file.block_max_bytes {
        config.block_max_bytes = bytes;
    }
    if let Some(level) = file.level {
        config.level = match level {
            ToolDetailLevelName::Low => DetailLevel::Low,
            ToolDetailLevelName::Medium => DetailLevel::Medium,
            ToolDetailLevelName::High => DetailLevel::High,
        };
    }
    config
}

/// Best-effort mouse opt-in from the user config file. A missing or
/// unreadable config means off: mouse capture stays disabled and the
/// terminal keeps native text selection.
pub(crate) fn load_mouse_opt_in() -> bool {
    let path = match crate::app_config::config_file_path() {
        Some(path) => path,
        None => return false,
    };
    match crate::app_config::ConfigManager::load(&path) {
        Ok(manager) => manager.config().behavior.mouse_capture,
        Err(_) => false,
    }
}

/// Best-effort session scrollbar opt-in from the user config file. A
/// missing or unreadable config means off to match the reference main
/// session default. Unlike mouse capture this takes effect instantly
/// because the width change grades a full frame.
pub(crate) fn load_session_scrollbar_opt_in() -> bool {
    let path = match crate::app_config::config_file_path() {
        Some(path) => path,
        None => return false,
    };
    match crate::app_config::ConfigManager::load(&path) {
        Ok(manager) => manager.config().behavior.session_scrollbar,
        Err(_) => false,
    }
}

/// Best-effort default copy target from `[terminal] copy_target`. A
/// missing or unreadable config (or an unknown value, which fails the
/// whole file parse and falls back here) means the message default.
pub(crate) fn load_copy_target() -> crate::copy::CopyDefaultTarget {
    use crate::app_config::CopyTargetName;
    let path = match crate::app_config::config_file_path() {
        Some(path) => path,
        None => return crate::copy::CopyDefaultTarget::Message,
    };
    let target = match crate::app_config::ConfigManager::load(&path) {
        Ok(manager) => manager.config().terminal.copy_target,
        Err(_) => None,
    };
    match target {
        Some(CopyTargetName::Code) => crate::copy::CopyDefaultTarget::Code,
        _ => crate::copy::CopyDefaultTarget::Message,
    }
}

/// Best-effort performance tier override from the config file. Auto/missing
/// returns None so the caller keeps capability-driven selection.
pub(crate) fn load_performance_tier_override() -> Option<crate::perf::PerformanceTier> {
    use crate::app_config::PerformanceTierName;
    let path = crate::app_config::config_file_path()?;
    let manager = crate::app_config::ConfigManager::load(&path).ok()?;
    match manager.config().motion.performance {
        Some(PerformanceTierName::Full) => Some(crate::perf::PerformanceTier::Full),
        Some(PerformanceTierName::Reduced) => Some(crate::perf::PerformanceTier::Reduced),
        Some(PerformanceTierName::Minimal) => Some(crate::perf::PerformanceTier::Minimal),
        _ => None,
    }
}

#[allow(dead_code)]
/// Best-effort motion mode override from the config file.
pub(crate) fn load_motion_mode_override() -> Option<tui_style::motion::MotionMode> {
    use crate::app_config::MotionModeName;
    let path = crate::app_config::config_file_path()?;
    let manager = crate::app_config::ConfigManager::load(&path).ok()?;
    match manager.config().motion.mode {
        Some(MotionModeName::Animated) => Some(tui_style::motion::MotionMode::Animated),
        Some(MotionModeName::Reduced) => Some(tui_style::motion::MotionMode::Reduced),
        Some(MotionModeName::Static) => Some(tui_style::motion::MotionMode::Static),
        _ => None,
    }
}

/// Best-effort theme choice from the config file.
pub(crate) fn load_theme_choice() -> Option<crate::app_config::ThemeChoice> {
    let path = crate::app_config::config_file_path()?;
    let manager = crate::app_config::ConfigManager::load(&path).ok()?;
    manager.config().appearance.theme
}

#[allow(dead_code)]
/// Best-effort approval timeout from behavior config or mini config.
pub(crate) fn load_approval_timeout_secs() -> Option<u64> {
    let path = crate::app_config::config_file_path()?;
    let manager = crate::app_config::ConfigManager::load(&path).ok()?;
    manager
        .config()
        .behavior
        .approval_timeout_secs
        .or(manager.config().mini.approval_timeout_secs)
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
    use tui_core::screen_data::{screen_digit, screen_for_digit};

    #[test]
    fn digit_to_screen_follows_the_canonical_map() {
        let mapped: Vec<_> = ('0'..='9').filter_map(screen_for_digit).collect();
        assert_eq!(
            mapped,
            vec![
                ScreenKind::Dashboard,
                ScreenKind::Workflow,
                ScreenKind::Executions,
                ScreenKind::Insights,
                ScreenKind::Interactive,
                ScreenKind::Checkpoints,
                ScreenKind::Settings,
            ]
        );
        assert!(screen_for_digit('7').is_none());
        assert!(screen_for_digit('8').is_none());
        assert!(screen_for_digit('9').is_none());
        assert!(screen_for_digit('z').is_none());
        // Screens added after the original seven carry no digit.
        for screen in [ScreenKind::Catalog, ScreenKind::Git] {
            assert_eq!(screen_digit(screen), None);
        }
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
    fn paste_routes_to_session_or_swallow() {
        // Modals swallow every paste regardless of the screen.
        for screen in ScreenKind::all() {
            assert_eq!(classify_paste(true, *screen), PasteTarget::Swallowed);
        }
        assert_eq!(
            classify_paste(false, ScreenKind::Interactive),
            PasteTarget::Session
        );
        // Screens without text entry drop the paste.
        for screen in [
            ScreenKind::Dashboard,
            ScreenKind::Workflow,
            ScreenKind::Executions,
            ScreenKind::Insights,
            ScreenKind::Checkpoints,
            ScreenKind::Settings,
            ScreenKind::Catalog,
            ScreenKind::Git,
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
    fn shift_bypasses_capture_for_native_selection() {
        assert!(mouse_bypasses_capture(KeyModifiers::SHIFT));
        assert!(mouse_bypasses_capture(
            KeyModifiers::SHIFT | KeyModifiers::CONTROL
        ));
        assert!(!mouse_bypasses_capture(KeyModifiers::empty()));
        assert!(!mouse_bypasses_capture(KeyModifiers::CONTROL));
    }

    #[test]
    fn left_button_detection_covers_press_release_drag() {
        assert!(mouse_is_left_button(MouseEventKind::Down(
            MouseButton::Left
        )));
        assert!(mouse_is_left_button(MouseEventKind::Up(MouseButton::Left)));
        assert!(mouse_is_left_button(MouseEventKind::Drag(
            MouseButton::Left
        )));
        assert!(!mouse_is_left_button(MouseEventKind::Down(
            MouseButton::Right
        )));
        assert!(!mouse_is_left_button(MouseEventKind::Up(
            MouseButton::Middle
        )));
        assert!(!mouse_is_left_button(MouseEventKind::ScrollUp));
        assert!(!mouse_is_left_button(MouseEventKind::Moved));
    }

    #[test]
    fn multi_click_chains_same_cell_inside_window() {
        assert_eq!(next_click_count(0, None, None, None, 1000, 5, 5), 1);
        assert_eq!(
            next_click_count(1, Some(1000), Some(5), Some(5), 1200, 5, 5),
            2
        );
        assert_eq!(
            next_click_count(2, Some(1200), Some(5), Some(5), 1300, 5, 5),
            3
        );
        assert_eq!(
            next_click_count(3, Some(1300), Some(5), Some(5), 1400, 5, 5),
            3
        );
        assert_eq!(
            next_click_count(2, Some(1000), Some(5), Some(5), 2000, 5, 5),
            1
        );
        assert_eq!(
            next_click_count(2, Some(1200), Some(5), Some(5), 1300, 6, 5),
            1
        );
        assert_eq!(click_kind_for_count(1), ClickKind::Single);
        assert_eq!(click_kind_for_count(2), ClickKind::Double);
        assert_eq!(click_kind_for_count(3), ClickKind::Triple);
        assert_eq!(click_kind_for_count(9), ClickKind::Triple);
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
