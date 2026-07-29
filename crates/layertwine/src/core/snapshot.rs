use crate::core::file_node::FileNode;
use crate::core::types::{ContentId, DeltaId, SnapshotId};
use crate::error::{LayertwineError, Result};
use serde::{Deserialize, Serialize};

/// Snapshot content type - supports multiple content forms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SnapshotContent {
    /// File content (existing file snapshots)
    FileContent(Vec<u8>),
    /// JSON metadata (for Agent/Graph execution state)
    JsonMetadata(serde_json::Value),
    /// Structured data (extensible for future formats)
    Structured(Vec<u8>),
}

impl SnapshotContent {
    /// Serialize content to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::FileContent(bytes) => bytes.clone(),
            Self::JsonMetadata(value) => serde_json::to_vec(value).unwrap_or_default(),
            Self::Structured(bytes) => bytes.clone(),
        }
    }

    /// Deserialize bytes to content based on source identifier
    pub fn from_bytes(source: &str, bytes: Vec<u8>) -> Result<Self> {
        match source {
            _ if source.starts_with("file://") => Ok(Self::FileContent(bytes)),
            _ if source.starts_with("agent://") | source.starts_with("graph://") => {
                Ok(Self::JsonMetadata(serde_json::from_slice(&bytes)?))
            }
            _ if source.starts_with("system://") => {
                Ok(Self::JsonMetadata(serde_json::from_slice(&bytes)?))
            }
            _ => Ok(Self::Structured(bytes)),
        }
    }

    /// Get content type label
    pub fn content_type(&self) -> &str {
        match self {
            Self::FileContent(_) => "file",
            Self::JsonMetadata(_) => "json",
            Self::Structured(_) => "structured",
        }
    }

    /// Check if content type matches a given source prefix
    pub fn matches_source(&self, source: &str) -> bool {
        match self {
            Self::FileContent(_) => source.starts_with("file://"),
            Self::JsonMetadata(_) => {
                source.starts_with("agent://")
                    || source.starts_with("graph://")
                    || source.starts_with("system://")
            }
            Self::Structured(_) => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: SnapshotId,
    /// File node (for file-type snapshots; may be absent for metadata snapshots)
    pub file: FileNode,
    pub deltas: Vec<DeltaId>,
    pub parents: Vec<SnapshotId>,
    pub partition_type: String,
    pub created_at: i64,
    pub has_conflicts: bool,
    /// Snapshot content (file bytes, JSON, or structured data)
    #[serde(default)]
    pub content: Option<SnapshotContent>,
    /// Source identifier (e.g. "file://src/main.ts", "agent://loop-1/iteration-5")
    #[serde(default)]
    pub source: String,
    /// Compression method
    #[serde(default)]
    pub compression: SnapshotCompression,
}

/// Snapshot compression method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SnapshotCompression {
    #[default]
    None,
    Zstd,
}

impl Snapshot {
    pub fn new_initial(file: FileNode, delta_id: DeltaId) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let snapshot = Snapshot {
            id: ContentId([0u8; 32]),
            file,
            deltas: vec![delta_id],
            parents: vec![],
            partition_type: String::new(),
            created_at: now,
            has_conflicts: false,
            content: None,
            source: String::new(),
            compression: SnapshotCompression::None,
        };
        let mut s = snapshot;
        s.id = s.compute_id();
        s
    }

    pub fn from_parent(parent: &Snapshot, delta_id: DeltaId, partition_type: String) -> Self {
        let mut deltas = parent.deltas.clone();
        deltas.push(delta_id);

        let now = chrono::Utc::now().timestamp_millis();
        let snapshot = Snapshot {
            id: ContentId([0u8; 32]),
            file: parent.file.clone(),
            deltas,
            parents: vec![parent.id],
            partition_type,
            created_at: now,
            has_conflicts: false,
            content: parent.content.clone(),
            source: parent.source.clone(),
            compression: parent.compression,
        };
        let mut s = snapshot;
        s.id = s.compute_id();
        s
    }

    /// Create a new snapshot with full metadata support
    pub fn new_with_content(
        file: FileNode,
        content: SnapshotContent,
        source: String,
        partition_type: String,
        parents: Vec<SnapshotId>,
        deltas: Vec<DeltaId>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let snapshot = Snapshot {
            id: ContentId([0u8; 32]),
            file,
            deltas,
            parents,
            partition_type,
            created_at: now,
            has_conflicts: false,
            content: Some(content),
            source,
            compression: SnapshotCompression::None,
        };
        let mut s = snapshot;
        s.id = s.compute_id();
        s
    }

    pub fn apply_delta(&self, delta_id: DeltaId) -> Self {
        Snapshot::from_parent(self, delta_id, self.partition_type.clone())
    }

    /// Create a merge snapshot from multiple parents.
    ///
    /// Convention: `parents[0]` MUST be the "destination" partition's current snapshot
    /// (the partition being merged INTO). Its file, delta chain, content, and source
    /// metadata are used as the baseline for the merge result.
    ///
    /// All downstream merge functions follow this convention:
    ///   - merge_manual_to_staged:   [staged, manual]
    ///   - move_agent_to_approval:   [approval, agent]
    ///   - merge_agent_to_feature:   [integrated, approval, baseline]
    ///   - merge_feature_to_staged:  [staged, feature]
    pub fn merge(
        parents: Vec<&Snapshot>,
        delta_id: DeltaId,
        partition_type: String,
        has_conflicts: bool,
    ) -> Self {
        let file = parents[0].file.clone();
        let mut deltas = parents[0].deltas.clone();
        deltas.push(delta_id);

        let now = chrono::Utc::now().timestamp_millis();
        let snapshot = Snapshot {
            id: ContentId([0u8; 32]),
            file,
            deltas,
            parents: parents.iter().map(|p| p.id).collect(),
            partition_type,
            created_at: now,
            has_conflicts,
            content: parents[0].content.clone(),
            source: parents[0].source.clone(),
            compression: parents[0].compression,
        };
        let mut s = snapshot;
        s.id = s.compute_id();
        s
    }

    pub fn compute_id(&self) -> SnapshotId {
        let mut hasher = blake3::Hasher::new();

        let path = self.file.path_str();
        hasher.update(path.as_bytes());
        hasher.update(&self.file.base_hash);

        for delta in &self.deltas {
            hasher.update(delta.0.as_ref());
        }

        for parent in &self.parents {
            hasher.update(parent.0.as_ref());
        }

        hasher.update(self.partition_type.as_bytes());
        hasher.update(&[self.has_conflicts as u8]);
        hasher.update(self.source.as_bytes());

        match &self.content {
            None => {
                hasher.update(b"none");
            }
            Some(c) => {
                hasher.update(c.content_type().as_bytes());
                let content_bytes = c.to_bytes();
                let content_hash = blake3::hash(&content_bytes);
                hasher.update(content_hash.as_bytes().as_ref());
            }
        }

        ContentId(*hasher.finalize().as_bytes())
    }

    /// Compress the snapshot content
    pub fn compress_content(&mut self) -> Result<()> {
        if self.compression == SnapshotCompression::None {
            if let Some(ref content) = self.content {
                let bytes = content.to_bytes();
                let compressed = zstd::encode_all(bytes.as_slice(), 3).map_err(|e| {
                    LayertwineError::Serialization(format!("zstd compression failed: {}", e))
                })?;
                self.content = Some(SnapshotContent::Structured(compressed));
                self.compression = SnapshotCompression::Zstd;
                return Ok(());
            }
        }
        Ok(())
    }

    /// Decompress the snapshot content if compressed
    pub fn decompress_content(&mut self) -> Result<()> {
        if let SnapshotCompression::Zstd = self.compression {
            if let Some(SnapshotContent::Structured(ref bytes)) = self.content {
                let decompressed = zstd::decode_all(bytes.as_slice()).map_err(|e| {
                    LayertwineError::Serialization(format!("zstd decompression failed: {}", e))
                })?;
                self.content = Some(SnapshotContent::from_bytes(&self.source, decompressed)?);
                self.compression = SnapshotCompression::None;
            }
        }
        Ok(())
    }
}

/// Snapshot builder (chaining construction)
#[derive(Debug, Clone)]
pub struct SnapshotBuilder {
    file: Option<FileNode>,
    deltas: Vec<DeltaId>,
    parents: Vec<SnapshotId>,
    partition_type: String,
    has_conflicts: bool,
    content: Option<SnapshotContent>,
    source: String,
    compression: SnapshotCompression,
}

impl SnapshotBuilder {
    pub fn new() -> Self {
        SnapshotBuilder {
            file: None,
            deltas: vec![],
            parents: vec![],
            partition_type: String::new(),
            has_conflicts: false,
            content: None,
            source: String::new(),
            compression: SnapshotCompression::None,
        }
    }

    pub fn file(mut self, file: FileNode) -> Self {
        self.file = Some(file);
        self
    }

    pub fn add_delta(mut self, delta_id: DeltaId) -> Self {
        self.deltas.push(delta_id);
        self
    }

    pub fn with_parent(mut self, parent: SnapshotId) -> Self {
        self.parents.push(parent);
        self
    }

    pub fn with_partition_type(mut self, partition_type: String) -> Self {
        self.partition_type = partition_type;
        self
    }

    pub fn with_conflicts(mut self, has_conflicts: bool) -> Self {
        self.has_conflicts = has_conflicts;
        self
    }

    pub fn content(mut self, content: SnapshotContent) -> Self {
        self.content = Some(content);
        self
    }

    pub fn source(mut self, source: &str) -> Self {
        self.source = source.to_string();
        self
    }

    pub fn compression(mut self, compression: SnapshotCompression) -> Self {
        self.compression = compression;
        self
    }

    pub fn build(self) -> Result<Snapshot> {
        let file = self.file.ok_or_else(|| {
            LayertwineError::Checkpoint("file is required for snapshot".to_string())
        })?;
        let now = chrono::Utc::now().timestamp_millis();
        let snapshot = Snapshot {
            id: ContentId([0u8; 32]),
            file,
            deltas: self.deltas,
            parents: self.parents,
            partition_type: self.partition_type,
            created_at: now,
            has_conflicts: self.has_conflicts,
            content: self.content,
            source: self.source,
            compression: self.compression,
        };
        let mut s = snapshot;
        s.id = s.compute_id();
        Ok(s)
    }
}

impl Default for SnapshotBuilder {
    fn default() -> Self {
        Self::new()
    }
}
