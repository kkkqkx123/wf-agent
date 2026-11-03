//! Structured background-task linkage for tool stream events.
//!
//! A tool that hands a running shell command back to the caller instead of
//! blocking (for example the background shell starter or the manual
//! background action) attaches this marker to its stream completion event.
//! Frontends match on the marker instead of sniffing the flat result text,
//! so card badges and task aggregation stay correct when result wording
//! changes. Foreground tools and older servers carry no marker.

use serde::{Deserialize, Serialize};

/// Observer-side cap for background shell sessions tracked from the stream.
/// Mirrors the server settled-entry bound so footers and summaries stay
/// bounded on long runs. Observers keep completion order and drop the
/// oldest entry when full; the session itself stays server-owned.
pub const MAX_OBSERVED_BACKGROUND_TASKS: usize = 25;

/// Linkage between one tool call and the background session it produced.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BackgroundMarker {
    /// Session id returned by the tool; the follow-up handle for output
    /// reads, waits and termination.
    #[serde(default)]
    pub session_id: String,
    /// True when the caller was told not to wait and completion arrives
    /// through the shell completion event.
    #[serde(default)]
    pub backgrounded: bool,
}

impl BackgroundMarker {
    /// Whether the marker denotes an observable background shell session.
    /// Frontends only observe server-owned sessions; empty ids and
    /// non-backgrounded markers must not enter task aggregation.
    pub fn is_active(&self) -> bool {
        self.backgrounded && !self.session_id.is_empty()
    }
}
