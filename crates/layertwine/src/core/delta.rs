use crate::core::file_node::FileNode;
use crate::core::types::{ContentId, DeltaId, DiffOp, LineDiff, SourceType};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct DeltaForId<'a> {
    file: &'a FileNode,
    diff: &'a LineDiff,
    source: &'a SourceType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delta {
    pub id: DeltaId,
    pub file: FileNode,
    pub diff: LineDiff,
    pub source: SourceType,
    pub timestamp: i64,
}

impl Delta {
    pub fn new(file: FileNode, diff: LineDiff, source: SourceType) -> Self {
        let timestamp = chrono::Utc::now().timestamp_millis();
        let mut delta = Delta {
            id: ContentId([0u8; 32]),
            file,
            diff,
            source,
            timestamp,
        };
        delta.id = delta.compute_id();
        delta
    }

    pub fn compute_id(&self) -> DeltaId {
        let delta_for_id = DeltaForId {
            file: &self.file,
            diff: &self.diff,
            source: &self.source,
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
