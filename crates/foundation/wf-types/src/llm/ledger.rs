//! Per-array token estimation ledger (decision track).
//!
//! The ledger keeps per-named-array incremental estimates so budget checks
//! (preflight, warnings, limit exceeded, compression requests) never rescan
//! the full message history: `append` estimates only the new messages and
//! bumps the version, while an array *replacement* (compression write-back,
//! checkpoint restore) marks the entry dirty so the next read lazily
//! recomputes exactly once.
//!
//! The ledger serves the decision track only. Cost-track data (real provider
//! usage, history, costs) lives in `TokenUsageTracker` and never touches the
//! ledger. It is pure serde data: estimation itself happens at the call sites
//! (`wf_llm::estimate_message_tokens` / `estimate_messages`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Estimation ledger entry for one named message array.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LedgerEntry {
    /// Estimated token total of the array (recomputed when dirty).
    pub estimated_tokens: u64,
    /// Number of messages the estimate covers.
    pub message_count: usize,
    /// Monotonic array version: bumped on every append and replacement.
    /// Events carry the version so emitters, listeners and write-back agree
    /// on the same array snapshot.
    pub version: u64,
    /// True when the array was replaced/restored without a recompute; the
    /// next reader recomputes the estimate once and clears the flag.
    pub dirty: bool,
    /// Version of the array for which a compression request was last emitted
    /// (single-shot guard, checkpointed with the ledger). A later emission
    /// for the same version is suppressed; a version change re-arms it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_emitted_version: Option<u64>,
}

/// Incremental estimation ledger for all named message arrays of one
/// execution (agent conversation session or workflow variable map).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TokenLedger {
    pub entries: HashMap<String, LedgerEntry>,
}

impl TokenLedger {
    /// Record an incremental append of `added_count` messages estimated at
    /// `added_tokens` into `context_id`. O(1): no rescan of existing
    /// messages. Bumps the version.
    pub fn append(&mut self, context_id: &str, added_tokens: u64, added_count: usize) {
        let entry = self.entries.entry(context_id.to_string()).or_default();
        entry.estimated_tokens = entry.estimated_tokens.saturating_add(added_tokens);
        entry.message_count = entry.message_count.saturating_add(added_count);
        entry.version = entry.version.saturating_add(1);
        entry.dirty = false;
    }

    /// Record a full-array replacement (`register_context`, compression
    /// write-back, checkpoint restore). The new content is not estimated
    /// here: the entry is marked dirty and the next read recomputes once.
    /// Bumps the version so stale events/guards are invalidated.
    pub fn replace(&mut self, context_id: &str) {
        let entry = self.entries.entry(context_id.to_string()).or_default();
        entry.estimated_tokens = 0;
        entry.message_count = 0;
        entry.dirty = true;
        entry.version = entry.version.saturating_add(1);
    }

    /// Mark the entry dirty without bumping the version (used when the array
    /// content changed in a way the ledger did not observe; the estimate is
    /// recomputed on the next read).
    pub fn mark_dirty(&mut self, context_id: &str) {
        if let Some(entry) = self.entries.get_mut(context_id) {
            entry.dirty = true;
        }
    }

    /// Full recompute after a replacement: store the freshly estimated
    /// `estimated_tokens` for `message_count` messages and clear the flag.
    pub fn recompute(&mut self, context_id: &str, estimated_tokens: u64, message_count: usize) {
        let entry = self.entries.entry(context_id.to_string()).or_default();
        entry.estimated_tokens = estimated_tokens;
        entry.message_count = message_count;
        entry.dirty = false;
    }

    /// Version of a named array (0 when the array is not tracked).
    pub fn version(&self, context_id: &str) -> u64 {
        self.entries
            .get(context_id)
            .map(|entry| entry.version)
            .unwrap_or(0)
    }

    /// Whether the entry needs a lazy recompute before its estimate is used.
    pub fn is_dirty(&self, context_id: &str) -> bool {
        self.entries
            .get(context_id)
            .map(|entry| entry.dirty)
            .unwrap_or(false)
    }

    /// Estimate of a named array (0 when not tracked).
    pub fn estimated_tokens(&self, context_id: &str) -> u64 {
        self.entries
            .get(context_id)
            .map(|entry| entry.estimated_tokens)
            .unwrap_or(0)
    }

    /// Message count of a named array (0 when not tracked).
    pub fn message_count(&self, context_id: &str) -> usize {
        self.entries
            .get(context_id)
            .map(|entry| entry.message_count)
            .unwrap_or(0)
    }

    /// Whether a compression request may be emitted for the array at
    /// `version`: the single-shot guard passes when no emission happened for
    /// that exact version yet (or the version moved past it).
    pub fn should_emit(&self, context_id: &str, version: u64) -> bool {
        match self.entries.get(context_id) {
            Some(entry) => entry.last_emitted_version != Some(version),
            None => true,
        }
    }

    /// Record that a compression request was emitted for `context_id` at
    /// `version` (re-arms the guard on the next version change).
    pub fn mark_emitted(&mut self, context_id: &str, version: u64) {
        let entry = self.entries.entry(context_id.to_string()).or_default();
        entry.last_emitted_version = Some(version);
    }

    /// Forget the emission guard (used by write-back after a successful
    /// compression replaces the array).
    pub fn clear_emitted(&mut self, context_id: &str) {
        if let Some(entry) = self.entries.get_mut(context_id) {
            entry.last_emitted_version = None;
        }
    }

    /// Whether no array is tracked at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove a named array from the ledger entirely.
    pub fn remove(&mut self, context_id: &str) {
        self.entries.remove(context_id);
    }

    /// Total estimated tokens across all tracked arrays.
    pub fn total(&self) -> u64 {
        self.entries
            .values()
            .map(|entry| entry.estimated_tokens)
            .sum()
    }
}
