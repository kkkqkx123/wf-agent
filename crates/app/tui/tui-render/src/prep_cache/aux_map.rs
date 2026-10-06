//! Auxiliary rendered-to-logical map: how many display rows one message
//! produced at a width, for text selection and row-count mapping without
//! re-parsing the message.

use std::collections::HashMap;

/// Maximum entries in the auxiliary rendered-to-logical map.
pub const AUX_MAP_MAX_ENTRIES: usize = 2048;

fn oldest_aux_key(map: &HashMap<u64, GenerationalAux>) -> Option<u64> {
    map.iter()
        .min_by_key(|(_, entry)| entry.generation)
        .map(|(key, _)| *key)
}

/// Auxiliary rendered-to-logical entry: how many display rows one message
/// produced at a width. Used for text selection and row-count mapping
/// without re-parsing the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuxEntry {
    /// Display rows produced for the message.
    pub display_len: usize,
    /// Width the entry was recorded at.
    pub width: u16,
}

/// Auxiliary map from message identity to [`AuxEntry`]. It is never cleared
/// by body-cache invalidation (that is its purpose: surviving a last-message
/// edit without re-parsing the same assistant message); entries age out by
/// oldest generation once the capacity cap is reached.
#[derive(Debug, Clone, Default)]
pub struct AuxLineMap {
    entries: HashMap<u64, GenerationalAux>,
    generation: u64,
}

#[derive(Debug, Clone, Copy)]
struct GenerationalAux {
    generation: u64,
    entry: AuxEntry,
}

impl AuxLineMap {
    /// Empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the display length of `message_hash` at `width`.
    pub fn record(&mut self, message_hash: u64, width: u16, display_len: usize) {
        self.generation = self.generation.wrapping_add(1);
        if self.entries.len() >= AUX_MAP_MAX_ENTRIES && !self.entries.contains_key(&message_hash) {
            if let Some(oldest) = oldest_aux_key(&self.entries) {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            message_hash,
            GenerationalAux {
                generation: self.generation,
                entry: AuxEntry { display_len, width },
            },
        );
    }

    /// Display length recorded for `message_hash` at `width`, if any.
    pub fn display_len(&self, message_hash: u64, width: u16) -> Option<usize> {
        self.entries.get(&message_hash).and_then(|wrapper| {
            if wrapper.entry.width == width {
                Some(wrapper.entry.display_len)
            } else {
                None
            }
        })
    }
}
