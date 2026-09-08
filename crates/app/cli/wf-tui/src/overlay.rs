//! Overlay mode and event feedback types for the TUI shell.

use crate::screens::ScreenKind;

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
    /// Command palette for quick actions
    CommandPalette,
}
