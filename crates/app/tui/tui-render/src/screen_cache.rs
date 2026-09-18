//! Preparation cache for management screens.
//!
//! Session scrollback already lays out incrementally; dashboard, workflow,
//! execution, checkpoint, search and settings screens rebuilt their strings
//! on every frame. This cache stores the formatted rows per screen kind,
//! keyed by width, content hash and render mode, with generational eviction
//! and a small oversize queue so long lists stay warm across screen switches.

use std::collections::{HashMap, VecDeque};

use tui_core::screen_data::ScreenData;

/// Maximum cached management screens.
pub const SCREEN_CACHE_MAX_ENTRIES: usize = 8;
/// Retained oversize management screens.
pub const SCREEN_OVERSIZE_MAX_ENTRIES: usize = 2;
/// Rough row estimate marking a management screen as oversize.
pub const SCREEN_OVERSIZE_ROWS: usize = 2000;

/// Identity of one prepared management screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenPrepKey {
    pub kind: ScreenKindTag,
    pub width: u16,
    pub content_hash: u64,
    pub render_mode: u8,
}

/// Stable tag for the screen kind driving the cache identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenKindTag {
    Dashboard,
    Workflow,
    Executions,
    Checkpoints,
    Search,
    Settings,
    Help,
    Other,
}

#[derive(Debug, Clone)]
struct ScreenEntry {
    generation: u64,
    rows: Vec<String>,
}

/// Cached formatted rows for management screens.
#[derive(Debug, Default)]
pub struct ScreenPrepCache {
    entries: HashMap<ScreenPrepKey, ScreenEntry>,
    oversize: VecDeque<(ScreenPrepKey, Vec<String>)>,
    generation: u64,
    pub hits: u64,
    pub misses: u64,
}

impl ScreenPrepCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&mut self, key: ScreenPrepKey) -> Option<&[String]> {
        if self.entries.contains_key(&key) {
            self.hits += 1;
            return self.entries.get(&key).map(|entry| entry.rows.as_slice());
        }
        if let Some(pos) = self.oversize.iter().position(|(k, _)| *k == key) {
            self.hits += 1;
            return self.oversize.get(pos).map(|(_, rows)| rows.as_slice());
        }
        self.misses += 1;
        None
    }

    pub fn insert(&mut self, key: ScreenPrepKey, rows: Vec<String>) {
        self.generation = self.generation.wrapping_add(1);
        let oversize = rows.len() > SCREEN_OVERSIZE_ROWS;
        if oversize {
            if let Some(pos) = self.oversize.iter().position(|(k, _)| *k == key) {
                self.oversize.remove(pos);
            }
            while self.oversize.len() >= SCREEN_OVERSIZE_MAX_ENTRIES {
                self.oversize.pop_front();
            }
            self.oversize.push_back((key, rows));
            return;
        }
        if self.entries.len() >= SCREEN_CACHE_MAX_ENTRIES && !self.entries.contains_key(&key) {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.generation)
                .map(|(key, _)| *key)
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            key,
            ScreenEntry {
                generation: self.generation,
                rows,
            },
        );
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn oversize_len(&self) -> usize {
        self.oversize.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.oversize.is_empty()
    }
}

/// Hash management screen content for cache identity (FNV-1a, no dep).
pub fn hash_screen_data(data: &ScreenData) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    match data {
        ScreenData::None => mix(b"none"),
        ScreenData::Dashboard(d) => {
            mix(b"dashboard");
            mix(&d.workflow_count.to_le_bytes());
            mix(&d.execution_count.to_le_bytes());
            mix(&d.running_count.to_le_bytes());
            mix(&d.checkpoint_count.to_le_bytes());
            for row in d.recent.iter().take(5) {
                mix(row.as_bytes());
                mix(&[0xff]);
            }
        }
        ScreenData::Workflow(rows) => {
            mix(b"workflow");
            mix(&rows.len().to_le_bytes());
            for row in rows {
                mix(row.id.as_bytes());
                mix(row.name.as_bytes());
                mix(&row.node_count.to_le_bytes());
            }
        }
        ScreenData::Executions(rows) => {
            mix(b"executions");
            mix(&rows.len().to_le_bytes());
            for row in rows {
                mix(row.id.as_bytes());
                mix(row.status.as_bytes());
                mix(&row.iteration.to_le_bytes());
            }
        }
        ScreenData::Checkpoints(rows) => {
            mix(b"checkpoints");
            mix(&rows.len().to_le_bytes());
            for row in rows {
                mix(row.id.as_bytes());
                mix(row.entity.as_bytes());
                mix(row.timestamp.as_bytes());
            }
        }
        ScreenData::Search(d) => {
            mix(b"search");
            mix(d.query.as_bytes());
            mix(&[u8::from(d.running)]);
            mix(&[u8::from(d.truncated)]);
            for row in &d.results {
                mix(row.id.as_bytes());
                mix(row.label.as_bytes());
            }
        }
        ScreenData::Settings(d) => {
            mix(b"settings");
            mix(d.theme.as_bytes());
            if let Some(default) = &d.default_profile {
                mix(default.as_bytes());
            }
            for profile in &d.profiles {
                mix(profile.id.as_bytes());
                mix(profile.model.as_bytes());
            }
        }
    }
    hash
}

/// Tag for a screen data variant.
pub fn tag_for_data(data: &ScreenData) -> ScreenKindTag {
    match data {
        ScreenData::None => ScreenKindTag::Other,
        ScreenData::Dashboard(_) => ScreenKindTag::Dashboard,
        ScreenData::Workflow(_) => ScreenKindTag::Workflow,
        ScreenData::Executions(_) => ScreenKindTag::Executions,
        ScreenData::Checkpoints(_) => ScreenKindTag::Checkpoints,
        ScreenData::Search(_) => ScreenKindTag::Search,
        ScreenData::Settings(_) => ScreenKindTag::Settings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tui_core::screen_data::{DashboardData, ScreenData};

    #[test]
    fn identical_screens_hit() {
        let mut cache = ScreenPrepCache::new();
        let data = ScreenData::Dashboard(DashboardData {
            workflow_count: 2,
            execution_count: 3,
            running_count: 1,
            checkpoint_count: 0,
            recent: vec!["a".to_string()],
        });
        let key = ScreenPrepKey {
            kind: tag_for_data(&data),
            width: 80,
            content_hash: hash_screen_data(&data),
            render_mode: 0,
        };
        assert!(cache.get(key).is_none());
        cache.insert(key, vec!["row".to_string()]);
        assert!(cache.get(key).is_some());
        assert_eq!(cache.hits, 1);
        assert_eq!(cache.misses, 1);
    }

    #[test]
    fn content_change_misses() {
        let first = ScreenData::Dashboard(DashboardData {
            workflow_count: 1,
            ..DashboardData::default()
        });
        let second = ScreenData::Dashboard(DashboardData {
            workflow_count: 2,
            ..DashboardData::default()
        });
        assert_ne!(hash_screen_data(&first), hash_screen_data(&second));
    }

    #[test]
    fn oversize_screens_use_side_queue() {
        let mut cache = ScreenPrepCache::new();
        let key = ScreenPrepKey {
            kind: ScreenKindTag::Workflow,
            width: 80,
            content_hash: 7,
            render_mode: 0,
        };
        let rows = vec!["row".to_string(); SCREEN_OVERSIZE_ROWS + 1];
        cache.insert(key, rows);
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.oversize_len(), 1);
        assert!(cache.get(key).is_some());
    }

    #[test]
    fn full_cache_evicts_oldest_generation() {
        let mut cache = ScreenPrepCache::new();
        for index in 0..SCREEN_CACHE_MAX_ENTRIES {
            let key = ScreenPrepKey {
                kind: ScreenKindTag::Workflow,
                width: 80,
                content_hash: index as u64,
                render_mode: 0,
            };
            cache.insert(key, vec![format!("row {index}")]);
        }
        let fresh = ScreenPrepKey {
            kind: ScreenKindTag::Workflow,
            width: 80,
            content_hash: 9999,
            render_mode: 0,
        };
        cache.insert(fresh, vec!["fresh".to_string()]);
        assert_eq!(cache.len(), SCREEN_CACHE_MAX_ENTRIES);
        assert!(cache.get(fresh).is_some());
    }
}
