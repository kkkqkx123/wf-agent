//! Event reduction kernel shared by every CLI form (mini footer, full TUI
//! and the headless summary renderer).
//!
//! `Vec<UnifiedEvent>` → `MiniCommit[] + FooterState` as pure functions.
//! The reducer is a two-layer design:
//!
//! * [`fold`] — stateless pure function: one batch in, commit sequence +
//!   final footer out (deterministic, snapshot-testable, replay-safe).
//! * [`SessionReducer`] — stateful streaming reducer; `push_batch` drives
//!   the same fold kernel per event and is equivalent to `fold` on the
//!   concatenated stream.
//!
//! No IO, no TTY, no side effects: both layers are pure data transforms over
//! [`UnifiedEvent`], so tests feed synthetic event sequences and snapshot
//! the resulting commits.

use crate::events::UnifiedEvent;

/// Grouping key for a commit: `execution_id + iteration + tool_call_id`.
///
/// Tool events carry the tool call id; everything else uses `None`.
/// (mini) uses the same key to group commits into scrollback blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitGroup {
    pub execution_id: String,
    pub iteration: u32,
    pub tool_call_id: Option<String>,
}

/// One reduced, UI-consumable commit derived from the unified event stream.
///
/// `AssistantText` groups all `TextDelta` chunks of an iteration (frame
/// batched); tool lifecycle pairs produce `ToolStart`/`ToolEnd`; iteration
/// boundaries and terminal events map one-to-one. `User` is reserved for
/// callers that inject the user message explicitly (mini form) — it is never
/// derived from events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiniCommit {
    /// A user message (injected by the caller; not produced by the reducer).
    User { content: String },
    /// A settled block of assistant text (merged TextDelta chunks).
    AssistantText { content: String },
    /// A tool call started.
    ToolStart { tool_name: String },
    /// A tool call ended.
    ToolEnd {
        tool_name: String,
        success: bool,
        duration_ms: Option<u64>,
    },
    /// An iteration ended (scrollback flush boundary).
    IterationBoundary,
    /// The agent session completed successfully.
    Completed { iterations: u32 },
    /// The agent session failed.
    Failed { error: String },
    /// The agent session was interrupted.
    Interrupted { reason: String },
}

/// Footer lifecycle phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// No active agent work (initial / after completion).
    #[default]
    Idle,
    /// An agent session is streaming.
    Streaming,
}

/// Pure footer state consumed by the mini footer. Maintained
/// incrementally by the reduction pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterState {
    pub phase: Phase,
    pub iteration: u32,
    pub active_tools: Vec<String>,
    pub message_count: u32,
    pub last_error: Option<String>,
}

impl Default for FooterState {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            iteration: 0,
            active_tools: Vec::new(),
            message_count: 0,
            last_error: None,
        }
    }
}

/// Signature of a unified event used for idempotent dedup inside one batch:
/// consecutive identical events in the same group are skipped so replay /
/// duplicate delivery never double-commits.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EventSig {
    IterationStarted(u32),
    IterationEnded(u32),
    Text(String),
    ToolStart(String, String),
    ToolEnd(String, String, bool, Option<u64>),
    Completed(u32),
    Failed(String),
    Interrupted(String),
    Execution,
}

impl EventSig {
    /// Kinds that are deduped when identical and consecutive in a batch.
    fn is_dedupable(&self) -> bool {
        matches!(
            self,
            EventSig::ToolStart(..) | EventSig::ToolEnd(..) | EventSig::IterationEnded(..)
        )
    }
}

impl From<&UnifiedEvent> for EventSig {
    fn from(event: &UnifiedEvent) -> Self {
        match event {
            UnifiedEvent::IterationStarted { index } => EventSig::IterationStarted(*index),
            UnifiedEvent::IterationEnded { index } => EventSig::IterationEnded(*index),
            UnifiedEvent::TextDelta { content } => EventSig::Text(content.clone()),
            UnifiedEvent::ToolStart {
                tool_call_id,
                tool_name,
            } => EventSig::ToolStart(tool_call_id.clone(), tool_name.clone()),
            UnifiedEvent::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                duration_ms,
            } => EventSig::ToolEnd(
                tool_call_id.clone(),
                tool_name.clone(),
                *success,
                *duration_ms,
            ),
            UnifiedEvent::Completed { iterations, .. } => EventSig::Completed(*iterations),
            UnifiedEvent::Failed { error } => EventSig::Failed(error.clone()),
            UnifiedEvent::Interrupted { reason } => EventSig::Interrupted(reason.clone()),
            UnifiedEvent::Execution(_) => EventSig::Execution,
        }
    }
}

/// Last seen event (group + signature) used for batch-internal dedup.
struct LastEvent {
    group: CommitGroup,
    sig: EventSig,
}

/// Stateful streaming reducer: `execution_id` + event batches → commits and
/// an incrementally maintained [`FooterState`].
pub struct SessionReducer {
    execution_id: String,
    /// Text delta accumulation for the current iteration (flushed on any
    /// non-text event and at batch end).
    pending_text: String,
    last: Option<LastEvent>,
    footer: FooterState,
}

impl SessionReducer {
    pub fn new(execution_id: impl Into<String>) -> Self {
        Self {
            execution_id: execution_id.into(),
            pending_text: String::new(),
            last: None,
            footer: FooterState::default(),
        }
    }

    /// Feed a batch of events; returns the commits produced by this batch.
    ///
    /// The reduction is idempotent: consecutive duplicate tool /
    /// iteration-boundary events in the same group are skipped and empty
    /// text deltas are ignored. Pending text is flushed at the batch end.
    pub fn push_batch(&mut self, events: &[UnifiedEvent]) -> Vec<MiniCommit> {
        let mut commits = Vec::new();
        for event in events {
            let sig = EventSig::from(event);
            let group = self.group_for(event);
            let duplicate = sig.is_dedupable()
                && self
                    .last
                    .as_ref()
                    .is_some_and(|l| l.group == group && l.sig == sig);
            self.last = Some(LastEvent { group, sig });
            if duplicate {
                continue;
            }
            self.apply(event, &mut commits);
        }
        self.flush_text(&mut commits);
        commits
    }

    /// Snapshot of the current footer state.
    pub fn footer(&self) -> &FooterState {
        &self.footer
    }

    /// Derive the grouping key for an event under the current iteration.
    fn group_for(&self, event: &UnifiedEvent) -> CommitGroup {
        let tool_call_id = match event {
            UnifiedEvent::ToolStart { tool_call_id, .. }
            | UnifiedEvent::ToolEnd { tool_call_id, .. } => Some(tool_call_id.clone()),
            _ => None,
        };
        CommitGroup {
            execution_id: self.execution_id.clone(),
            iteration: self.footer.iteration,
            tool_call_id,
        }
    }

    fn apply(&mut self, event: &UnifiedEvent, commits: &mut Vec<MiniCommit>) {
        match event {
            UnifiedEvent::IterationStarted { index } => {
                self.flush_text(commits);
                self.footer.iteration = *index;
                self.footer.phase = Phase::Streaming;
            }
            UnifiedEvent::IterationEnded { .. } => {
                self.flush_text(commits);
                commits.push(MiniCommit::IterationBoundary);
            }
            UnifiedEvent::TextDelta { content } => {
                if content.is_empty() {
                    return;
                }
                self.footer.phase = Phase::Streaming;
                self.pending_text.push_str(content);
            }
            UnifiedEvent::ToolStart { tool_name, .. } => {
                self.flush_text(commits);
                self.footer.phase = Phase::Streaming;
                self.footer.active_tools.push(tool_name.clone());
                commits.push(MiniCommit::ToolStart {
                    tool_name: tool_name.clone(),
                });
            }
            UnifiedEvent::ToolEnd {
                tool_name,
                success,
                duration_ms,
                ..
            } => {
                self.flush_text(commits);
                if let Some(pos) = self.footer.active_tools.iter().position(|t| t == tool_name) {
                    self.footer.active_tools.remove(pos);
                }
                commits.push(MiniCommit::ToolEnd {
                    tool_name: tool_name.clone(),
                    success: *success,
                    duration_ms: *duration_ms,
                });
            }
            UnifiedEvent::Completed { iterations, .. } => {
                self.flush_text(commits);
                self.footer.phase = Phase::Idle;
                commits.push(MiniCommit::Completed {
                    iterations: *iterations,
                });
            }
            UnifiedEvent::Failed { error } => {
                self.flush_text(commits);
                self.footer.phase = Phase::Idle;
                self.footer.last_error = Some(error.clone());
                commits.push(MiniCommit::Failed {
                    error: error.clone(),
                });
            }
            UnifiedEvent::Interrupted { reason } => {
                self.flush_text(commits);
                self.footer.phase = Phase::Idle;
                self.footer.last_error = Some(reason.clone());
                commits.push(MiniCommit::Interrupted {
                    reason: reason.clone(),
                });
            }
            UnifiedEvent::Execution(_) => {}
        }
    }

    fn flush_text(&mut self, commits: &mut Vec<MiniCommit>) {
        if !self.pending_text.is_empty() {
            let content = std::mem::take(&mut self.pending_text);
            self.footer.message_count += 1;
            commits.push(MiniCommit::AssistantText { content });
        }
    }
}

/// Pure fold: one batch in, commit sequence + final footer out. Equivalent
/// to driving a single [`SessionReducer`] over the whole batch.
pub fn fold(
    events: &[UnifiedEvent],
    execution_id: impl Into<String>,
) -> (Vec<MiniCommit>, FooterState) {
    let mut reducer = SessionReducer::new(execution_id);
    let commits = reducer.push_batch(events);
    (commits, reducer.footer().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(text: &str) -> UnifiedEvent {
        UnifiedEvent::TextDelta {
            content: text.to_string(),
        }
    }

    fn it_start(index: u32) -> UnifiedEvent {
        UnifiedEvent::IterationStarted { index }
    }

    fn it_end(index: u32) -> UnifiedEvent {
        UnifiedEvent::IterationEnded { index }
    }

    fn t_start(id: &str, name: &str) -> UnifiedEvent {
        UnifiedEvent::ToolStart {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
        }
    }

    fn t_end(id: &str, name: &str, ok: bool, ms: Option<u64>) -> UnifiedEvent {
        UnifiedEvent::ToolEnd {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            success: ok,
            duration_ms: ms,
        }
    }

    #[test]
    fn empty_batch_yields_no_commits_and_idle_footer() {
        let (commits, footer) = fold(&[], "exec-1");
        assert!(commits.is_empty());
        assert_eq!(footer, FooterState::default());
        assert_eq!(footer.phase, Phase::Idle);
    }

    #[test]
    fn consecutive_text_deltas_merge_into_one_assistant_commit() {
        let events = vec![delta("hello "), delta("world"), delta("!")];
        let (commits, footer) = fold(&events, "exec-1");
        assert_eq!(
            commits,
            vec![MiniCommit::AssistantText {
                content: "hello world!".to_string(),
            }]
        );
        assert_eq!(footer.message_count, 1);
        assert_eq!(footer.phase, Phase::Streaming);
    }

    #[test]
    fn text_deltas_accumulate_across_batches_per_iteration() {
        let mut reducer = SessionReducer::new("exec-1");
        let c1 = reducer.push_batch(&[delta("part "), delta("one")]);
        assert_eq!(
            c1,
            vec![MiniCommit::AssistantText {
                content: "part one".to_string(),
            }]
        );
        let c2 = reducer.push_batch(&[delta("two")]);
        assert_eq!(
            c2,
            vec![MiniCommit::AssistantText {
                content: "two".to_string(),
            }]
        );
        assert_eq!(reducer.footer().message_count, 2);
    }

    #[test]
    fn empty_text_delta_is_skipped() {
        let (commits, _footer) = fold(&[delta(""), delta("x")], "exec-1");
        assert_eq!(
            commits,
            vec![MiniCommit::AssistantText {
                content: "x".to_string(),
            }]
        );
    }

    #[test]
    fn iteration_start_updates_footer_and_flushes_previous_text() {
        let events = vec![delta("old"), it_start(2), delta("new")];
        let (commits, footer) = fold(&events, "exec-1");
        assert_eq!(
            commits,
            vec![
                MiniCommit::AssistantText {
                    content: "old".to_string(),
                },
                MiniCommit::AssistantText {
                    content: "new".to_string(),
                },
            ]
        );
        assert_eq!(footer.iteration, 2);
        assert_eq!(footer.phase, Phase::Streaming);
        assert_eq!(footer.message_count, 2);
    }

    #[test]
    fn iteration_ended_emits_boundary_and_flushes_text() {
        let events = vec![delta("text"), it_end(1)];
        let (commits, _footer) = fold(&events, "exec-1");
        assert_eq!(
            commits,
            vec![
                MiniCommit::AssistantText {
                    content: "text".to_string(),
                },
                MiniCommit::IterationBoundary,
            ]
        );
    }

    #[test]
    fn tool_lifecycle_pairs_update_active_tools() {
        let events = vec![
            t_start("t1", "bash"),
            t_start("t2", "read_file"),
            t_end("t1", "bash", true, Some(12)),
            t_end("t2", "read_file", false, None),
        ];
        let (commits, footer) = fold(&events, "exec-1");
        assert_eq!(
            commits,
            vec![
                MiniCommit::ToolStart {
                    tool_name: "bash".to_string(),
                },
                MiniCommit::ToolStart {
                    tool_name: "read_file".to_string(),
                },
                MiniCommit::ToolEnd {
                    tool_name: "bash".to_string(),
                    success: true,
                    duration_ms: Some(12),
                },
                MiniCommit::ToolEnd {
                    tool_name: "read_file".to_string(),
                    success: false,
                    duration_ms: None,
                },
            ]
        );
        assert!(footer.active_tools.is_empty());
    }

    #[test]
    fn tool_start_marks_streaming_and_active_until_end() {
        let mut reducer = SessionReducer::new("exec-1");
        reducer.push_batch(&[t_start("t1", "bash")]);
        assert_eq!(reducer.footer().active_tools, vec!["bash"]);
        assert_eq!(reducer.footer().phase, Phase::Streaming);
        reducer.push_batch(&[t_end("t1", "bash", true, None)]);
        assert!(reducer.footer().active_tools.is_empty());
    }

    #[test]
    fn duplicate_consecutive_tool_events_are_deduped() {
        let events = vec![
            t_start("t1", "bash"),
            t_start("t1", "bash"), // duplicate delivery
            t_end("t1", "bash", true, None),
            t_end("t1", "bash", true, None), // duplicate delivery
        ];
        let (commits, _footer) = fold(&events, "exec-1");
        assert_eq!(
            commits,
            vec![
                MiniCommit::ToolStart {
                    tool_name: "bash".to_string(),
                },
                MiniCommit::ToolEnd {
                    tool_name: "bash".to_string(),
                    success: true,
                    duration_ms: None,
                },
            ]
        );
    }

    #[test]
    fn different_tool_ids_are_not_deduped() {
        let events = vec![
            t_start("t1", "bash"),
            t_start("t2", "bash"),
            t_end("t1", "bash", true, None),
            t_end("t2", "bash", true, None),
        ];
        let (commits, _footer) = fold(&events, "exec-1");
        assert_eq!(commits.len(), 4);
    }

    #[test]
    fn duplicate_iteration_boundaries_are_deduped() {
        let events = vec![it_end(1), it_end(1)];
        let (commits, _footer) = fold(&events, "exec-1");
        assert_eq!(commits, vec![MiniCommit::IterationBoundary]);
    }

    #[test]
    fn completed_returns_footer_to_idle() {
        let events = vec![
            delta("done"),
            UnifiedEvent::Completed {
                result: serde_json::Value::Null,
                iterations: 3,
            },
        ];
        let (commits, footer) = fold(&events, "exec-1");
        assert_eq!(
            commits,
            vec![
                MiniCommit::AssistantText {
                    content: "done".to_string(),
                },
                MiniCommit::Completed { iterations: 3 },
            ]
        );
        assert_eq!(footer.phase, Phase::Idle);
        assert_eq!(footer.iteration, 0);
    }

    #[test]
    fn failed_records_last_error_and_returns_to_idle() {
        let events = vec![
            delta("oops"),
            UnifiedEvent::Failed {
                error: "boom".to_string(),
            },
        ];
        let (commits, footer) = fold(&events, "exec-1");
        assert_eq!(
            commits,
            vec![
                MiniCommit::AssistantText {
                    content: "oops".to_string(),
                },
                MiniCommit::Failed {
                    error: "boom".to_string(),
                },
            ]
        );
        assert_eq!(footer.phase, Phase::Idle);
        assert_eq!(footer.last_error.as_deref(), Some("boom"));
    }

    #[test]
    fn interrupted_records_reason_and_returns_to_idle() {
        let events = vec![UnifiedEvent::Interrupted {
            reason: "user".to_string(),
        }];
        let (commits, footer) = fold(&events, "exec-1");
        assert_eq!(
            commits,
            vec![MiniCommit::Interrupted {
                reason: "user".to_string(),
            }]
        );
        assert_eq!(footer.phase, Phase::Idle);
        assert_eq!(footer.last_error.as_deref(), Some("user"));
    }

    #[test]
    fn out_of_order_events_produce_deterministic_commit_sequence() {
        // Interleaved tool/iteration events: each non-text event flushes the
        // pending text, so the commit sequence matches arrival order.
        let events = vec![
            delta("a"),
            t_start("t1", "bash"),
            delta("b"),
            it_end(1),
            t_end("t1", "bash", true, None),
        ];
        let (commits, footer) = fold(&events, "exec-1");
        assert_eq!(
            commits,
            vec![
                MiniCommit::AssistantText {
                    content: "a".to_string(),
                },
                MiniCommit::ToolStart {
                    tool_name: "bash".to_string(),
                },
                MiniCommit::AssistantText {
                    content: "b".to_string(),
                },
                MiniCommit::IterationBoundary,
                MiniCommit::ToolEnd {
                    tool_name: "bash".to_string(),
                    success: true,
                    duration_ms: None,
                },
            ]
        );
        assert_eq!(footer.message_count, 2);
    }

    #[test]
    fn execution_lifecycle_events_are_ignored_by_the_reducer() {
        use wf_types::execution::events::{ErrorOccurredEvent, ExecutionEvent};
        let exec = UnifiedEvent::Execution(ExecutionEvent::ErrorOccurred(ErrorOccurredEvent {
            execution_id: "e1".to_string(),
            timestamp: 1,
            message: "nope".to_string(),
            error_type: None,
            iteration: None,
            node_id: None,
        }));
        let (commits, _footer) = fold(&[exec, delta("x")], "exec-1");
        assert_eq!(
            commits,
            vec![MiniCommit::AssistantText {
                content: "x".to_string(),
            }]
        );
    }

    #[test]
    fn fold_and_streaming_reducer_agree_on_the_same_stream() {
        let events = vec![
            it_start(1),
            delta("hello"),
            t_start("t1", "bash"),
            t_end("t1", "bash", true, Some(5)),
            it_end(1),
            UnifiedEvent::Completed {
                result: serde_json::Value::Null,
                iterations: 1,
            },
        ];
        let (fold_commits, fold_footer) = fold(&events, "exec-1");

        let mut reducer = SessionReducer::new("exec-1");
        let mut stream_commits = Vec::new();
        for event in &events {
            stream_commits.extend(reducer.push_batch(std::slice::from_ref(event)));
        }
        assert_eq!(stream_commits, fold_commits);
        assert_eq!(reducer.footer(), &fold_footer);
    }
}
