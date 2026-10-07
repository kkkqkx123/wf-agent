//! Approval read models and conflict views.
//!
//! Read domain only: pending items, merge outcomes and conflict views plus
//! pure marker injection. Approval orchestration (submit, approve, reject,
//! merge into features) lives on `FileCheckpointManager` in `file/approval`.

use crate::file::git_merge::GitConflictDetail;
use crate::provenance::DeltaSummary;

/// One merge conflict region inside a file (whole-file granularity in the
/// Git model: one region per conflicted file starting at line 0).
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
    /// Git-style marker block of the conflict.
    pub fn to_conflict_marker(&self) -> String {
        let mut out = String::new();
        out.push_str("<<<<<<< ours\n");
        for line in &self.ours {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("=======\n");
        for line in &self.theirs {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(">>>>>>> theirs\n");
        out
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
/// Merged bytes already carry standard markers at conflict regions; this
/// replaces each recorded region with an explicit marker block. Regions
/// are processed in reverse order so line offsets of already-processed
/// regions stay valid.
pub fn inject_conflict_markers(text: &str, details: &[GitConflictDetail]) -> String {
    if details.is_empty() {
        return text.to_string();
    }
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    // Whole-file regions all start at line 0: apply innermost last so the
    // final text carries every file's marker block.
    for detail in details.iter().rev() {
        let mut marker =
            Vec::with_capacity(detail.ours_lines.len() + detail.theirs_lines.len() + 3);
        marker.push("<<<<<<< ours".to_string());
        marker.extend(detail.ours_lines.iter().cloned());
        marker.push("=======".to_string());
        marker.extend(detail.theirs_lines.iter().cloned());
        marker.push(">>>>>>> theirs".to_string());
        lines.splice(0..0, marker);
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Convert merge conflict details into [`ConflictView`]s.
pub fn to_conflict_views(details: &[GitConflictDetail]) -> Vec<ConflictView> {
    details
        .iter()
        .map(|d| ConflictView {
            file: d.file.clone(),
            start_line: 0,
            base: vec![],
            ours: d.ours_lines.clone(),
            theirs: d.theirs_lines.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(ours: &[&str], theirs: &[&str]) -> GitConflictDetail {
        GitConflictDetail {
            file: "a.txt".to_string(),
            ours_lines: ours.iter().map(|s| s.to_string()).collect(),
            theirs_lines: theirs.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn inject_markers_replaces_ours_region() {
        let text = "a\nX\nc\n";
        let details = vec![detail(&["X"], &["Y"])];
        let marked = inject_conflict_markers(text, &details);
        assert!(marked.contains("<<<<<<< ours"));
        assert!(marked.contains("======="));
        assert!(marked.contains(">>>>>>> theirs"));
        assert!(marked.contains("X"));
        assert!(marked.contains("Y"));
        assert!(marked.contains("a\n"));
        assert!(marked.ends_with("c\n"));
    }

    #[test]
    fn inject_markers_multiple_conflicts_reverse_order() {
        let text = "a\nX\nc\nd\nY\nf\n";
        let details = vec![detail(&["X"], &["P"]), detail(&["Y"], &["Q"])];
        let marked = inject_conflict_markers(text, &details);
        let x_pos = marked.find('X').unwrap();
        let y_pos = marked.find('Y').unwrap();
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
        let views = to_conflict_views(&[GitConflictDetail {
            file: "src/a.txt".to_string(),
            ours_lines: vec!["X".to_string()],
            theirs_lines: vec!["Y".to_string()],
        }]);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].file, "src/a.txt");
    }
}
