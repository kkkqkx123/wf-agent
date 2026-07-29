//! Time Index Module (Phase 4.3)
//!
//! Time-based index for fast checkpoint lookup by creation time.
//! Uses BTreeMap for O(log n) range queries and nearest-neighbor search.

use crate::checkpoint::types::Checkpoint;
use crate::core::types::CheckpointId;
use smallvec::SmallVec;
use std::collections::BTreeMap;

/// Time index mapping creation timestamps to checkpoint IDs
///
/// Supports:
/// - Range queries: find checkpoints within a time window
/// - Nearest search: find checkpoint closest to a given time
#[derive(Debug, Clone, Default)]
pub struct TimeIndex {
    /// Timestamp -> CheckpointId (ordered by timestamp)
    entries: BTreeMap<i64, SmallVec<[CheckpointId; 2]>>,
}

impl TimeIndex {
    /// Create an empty time index
    pub fn new() -> Self {
        TimeIndex {
            entries: BTreeMap::new(),
        }
    }

    /// Index a checkpoint by its creation time
    pub fn insert(&mut self, cp: &Checkpoint) {
        self.entries.entry(cp.created_at).or_default().push(cp.id);
    }

    /// Remove a checkpoint from the index
    pub fn remove(&mut self, cp: &Checkpoint) {
        if let Some(ids) = self.entries.get_mut(&cp.created_at) {
            ids.retain(|id| id != &cp.id);
            if ids.is_empty() {
                self.entries.remove(&cp.created_at);
            }
        }
    }

    /// Query checkpoints within a time range [from, to] inclusive
    pub fn query_range(&self, from: i64, to: i64) -> Vec<(i64, CheckpointId)> {
        self.entries
            .range(from..=to)
            .flat_map(|(time, ids)| ids.iter().map(move |id| (*time, *id)))
            .collect()
    }

    /// Find the checkpoint nearest to target_time
    ///
    /// Returns (timestamp, checkpoint_id) of the nearest checkpoint.
    /// When two checkpoints are equally distant, prefers the earlier one.
    pub fn find_nearest(&self, target_time: i64) -> Option<(i64, CheckpointId)> {
        // Find first entry at or after target
        let after = self.entries.range(target_time..).next();
        // Find last entry before target
        let before = self.entries.range(..target_time).next_back();

        match (before, after) {
            (Some((t1, ids1)), Some((t2, ids2))) => {
                let diff1 = (target_time - t1).unsigned_abs();
                let diff2 = (t2 - target_time).unsigned_abs();
                if diff1 <= diff2 {
                    Some((*t1, *ids1.first()?))
                } else {
                    Some((*t2, *ids2.first()?))
                }
            }
            (Some((t, ids)), None) => Some((*t, *ids.first()?)),
            (None, Some((t, ids))) => Some((*t, *ids.first()?)),
            (None, None) => None,
        }
    }

    /// Get all checkpoints before a given time
    pub fn before(&self, timestamp: i64) -> Vec<(i64, CheckpointId)> {
        self.entries
            .range(..timestamp)
            .flat_map(|(time, ids)| ids.iter().map(move |id| (*time, *id)))
            .collect()
    }

    /// Get all checkpoints after a given time
    pub fn after(&self, timestamp: i64) -> Vec<(i64, CheckpointId)> {
        self.entries
            .range(timestamp..)
            .flat_map(|(time, ids)| ids.iter().map(move |id| (*time, *id)))
            .collect()
    }

    /// Return total number of checkpoint entries in the index
    pub fn len(&self) -> usize {
        self.entries.values().map(|ids| ids.len()).sum()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Build index from a collection of checkpoints
    pub fn from_checkpoints(checkpoints: &[Checkpoint]) -> Self {
        let mut index = TimeIndex::new();
        for cp in checkpoints {
            index.insert(cp);
        }
        index
    }

    /// Get the earliest checkpoint timestamp
    pub fn earliest(&self) -> Option<i64> {
        self.entries.first_key_value().map(|(&k, _)| k)
    }

    /// Get the latest checkpoint timestamp
    pub fn latest(&self) -> Option<i64> {
        self.entries.last_key_value().map(|(&k, _)| k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::types::{Checkpoint, CheckpointMetadata};
    use crate::core::types::ContentId;

    fn make_checkpoint(timestamp: i64, seed: u8) -> Checkpoint {
        let snap_id = ContentId::from_content(&[seed; 8]);
        let metadata = CheckpointMetadata::new("test", &format!("cp-{}", seed));
        let mut cp = Checkpoint::new(vec![snap_id], vec![], metadata);
        cp.created_at = timestamp;
        cp.id = ContentId::from_content(&[seed; 16]);
        cp
    }

    #[test]
    fn test_empty_index() {
        let index = TimeIndex::new();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert_eq!(index.find_nearest(100), None);
    }

    #[test]
    fn test_insert_and_query_range() {
        let mut index = TimeIndex::new();
        let cp1 = make_checkpoint(100, 1);
        let cp2 = make_checkpoint(200, 2);
        let cp3 = make_checkpoint(300, 3);

        index.insert(&cp1);
        index.insert(&cp2);
        index.insert(&cp3);

        assert_eq!(index.len(), 3);

        let results = index.query_range(100, 200);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_find_nearest() {
        let mut index = TimeIndex::new();
        index.insert(&make_checkpoint(100, 1));
        index.insert(&make_checkpoint(300, 2));
        index.insert(&make_checkpoint(500, 3));

        // Exact match
        let result = index.find_nearest(300);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 300);

        // Closer to 100
        let result = index.find_nearest(150);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 100);

        // Closer to 500
        let result = index.find_nearest(450);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 500);
    }

    #[test]
    fn test_find_nearest_before_first() {
        let mut index = TimeIndex::new();
        index.insert(&make_checkpoint(100, 1));
        index.insert(&make_checkpoint(200, 2));

        let result = index.find_nearest(50);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 100);
    }

    #[test]
    fn test_find_nearest_after_last() {
        let mut index = TimeIndex::new();
        index.insert(&make_checkpoint(100, 1));
        index.insert(&make_checkpoint(200, 2));

        let result = index.find_nearest(300);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 200);
    }

    #[test]
    fn test_remove_checkpoint() {
        let mut index = TimeIndex::new();
        let cp1 = make_checkpoint(100, 1);
        let cp2 = make_checkpoint(200, 2);

        index.insert(&cp1);
        index.insert(&cp2);
        assert_eq!(index.len(), 2);

        index.remove(&cp1);
        assert_eq!(index.len(), 1);
        assert_eq!(index.find_nearest(100).unwrap().0, 200);
    }

    #[test]
    fn test_from_checkpoints() {
        let checkpoints = vec![
            make_checkpoint(100, 1),
            make_checkpoint(200, 2),
            make_checkpoint(300, 3),
        ];
        let index = TimeIndex::from_checkpoints(&checkpoints);
        assert_eq!(index.len(), 3);
        assert_eq!(index.earliest(), Some(100));
        assert_eq!(index.latest(), Some(300));
    }

    #[test]
    fn test_before_and_after() {
        let mut index = TimeIndex::new();
        index.insert(&make_checkpoint(100, 1));
        index.insert(&make_checkpoint(200, 2));
        index.insert(&make_checkpoint(300, 3));

        let before = index.before(200);
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].0, 100);

        let after = index.after(200);
        assert_eq!(after.len(), 2);
    }

    #[test]
    fn test_multiple_checkpoints_same_timestamp() {
        let mut index = TimeIndex::new();
        index.insert(&make_checkpoint(100, 1));
        index.insert(&make_checkpoint(100, 2));

        assert_eq!(index.len(), 2);

        let results = index.query_range(100, 100);
        assert_eq!(results.len(), 2);
    }
}
