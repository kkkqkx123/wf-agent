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
                    for kind in screens_for_event(&frame.data) {
                        let _ = tx.send(Feedback::Refresh(kind));
                    }
                }
            }
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    })
}

/// Screens invalidated by one event payload (its `type` field). The
/// vocabulary mirrors the checkpoint event types published by the server;
/// unknown types invalidate nothing.
fn screens_for_event(payload: &serde_json::Value) -> Vec<ScreenKind> {
    match payload.get("type").and_then(|v| v.as_str()) {
        Some("CHECKPOINT_CREATED")
        | Some("CHECKPOINT_RESTORED")
        | Some("CHECKPOINT_DELETED")
        | Some("CHECKPOINT_FAILED")
        | Some("CHECKPOINT_GC_COMPLETED")
        | Some("CHECKPOINT_RESTORE_STARTED")
        | Some("CHECKPOINT_RESTORE_COMPLETED") => {
            vec![ScreenKind::Checkpoints, ScreenKind::Dashboard]
        }
        Some("CHECKPOINT_FILE_CHANGED")
        | Some("CHECKPOINT_MERGE_CONFLICTED")
        | Some("CHECKPOINT_APPROVAL_RESOLVED") => vec![
            ScreenKind::FileApprovals,
            ScreenKind::FileWorkspace,
            ScreenKind::Checkpoints,
            ScreenKind::Conflicts,
        ],
        Some("CHECKPOINT_REVIEW_SUBMITTED") => vec![ScreenKind::FileApprovals],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_events_map_to_their_screens() {
        let created = serde_json::json!({ "type": "CHECKPOINT_CREATED" });
        assert_eq!(
            screens_for_event(&created),
            vec![ScreenKind::Checkpoints, ScreenKind::Dashboard]
        );

        let changed = serde_json::json!({ "type": "CHECKPOINT_FILE_CHANGED" });
        assert_eq!(
            screens_for_event(&changed),
            vec![
                ScreenKind::FileApprovals,
                ScreenKind::FileWorkspace,
                ScreenKind::Checkpoints,
                ScreenKind::Conflicts
            ]
        );

        let submitted = serde_json::json!({ "type": "CHECKPOINT_REVIEW_SUBMITTED" });
        assert_eq!(
            screens_for_event(&submitted),
            vec![ScreenKind::FileApprovals]
        );
    }

    #[test]
    fn unrelated_and_malformed_frames_invalidate_nothing() {
        assert!(screens_for_event(&serde_json::json!({ "type": "connected" })).is_empty());
        assert!(screens_for_event(&serde_json::json!({ "type": "MESSAGE_ADDED" })).is_empty());
        assert!(screens_for_event(&serde_json::json!({})).is_empty());
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
