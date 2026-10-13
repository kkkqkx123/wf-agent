use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use wf_types::events::{BaseEvent, EventType};

use crate::gc::GcStats;
use crate::provenance::DeltaSummary;

const DEFAULT_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointData {
    pub checkpoint_id: Option<String>,
    pub execution_id: Option<String>,
    pub operation: Option<String>,
    pub error: Option<String>,
    pub description: Option<String>,
    pub reason: Option<String>,
}

/// How a pending approval left the pending state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Rejected,
}

impl ApprovalDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalDecision::Approved => "approved",
            ApprovalDecision::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckpointEvent {
    Created {
        base: BaseEvent,
        data: CheckpointData,
    },
    Restored {
        base: BaseEvent,
        data: CheckpointData,
    },
    Deleted {
        base: BaseEvent,
        data: CheckpointData,
    },
    Failed {
        base: BaseEvent,
        data: CheckpointData,
    },
    /// An expected race was skipped (cleanup lost to a concurrent write,
    /// duplicate creation merged back). Skipped outcomes are not failures and
    /// must not pollute failure dashboards.
    Skipped {
        base: BaseEvent,
        data: CheckpointData,
    },
    /// A file change was recorded into a file-checkpoint partition.
    /// `data.checkpoint_id` carries the resulting snapshot
    /// id, `data.description` the file path, and `data.reason` the source
    /// label (`agent:{actor}` / `manual`). `summary` carries the full
    /// `DeltaSummary` (file / source / timestamp / snapshot id / content
    /// hash) for event-stream consumers.
    FileChanged {
        base: BaseEvent,
        data: CheckpointData,
        summary: Option<DeltaSummary>,
    },
    /// A merge produced conflicts. `data.description` carries the
    /// affected file paths (comma-separated) and `paths` the structured
    /// list, so consumers do not have to re-parse the description.
    /// `data.execution_id` carries the acting actor id (named for the
    /// shared-event envelope; file-checkpoint merges have no execution).
    MergeConflicted {
        base: BaseEvent,
        data: CheckpointData,
        paths: Vec<String>,
    },
    /// An actor submission was copied to a review ref and marked pending.
    /// `data.checkpoint_id` carries the review head commit id and
    /// `data.reason` the submitting actor.
    ReviewSubmitted {
        base: BaseEvent,
        data: CheckpointData,
    },
    /// A pending approval left the pending state. `data.checkpoint_id`
    /// carries the resulting commit id, `data.reason` the deciding actor,
    /// and `decision` whether it was approved or rejected.
    ApprovalResolved {
        base: BaseEvent,
        data: CheckpointData,
        decision: ApprovalDecision,
    },
    /// A garbage collection run completed. `data.operation` is `"gc"`,
    /// `data.description` a human summary, and `stats` the full `GcStats`
    /// (removed checkpoints / snapshots).
    GcCompleted {
        base: BaseEvent,
        data: CheckpointData,
        stats: GcStats,
    },
}

pub type EventSender = broadcast::Sender<CheckpointEvent>;
pub type EventReceiver = broadcast::Receiver<CheckpointEvent>;

#[derive(Clone)]
pub struct CheckpointEventBus {
    sender: Arc<EventSender>,
}

impl CheckpointEventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(DEFAULT_CHANNEL_CAPACITY);
        Self {
            sender: Arc::new(sender),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender: Arc::new(sender),
        }
    }

    pub fn subscribe(&self) -> EventReceiver {
        self.sender.subscribe()
    }

    /// Publish a checkpoint event. Returns the number of receivers that got
    /// it. The channel is bounded and non-blocking: when the bus has
    /// subscribers but the slowest one lags, the event is dropped and a
    /// warning is emitted. High fanout per-file bursts should publish one
    /// summary event instead of one event per file.
    pub fn publish(&self, event: CheckpointEvent) -> usize {
        match self.sender.send(event) {
            Ok(received) => received,
            Err(dropped) => {
                if self.sender.receiver_count() > 0 {
                    tracing::warn!(
                        event = ?dropped.0,
                        "checkpoint event dropped: slow subscriber lagged"
                    );
                }
                0
            }
        }
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    fn make_base(event_type: EventType) -> BaseEvent {
        Self::make_base_at(event_type, chrono::Utc::now().timestamp_millis())
    }

    fn make_base_at(event_type: EventType, timestamp_ms: i64) -> BaseEvent {
        BaseEvent {
            id: wf_common::generate_id(),
            r#type: event_type,
            timestamp: timestamp_ms,
            workflow_id: None,
            execution_id: None,
            agent_loop_id: None,

            event_name: None,
            metadata: None,
        }
    }

    pub fn created(checkpoint_id: impl Into<String>) -> CheckpointEvent {
        Self::created_with(checkpoint_id, None, None)
    }

    /// Create a `Created` event with execution id and description filled in.
    pub fn created_with(
        checkpoint_id: impl Into<String>,
        execution_id: Option<String>,
        description: Option<String>,
    ) -> CheckpointEvent {
        CheckpointEvent::Created {
            base: Self::make_base(EventType::CheckpointCreated),
            data: CheckpointData {
                checkpoint_id: Some(checkpoint_id.into()),
                execution_id,
                operation: None,
                error: None,
                description,
                reason: None,
            },
        }
    }

    pub fn restored(
        checkpoint_id: impl Into<String>,
        execution_id: impl Into<String>,
    ) -> CheckpointEvent {
        let mut base = Self::make_base(EventType::CheckpointRestored);
        base.execution_id = Some(execution_id.into());
        CheckpointEvent::Restored {
            base,
            data: CheckpointData {
                checkpoint_id: Some(checkpoint_id.into()),
                execution_id: None,
                operation: None,
                error: None,
                description: None,
                reason: None,
            },
        }
    }

    pub fn deleted(checkpoint_id: impl Into<String>) -> CheckpointEvent {
        Self::deleted_with(checkpoint_id, None)
    }

    /// Create a `Deleted` event carrying the deletion reason
    /// (`"manual" | "cleanup" | "policy"`).
    pub fn deleted_with(
        checkpoint_id: impl Into<String>,
        reason: Option<String>,
    ) -> CheckpointEvent {
        CheckpointEvent::Deleted {
            base: Self::make_base(EventType::CheckpointDeleted),
            data: CheckpointData {
                checkpoint_id: Some(checkpoint_id.into()),
                execution_id: None,
                operation: None,
                error: None,
                description: None,
                reason,
            },
        }
    }

    pub fn failed(operation: impl Into<String>, error: impl Into<String>) -> CheckpointEvent {
        Self::failed_with(None, operation, error, None)
    }

    /// Create a `Failed` event with the checkpoint id and execution id filled
    /// in, so consumers can correlate the failure with the checkpoint.
    pub fn failed_with(
        checkpoint_id: Option<String>,
        operation: impl Into<String>,
        error: impl Into<String>,
        execution_id: Option<String>,
    ) -> CheckpointEvent {
        CheckpointEvent::Failed {
            base: Self::make_base(EventType::CheckpointFailed),
            data: CheckpointData {
                checkpoint_id,
                execution_id,
                operation: Some(operation.into()),
                error: Some(error.into()),
                description: None,
                reason: None,
            },
        }
    }

    /// Create a `FileChanged` event carrying the snapshot id, file path and
    /// source label (provenance feed).
    pub fn file_changed(
        snapshot_id: impl Into<String>,
        file_path: impl Into<String>,
        source: impl Into<String>,
    ) -> CheckpointEvent {
        Self::file_changed_with_summary(snapshot_id, file_path, source, None)
    }

    /// Create a `FileChanged` event carrying the snapshot id, file path,
    /// source label and the full `DeltaSummary` (event-stream consumers).
    pub fn file_changed_with_summary(
        snapshot_id: impl Into<String>,
        file_path: impl Into<String>,
        source: impl Into<String>,
        summary: Option<DeltaSummary>,
    ) -> CheckpointEvent {
        CheckpointEvent::FileChanged {
            base: Self::make_base(EventType::CheckpointFileChanged),
            data: CheckpointData {
                checkpoint_id: Some(snapshot_id.into()),
                execution_id: None,
                operation: None,
                error: None,
                description: Some(file_path.into()),
                reason: Some(source.into()),
            },
            summary,
        }
    }

    /// Create a `MergeConflicted` event carrying the affected file paths
    /// (structured `paths` and a comma-separated `description`), the
    /// merge snapshot id and the acting actor id.
    pub fn merge_conflicted(
        snapshot_id: impl Into<String>,
        paths: Vec<String>,
        actor: Option<String>,
    ) -> CheckpointEvent {
        let description = paths.join(", ");
        CheckpointEvent::MergeConflicted {
            base: Self::make_base(EventType::CheckpointMergeConflicted),
            data: CheckpointData {
                checkpoint_id: Some(snapshot_id.into()),
                execution_id: actor,
                operation: Some("merge".to_string()),
                error: None,
                description: Some(description),
                reason: None,
            },
            paths,
        }
    }

    /// Create a `ReviewSubmitted` event carrying the submitting actor
    /// and the review head commit id.
    pub fn review_submitted(
        actor: impl Into<String>,
        snapshot_id: impl Into<String>,
    ) -> CheckpointEvent {
        CheckpointEvent::ReviewSubmitted {
            base: Self::make_base(EventType::CheckpointReviewSubmitted),
            data: CheckpointData {
                checkpoint_id: Some(snapshot_id.into()),
                execution_id: None,
                operation: None,
                error: None,
                description: None,
                reason: Some(actor.into()),
            },
        }
    }

    /// Create an `ApprovalResolved` event carrying the deciding actor,
    /// the decision and the resulting commit id.
    pub fn approval_resolved(
        actor: impl Into<String>,
        decision: ApprovalDecision,
        snapshot_id: impl Into<String>,
    ) -> CheckpointEvent {
        CheckpointEvent::ApprovalResolved {
            base: Self::make_base(EventType::CheckpointApprovalResolved),
            data: CheckpointData {
                checkpoint_id: Some(snapshot_id.into()),
                execution_id: None,
                operation: Some(decision.as_str().to_string()),
                error: None,
                description: None,
                reason: Some(actor.into()),
            },
            decision,
        }
    }
    /// Create a `GcCompleted` event carrying the GC statistics.
    pub fn gc_completed(stats: GcStats) -> CheckpointEvent {
        let description = format!(
            "gc removed {} checkpoints, {} snapshots",
            stats.removed_checkpoints, stats.removed_snapshots
        );
        CheckpointEvent::GcCompleted {
            base: Self::make_base(EventType::CheckpointGcCompleted),
            data: CheckpointData {
                checkpoint_id: None,
                execution_id: None,
                operation: Some("gc".to_string()),
                error: None,
                description: Some(description),
                reason: None,
            },
            stats,
        }
    }

    /// Create a `Skipped` event for expected races (cleanup contention,
    /// duplicate merge-back). Callers must use this instead of `Failed` so
    /// failure dashboards stay clean.
    pub fn skipped(
        operation: impl Into<String>,
        reason: impl Into<String>,
        checkpoint_id: Option<String>,
    ) -> CheckpointEvent {
        CheckpointEvent::Skipped {
            base: Self::make_base(EventType::CheckpointFailed),
            data: CheckpointData {
                checkpoint_id,
                execution_id: None,
                operation: Some(operation.into()),
                error: None,
                description: None,
                reason: Some(reason.into()),
            },
        }
    }

    /// Timestamped variant of `skipped` for clock-injected paths.
    pub fn skipped_at(
        operation: impl Into<String>,
        reason: impl Into<String>,
        checkpoint_id: Option<String>,
        timestamp_ms: i64,
    ) -> CheckpointEvent {
        CheckpointEvent::Skipped {
            base: Self::make_base_at(EventType::CheckpointFailed, timestamp_ms),
            data: CheckpointData {
                checkpoint_id,
                execution_id: None,
                operation: Some(operation.into()),
                error: None,
                description: None,
                reason: Some(reason.into()),
            },
        }
    }
}

impl Default for CheckpointEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_event_has_correct_type() {
        let event = CheckpointEventBus::created("cp-1");
        match &event {
            CheckpointEvent::Created { base, data } => {
                assert_eq!(data.checkpoint_id, Some("cp-1".to_string()));
                assert_eq!(base.r#type, EventType::CheckpointCreated);
            }
            _ => panic!("expected Created event"),
        }
    }

    #[test]
    fn review_submitted_carries_actor_and_head() {
        let event = CheckpointEventBus::review_submitted("agent:x", "head-1");
        match &event {
            CheckpointEvent::ReviewSubmitted { base, data } => {
                assert_eq!(base.r#type, EventType::CheckpointReviewSubmitted);
                assert_eq!(data.checkpoint_id, Some("head-1".to_string()));
                assert_eq!(data.reason, Some("agent:x".to_string()));
            }
            _ => panic!("expected ReviewSubmitted event"),
        }
    }

    #[test]
    fn approval_resolved_carries_decision() {
        for (decision, wire) in [
            (ApprovalDecision::Approved, "approved"),
            (ApprovalDecision::Rejected, "rejected"),
        ] {
            let event =
                CheckpointEventBus::approval_resolved("agent:x", decision, "snap-1");
            match &event {
                CheckpointEvent::ApprovalResolved {
                    base,
                    data,
                    decision: back,
                } => {
                    assert_eq!(base.r#type, EventType::CheckpointApprovalResolved);
                    assert_eq!(data.checkpoint_id, Some("snap-1".to_string()));
                    assert_eq!(data.reason, Some("agent:x".to_string()));
                    assert_eq!(data.operation, Some(wire.to_string()));
                    assert_eq!(*back, decision);
                }
                _ => panic!("expected ApprovalResolved event"),
            }
        }
    }

    #[test]
    fn merge_conflicted_carries_actor() {
        let event = CheckpointEventBus::merge_conflicted(
            "snap-1",
            vec!["a.txt".to_string()],
            Some("agent:x".to_string()),
        );
        match &event {
            CheckpointEvent::MergeConflicted { base, data, paths } => {
                assert_eq!(base.r#type, EventType::CheckpointMergeConflicted);
                assert_eq!(data.execution_id, Some("agent:x".to_string()));
                assert_eq!(paths, &vec!["a.txt".to_string()]);
            }
            _ => panic!("expected MergeConflicted event"),
        }
    }

    #[test]
    fn publish_and_receive() {
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();

        bus.publish(CheckpointEventBus::created("cp-1"));
        let event = rx.try_recv().unwrap();

        match event {
            CheckpointEvent::Created { data, .. } => {
                assert_eq!(data.checkpoint_id, Some("cp-1".to_string()))
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn multiple_subscribers() {
        let bus = CheckpointEventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(CheckpointEventBus::deleted("cp-1"));

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }
}
