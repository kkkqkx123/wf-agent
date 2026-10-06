//! Frame transport boundary and its recording double.

use crate::redraw::RedrawScope;

/// Transport boundary of the event loop: the single submit point behind
/// `Terminal::draw` in production and a recording double in tests. Replay
/// drives the full loop through this trait without a real terminal.
pub trait FrameTransport {
    /// Submit one frame of `scope`; returns whether a frame was emitted.
    fn submit(&mut self, scope: RedrawScope) -> bool;
}

/// Recording transport double: replays submit decisions headlessly.
#[derive(Debug, Default)]
pub struct TestTransport {
    submitted: Vec<RedrawScope>,
}

impl TestTransport {
    /// Empty double.
    pub fn new() -> Self {
        Self::default()
    }

    /// Submitted scopes in order.
    pub fn submitted(&self) -> &[RedrawScope] {
        &self.submitted
    }
}

impl FrameTransport for TestTransport {
    fn submit(&mut self, scope: RedrawScope) -> bool {
        if scope == RedrawScope::None {
            return false;
        }
        self.submitted.push(scope);
        true
    }
}
