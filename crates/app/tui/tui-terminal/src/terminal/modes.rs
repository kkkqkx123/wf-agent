//! Terminal mode switches tracked by [`TerminalGuard`](super::guard::TerminalGuard).

/// Terminal mode switches tracked by the guard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalModes {
    /// crossterm raw mode (no line buffering / echo).
    pub raw: bool,
    /// Alternate screen buffer (full TUI only).
    pub alt_screen: bool,
    /// Bracketed paste mode.
    pub bracketed_paste: bool,
    /// Cursor hidden.
    pub cursor_hidden: bool,
    /// Focus in/out reporting (`CSI ? 1004 h/l`).
    pub focus_change: bool,
    /// Mouse button/drag/wheel capture with SGR extended coordinates.
    pub mouse_capture: bool,
    /// Alternate scroll (`CSI ? 1007 h/l`): the wheel arrives as up/down
    /// keys without capturing the mouse, so native selection keeps working.
    pub alternate_scroll: bool,
    /// Kitty keyboard enhancement (push on enter, pop on restore).
    pub keyboard_enhancement: bool,
}

impl TerminalModes {
    /// All switches off (the restored baseline).
    pub const OFF: Self = Self {
        raw: false,
        alt_screen: false,
        bracketed_paste: false,
        cursor_hidden: false,
        focus_change: false,
        mouse_capture: false,
        alternate_scroll: false,
        keyboard_enhancement: false,
    };

    /// Typical TUI inline session: inline viewport, no alt screen.
    /// Input capabilities stay off so the byte behavior matches the legacy
    /// baseline; callers opt in via [`Self::with_input_modes`].
    pub const MINI: Self = Self {
        raw: true,
        alt_screen: false,
        bracketed_paste: true,
        cursor_hidden: false,
        focus_change: false,
        mouse_capture: false,
        alternate_scroll: false,
        keyboard_enhancement: false,
    };

    /// Typical full TUI session: alt screen + hidden cursor.
    /// Input capabilities stay off so the byte behavior matches the legacy
    /// baseline; callers opt in via [`Self::with_input_modes`].
    pub const TUI: Self = Self {
        raw: true,
        alt_screen: true,
        bracketed_paste: true,
        cursor_hidden: true,
        focus_change: false,
        mouse_capture: false,
        alternate_scroll: false,
        keyboard_enhancement: false,
    };

    /// Layer the input-capability switches on top of a base mode.
    /// The policy object is the only caller: modes never hardcode sequences.
    pub const fn with_input_modes(
        mut self,
        focus_change: bool,
        mouse_capture: bool,
        alternate_scroll: bool,
        keyboard_enhancement: bool,
    ) -> Self {
        self.focus_change = focus_change;
        self.mouse_capture = mouse_capture;
        self.alternate_scroll = alternate_scroll;
        self.keyboard_enhancement = keyboard_enhancement;
        self
    }
}
