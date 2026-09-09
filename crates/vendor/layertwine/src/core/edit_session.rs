use crate::core::types::{DeltaId, EditSessionId};
use serde::{Deserialize, Serialize};

/// An EditSession groups multiple deltas produced by a single user/Agent
/// operation (e.g. a multi-file edit, or a tool invocation that touches
/// several files). Sessions enable atomic rollback of an entire logical
/// operation rather than requiring the caller to manually pop individual
/// snapshots.
///
/// Sessions are lightweight metadata — the immutable delta and snapshot
/// chains remain unchanged. The session merely records which deltas
/// belong together so the layered API can roll them back as a unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditSession {
    /// Unique session identifier.
    pub id: EditSessionId,
    /// Deltas produced during this session, in creation order.
    pub delta_ids: Vec<DeltaId>,
    /// Snapshots produced during this session, in creation order. Covers
    /// full-content snapshots that carry no delta, so a session rollback can
    /// find every record it produced.
    #[serde(default)]
    pub snapshot_ids: Vec<crate::core::types::SnapshotId>,
    /// Human-readable label (e.g. "format file", "refactor module").
    #[serde(default)]
    pub label: Option<String>,
    /// Timestamp when the session was created (millis since epoch).
    pub created_at: i64,
}

impl EditSession {
    /// Create a new empty session.
    pub fn new(label: Option<String>) -> Self {
        EditSession {
            id: uuid::Uuid::now_v7(),
            delta_ids: Vec::new(),
            snapshot_ids: Vec::new(),
            label,
            created_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Append a delta to this session.
    pub fn add_delta(&mut self, delta_id: DeltaId) {
        self.delta_ids.push(delta_id);
    }

    /// Append a snapshot to this session.
    pub fn add_snapshot(&mut self, snapshot_id: crate::core::types::SnapshotId) {
        self.snapshot_ids.push(snapshot_id);
    }

    /// Whether the session contains any deltas or snapshots.
    pub fn is_empty(&self) -> bool {
        self.delta_ids.is_empty() && self.snapshot_ids.is_empty()
    }

    /// Number of deltas in this session.
    pub fn len(&self) -> usize {
        self.delta_ids.len()
    }

    /// Number of snapshots in this session.
    pub fn snapshot_len(&self) -> usize {
        self.snapshot_ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::ContentId;

    #[test]
    fn test_edit_session_new() {
        let session = EditSession::new(Some("test".to_string()));
        assert!(session.delta_ids.is_empty());
        assert!(session.label.is_some());
    }

    #[test]
    fn test_edit_session_add_delta() {
        let mut session = EditSession::new(None);
        let delta_id = ContentId::from_content(b"delta1");
        session.add_delta(delta_id);
        assert_eq!(session.len(), 1);
        assert!(!session.is_empty());
    }
}
