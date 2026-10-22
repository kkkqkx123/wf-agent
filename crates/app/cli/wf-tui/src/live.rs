//! Live checkpoint updates over the server event stream.
//!
//! The full TUI is a pure HTTP client; without a subscription it only sees
//! checkpoint changes on the next manual refresh or cache-TTL expiry. This
//! module owns one long-lived `GET /api/v1/events/stream` subscription
//! (checkpoint-domain only) and turns each event into a screen
//! invalidation, so the checkpoint screens converge without user input.
//!
//! The task never blocks the UI: it forwards [`Feedback::Refresh`] through
//! the same channel the action handlers use, and the event loop picks it up
//! on its next wakeup.

use std::time::Duration;

use futures::StreamExt;
use tokio::sync::mpsc::UnboundedSender;

use wf_client::WfClient;

use crate::overlay::Feedback;
use crate::screens::ScreenKind;

/// Delay before retrying a dropped event stream.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// Screens that benefit from live invalidation; the event loop shortens its
/// poll timeout while one of these is visible so updates land promptly.
pub fn is_live_screen(kind: ScreenKind) -> bool {
    matches!(
        kind,
        ScreenKind::Checkpoints
            | ScreenKind::FileWorkspace
            | ScreenKind::FileApprovals
            | ScreenKind::Conflicts
    )
}

/// Spawn the checkpoint event-stream consumer. It runs until the process
/// exits (the handle is aborted during teardown) and reconnects on drop.
pub fn spawn(client: WfClient, tx: UnboundedSender<Feedback>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let path = format!(
                "{}?checkpoint_only=true",
                wf_client::endpoints::EVENTS_STREAM
            );
            if let Ok(stream) = client.get_stream(&path).await {
                let mut stream = Box::pin(stream);
                while let Some(frame) = stream.next().await {
                    let Ok(frame) = frame else {
                        break;
                    };
                    let decoded = wf_client::sse::decode_system_frame(&frame);
                    let Some(base) = decoded.into_base_event() else {
                        continue;
                    };
                    for kind in screens_for_event(&base) {
                        let _ = tx.send(Feedback::Refresh(kind));
                    }
                }
            }
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    })
}

/// Screens invalidated by one bus event. The vocabulary mirrors the
/// checkpoint event types published by the server; unknown types invalidate
/// nothing.
fn screens_for_event(event: &wf_types::events::BaseEvent) -> Vec<ScreenKind> {
    use wf_types::events::EventType as T;
    match event.r#type {
        T::CheckpointCreated
        | T::CheckpointRestored
        | T::CheckpointDeleted
        | T::CheckpointFailed
        | T::CheckpointGcCompleted
        | T::CheckpointRestoreStarted
        | T::CheckpointRestoreCompleted => {
            vec![ScreenKind::Checkpoints, ScreenKind::Dashboard]
        }
        T::CheckpointFileChanged | T::CheckpointMergeConflicted | T::CheckpointApprovalResolved => {
            vec![
                ScreenKind::FileApprovals,
                ScreenKind::FileWorkspace,
                ScreenKind::Checkpoints,
                ScreenKind::Conflicts,
            ]
        }
        T::CheckpointReviewSubmitted => vec![ScreenKind::FileApprovals],
        _ => Vec::new(),
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
        assert_eq!(
            screens_for_event(&base_fixture(EventType::CheckpointCreated)),
            vec![ScreenKind::Checkpoints, ScreenKind::Dashboard]
        );

        assert_eq!(
            screens_for_event(&base_fixture(EventType::CheckpointFileChanged)),
            vec![
                ScreenKind::FileApprovals,
                ScreenKind::FileWorkspace,
                ScreenKind::Checkpoints,
                ScreenKind::Conflicts
            ]
        );

        assert_eq!(
            screens_for_event(&base_fixture(EventType::CheckpointReviewSubmitted)),
            vec![ScreenKind::FileApprovals]
        );
    }

    #[test]
    fn unrelated_and_malformed_frames_invalidate_nothing() {
        assert!(screens_for_event(&base_fixture(EventType::MessageAdded)).is_empty());
        assert!(screens_for_event(&base_fixture(EventType::Heartbeat)).is_empty());
    }

    #[test]
    fn hello_and_unknown_frames_decode_to_no_base_event() {
        let hello = wf_client::SseFrame {
            event: String::new(),
            data: serde_json::json!({"type": "connected", "data": {"executionId": "e1"}}),
        };
        assert!(
            wf_client::sse::decode_system_frame(&hello)
                .into_base_event()
                .is_none()
        );
        let unknown = wf_client::SseFrame {
            event: String::new(),
            data: serde_json::json!({"type": "future_unknown", "x": 1}),
        };
        assert!(
            wf_client::sse::decode_system_frame(&unknown)
                .into_base_event()
                .is_none()
        );
    }

    #[test]
    fn live_screens_are_the_checkpoint_surfaces() {
        assert!(is_live_screen(ScreenKind::Checkpoints));
        assert!(is_live_screen(ScreenKind::FileWorkspace));
        assert!(is_live_screen(ScreenKind::FileApprovals));
        assert!(!is_live_screen(ScreenKind::Dashboard));
        assert!(!is_live_screen(ScreenKind::Executions));
    }
}
