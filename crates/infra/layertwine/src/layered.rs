//! Layered State Machine Module
//!
//! Manages the layered pipeline: manual_edit / agent_edit → approval → integrated → staged.
//! Provides forward flow and reverse rollback with ironclad layer-gating rules.

pub mod agent;
pub mod approval;
pub mod integrated;
pub mod manual;
pub mod staged;
pub mod transition;

use crate::core::types::SnapshotId;

/// Merge result shared by all layer merge operations
///
/// Replaces FeatureMergeResult and UnifiedMergeResult with a single type.
#[derive(Debug, Clone)]
pub struct MergeResult {
    pub snapshot_id: SnapshotId,
    pub conflicts: Vec<crate::engine::merge::MergeConflict>,
}

impl MergeResult {
    /// Check if merge has conflicts
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }

    /// Get conflict count
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    /// Format all conflicts as Git-style markers
    pub fn format_conflicts(&self) -> String {
        if self.conflicts.is_empty() {
            return String::new();
        }
        let mut result = String::new();
        for (i, conflict) in self.conflicts.iter().enumerate() {
            result.push_str(&format!(
                "Conflict #{} (line {}):\n",
                i + 1,
                conflict.start_line
            ));
            result.push_str(&conflict.to_conflict_marker());
            result.push('\n');
        }
        result
    }
}
