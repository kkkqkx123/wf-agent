//! Bounded queue of large frames kept warm so a long session does not lose
//! its single large transcript to eviction.

use std::collections::VecDeque;

use ratatui::text::Line;

use tui_core::prep_keys::ScrollPrepKey;

/// Retained oversize frames: long sessions keep their large transcript warm.
pub const OVERSIZE_CACHE_MAX_ENTRIES: usize = 2;

/// Retained oversize preparation: a bounded queue of large frames kept warm
/// so a long session does not lose its single large transcript to eviction.
#[derive(Debug, Clone, Default)]
pub struct OversizeCache {
    entries: VecDeque<(ScrollPrepKey, Vec<Line<'static>>)>,
}

impl OversizeCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: ScrollPrepKey, rows: Vec<Line<'static>>) {
        if let Some(pos) = self.entries.iter().position(|(k, _)| *k == key) {
            self.entries.remove(pos);
        }
        while self.entries.len() >= OVERSIZE_CACHE_MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back((key, rows));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Keep at most the newest entry when the global byte budget is blown.
    /// Eviction order is FIFO: oldest oversize frames drop first.
    pub fn retain_newest(&mut self) {
        while self.entries.len() > 1 {
            self.entries.pop_front();
        }
    }
}
