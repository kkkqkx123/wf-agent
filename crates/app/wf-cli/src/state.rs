//! Layered state management for the TUI application.
//!
//! [`AppState`] is the top-level state container, subdivided into:
//! - [`ScreenState`]: screen navigation, data cache, focus
//! - [`SessionState`]: interactive session state (scrollback, approval, composer)
//! - [`UiState`]: modals, overlays, theme, notices
//!
//! This replaces the flat state previously scattered across `TuiApp` fields,
//! improving testability and making state transitions explicit.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::approval_overlay::ApprovalView;
use crate::footer::{FooterView, FooterRoute, PanelState};
use crate::question_overlay::QuestionView;
use crate::screens::{ExecStatusFilter, ScreenData, ScreenKind, Screens};
use crate::theme::Theme;

/// Top-level application state.
#[derive(Debug)]
pub struct AppState {
    pub screen: ScreenState,
    pub session: SessionState,
    pub ui: UiState,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            screen: ScreenState::new(),
            session: SessionState::new(),
            ui: UiState::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Screen navigation state.
#[derive(Debug)]
pub struct ScreenState {
    pub navigation: Screens,
    pub data_cache: HashMap<ScreenKind, (ScreenData, Instant)>,
    pub inflight: HashMap<ScreenKind, Instant>,
    pub current_focus: FocusTarget,
    pub search_input: String,
    pub exec_filter: ExecStatusFilter,
}

impl ScreenState {
    pub fn new() -> Self {
        Self {
            navigation: Screens::new(),
            data_cache: HashMap::new(),
            inflight: HashMap::new(),
            current_focus: FocusTarget::Scrollback,
            search_input: String::new(),
            exec_filter: ExecStatusFilter::All,
        }
    }

    /// Current screen kind.
    pub fn current_kind(&self) -> ScreenKind {
        self.navigation.current_kind()
    }

    /// Selected index on the current screen.
    pub fn selected(&self) -> usize {
        self.navigation.selected()
    }

    /// Whether fresh data is cached for `kind`.
    pub fn is_fresh(&self, kind: ScreenKind, ttl: Duration) -> bool {
        self.data_cache
            .get(&kind)
            .map(|(_, at)| at.elapsed() < ttl)
            .unwrap_or(false)
    }

    /// Whether a fetch is in flight for `kind`.
    pub fn has_inflight(&self, kind: ScreenKind, timeout: Duration) -> bool {
        self.inflight
            .get(&kind)
            .map(|started| started.elapsed() < timeout)
            .unwrap_or(false)
    }

    /// Get cached data for the current screen.
    pub fn current_data(&self) -> ScreenData {
        self.data_cache
            .get(&self.current_kind())
            .map(|(data, _)| data.clone())
            .unwrap_or(ScreenData::None)
    }

    /// Drop cached data for `kind`.
    pub fn invalidate(&mut self, kind: ScreenKind) {
        self.data_cache.remove(&kind);
    }
}

impl Default for ScreenState {
    fn default() -> Self {
        Self::new()
    }
}

/// Interactive session state (when on the Interactive screen).
#[derive(Debug)]
pub struct SessionState {
    /// Current footer view (Prompt / Permission / Question).
    pub footer_view: FooterView,
    /// Current route inside the prompt view.
    pub footer_route: FooterRoute,
    /// Active panel state.
    pub panel: Option<PanelState>,
    /// Pending tool approval view.
    pub approval: Option<ApprovalView>,
    /// Pending follow-up question view.
    pub question: Option<QuestionView>,
    /// The interactive controller has requested exit.
    pub exit_requested: bool,
    /// Execution id selected for replay.
    pub pending_replay: Option<String>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            footer_view: FooterView::Prompt,
            footer_route: FooterRoute::Composer,
            panel: None,
            approval: None,
            question: None,
            exit_requested: false,
            pending_replay: None,
        }
    }

    /// Reset to the default prompt state.
    pub fn reset_to_prompt(&mut self) {
        self.footer_view = FooterView::Prompt;
        self.footer_route = FooterRoute::Composer;
        self.panel = None;
        self.approval = None;
        self.question = None;
        self.exit_requested = false;
        self.pending_replay = None;
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

/// UI-only state: modals, overlays, theme, notices.
#[derive(Debug)]
pub struct UiState {
    pub overlay: OverlayState,
    pub theme: Theme,
    pub notices: NoticeQueue,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            overlay: OverlayState::None,
            theme: Theme::dark_default(),
            notices: NoticeQueue::new(),
        }
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

/// Overlay mode (sidebar, history, command palette).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayState {
    None,
    Sidebar,
    History,
    CommandPalette,
}

/// Focus target within the current screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Scrollback,
    Composer,
    Modal,
    Overlay,
}

/// A queue of transient notice messages with TTL.
#[derive(Debug)]
pub struct NoticeQueue {
    current: Option<(String, Instant)>,
    ttl: Duration,
}

impl NoticeQueue {
    pub fn new() -> Self {
        Self {
            current: None,
            ttl: Duration::from_secs(6),
        }
    }

    /// Set a notice (replaces any active notice).
    pub fn set(&mut self, text: impl Into<String>) {
        self.current = Some((text.into(), Instant::now()));
    }

    /// Get the current notice text if not expired.
    pub fn current_text(&self) -> Option<String> {
        let (text, at) = self.current.as_ref()?;
        if at.elapsed() < self.ttl {
            Some(text.clone())
        } else {
            None
        }
    }

    /// Expire the notice if its TTL has elapsed. Returns whether a repaint
    /// is needed.
    pub fn expire(&mut self) -> bool {
        if let Some((_, at)) = &self.current {
            if at.elapsed() >= self.ttl {
                self.current = None;
                return true;
            }
        }
        false
    }

    /// Whether a notice is currently active (forcing repaints).
    pub fn is_active(&self) -> bool {
        self.current_text().is_some()
    }
}

impl Default for NoticeQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_state_fresh_detection() {
        let mut state = ScreenState::new();
        let ttl = Duration::from_secs(5);
        assert!(!state.is_fresh(ScreenKind::Dashboard, ttl));
        state
            .data_cache
            .insert(ScreenKind::Dashboard, (ScreenData::None, Instant::now()));
        assert!(state.is_fresh(ScreenKind::Dashboard, ttl));
    }

    #[test]
    fn notice_queue_expiry() {
        let mut queue = NoticeQueue::new();
        queue.set("hello");
        assert!(queue.current_text().is_some());
        assert!(!queue.expire());
    }

    #[test]
    fn session_state_reset() {
        let mut state = SessionState::new();
        state.exit_requested = true;
        state.pending_replay = Some("exec-1".to_string());
        state.reset_to_prompt();
        assert!(!state.exit_requested);
        assert!(state.pending_replay.is_none());
        assert_eq!(state.footer_view, FooterView::Prompt);
    }
}
