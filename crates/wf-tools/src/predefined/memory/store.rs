//! Shared in-memory stores for the memory tools.
//!
//! Session notes (record_note / recall_notes / list_categories) are scoped
//! per execution. Long-term memory (memory_remember / memory_forget /
//! memory_list) is shared across executions.

use dashmap::DashMap;
use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct SessionNote {
    pub(crate) note: String,
    pub(crate) category: Option<String>,
    pub(crate) timestamp: i64,
}

pub(crate) type NoteMap = DashMap<String, Vec<SessionNote>>;

pub(crate) type MemoryMap = DashMap<String, Value>;

/// Shared state across the memory tools: per-execution session notes and
/// cross-execution long-term memory.
pub(crate) struct MemoryStore {
    pub(crate) notes: NoteMap,
    pub(crate) memory: MemoryMap,
}

impl MemoryStore {
    pub(crate) fn new() -> Self {
        Self {
            notes: DashMap::new(),
            memory: DashMap::new(),
        }
    }
}
