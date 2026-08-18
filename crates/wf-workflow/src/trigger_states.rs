//! Trigger runtime state registry (checkpoint audit support).
//!
//! Tracks event-driven trigger executions attached to their parent
//! execution: which trigger fired, from which event, when, and whether the
//! triggered sub-workflow is still running. The workflow checkpoint
//! integration snapshots these records into `trigger_states` so a checkpoint
//! is *auditable* ("which triggers fired up to this point"). Restoring
//! in-flight triggered sub-workflows is intentionally not attempted yet.

use dashmap::DashMap;

/// One event-driven trigger execution attached to a parent execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TriggerStateRecord {
    /// Trigger template name that fired.
    pub trigger_name: String,
    /// Event id that fired the trigger.
    pub event_id: String,
    /// Canonical event type name.
    pub event_type: String,
    /// Status of the triggered run: `running` | `completed` | `failed` |
    /// `aborted`.
    pub status: String,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

impl TriggerStateRecord {
    pub fn running(
        trigger_name: String,
        event_id: String,
        event_type: String,
        started_at: i64,
    ) -> Self {
        Self {
            trigger_name,
            event_id,
            event_type,
            status: "running".to_string(),
            started_at,
            completed_at: None,
        }
    }

    pub fn finish(&mut self, status: &str) {
        self.status = status.to_string();
        self.completed_at = Some(wf_common::now());
    }
}

/// In-memory registry of trigger executions, keyed by parent execution id.
#[derive(Default)]
pub struct TriggerStateRegistry {
    inner: DashMap<String, Vec<TriggerStateRecord>>,
}

impl TriggerStateRegistry {
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// Record a trigger firing for a parent execution.
    ///
    /// Defensive: when a record with the same `event_id` already exists
    /// (duplicate delivery / replayed event), it is replaced instead of
    /// appended, so a late `record_end` still pairs with a single record and
    /// the audit trail never accumulates duplicates.
    pub fn record_start(&self, parent_execution_id: &str, record: TriggerStateRecord) {
        let mut records = self
            .inner
            .entry(parent_execution_id.to_string())
            .or_default();
        if let Some(existing) = records.iter_mut().find(|r| r.event_id == record.event_id) {
            *existing = record;
        } else {
            records.push(record);
        }
    }

    /// Mark the trigger fired by `event_id` as finished on the parent.
    pub fn record_end(&self, parent_execution_id: &str, event_id: &str, status: &str) {
        if let Some(mut records) = self.inner.get_mut(parent_execution_id) {
            for record in records.iter_mut() {
                if record.event_id == event_id {
                    record.finish(status);
                }
            }
        }
    }

    /// Snapshot of the trigger records for a parent execution, serialized for
    /// the checkpoint `trigger_states` field. `None` when nothing fired.
    pub fn snapshot_for(&self, parent_execution_id: &str) -> Option<serde_json::Value> {
        let records = self.inner.get(parent_execution_id)?;
        if records.is_empty() {
            return None;
        }
        serde_json::to_value(records.iter().cloned().collect::<Vec<_>>()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_snapshots_lifecycle() {
        let registry = TriggerStateRegistry::new();
        assert!(registry.snapshot_for("exec-1").is_none());

        registry.record_start(
            "exec-1",
            TriggerStateRecord::running(
                "on_hook".to_string(),
                "event-1".to_string(),
                "HOOK_TRIGGERED".to_string(),
                1000,
            ),
        );
        let snapshot = registry.snapshot_for("exec-1").expect("record present");
        assert_eq!(snapshot[0]["triggerName"], serde_json::json!("on_hook"));
        assert_eq!(snapshot[0]["status"], serde_json::json!("running"));

        registry.record_end("exec-1", "event-1", "completed");
        let snapshot = registry.snapshot_for("exec-1").expect("record present");
        assert_eq!(snapshot[0]["status"], serde_json::json!("completed"));
        assert!(snapshot[0]["completedAt"].as_i64().is_some());

        // Other executions are isolated.
        assert!(registry.snapshot_for("exec-2").is_none());
    }

    #[test]
    fn end_only_updates_the_matching_event() {
        let registry = TriggerStateRegistry::new();
        registry.record_start(
            "exec-1",
            TriggerStateRecord::running("a".to_string(), "event-1".to_string(), "T".to_string(), 0),
        );
        registry.record_start(
            "exec-1",
            TriggerStateRecord::running("b".to_string(), "event-2".to_string(), "T".to_string(), 0),
        );
        registry.record_end("exec-1", "event-1", "failed");
        let snapshot = registry.snapshot_for("exec-1").expect("records present");
        assert_eq!(snapshot[0]["status"], serde_json::json!("failed"));
        assert_eq!(snapshot[1]["status"], serde_json::json!("running"));
    }

    #[test]
    fn duplicate_event_replaces_previous_record() {
        let registry = TriggerStateRegistry::new();
        registry.record_start(
            "exec-1",
            TriggerStateRecord::running("a".to_string(), "event-1".to_string(), "T".to_string(), 0),
        );
        registry.record_start(
            "exec-1",
            TriggerStateRecord::running("b".to_string(), "event-1".to_string(), "T".to_string(), 0),
        );
        let snapshot = registry.snapshot_for("exec-1").expect("records present");
        assert_eq!(snapshot.as_array().unwrap().len(), 1);
        assert_eq!(snapshot[0]["triggerName"], serde_json::json!("b"));
    }
}
