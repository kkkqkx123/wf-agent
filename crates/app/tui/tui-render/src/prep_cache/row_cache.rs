//! Row-level message cache: message identity plus width → laid-out display
//! rows, with generational eviction.

use std::collections::HashMap;

use ratatui::text::Line;

use tui_core::prep_keys::AuxMapKey;

/// Maximum cached messages in the row-level cache.
pub const ROW_CACHE_MAX_ENTRIES: usize = 512;

fn oldest_key(map: &HashMap<AuxMapKey, CachedRows>) -> Option<AuxMapKey> {
    map.iter()
        .min_by_key(|(_, entry)| entry.generation)
        .map(|(key, _)| *key)
}

/// Row-level message cache: message identity plus width → laid-out display
/// rows. Unlike the scrollback preparation (keyed by content version), this
/// layer survives replace cycles, so reloading identical content lays out
/// nothing. Width changes invalidate every entry because rows are
/// width-specific.
#[derive(Debug, Clone, Default)]
pub struct RowCache {
    entries: HashMap<AuxMapKey, CachedRows>,
    generation: u64,
    hits: u64,
    misses: u64,
}

#[derive(Debug, Clone)]
struct CachedRows {
    generation: u64,
    rows: Vec<Line<'static>>,
}

impl RowCache {
    /// Empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cached rows for `key`, if present. Hits promote the entry to the
    /// newest generation so steady content survives churn.
    pub fn get(&mut self, key: AuxMapKey) -> Option<&[Line<'static>]> {
        if self.entries.contains_key(&key) {
            self.generation = self.generation.wrapping_add(1);
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.generation = self.generation;
            }
            self.hits += 1;
            return self.entries.get(&key).map(|entry| entry.rows.as_slice());
        }
        self.misses += 1;
        None
    }

    /// Store laid-out rows. When full, evict the oldest generation instead
    /// of dropping the whole cache, so steady content keeps its hits while
    /// churned identities age out.
    pub fn insert(&mut self, key: AuxMapKey, rows: Vec<Line<'static>>) {
        self.generation = self.generation.wrapping_add(1);
        if self.entries.len() >= ROW_CACHE_MAX_ENTRIES && !self.entries.contains_key(&key) {
            if let Some(oldest) = oldest_key(&self.entries) {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            key,
            CachedRows {
                generation: self.generation,
                rows,
            },
        );
    }

    /// Drop all entries (width change).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of cached messages.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cache hits observed since creation.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Cache misses observed since creation.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Hit rate across all probes (1.0 when nothing was probed).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 1.0;
        }
        self.hits as f64 / total as f64
    }
}
