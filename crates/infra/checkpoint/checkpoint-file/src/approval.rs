//! Approval read models and conflict views.
//!
//! Read domain only: pending items, merge outcomes and conflict views plus
//! pure marker injection. Approval orchestration (submit, approve, reject,
//! merge into features) lives on `FileCheckpointManager` in `file/approval`.

use crate::file::git_merge::{GitConflictDetail, GitConflictRegion};
use crate::provenance::DeltaSummary;

/// One merge conflict region inside a file (true marker intervals parsed
/// back from the merged bytes; binary conflicts stay file-level only and
/// contribute no regions).
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
    /// Git-style marker block of the conflict, with the base section only
    /// when the region carries one.
    pub fn to_conflict_marker(&self) -> String {
        let mut out = String::new();
        out.push_str("<<<<<<< ours\n");
        for line in &self.ours {
            out.push_str(line);
            out.push('\n');
        }
        if !self.base.is_empty() {
            out.push_str("||||||| base\n");
            for line in &self.base {
                out.push_str(line);
                out.push('\n');
            }
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
        !self.conflicts.is_empty() || !self.conflict_files.is_empty()
    }
}

/// Inject git-style conflict markers into merged text (`marker` strategy).
///
/// Each recorded region replaces its own merged-output interval
/// (`start_line..end_line`), processed in reverse order so earlier line
/// offsets stay valid. Details without regions (binary conflicts) leave the
/// text untouched: they are file-level only.
pub fn inject_conflict_markers(text: &str, details: &[GitConflictDetail]) -> String {
    let mut regions: Vec<&GitConflictRegion> =
        details.iter().flat_map(|d| d.regions.iter()).collect();
    if regions.is_empty() {
        return text.to_string();
    }
    regions.sort_by_key(|r| std::cmp::Reverse(r.start_line));
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    for region in regions {
        let end = region.end_line.min(lines.len());
        let start = region.start_line.min(end);
        let mut marker = Vec::with_capacity(
            region.ours_lines.len() + region.base_lines.len() + region.theirs_lines.len() + 4,
        );
        marker.push("<<<<<<< ours".to_string());
        marker.extend(region.ours_lines.iter().cloned());
        if !region.base_lines.is_empty() {
            marker.push("||||||| base".to_string());
            marker.extend(region.base_lines.iter().cloned());
        }
        marker.push("=======".to_string());
        marker.extend(region.theirs_lines.iter().cloned());
        marker.push(">>>>>>> theirs".to_string());
        lines.splice(start..end, marker);
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Convert merge conflict details into [`ConflictView`]s, one per recorded
/// region with its true start line and base. Binary details contribute no
/// views; their files stay visible through `conflict_files`.
pub fn to_conflict_views(details: &[GitConflictDetail]) -> Vec<ConflictView> {
    details
        .iter()
        .flat_map(|d| {
            d.regions.iter().map(|r| ConflictView {
                file: d.file.clone(),
                start_line: r.start_line,
                base: r.base_lines.clone(),
                ours: r.ours_lines.clone(),
                theirs: r.theirs_lines.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(start: usize, end: usize, ours: &[&str], theirs: &[&str]) -> GitConflictRegion {
        GitConflictRegion {
            start_line: start,
            end_line: end,
            base_lines: vec![],
            ours_lines: ours.iter().map(|s| s.to_string()).collect(),
            theirs_lines: theirs.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn detail(regions: Vec<GitConflictRegion>) -> GitConflictDetail {
        GitConflictDetail {
            file: "a.txt".to_string(),
            binary: regions.is_empty(),
            regions,
        }
    }

    #[test]
    fn inject_markers_replaces_region_interval() {
        let text = "a\nX\nc\n";
        let details = vec![detail(vec![region(1, 2, &["X"], &["Y"])])];
        let marked = inject_conflict_markers(text, &details);
        assert!(marked.contains("<<<<<<< ours"));
        assert!(marked.contains("======="));
        assert!(marked.contains(">>>>>>> theirs"));
        assert!(marked.contains("X"));
        assert!(marked.contains("Y"));
        assert!(marked.starts_with("a\n"));
        assert!(marked.ends_with("c\n"));
    }

    #[test]
    fn inject_markers_multiple_conflicts_keep_order() {
        let text = "a\nX\nc\nd\nY\nf\n";
        let details = vec![detail(vec![
            region(1, 2, &["X"], &["P"]),
            region(4, 5, &["Y"], &["Q"]),
        ])];
        let marked = inject_conflict_markers(text, &details);
        let x_pos = marked.find('X').unwrap();
        let y_pos = marked.find('Y').unwrap();
        assert!(x_pos < y_pos);
        assert!(marked.contains("P"));
        assert!(marked.contains("Q"));
    }

    #[test]
    fn binary_details_leave_text_untouched() {
        let text = "a\nb\nc\n";
        let binary = GitConflictDetail {
            file: "bin/img".to_string(),
            binary: true,
            regions: vec![],
        };
        assert_eq!(inject_conflict_markers(text, &[binary]), text);
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
        assert!(markers.contains("||||||| base"));
        assert!(markers.contains("======="));
        assert!(markers.contains(">>>>>>> theirs"));
    }

    #[test]
    fn to_conflict_views_carries_file_and_interval() {
        let views = to_conflict_views(&[GitConflictDetail {
            file: "src/a.txt".to_string(),
            binary: false,
            regions: vec![GitConflictRegion {
                start_line: 4,
                end_line: 9,
                base_lines: vec!["B".to_string()],
                ours_lines: vec!["X".to_string()],
                theirs_lines: vec!["Y".to_string()],
            }],
        }]);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].file, "src/a.txt");
        assert_eq!(views[0].start_line, 4);
        assert_eq!(views[0].base, vec!["B".to_string()]);
    }

    #[test]
    fn binary_details_contribute_no_views() {
        let views = to_conflict_views(&[GitConflictDetail {
            file: "bin/img".to_string(),
            binary: true,
            regions: vec![],
        }]);
        assert!(views.is_empty());
    }
}
