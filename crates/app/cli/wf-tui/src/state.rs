//! Layered state management for the TUI application.
//!
//! [`AppState`] is the single application state owning three duties:
//! navigation data ([`ScreenState`]), session overlay
//! ([`crate::session_holder::SessionHolder`] lives beside it in the shell)
//! and interface notices ([`NoticeQueue`] plus client focus). The shell keeps
//! one [`AppState`] instead of scattering flat fields, so cache freshness,
//! notice expiry and focus gating stay testable in isolation.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::screens::{ExecStatusFilter, ScreenData, ScreenKind, Screens};

/// Unified application state: navigation data, interface notices and client
/// focus in one place. Session content itself stays in
/// [`crate::interactive::InteractiveController`] (scroll preparation plus
/// stream pump); this state only tracks where the user is and what transient
/// chrome is visible.
#[derive(Debug)]
pub struct AppState {
    /// Navigation, data cache, drafts and filter.
    pub screen: ScreenState,
    /// Transient status/error line rendered under the screen.
    pub notice: NoticeQueue,
    /// Whether the client terminal currently holds focus. Starts focused;
    /// any key / mouse / paste delivery re-asserts it.
    pub focused: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            screen: ScreenState::new(),
            notice: NoticeQueue::new(),
            focused: true,
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
    pub search_input: String,
    pub exec_filter: ExecStatusFilter,
}

impl ScreenState {
    pub fn new() -> Self {
        Self {
            navigation: Screens::new(),
            data_cache: HashMap::new(),
            inflight: HashMap::new(),
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

/// Focus target within the current screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Scrollback,
    Composer,
    Modal,
    Overlay,
}

/// A queue of transient notice messages with TTL, driven by the injectable
/// clock so expiry is deterministic in tests.
#[derive(Debug)]
pub struct NoticeQueue {
    current: Option<(String, u64)>,
    ttl_ms: u64,
}

impl NoticeQueue {
    pub fn new() -> Self {
        Self {
            current: None,
            ttl_ms: Duration::from_secs(6).as_millis() as u64,
        }
    }

    /// Set a notice (replaces any active notice).
    pub fn set(&mut self, text: impl Into<String>) {
        self.current = Some((text.into(), crate::clock::now_ms()));
    }

    /// Get the current notice text if not expired.
    pub fn current_text(&self) -> Option<String> {
        let (text, at) = self.current.as_ref()?;
        if crate::clock::now_ms().saturating_sub(*at) < self.ttl_ms {
            Some(text.clone())
        } else {
            None
        }
    }

    /// Expire the notice if its TTL has elapsed. Returns whether a repaint
    /// is needed.
    pub fn expire(&mut self) -> bool {
        if let Some((_, at)) = &self.current {
            if crate::clock::now_ms().saturating_sub(*at) >= self.ttl_ms {
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
}
