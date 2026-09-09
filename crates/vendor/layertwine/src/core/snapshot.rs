use crate::core::file_node::FileNode;
use crate::core::types::{ContentId, DeltaId, SnapshotId};
use crate::error::{LayertwineError, Result};
use serde::{Deserialize, Serialize};

/// Compute a pure content hash from snapshot content bytes. Returns `None`
/// when the content is absent (delta-chain reconstructed) or a deletion
/// marker — these cases have no fixed content to hash.
pub fn compute_snapshot_content_hash(content: &Option<SnapshotContent>) -> Option<ContentId> {
    match content {
        Some(c) if !c.is_deleted() => {
            let bytes = c.to_bytes();
            Some(ContentId::from_content(&bytes))
        }
        _ => None,
    }
}

/// Snapshot content type - supports multiple content forms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SnapshotContent {
    /// File content (existing file snapshots)
    FileContent(Vec<u8>),
    /// JSON metadata (for Agent/Graph execution state)
    JsonMetadata(serde_json::Value),
    /// Structured data (extensible for future formats)
    Structured(Vec<u8>),
    /// Explicit file deletion marker: the file was removed by an
    /// agent/manual edit. Carries no content; reconstruction yields an
    /// empty string and the projection/restore layers treat the path as
    /// missing rather than cleared.
    Deleted,
}

impl SnapshotContent {
    /// Serialize content to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::FileContent(bytes) => bytes.clone(),
            Self::JsonMetadata(value) => serde_json::to_vec(value).unwrap_or_default(),
            Self::Structured(bytes) => bytes.clone(),
            Self::Deleted => Vec::new(),
        }
    }

    /// Get content type label
    pub fn content_type(&self) -> &str {
        match self {
            Self::FileContent(_) => "file",
            Self::JsonMetadata(_) => "json",
            Self::Structured(_) => "structured",
            Self::Deleted => "deleted",
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
            Self::Deleted => source.starts_with("file://"),
        }
    }

    /// Whether this content is the explicit deletion marker.
    pub fn is_deleted(&self) -> bool {
        matches!(self, Self::Deleted)
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
    /// Pure content hash of the snapshot's resulting file bytes. Enables
    /// storage-level deduplication and cross-layer content identity checks
    /// without depending on the delta chain (which embeds timestamps).
    #[serde(default)]
    pub content_hash: Option<ContentId>,
    /// Optional human-readable description of the snapshot intent. Persisted
    /// in the `snapshots.message` column so it survives process restarts.
    #[serde(default)]
    pub message: Option<String>,
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
            content_hash: None,
            message: None,
        };
        let mut s = snapshot;
        s.id = s.compute_id();
        s
    }

    pub fn from_parent(parent: &Snapshot, delta_id: DeltaId, partition_type: String) -> Self {
        let mut deltas = parent.deltas.clone();
        deltas.push(delta_id);

        let now = chrono::Utc::now().timestamp_millis();
        let content = match &parent.content {
            Some(SnapshotContent::FileContent(bytes)) if std::str::from_utf8(bytes).is_err() => {
                parent.content.clone()
            }
            Some(SnapshotContent::FileContent(_)) | None => None,
            other => other.clone(),
        };
        let snapshot = Snapshot {
            id: ContentId([0u8; 32]),
            file: parent.file.clone(),
            deltas,
            parents: vec![parent.id],
            partition_type,
            created_at: now,
            has_conflicts: false,
            content,
            source: parent.source.clone(),
            compression: parent.compression,
            content_hash: compute_snapshot_content_hash(&None),
            message: None,
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
        Self::new_with_content_and_message(
            file, content, source, partition_type, parents, deltas, None,
        )
    }

    /// Create a new snapshot with full metadata support and an optional message.
    pub fn new_with_content_and_message(
        file: FileNode,
        content: SnapshotContent,
        source: String,
        partition_type: String,
        parents: Vec<SnapshotId>,
        deltas: Vec<DeltaId>,
        message: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let content_hash = compute_snapshot_content_hash(&Some(content.clone()));
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
            content_hash,
            message,
        };
        let mut s = snapshot;
        s.id = s.compute_id();
        s
    }

    pub fn apply_delta(&self, delta_id: DeltaId) -> Self {
        Snapshot::from_parent(self, delta_id, self.partition_type.clone())
    }

    /// Whether this snapshot carries the explicit deletion marker. Deleted
    /// snapshots reconstruct to an empty string; the projection/restore
    /// layers use this to treat the path as missing rather than cleared.
    pub fn is_deleted(&self) -> bool {
        self.content
            .as_ref()
            .map(|c| c.is_deleted())
            .unwrap_or(false)
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
    ) -> crate::error::Result<Self> {
        let file = parents
            .first()
            .ok_or_else(|| {
                crate::error::LayertwineError::Snapshot(
                    "merge requires at least one parent".to_string(),
                )
            })?
            .file
            .clone();
        let mut deltas = parents[0].deltas.clone();
        deltas.push(delta_id);

        let now = chrono::Utc::now().timestamp_millis();
        let content = match &parents[0].content {
            Some(SnapshotContent::FileContent(bytes)) if std::str::from_utf8(bytes).is_err() => {
                parents[0].content.clone()
            }
            Some(SnapshotContent::FileContent(_)) | None => None,
            other => other.clone(),
        };
        let snapshot = Snapshot {
            id: ContentId([0u8; 32]),
            file,
            deltas,
            parents: parents.iter().map(|p| p.id).collect(),
            partition_type,
            created_at: now,
            has_conflicts,
            content,
            source: parents[0].source.clone(),
            compression: parents[0].compression,
            content_hash: None,
            message: None,
        };
        let mut s = snapshot;
        s.id = s.compute_id();
        Ok(s)
    }

    pub fn compute_id(&self) -> SnapshotId {
        let mut hasher = blake3::Hasher::new();

        // Content-addressed identity: file identity plus the reconstruction
        // inputs (base content hash + delta chain) or the explicit content
        // payload. Lineage and contextual metadata (parents, partition_type,
        // source, conflict flag) do not participate. Delta ids embed
        // timestamp + seq, so identical content reached through different
        // edit records already yields different snapshot ids.
        //
        // Exception: full-content snapshots that carry no delta (same path +
        // same bytes recorded at different times) would otherwise collide and
        // be dropped by INSERT deduplication, losing a history record. Only
        // in that case the creation timestamp joins the hash so repeated
        // full snapshots remain distinct records.
        let path = self.file.path_str();
        hasher.update(path.as_bytes());
        hasher.update(&self.file.base_hash);

        for delta in &self.deltas {
            hasher.update(delta.0.as_ref());
        }

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
        if self.deltas.is_empty() {
            hasher.update(&self.created_at.to_le_bytes());
        }

        ContentId(*hasher.finalize().as_bytes())
    }

    /// Compress the snapshot content
    pub fn compress_content(&mut self) -> Result<()> {
        if self.compression == SnapshotCompression::None {
            if let Some(ref content) = self.content {
                // The deletion marker is a flag, not a payload: compressing
                // it would rewrite it as `Structured` and lose the marker.
                if content.is_deleted() {
                    return Ok(());
                }
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
                // The payload stays Structured: content type is an explicit
                // attribute (stored in the content_type column), never inferred
                // from the source prefix. Callers that know the original type
                // rewrap the decoded bytes accordingly.
                self.content = Some(SnapshotContent::Structured(decompressed));
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
    content_hash: Option<ContentId>,
    message: Option<String>,
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
            content_hash: None,
            message: None,
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

    pub fn with_content_hash(mut self, content_hash: ContentId) -> Self {
        self.content_hash = Some(content_hash);
        self
    }

    pub fn message(mut self, message: Option<String>) -> Self {
        self.message = message;
        self
    }

    pub fn build(self) -> Result<Snapshot> {
        let file = self.file.ok_or_else(|| {
            LayertwineError::Checkpoint("file is required for snapshot".to_string())
        })?;
        let now = chrono::Utc::now().timestamp_millis();
        let content_hash = self
            .content_hash
            .or_else(|| compute_snapshot_content_hash(&self.content));
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
            content_hash,
            message: self.message,
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
