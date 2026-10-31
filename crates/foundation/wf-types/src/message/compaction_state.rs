use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Lifecycle mode of an agent conversation under budget-driven compaction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CompactionMode {
    #[default]
    Normal,
    Handoff,
}

/// Lifecycle state of one compression block.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum BlockState {
    /// Never compressed; pending object of regular compaction.
    #[default]
    Raw,
    /// Compressed into a summary message; eligible for re-compression.
    Compressed,
    /// Re-compression budget spent; only deletable now.
    Sealed,
}

/// Closed sequence interval covered by a block span.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeqRange {
    pub start: u64,
    pub end: u64,
}

/// One compression block: an interval of history plus its summary identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompressionBlock {
    pub block_id: String,
    pub state: BlockState,
    #[serde(default)]
    pub topic: String,
    /// Summary message id in history; empty while `Raw`.
    #[serde(default)]
    pub summary_message_id: String,
    /// Original history intervals covered by this block.
    #[serde(default)]
    pub spans: Vec<SeqRange>,
    /// Block ids absorbed into this one through merges.
    #[serde(default)]
    pub merged_blocks: Vec<String>,
    /// Hot-view token cost (interval estimate while `Raw`, summary estimate
    /// once compressed).
    pub size_tokens: u64,
    /// Pre-compression token cost of the covered intervals.
    #[serde(default)]
    pub source_tokens: u64,
    /// Re-compression count.
    #[serde(default)]
    pub repress_count: u32,
    /// Whether the block participates in the hot view.
    pub active: bool,
    pub created_at: i64,
    #[serde(default)]
    pub compressed_at: Option<i64>,
}

/// One deduplicated file read: same path plus same content hash reuses the
/// first message instead of re-reading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileReadEntry {
    pub path: String,
    pub content_hash: String,
    #[serde(default)]
    pub commit: String,
    pub last_read_at: i64,
    pub message_seq: u64,
    #[serde(default)]
    pub archive_ref: String,
}

/// One handoff index entry pointing at archived block content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexEntry {
    pub block_id: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub time_range: String,
    #[serde(default)]
    pub archive_ref: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub size_tokens: u64,
}

/// The handoff artifact: a handoff document message plus its archive index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HandoffArtifact {
    pub handoff_message_id: String,
    #[serde(default)]
    pub index: Vec<IndexEntry>,
    #[serde(default)]
    pub reason: String,
    pub created_at: i64,
    #[serde(default)]
    pub validation_note: Option<String>,
}

/// Absolute token thresholds derived from one model window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CompactionThresholds {
    pub h_target: u64,
    pub t_handoff: u64,
    pub t_compact: u64,
    pub t_hard: u64,
}

/// One compaction work item: a slice of the emitted snapshot identified by
/// snapshot indices, not by session sequence numbers. The emitter owns the
/// snapshot, so indices stay valid for the one signal they travel with. An
/// empty plan means the legacy single-summary mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionPlanItem {
    pub block_id: String,
    pub start: usize,
    pub end: usize,
    /// Zero for a first compression, greater for a re-compression used to
    /// pick the prompt variant on the service side.
    #[serde(default)]
    pub depth: u32,
    /// Blocks merged into this item by adjacent-item merging: the service
    /// reports one outcome per covered block sharing the item summary.
    /// Empty for unmerged items; absent in older payloads.
    #[serde(default)]
    pub extra_block_ids: Vec<String>,
}

impl CompactionPlanItem {
    /// Clamp the item interval to a snapshot length; `None` when the clamped
    /// interval is empty and the item carries no work.
    pub fn clamped_range(&self, len: usize) -> Option<(usize, usize)> {
        let start = self.start.min(len);
        let end = self.end.min(len);
        if start < end {
            Some((start, end))
        } else {
            None
        }
    }
}

/// One block result carried by a compression completion event: the service
/// reports which summary message each plan item produced, the agent owns
/// the state transition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockOutcome {
    pub block_id: String,
    pub summary_message_id: String,
    #[serde(default)]
    pub archive_refs: Vec<String>,
}

/// Budget-driven compaction state of one agent conversation. Persisted with
/// the session and the checkpoint; history stays append-only, this state
/// only decides which summaries and intervals enter the hot view.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CompactionState {
    #[serde(default)]
    pub mode: CompactionMode,
    #[serde(default)]
    pub blocks: Vec<CompressionBlock>,
    #[serde(default)]
    pub file_reads: HashMap<String, FileReadEntry>,
    #[serde(default)]
    pub handoff: Option<HandoffArtifact>,
    #[serde(default)]
    pub hot_tokens: u64,
    #[serde(default)]
    pub growth_baseline: u64,
    #[serde(default)]
    pub window_tokens: u64,
    #[serde(default)]
    pub thresholds: CompactionThresholds,
    /// The next compaction emission should enter handoff directly (a
    /// still-over-budget completion landed in `Normal` mode).
    #[serde(default)]
    pub handoff_requested: bool,
    /// Highest history sequence number already synced into blocks.
    #[serde(default)]
    pub synced_through_seq: u64,
    /// Never-decreasing block id counter; survives branch truncation so
    /// rebuilt blocks never reuse an absorbed block id.
    #[serde(default)]
    pub next_block_seq: u64,
    /// Plan emitted by the latest compaction signal, consumed by its
    /// write-back. Cleared on apply, expiry or branch truncation.
    #[serde(default)]
    pub pending_plan: Vec<CompactionPlanItem>,
    /// Block ids the pending plan chose to drop on success.
    #[serde(default)]
    pub pending_drops: Vec<String>,
    /// Restored state whose hot estimate was not recomputed yet.
    #[serde(default)]
    pub dirty: bool,
}

impl CompactionState {
    /// True when no compaction bookkeeping exists yet.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
            && self.file_reads.is_empty()
            && self.handoff.is_none()
            && self.window_tokens == 0
    }

    /// Look up an active block by id.
    pub fn active_block(&self, block_id: &str) -> Option<&CompressionBlock> {
        self.blocks
            .iter()
            .find(|b| b.active && b.block_id == block_id)
    }

    /// Mutable look up of an active block by id.
    pub fn active_block_mut(&mut self, block_id: &str) -> Option<&mut CompressionBlock> {
        self.blocks
            .iter_mut()
            .find(|b| b.active && b.block_id == block_id)
    }
}
