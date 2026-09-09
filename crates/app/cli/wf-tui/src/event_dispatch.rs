//! Unified event dispatch system with handlers, middleware, and async support.
//!
//! [`EventDispatcher`] routes events through a chain of [`EventHandler`]s
//! with configurable priority, intercepted by [`Middleware`] that can
//! suppress or post-process events.

use crate::keymap::Key;
use crate::state::AppState;

/// Event types routed through the dispatcher.
pub enum EventType {
    /// A terminal key press.
    Key(Key),
    /// Terminal resize.
    Resize(u16, u16),
    /// A custom event (used for async notifications).
    Custom(Box<dyn std::any::Any + Send>),
}

/// Result of event handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    /// Event was consumed; stop propagation.
    Consumed,
    /// Event was not handled; continue to next handler.
    Ignored,
    /// A redraw is needed after this event.
    NeedRedraw,
    /// The application should exit.
    Exit,
}

/// Event handler trait: the core unit of event processing.
pub trait EventHandler: Send + Sync {
    /// Handle an event, optionally mutating state.
    fn handle(&self, event: &EventType, state: &mut AppState) -> EventResult;

    /// Handler priority (lower = higher priority). Default: 0.
    fn priority(&self) -> i32 {
        0
    }

    /// Whether this handler is currently enabled.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Handler name for debugging.
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Middleware trait: intercept events before/after dispatch.
pub trait Middleware: Send + Sync {
    /// Called before event dispatch. Return `false` to suppress the event.
    fn before_dispatch(&self, _event: &EventType, _state: &mut AppState) -> bool {
        true
    }

    /// Called after event dispatch with the result.
    fn after_dispatch(&self, _event: &EventType, _state: &mut AppState, _result: &EventResult) {}
}

/// The event dispatcher: routes events through handlers with middleware.
pub struct EventDispatcher {
    handlers: Vec<Box<dyn EventHandler>>,
    middleware: Vec<Box<dyn Middleware>>,
}

impl EventDispatcher {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
            middleware: Vec::new(),
        }
    }

    /// Register an event handler (sorted by priority after insertion).
    pub fn register_handler(&mut self, handler: Box<dyn EventHandler>) {
        self.handlers.push(handler);
        self.handlers.sort_by_key(|h| h.priority());
    }

    /// Register middleware.
    pub fn register_middleware(&mut self, mw: Box<dyn Middleware>) {
        self.middleware.push(mw);
    }

    /// Dispatch an event through the middleware and handler chain.
    pub fn dispatch(&self, event: &EventType, state: &mut AppState) -> EventResult {
        // Run before_dispatch on all middleware.
        for mw in &self.middleware {
            if !mw.before_dispatch(event, state) {
                return EventResult::Ignored;
            }
        }

        // Run handlers in priority order.
        let mut result = EventResult::Ignored;
        for handler in &self.handlers {
            if !handler.is_enabled() {
                continue;
            }
            result = handler.handle(event, state);
            if result == EventResult::Consumed {
                break;
            }
        }

        // Run after_dispatch on all middleware.
        for mw in &self.middleware {
            mw.after_dispatch(event, state, &result);
        }

        result
    }

    /// Number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Number of registered middleware.
    pub fn middleware_count(&self) -> usize {
        self.middleware.len()
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// A logging middleware that prints dispatched events to stderr (debug builds).
pub struct LoggingMiddleware;

impl Middleware for LoggingMiddleware {
    fn after_dispatch(&self, event: &EventType, _state: &mut AppState, result: &EventResult) {
        let label = match event {
            EventType::Key(k) => format!("key({:?})", k.code),
            EventType::Resize(w, h) => format!("resize({w}x{h})"),
            EventType::Custom(_) => "custom".to_string(),
        };
        tracing::debug!("event_dispatch: {label} -> {result:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{CKey, KeyAction};

    struct TestHandler {
        priority: i32,
        result: EventResult,
    }

    impl EventHandler for TestHandler {
        fn handle(&self, _event: &EventType, _state: &mut AppState) -> EventResult {
            self.result
        }
        fn priority(&self) -> i32 {
            self.priority
        }
    }

    struct SuppressMiddleware;

    impl Middleware for SuppressMiddleware {
        fn before_dispatch(&self, _event: &EventType, _state: &mut AppState) -> bool {
            false
        }
    }

    fn test_key() -> EventType {
        EventType::Key(Key {
            code: CKey::Char('a'),
            ctrl: false,
            alt: false,
            shift: false,
        })
    }

    #[test]
    fn dispatch_returns_ignored_when_no_handlers() {
        let dispatcher = EventDispatcher::new();
        let mut state = AppState::new();
        assert_eq!(
            dispatcher.dispatch(&test_key(), &mut state),
            EventResult::Ignored
        );
    }

    #[test]
    fn handler_priority_determines_order() {
        let mut dispatcher = EventDispatcher::new();
        dispatcher.register_handler(Box::new(TestHandler {
            priority: 10,
            result: EventResult::Consumed,
        }));
        dispatcher.register_handler(Box::new(TestHandler {
            priority: 1,
            result: EventResult::Ignored,
        }));
        let mut state = AppState::new();
        // Priority 1 handler runs first and returns Ignored,
        // so priority 10 handler runs next and returns Consumed.
        assert_eq!(
            dispatcher.dispatch(&test_key(), &mut state),
            EventResult::Consumed
        );
    }

    #[test]
    fn middleware_can_suppress_events() {
        let mut dispatcher = EventDispatcher::new();
        dispatcher.register_handler(Box::new(TestHandler {
            priority: 0,
            result: EventResult::Consumed,
        }));
        dispatcher.register_middleware(Box::new(SuppressMiddleware));
        let mut state = AppState::new();
        assert_eq!(
            dispatcher.dispatch(&test_key(), &mut state),
            EventResult::Ignored
        );
    }

    #[test]
    fn disabled_handler_is_skipped() {
        struct DisabledHandler;
        impl EventHandler for DisabledHandler {
            fn handle(&self, _event: &EventType, _state: &mut AppState) -> EventResult {
                EventResult::Consumed
            }
            fn is_enabled(&self) -> bool {
                false
            }
        }

        let mut dispatcher = EventDispatcher::new();
        dispatcher.register_handler(Box::new(DisabledHandler));
        let mut state = AppState::new();
        assert_eq!(
            dispatcher.dispatch(&test_key(), &mut state),
            EventResult::Ignored
        );
    }
}
