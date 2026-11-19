use serde::{Deserialize, Serialize};

/// One hashable part of the restorable projection of an execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionComponent {
    pub kind: String,
    pub id: String,
    pub hash: String,
}

/// Durable revision row of one execution. The sequence drives drift
/// detection; the digest verifies that the recorded content still matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRevision {
    pub execution_id: String,
    pub sequence: u64,
    pub digest: String,
    pub updated_at_ms: i64,
    pub source: String,
    pub reason: String,
    pub components: Vec<RevisionComponent>,
}
