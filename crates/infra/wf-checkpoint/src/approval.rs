//! Layered approval views and conflict handling.
//!
//! The manager-level approval flow lives on `FileCheckpointManager`; this
//! module provides the read models (`PendingApproval` / `MergeOutcome` /
//! `ConflictView`) and the pure conflict-marker injection used by the
//! `ConflictBehavior::Marker` strategy.

use crate::provenance::DeltaSummary;
use layertwine::engine::merge::MergeConflict as LayertwineMergeConflict;

/// One pending approval: the actor submitted changes into the approval layer
/// and they are not yet merged into a feature (manual approval mode:
/// `history.len() > 1`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PendingApproval {
    /// Actor id string (e.g. `agent:{loop_id}`).
    pub actor: String,
    /// Approval snapshot id (hex).
    pub snapshot_id: String,
    /// Submission time (Unix milliseconds).
    pub submitted_at: i64,
    /// The submitted changes (chunked per file, chronological).
    pub changes: Vec<DeltaSummary>,
}

/// Read view of a three-way merge conflict.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConflictView {
    /// Relative file path the conflict applies to.
    pub file: String,
    /// Start line in the merged output (0-indexed).
    pub start_line: usize,
    pub base: Vec<String>,
    pub ours: Vec<String>,
    pub theirs: Vec<String>,
}

impl ConflictView {
    /// Git-style marker block of the conflict (layertwine
    /// `MergeConflict::to_conflict_marker`).
    pub fn to_conflict_marker(&self) -> String {
        let mut buf = String::new();
        buf.push_str("<<<<<<< ours\n");
        for line in &self.ours {
            buf.push_str(line);
            buf.push('\n');
        }
        buf.push_str("=======\n");
        for line in &self.theirs {
            buf.push_str(line);
            buf.push('\n');
        }
        buf.push_str(">>>>>>> theirs\n");
        buf
    }
}

/// Outcome of an approval/merge operation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergeOutcome {
    /// Whether the changes were merged into the feature.
    pub merged: bool,
    /// Resulting snapshot id (hex) of the target partition.
    pub snapshot_id: String,
    /// Conflicts detected by the three-way merge (empty when merged
    /// cleanly).
    pub conflicts: Vec<ConflictView>,
    /// Distinct files with conflicts (sorted).
    pub conflict_files: Vec<String>,
    /// Human-readable outcome message.
    pub message: String,
}

impl MergeOutcome {
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }
}

/// Inject git-style conflict markers into merged text (`marker` strategy).
///
/// `merge_texts` emits the "ours" content for conflict regions; this replaces
/// each conflict region with `<<<<<<< ours / ======= / theirs / >>>>>>>`
/// markers. Conflicts are processed in reverse order so line offsets of
/// already-processed regions stay valid.
pub fn inject_conflict_markers(text: &str, conflicts: &[LayertwineMergeConflict]) -> String {
    if conflicts.is_empty() {
        return text.to_string();
    }
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    for conflict in conflicts.iter().rev() {
        let start = conflict.start_line.min(lines.len());
        let end = (start + conflict.ours.len()).min(lines.len());
        let mut marker = Vec::with_capacity(conflict.ours.len() + conflict.theirs.len() + 3);
        marker.push("<<<<<<< ours".to_string());
        marker.extend(conflict.ours.iter().cloned());
        marker.push("=======".to_string());
        marker.extend(conflict.theirs.iter().cloned());
        marker.push(">>>>>>> theirs".to_string());
        lines.splice(start..end, marker);
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Convert layertwine conflicts + file path into [`ConflictView`]s.
pub fn to_conflict_views(file: &str, conflicts: &[LayertwineMergeConflict]) -> Vec<ConflictView> {
    conflicts
        .iter()
        .map(|c| ConflictView {
            file: file.to_string(),
            start_line: c.start_line,
            base: c.base.clone(),
            ours: c.ours.clone(),
            theirs: c.theirs.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conflict(start: usize, ours: &[&str], theirs: &[&str]) -> LayertwineMergeConflict {
        LayertwineMergeConflict {
            start_line: start,
            base: vec!["b".to_string()],
            ours: ours.iter().map(|s| s.to_string()).collect(),
            theirs: theirs.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn inject_markers_replaces_ours_region() {
        let text = "a\nX\nc\n";
        let conflicts = vec![conflict(1, &["X"], &["Y"])];
        let marked = inject_conflict_markers(text, &conflicts);
        assert!(marked.contains("<<<<<<< ours"));
        assert!(marked.contains("======="));
        assert!(marked.contains(">>>>>>> theirs"));
        assert!(marked.contains("X"));
        assert!(marked.contains("Y"));
        assert!(marked.starts_with("a\n"));
        assert!(marked.ends_with("c\n"));
    }

    #[test]
    fn inject_markers_multiple_conflicts_reverse_order() {
        let text = "a\nX\nc\nd\nY\nf\n";
        let conflicts = vec![conflict(1, &["X"], &["P"]), conflict(4, &["Y"], &["Q"])];
        let marked = inject_conflict_markers(text, &conflicts);
        let x_pos = marked.find("<<<<<<<").unwrap();
        let y_pos = marked.rfind("<<<<<<<").unwrap();
        assert!(x_pos < y_pos);
        assert!(marked.contains("P"));
        assert!(marked.contains("Q"));
    }

    #[test]
    fn no_conflicts_returns_original() {
        let text = "a\nb\nc\n";
        assert_eq!(inject_conflict_markers(text, &[]), text);
    }

    #[test]
    fn conflict_view_renders_marker() {
        let view = ConflictView {
            file: "a.txt".to_string(),
            start_line: 1,
            base: vec!["b".to_string()],
            ours: vec!["X".to_string()],
            theirs: vec!["Y".to_string()],
        };
        let markers = view.to_conflict_marker();
        assert!(markers.contains("<<<<<<< ours"));
        assert!(markers.contains("======="));
        assert!(markers.contains(">>>>>>> theirs"));
    }

    #[test]
    fn to_conflict_views_carries_file() {
        let views = to_conflict_views("src/a.txt", &[conflict(1, &["X"], &["Y"])]);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].file, "src/a.txt");
    }
}
