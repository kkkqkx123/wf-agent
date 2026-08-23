//! EventBus bridge for file-checkpoint events.
//!
//! Forwards `wf_checkpoint::event::CheckpointEvent`s (file changes and merge
//! conflicts) to the shared `wf_core::EventBus` as
//! `wf_types::events::BaseEvent`s. The `DeltaSummary` of a file change is
//! carried in the event metadata (`file` / `source` / `timestamp` /
//! `snapshot_id` / `hash`), so event-stream consumers see exactly what the
//! provenance API returns. The bridge lives in `wf-runtime` (an upper
//! crate) so `wf-checkpoint` keeps its strict dependency DAG.

use std::collections::HashMap;
use std::sync::Arc;

use wf_core::event::EventBus;
use wf_types::events::{BaseEvent, EventType};

use wf_checkpoint::event::{CheckpointEvent, CheckpointEventBus};

/// Spawn a forwarding task subscribing to the checkpoint event bus and
/// publishing onto the shared event bus. Returns the task handle; the
/// attached `CheckpointEventBus` should be given to the file checkpoint
/// manager (`with_event_bus`).
pub fn spawn(
    bus: Arc<EventBus>,
    checkpoint_bus: CheckpointEventBus,
) -> tokio::task::JoinHandle<()> {
    let receiver = checkpoint_bus.subscribe();
    tokio::spawn(async move {
        let mut receiver = receiver;
        loop {
            match receiver.recv().await {
                Ok(event) => forward(&bus, &event),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

/// Forward one checkpoint event onto the shared event bus (best-effort).
fn forward(bus: &Arc<EventBus>, event: &CheckpointEvent) {
    let (event_type, metadata) = match event {
        CheckpointEvent::FileChanged { data, summary, .. } => {
            let mut metadata = HashMap::new();
            if let Some(snapshot_id) = &data.checkpoint_id {
                metadata.insert("snapshot_id".to_string(), serde_json::json!(snapshot_id));
            }
            if let Some(file) = &data.description {
                metadata.insert("file".to_string(), serde_json::json!(file));
            }
            if let Some(source) = &data.reason {
                metadata.insert("source".to_string(), serde_json::json!(source));
            }
            if let Some(summary) = summary {
                metadata.insert(
                    "timestamp".to_string(),
                    serde_json::json!(summary.timestamp),
                );
                metadata.insert("hash".to_string(), serde_json::json!(summary.hash));
            }
            (EventType::CheckpointFileChanged, metadata)
        }
        CheckpointEvent::MergeConflicted { data, .. } => {
            let mut metadata = HashMap::new();
            if let Some(snapshot_id) = &data.checkpoint_id {
                metadata.insert("snapshot_id".to_string(), serde_json::json!(snapshot_id));
            }
            if let Some(files) = &data.description {
                metadata.insert("conflict_files".to_string(), serde_json::json!(files));
            }
            if let Some(actor) = &data.reason {
                metadata.insert("actor".to_string(), serde_json::json!(actor));
            }
            (EventType::CheckpointMergeConflicted, metadata)
        }
        // Checkpoint lifecycle events (created / restored / deleted /
        // failed) are published by the checkpoint coordinators through their
        // own bus; they are not bridged here.
        CheckpointEvent::Created { .. }
        | CheckpointEvent::Restored { .. }
        | CheckpointEvent::Deleted { .. }
        | CheckpointEvent::Failed { .. }
        | CheckpointEvent::GcCompleted { .. } => return,
    };

    let _ = bus.publish(BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: None,
        agent_loop_id: None,
        event_name: None,
        metadata: Some(metadata),
    });
}
