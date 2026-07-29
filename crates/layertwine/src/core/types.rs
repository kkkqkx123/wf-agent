use serde::{Deserialize, Serialize};
use std::fmt;

/// Agent Instance ID Type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentInstanceId(pub String);

impl fmt::Display for AgentInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for AgentInstanceId {
    fn from(s: &str) -> Self {
        AgentInstanceId(s.to_string())
    }
}

impl From<String> for AgentInstanceId {
    fn from(s: String) -> Self {
        AgentInstanceId(s)
    }
}

/// Content ID - Blake3 hash packing type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentId(pub [u8; 32]);

impl ContentId {
    /// Calculate content ID from byte data
    pub fn from_content(data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        ContentId(*hash.as_bytes())
    }

    /// Returns a hexadecimal string representation
    pub fn to_hex(&self) -> String {
        hex_encode(&self.0)
    }

    /// Parsing from hexadecimal strings
    pub fn from_hex(s: &str) -> Option<Self> {
        let bytes = hex_decode(s)?;
        if bytes.len() != 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Some(ContentId(arr))
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// type alias
pub type SnapshotId = ContentId;
pub type DeltaId = ContentId;
pub type CheckpointId = ContentId;
pub type BackupId = ContentId;
pub type PartitionId = uuid::Uuid;

/// Type of source
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    Manual,
    Agent(AgentInstanceId),
    Backup,
}

/// Source identifier constants for snapshot source prefixes
pub mod source {
    pub const AGENT_PREFIX: &str = "agent://";
    pub const GRAPH_PREFIX: &str = "graph://";
    pub const FILE_PREFIX: &str = "file://";
    pub const SYSTEM_PREFIX: &str = "system://";

    /// Check if a source string matches a given wildcard pattern
    /// Supports prefix matching (e.g., "agent://" matches "agent://loop-1/iteration-5")
    /// and simple glob patterns (e.g., "file://src/**")
    pub fn matches_glob(source: &str, pattern: &str) -> bool {
        if pattern == source {
            return true;
        }
        if pattern.ends_with('/') && source.starts_with(pattern) {
            return true;
        }
        if let Some(prefix) = pattern.strip_suffix("/**") {
            return source.starts_with(prefix);
        }
        if let Some(stripped) = pattern.strip_suffix('*') {
            return source.starts_with(stripped);
        }
        source.starts_with(pattern)
    }
}

/// Layering type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayerType {
    ManualEdit,
    AgentEdit,
    Approval,
    Integrated,
    Unified,
    Staged,
}

impl LayerType {
    pub fn name(&self) -> &str {
        match self {
            LayerType::ManualEdit => "manual_edit",
            LayerType::AgentEdit => "agent_edit",
            LayerType::Approval => "approval",
            LayerType::Integrated => "integrated",
            LayerType::Unified => "unified",
            LayerType::Staged => "staged",
        }
    }

    /// Reverse lookup: convert a DB string back to a LayerType variant.
    /// Returns `None` for unknown strings.
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "manual_edit" => Some(LayerType::ManualEdit),
            "agent_edit" => Some(LayerType::AgentEdit),
            "approval" => Some(LayerType::Approval),
            "integrated" => Some(LayerType::Integrated),
            "unified" => Some(LayerType::Unified),
            "staged" => Some(LayerType::Staged),
            _ => None,
        }
    }
}

/// Partition type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartitionType {
    /// manual_edit Unique partition
    Manual,
    /// agent_edit Partitioning by Agent ID
    Agent(AgentInstanceId),
    /// approval Partitioning by instance
    Approval(AgentInstanceId),
    /// INTEGRATED Merged area
    Integrated(String),
    /// UNIFIED catchment area
    Unified,
    /// staged unique partition
    Staged,
}

impl PartitionType {
    pub fn name(&self) -> String {
        match self {
            PartitionType::Manual => "manual".to_string(),
            PartitionType::Agent(id) => format!("agent/{}", id),
            PartitionType::Approval(id) => format!("approval/{}", id),
            PartitionType::Integrated(name) => format!("integrated/{}", name),
            PartitionType::Unified => "unified".to_string(),
            PartitionType::Staged => "staged".to_string(),
        }
    }

    pub fn to_layer(&self) -> crate::core::types::LayerType {
        match self {
            PartitionType::Manual => crate::core::types::LayerType::ManualEdit,
            PartitionType::Agent(_) => crate::core::types::LayerType::AgentEdit,
            PartitionType::Approval(_) => crate::core::types::LayerType::Approval,
            PartitionType::Integrated(_) => crate::core::types::LayerType::Integrated,
            PartitionType::Unified => crate::core::types::LayerType::Unified,
            PartitionType::Staged => crate::core::types::LayerType::Staged,
        }
    }

    /// Parse a string representation back into a PartitionType.
    ///
    /// This is the inverse of `name()` and supports the canonical formats:
    /// - `"manual"`, `"unified"`, `"staged"` (singleton layers)
    /// - `"agent/<id>"`, `"approval/<id>"`, `"integrated/<name>"` (multi-instance layers)
    ///
    /// Returns `None` for unrecognized formats instead of panicking.
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "manual" => Some(PartitionType::Manual),
            "unified" => Some(PartitionType::Unified),
            "staged" => Some(PartitionType::Staged),
            _ => {
                if let Some(rest) = s.strip_prefix("agent/") {
                    Some(PartitionType::Agent(AgentInstanceId(rest.to_string())))
                } else if let Some(rest) = s.strip_prefix("approval/") {
                    Some(PartitionType::Approval(AgentInstanceId(rest.to_string())))
                } else if let Some(rest) = s.strip_prefix("integrated/") {
                    Some(PartitionType::Integrated(rest.to_string()))
                } else {
                    None
                }
            }
        }
    }
}

/// Diff operation type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffOp {
    /// Reservation (same line)
    Equal { count: u32 },
    /// removing
    Delete { old_start: u32, count: u32 },
    /// stick
    Insert { new_start: u32, lines: Vec<String> },
    /// interchangeability
    Replace {
        old_start: u32,
        old_count: u32,
        new_start: u32,
        lines: Vec<String>,
    },
}

/// Hunk - a continuous block of differences
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    pub ops: Vec<DiffOp>,
}

/// Row level differences
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineDiff {
    pub hunks: Vec<Hunk>,
}

impl LineDiff {
    pub fn new(hunks: Vec<Hunk>) -> Self {
        LineDiff { hunks }
    }

    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }
}

// Auxiliary functions -

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_id_from_content() {
        let data = b"hello world";
        let id1 = ContentId::from_content(data);
        let id2 = ContentId::from_content(data);
        assert_eq!(id1, id2);

        let data2 = b"hello world!";
        let id3 = ContentId::from_content(data2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_content_id_hex_roundtrip() {
        let data = b"test data";
        let id1 = ContentId::from_content(data);
        let hex = id1.to_hex();
        let id2 = ContentId::from_hex(&hex).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_content_id_display() {
        let data = b"hello";
        let id = ContentId::from_content(data);
        assert_eq!(id.to_string(), id.to_hex());
    }

    #[test]
    fn test_partition_type_to_layer() {
        assert_eq!(PartitionType::Manual.to_layer(), LayerType::ManualEdit);

        assert_eq!(
            PartitionType::Agent(AgentInstanceId("test".into())).to_layer(),
            LayerType::AgentEdit
        );

        assert_eq!(
            PartitionType::Approval(AgentInstanceId("test".into())).to_layer(),
            LayerType::Approval
        );

        assert_eq!(
            PartitionType::Integrated("test".to_string()).to_layer(),
            LayerType::Integrated
        );

        assert_eq!(PartitionType::Unified.to_layer(), LayerType::Unified);

        assert_eq!(PartitionType::Staged.to_layer(), LayerType::Staged);
    }
}
