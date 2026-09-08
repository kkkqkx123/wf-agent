//! Status line state for the footer.
//!
//! [`FooterState`] merges the reducer's incremental snapshot with
//! UI-only fields (model label, execution id, elapsed time, notice)
//! and is owned by [`crate::footer::Footer`].

use crate::reducer::{Phase, UsageMeta};

/// UI-side footer state: the reducer's [`crate::reducer::FooterState`] plus
/// the interactive presentation fields (model label, execution id, elapsed
/// time, notice).
#[derive(Debug, Clone, PartialEq)]
pub struct FooterState {
    pub phase: Phase,
    pub iteration: u32,
    pub active_tools: Vec<String>,
    pub message_count: u32,
    pub last_error: Option<String>,
    /// Active model profile label (status line).
    pub model: Option<String>,
    /// Active execution id (right summary block / exit hint).
    pub execution_id: Option<String>,
    /// Wall-clock duration of the current turn (ms).
    pub duration_ms: u64,
    /// Cumulative token usage (status line `tokens · cost`).
    pub usage: Option<UsageMeta>,
    /// Number of sub-agents currently running (status line hint).
    pub subagent_count: u32,
    /// Pending notice: `(text, expires_at_ms)`.
    pub notice: Option<(String, u64)>,
}

impl Default for FooterState {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            iteration: 0,
            active_tools: Vec::new(),
            message_count: 0,
            last_error: None,
            model: None,
            execution_id: None,
            duration_ms: 0,
            usage: None,
            subagent_count: 0,
            notice: None,
        }
    }
}

impl FooterState {
    /// Adopt the reducer footer snapshot (keeps the UI-only fields).
    pub fn merge_reducer(&mut self, reducer: &crate::reducer::FooterState) {
        self.phase = reducer.phase;
        self.iteration = reducer.iteration;
        self.active_tools = reducer.active_tools.clone();
        self.message_count = reducer.message_count;
        self.last_error = reducer.last_error.clone();
        self.usage = reducer.usage;
        self.subagent_count = reducer.subagent_count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::FooterState as ReducerFooter;

    #[test]
    fn default_state_is_idle() {
        let state = FooterState::default();
        assert_eq!(state.phase, Phase::Idle);
        assert_eq!(state.iteration, 0);
        assert!(state.active_tools.is_empty());
        assert_eq!(state.message_count, 0);
        assert!(state.last_error.is_none());
        assert!(state.model.is_none());
        assert!(state.execution_id.is_none());
        assert_eq!(state.duration_ms, 0);
        assert!(state.usage.is_none());
        assert_eq!(state.subagent_count, 0);
        assert!(state.notice.is_none());
    }

    #[test]
    fn merge_reducer_overwrites_dynamic_fields() {
        let mut state = FooterState {
            model: Some("gpt-4".to_string()),
            execution_id: Some("exec-1".to_string()),
            duration_ms: 500,
            ..Default::default()
        };

        let reducer = ReducerFooter {
            phase: Phase::Streaming,
            iteration: 3,
            active_tools: vec!["bash".to_string()],
            message_count: 7,
            last_error: None,
            usage: None,
            subagent_count: 0,
        };
        state.merge_reducer(&reducer);

        assert_eq!(state.phase, Phase::Streaming);
        assert_eq!(state.iteration, 3);
        assert_eq!(state.active_tools, vec!["bash".to_string()]);
        assert_eq!(state.message_count, 7);
        // UI-only fields preserved.
        assert_eq!(state.model.as_deref(), Some("gpt-4"));
        assert_eq!(state.execution_id.as_deref(), Some("exec-1"));
        assert_eq!(state.duration_ms, 500);
    }
}
