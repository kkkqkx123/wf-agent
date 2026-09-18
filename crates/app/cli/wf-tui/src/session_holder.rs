//! Session holding versus rendering: remote persistence contract.
//!
//! The session outlives any single UI client. The holder owns the lifecycle
//! (held, attached, detached, reconnecting) while the event loop only borrows
//! the attached view for one frame. Reconnect reuses the idle backoff
//! intervals, so no new timer is introduced. A single holder owns a session
//! id at a time; attaching a second client detaches the first.

/// Lifecycle of one held session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionHold {
    #[default]
    Empty,
    Held,
    Attached,
    Detached,
    Reconnecting,
}

/// Holder for one remote session id with reconnect attempts.
#[derive(Debug, Default)]
pub struct SessionHolder {
    session_id: Option<String>,
    state: SessionHold,
    reconnects: u32,
}

impl SessionHolder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> SessionHold {
        self.state
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn reconnects(&self) -> u32 {
        self.reconnects
    }

    /// Hold a session id without attaching a renderer.
    pub fn hold(&mut self, id: impl Into<String>) {
        self.session_id = Some(id.into());
        self.state = SessionHold::Held;
    }

    /// Attach the renderer to the held session.
    pub fn attach(&mut self) -> bool {
        if self.session_id.is_none() {
            return false;
        }
        self.state = SessionHold::Attached;
        true
    }

    /// Detach the renderer; the session stays held in the background.
    pub fn detach(&mut self) {
        if self.session_id.is_some() {
            self.state = SessionHold::Detached;
        }
    }

    /// Mark a reconnect attempt; the caller backs off with the idle schedule.
    pub fn reconnect(&mut self) {
        if self.session_id.is_some() {
            self.state = SessionHold::Reconnecting;
            self.reconnects = self.reconnects.saturating_add(1);
        }
    }

    /// Re-attach after a successful reconnect.
    pub fn reattached(&mut self) {
        if self.session_id.is_some() {
            self.state = SessionHold::Attached;
        }
    }

    /// Release the held session entirely.
    pub fn release(&mut self) {
        self.session_id = None;
        self.state = SessionHold::Empty;
        self.reconnects = 0;
    }

    /// True when rendering may proceed.
    pub fn can_render(&self) -> bool {
        matches!(self.state, SessionHold::Attached)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_then_attach_renders() {
        let mut holder = SessionHolder::new();
        assert!(!holder.attach());
        holder.hold("exec-1");
        assert!(holder.attach());
        assert!(holder.can_render());
    }

    #[test]
    fn detach_keeps_session_held() {
        let mut holder = SessionHolder::new();
        holder.hold("exec-1");
        holder.attach();
        holder.detach();
        assert_eq!(holder.state(), SessionHold::Detached);
        assert_eq!(holder.session_id(), Some("exec-1"));
        assert!(!holder.can_render());
    }

    #[test]
    fn reconnect_counts_attempts() {
        let mut holder = SessionHolder::new();
        holder.hold("exec-1");
        holder.reconnect();
        assert_eq!(holder.state(), SessionHold::Reconnecting);
        assert_eq!(holder.reconnects(), 1);
        holder.reattached();
        assert!(holder.can_render());
    }

    #[test]
    fn release_empties_holder() {
        let mut holder = SessionHolder::new();
        holder.hold("exec-1");
        holder.release();
        assert_eq!(holder.state(), SessionHold::Empty);
        assert_eq!(holder.session_id(), None);
    }
}
