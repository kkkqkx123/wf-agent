//! Overlay mode and event feedback types for the TUI shell.

use tui_core::screen_data::ScreenKind;

/// What the event loop should do after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopAction {
    Continue,
    Quit,
}

/// Feedback produced by a background write / modal action.
pub enum Feedback {
    Notice(String),
    Refresh(ScreenKind),
    /// Drop the sidebar cache so the next loop iteration refetches it; the
    /// live event feeds send this alongside screen refreshes.
    RefreshSidebar,
    Filter(String),
    /// Parsed rebase selector awaiting its destructive confirmation. The
    /// prompt answer arrives on a background task that cannot push modals,
    /// so the main loop pushes the confirm modal and runs the write.
    RebaseConfirm(String),
}

/// Overlay mode for secondary views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// No overlay - show main screen
    None,
    /// Sidebar showing other screens
    Sidebar,
    /// History overlay showing full conversation history
    History,
    /// Search palette for global search
    SearchPalette,
    /// Session list browser for open chat tabs
    SessionList,
    /// Session tree browser for the active session (double-Esc)
    SessionTree,
}
