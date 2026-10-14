//! Structured background-task linkage for tool stream events.
//!
//! A tool that hands a running shell command back to the caller instead of
//! blocking (for example the background shell starter or the manual
//! background action) attaches this marker to its stream completion event.
//! Frontends match on the marker instead of sniffing the flat result text,
//! so card badges and task aggregation stay correct when result wording
//! changes. Foreground tools and older servers carry no marker.

use serde::{Deserialize, Serialize};

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
