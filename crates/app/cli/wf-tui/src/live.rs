//! Live invalidation over the server event streams.
//!
//! The full TUI is a pure HTTP client; without subscriptions it only sees
//! changes on manual refresh or cache-TTL expiry. This module owns two
//! long-lived `GET /api/v1/events/stream` subscriptions and turns each
//! event into a screen or sidebar invalidation, so the screens converge
//! without user input:
//!
//! - checkpoint feed (`checkpoint_only=true`): file-checkpoint domain;
//! - execution feed (`event_types=...`): coarse execution lifecycle plus
//!   the git and definition-lock transitions, server-filtered so the
//!   high-frequency frame noise (LLM deltas, progress) never crosses.
//!
//! Pushes only invalidate; the next event-loop iteration refetches through
//! the normal TTL-guarded path. Bursts coalesce over a short window so one
//! transition never triggers more than one invalidation round.
//!
//! The tasks never block the UI: they forward [`Feedback`] through the same
//! channel the action handlers use, and the event loop picks it up on its
//! next wakeup.

use futures::StreamExt;
use tokio::sync::mpsc::UnboundedSender;

use wf_cli_shared::run_interaction::stream_reconnect_delay;
use wf_client::WfClient;

use crate::overlay::Feedback;
use crate::screens::ScreenKind;

/// Burst coalescing window: impacts landing together invalidate once.
const COALESCE_WINDOW: std::time::Duration = std::time::Duration::from_millis(300);

/// Coarse execution-lifecycle types for the server-filtered execution feed.
/// High-frequency frames (deltas, progress, heartbeats) stay out so the
/// feed only carries transitions the screens can show.
const EXECUTION_FEED_TYPES: &str = "WORKFLOW_EXECUTION_STARTED,WORKFLOW_EXECUTION_COMPLETED,WORKFLOW_EXECUTION_FAILED,WORKFLOW_EXECUTION_CANCELLED,WORKFLOW_EXECUTION_PAUSED,WORKFLOW_EXECUTION_RESUMED,WORKFLOW_EXECUTION_STATE_CHANGED,NODE_STARTED,NODE_COMPLETED,NODE_FAILED,NODE_SKIPPED,TOOL_CALL_STARTED,TOOL_CALL_COMPLETED,TOOL_CALL_FAILED,AGENT_STARTED,AGENT_COMPLETED,AGENT_FAILED,GIT_CHANGED,DEFINITION_LOCK_CHANGED";

/// Screens that benefit from live invalidation; the event loop shortens its
/// poll timeout while one of these is visible so updates land promptly.
pub fn is_live_screen(kind: ScreenKind) -> bool {
    matches!(
        kind,
        ScreenKind::Checkpoints
            | ScreenKind::FileWorkspace
            | ScreenKind::FileApprovals
            | ScreenKind::Conflicts
            | ScreenKind::Executions
            | ScreenKind::Workflow
            | ScreenKind::Dashboard
            | ScreenKind::Git
    )
}

/// Screen plus sidebar impact of one bus event.
struct FeedImpact {
    screens: Vec<ScreenKind>,
    sidebar: bool,
}

impl FeedImpact {
    fn empty() -> Self {
        Self {
            screens: Vec::new(),
            sidebar: false,
        }
    }

    fn of(screens: Vec<ScreenKind>, sidebar: bool) -> Self {
        Self { screens, sidebar }
    }

    fn is_empty(&self) -> bool {
        self.screens.is_empty() && !self.sidebar
    }

    /// Merge a burst companion into this impact, deduplicating screens.
    fn absorb(&mut self, other: Self) {
        for kind in other.screens {
            if !self.screens.contains(&kind) {
                self.screens.push(kind);
            }
        }
        self.sidebar |= other.sidebar;
    }
}

/// Spawn the event-stream consumers. They run until the process exits (the
/// handle is aborted during teardown) and reconnect with backoff on drop.
pub fn spawn(client: WfClient, tx: UnboundedSender<Feedback>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let checkpoint_path = format!(
            "{}?checkpoint_only=true",
            wf_client::endpoints::EVENTS_STREAM
        );
        let execution_path = format!(
            "{}?event_types={EXECUTION_FEED_TYPES}",
            wf_client::endpoints::EVENTS_STREAM
        );
        let checkpoint = tokio::spawn(run_feed(client.clone(), tx.clone(), checkpoint_path));
        let execution = tokio::spawn(run_feed(client, tx, execution_path));
        let _ = tokio::join!(checkpoint, execution);
    })
}

/// Consume one filtered event stream until the process exits, forwarding
/// coalesced invalidations and reconnecting with backoff on drop.
async fn run_feed(client: WfClient, tx: UnboundedSender<Feedback>, path: String) {
    let mut attempt = 0u32;
    loop {
        if let Ok(stream) = client.get_stream(&path).await {
            attempt = 0;
            let mut stream = Box::pin(stream);
            while let Some(frame) = stream.next().await {
                let Ok(frame) = frame else {
                    break;
                };
                let decoded = wf_client::sse::decode_system_frame(&frame);
                let Some(base) = decoded.into_base_event() else {
                    continue;
                };
                let mut impact = impact_for_event(&base);
                if impact.is_empty() {
                    continue;
                }
                // Coalesce bursts: one invalidation per quiet window.
                while let Ok(Some(Ok(next))) =
                    tokio::time::timeout(COALESCE_WINDOW, stream.next()).await
                {
                    let decoded = wf_client::sse::decode_system_frame(&next);
                    if let Some(next_base) = decoded.into_base_event() {
                        impact.absorb(impact_for_event(&next_base));
                    }
                }
                for kind in impact.screens {
                    let _ = tx.send(Feedback::Refresh(kind));
                }
                if impact.sidebar {
                    let _ = tx.send(Feedback::RefreshSidebar);
                }
            }
        }
        let delay = stream_reconnect_delay(attempt);
        attempt = attempt.saturating_add(1);
        tokio::time::sleep(delay).await;
    }
}

/// Invalidation impact of one bus event. The vocabulary mirrors the event
/// types published by the server; unknown or high-frequency types
/// invalidate nothing.
fn impact_for_event(event: &wf_types::events::BaseEvent) -> FeedImpact {
    use wf_types::events::EventType as T;
    match event.r#type {
        T::CheckpointCreated
        | T::CheckpointRestored
        | T::CheckpointDeleted
        | T::CheckpointRestoreStarted
        | T::CheckpointRestoreCompleted
        | T::CheckpointReviewSubmitted
        | T::CheckpointFileChanged
        | T::CheckpointApprovalResolved
        | T::CheckpointMergeConflicted => FeedImpact::of(
            vec![
                ScreenKind::FileApprovals,
                ScreenKind::FileWorkspace,
                ScreenKind::Checkpoints,
                ScreenKind::Conflicts,
                ScreenKind::Dashboard,
            ],
            true,
        ),
        T::CheckpointFailed | T::CheckpointGcCompleted => {
            FeedImpact::of(vec![ScreenKind::Checkpoints, ScreenKind::Dashboard], false)
        }
        T::WorkflowExecutionStarted
        | T::WorkflowExecutionCompleted
        | T::WorkflowExecutionFailed
        | T::WorkflowExecutionCancelled
        | T::WorkflowExecutionPaused
        | T::WorkflowExecutionResumed
        | T::WorkflowExecutionStateChanged => FeedImpact::of(
            vec![
                ScreenKind::Executions,
                ScreenKind::Workflow,
                ScreenKind::Dashboard,
            ],
            true,
        ),
        T::NodeStarted | T::NodeCompleted | T::NodeFailed | T::NodeSkipped => {
            FeedImpact::of(vec![ScreenKind::Executions, ScreenKind::Workflow], true)
        }
        T::ToolCallStarted | T::ToolCallCompleted | T::ToolCallFailed => {
            FeedImpact::of(vec![ScreenKind::Executions], true)
        }
        T::AgentStarted | T::AgentCompleted | T::AgentFailed => {
            FeedImpact::of(vec![ScreenKind::Dashboard, ScreenKind::Executions], false)
        }
        T::GitChanged => FeedImpact::of(vec![ScreenKind::Git], true),
        T::DefinitionLockChanged => FeedImpact::of(vec![ScreenKind::EditSessions], false),
        _ => FeedImpact::empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::events::{BaseEvent, EventType};

    fn base_fixture(event_type: EventType) -> BaseEvent {
        BaseEvent {
            id: "evt-1".to_string(),
            r#type: event_type,
            timestamp: 1,
            event_name: None,
            workflow_id: None,
            execution_id: Some("exec-1".to_string()),
            agent_loop_id: None,
            metadata: None,
        }
    }

    #[test]
    fn checkpoint_events_map_to_their_screens() {
        let impact = impact_for_event(&base_fixture(EventType::CheckpointCreated));
        assert_eq!(
            impact.screens,
            vec![
                ScreenKind::FileApprovals,
                ScreenKind::FileWorkspace,
                ScreenKind::Checkpoints,
                ScreenKind::Conflicts,
                ScreenKind::Dashboard,
            ]
        );
        assert!(impact.sidebar);

        let impact = impact_for_event(&base_fixture(EventType::CheckpointGcCompleted));
        assert_eq!(
            impact.screens,
            vec![ScreenKind::Checkpoints, ScreenKind::Dashboard]
        );
        assert!(!impact.sidebar);
    }

    #[test]
    fn execution_events_invalidate_execution_screens() {
        let impact = impact_for_event(&base_fixture(EventType::WorkflowExecutionCompleted));
        assert_eq!(
            impact.screens,
            vec![
                ScreenKind::Executions,
                ScreenKind::Workflow,
                ScreenKind::Dashboard,
            ]
        );
        assert!(impact.sidebar);

        let impact = impact_for_event(&base_fixture(EventType::NodeCompleted));
        assert_eq!(
            impact.screens,
            vec![ScreenKind::Executions, ScreenKind::Workflow]
        );

        let impact = impact_for_event(&base_fixture(EventType::AgentCompleted));
        assert_eq!(
            impact.screens,
            vec![ScreenKind::Dashboard, ScreenKind::Executions]
        );
        assert!(!impact.sidebar);
    }

    #[test]
    fn git_and_lock_events_map_to_their_screens() {
        let impact = impact_for_event(&base_fixture(EventType::GitChanged));
        assert_eq!(impact.screens, vec![ScreenKind::Git]);
        assert!(impact.sidebar);

        let impact = impact_for_event(&base_fixture(EventType::DefinitionLockChanged));
        assert_eq!(impact.screens, vec![ScreenKind::EditSessions]);
        assert!(!impact.sidebar);
    }

    #[test]
    fn unrelated_and_malformed_frames_invalidate_nothing() {
        assert!(impact_for_event(&base_fixture(EventType::MessageAdded)).is_empty());
        assert!(impact_for_event(&base_fixture(EventType::Heartbeat)).is_empty());
        assert!(impact_for_event(&base_fixture(EventType::LlmStreamChunk)).is_empty());
    }

    #[test]
    fn hello_and_unknown_frames_decode_to_no_base_event() {
        let hello = wf_client::SseFrame {
            event: String::new(),
            data: serde_json::json!({"type": "connected", "data": {"executionId": "e1"}}),
        };
        assert!(wf_client::sse::decode_system_frame(&hello)
            .into_base_event()
            .is_none());
        let unknown = wf_client::SseFrame {
            event: String::new(),
            data: serde_json::json!({"type": "future_unknown", "x": 1}),
        };
        assert!(wf_client::sse::decode_system_frame(&unknown)
            .into_base_event()
            .is_none());
    }

    #[test]
    fn burst_impacts_merge_without_duplicate_screens() {
        let mut first = impact_for_event(&base_fixture(EventType::NodeCompleted));
        first.absorb(impact_for_event(&base_fixture(
            EventType::WorkflowExecutionCompleted,
        )));
        assert_eq!(
            first.screens,
            vec![
                ScreenKind::Executions,
                ScreenKind::Workflow,
                ScreenKind::Dashboard,
            ]
        );
        assert!(first.sidebar);
    }

    #[test]
    fn live_screens_cover_subscribed_surfaces() {
        assert!(is_live_screen(ScreenKind::Checkpoints));
        assert!(is_live_screen(ScreenKind::FileWorkspace));
        assert!(is_live_screen(ScreenKind::FileApprovals));
        assert!(is_live_screen(ScreenKind::Executions));
        assert!(is_live_screen(ScreenKind::Workflow));
        assert!(is_live_screen(ScreenKind::Dashboard));
        assert!(is_live_screen(ScreenKind::Git));
        assert!(!is_live_screen(ScreenKind::Settings));
    }
}
