use crate::core::file_node::FileNode;
use crate::core::types::{ContentId, DeltaId, DiffOp, EditSessionId, LineDiff, SourceType};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Pure content hash of a Delta's diff payload. Enables storage-level
/// deduplication without affecting record uniqueness (Delta ID includes
/// timestamp + seq + instance_id for uniqueness).
pub fn compute_delta_content_hash(diff: &LineDiff) -> ContentId {
    let json = serde_json::to_vec(diff).unwrap_or_default();
    ContentId::from_content(&json)
}

/// Monotonic counter that makes each Delta::new invocation unique even when
/// two identical edits happen within the same millisecond.
static DELTA_SEQ: AtomicU64 = AtomicU64::new(0);

// Process-unique instance identifier, generated once at startup. Combined
// with the monotonic `DELTA_SEQ`, this guarantees Delta ID uniqueness across
// multiple processes writing to the same database — without external
// coordination (e.g. UUID v7).
lazy_static::lazy_static! {
    static ref INSTANCE_ID: u64 = rand::random::<u64>();
}

/// Return the process-unique instance id for Delta ID hashing.
pub fn delta_instance_id() -> u64 {
    *INSTANCE_ID
}

#[derive(Serialize)]
struct DeltaForId<'a> {
    file: &'a FileNode,
    diff: &'a LineDiff,
    source: &'a SourceType,
    timestamp: i64,
    seq: u64,
    /// Process-unique prefix that prevents ID collisions across processes.
    instance_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delta {
    pub id: DeltaId,
    pub file: FileNode,
    pub diff: LineDiff,
    pub source: SourceType,
    pub timestamp: i64,
    /// Per-invocation ordinal; guarantees id uniqueness for same-millisecond edits.
    pub seq: u64,
    /// Optional session grouping: when set, multiple deltas from the same
    /// user/Agent operation can be rolled back atomically as a unit.
    #[serde(default)]
    pub session_id: Option<EditSessionId>,
    /// Pure content hash of the diff payload. Enables storage-level
    /// deduplication: two deltas with identical diff content share the same
    /// `content_hash` even though their record IDs differ (timestamp + seq).
    #[serde(default)]
    pub content_hash: Option<ContentId>,
    /// Optional human-readable description of the edit intent (e.g. "fix typo",
    /// "rename variable", "add error handling"). Improves history browsing by
    /// providing semantic context without reconstructing the diff.
    #[serde(default)]
    pub message: Option<String>,
}

impl Delta {
    pub fn new(file: FileNode, diff: LineDiff, source: SourceType) -> Self {
        Self::new_with_session(file, diff, source, None)
    }

    pub fn new_with_session(
        file: FileNode,
        diff: LineDiff,
        source: SourceType,
        session_id: Option<EditSessionId>,
    ) -> Self {
        Self::new_with_session_and_message(file, diff, source, session_id, None)
    }

    pub fn new_with_session_and_message(
        file: FileNode,
        diff: LineDiff,
        source: SourceType,
        session_id: Option<EditSessionId>,
        message: Option<String>,
    ) -> Self {
        let timestamp = chrono::Utc::now().timestamp_millis();
        let seq = DELTA_SEQ.fetch_add(1, Ordering::Relaxed);
        let content_hash = compute_delta_content_hash(&diff);
        let mut delta = Delta {
            id: ContentId([0u8; 32]),
            file,
            diff,
            source,
            timestamp,
            seq,
            session_id,
            content_hash: Some(content_hash),
            message,
        };
        delta.id = delta.compute_id();
        delta
    }

    pub fn compute_id(&self) -> DeltaId {
        let delta_for_id = DeltaForId {
            file: &self.file,
            diff: &self.diff,
            source: &self.source,
            timestamp: self.timestamp,
            seq: self.seq,
            instance_id: delta_instance_id(),
        };
        let json = serde_json::to_vec(&delta_for_id).unwrap_or_default();
        ContentId::from_content(&json)
    }
}

/// Application summary: how many lines changed
pub struct DeltaSummary {
    pub inserts: usize,
    pub deletes: usize,
    pub replaces: usize,
    pub total_hunks: usize,
}

impl Delta {
    /// Volume of statistical change
    pub fn summary(&self) -> DeltaSummary {
        let mut inserts = 0;
        let mut deletes = 0;
        let mut replaces = 0;

        for hunk in &self.diff.hunks {
            for op in &hunk.ops {
                match op {
                    DiffOp::Insert { lines, .. } => inserts += lines.len(),
                    DiffOp::Delete { count, .. } => deletes += *count as usize,
                    DiffOp::Replace { lines, .. } => replaces += lines.len(),
                    DiffOp::Equal { .. } => {}
                }
            }
        }

        DeltaSummary {
            inserts,
            deletes,
            replaces,
            total_hunks: self.diff.hunks.len(),
        }
    }
}
