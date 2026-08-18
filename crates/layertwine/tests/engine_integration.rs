//! Integration tests for the engine module.
//!
//! These tests exercise the core diff/merge functionality end-to-end.
//! They verify:
//! - Text diff calculation using similar crate
//! - Delta application to reconstruct content
//! - Three-way text merge with conflict detection

use std::path::PathBuf;

use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::types::{LineDiff, SourceType};
use layertwine::engine::diff::{diff_to_line_diff, format_unified_diff};
use layertwine::engine::merge::{apply_deltas, merge_texts, MergeConflict};

// ---------------------------------------------------------------------------
// Test: Diff calculation with various scenarios
// ---------------------------------------------------------------------------

#[test]
fn test_diff_simple_replacement() {
    let old = "hello\nworld\n";
    let new = "hello\nrust\n";
    let line_diff = diff_to_line_diff(old, new);

    assert_eq!(line_diff.hunks.len(), 1, "should have 1 hunk");
    let hunk = &line_diff.hunks[0];

    let has_replace = hunk
        .ops
        .iter()
        .any(|op| matches!(op, layertwine::core::types::DiffOp::Replace { .. }));
    assert!(has_replace, "should contain Replace operation");
}

#[test]
fn test_diff_insert_operation() {
    let old = "a\n";
    let new = "a\nb\n";
    let line_diff = diff_to_line_diff(old, new);

    assert!(!line_diff.hunks.is_empty(), "should have hunks");

    let has_insert = line_diff.hunks.iter().any(|h| {
        h.ops
            .iter()
            .any(|op| matches!(op, layertwine::core::types::DiffOp::Insert { .. }))
    });
    assert!(has_insert, "should contain Insert operation");
}

#[test]
fn test_diff_delete_operation() {
    let old = "a\nb\n";
    let new = "a\n";
    let line_diff = diff_to_line_diff(old, new);

    assert!(!line_diff.hunks.is_empty(), "should have hunks");

    let has_delete = line_diff.hunks.iter().any(|h| {
        h.ops
            .iter()
            .any(|op| matches!(op, layertwine::core::types::DiffOp::Delete { .. }))
    });
    assert!(has_delete, "should contain Delete operation");
}

#[test]
fn test_diff_no_change() {
    let text = "line1\nline2\nline3\n";
    let line_diff = diff_to_line_diff(text, text);

    assert_eq!(line_diff.hunks.len(), 0, "no changes = no hunks");
}

#[test]
fn test_diff_multiple_hunks() {
    let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\n";
    let new = "a\nX\nc\nd\ne\nf\ng\nh\ni\nY\nk\nl\nm\nn\n";
    let line_diff = diff_to_line_diff(old, new);

    assert_eq!(
        line_diff.hunks.len(),
        2,
        "two separated changes should produce 2 hunks"
    );
}

#[test]
fn test_diff_empty_to_content() {
    let old = "";
    let new = "a\nb\nc\n";
    let line_diff = diff_to_line_diff(old, new);

    assert!(!line_diff.hunks.is_empty(), "should have hunks");
    assert_eq!(line_diff.hunks[0].old_start, 1);
}

#[test]
fn test_diff_content_to_empty() {
    let old = "a\nb\nc\n";
    let new = "";
    let line_diff = diff_to_line_diff(old, new);

    assert!(!line_diff.hunks.is_empty(), "should have hunks");

    let has_delete = line_diff.hunks.iter().any(|h| {
        h.ops
            .iter()
            .any(|op| matches!(op, layertwine::core::types::DiffOp::Delete { .. }))
    });
    assert!(has_delete, "should contain Delete operations");
}

#[test]
fn test_format_unified_diff() {
    let old = "a\nb\nc\n";
    let new = "a\nd\nc\n";
    let output = format_unified_diff(old, new, 1);

    assert!(output.contains("-b"), "should show deleted line");
    assert!(output.contains("+d"), "should show inserted line");
}

// ---------------------------------------------------------------------------
// Test: Delta application (apply_deltas)
// ---------------------------------------------------------------------------

#[test]
fn test_apply_deltas_empty() {
    let content = "hello\nworld\n";
    let result = apply_deltas(content, &[]).unwrap();

    assert_eq!(result, "hello\nworld\n");
}

#[test]
fn test_apply_deltas_single_insert() {
    let content = "line1\nline3\n";

    let hunk = layertwine::core::types::Hunk {
        old_start: 2,
        old_len: 1,
        new_start: 2,
        new_len: 2,
        ops: vec![
            layertwine::core::types::DiffOp::Insert {
                new_start: 2,
                lines: vec!["line2".to_string()],
            },
            layertwine::core::types::DiffOp::Equal { count: 1 },
        ],
    };

    let diff = LineDiff::new(vec![hunk]);
    let delta = Delta::new(
        FileNode::new(PathBuf::from("test.txt"), b""),
        diff,
        SourceType::Manual,
    );

    let result = apply_deltas(content, &[delta]).unwrap();
    assert_eq!(result, "line1\nline2\nline3\n");
}

#[test]
fn test_apply_deltas_single_delete() {
    let content = "line1\nline2\nline3\n";

    let hunk = layertwine::core::types::Hunk {
        old_start: 2,
        old_len: 1,
        new_start: 2,
        new_len: 0,
        ops: vec![layertwine::core::types::DiffOp::Delete {
            old_start: 2,
            count: 1,
        }],
    };

    let diff = LineDiff::new(vec![hunk]);
    let delta = Delta::new(
        FileNode::new(PathBuf::from("test.txt"), b""),
        diff,
        SourceType::Manual,
    );

    let result = apply_deltas(content, &[delta]).unwrap();
    assert_eq!(result, "line1\nline3\n");
}

#[test]
fn test_apply_deltas_single_replace() {
    let content = "aaa\nbbb\nccc\n";

    let hunk = layertwine::core::types::Hunk {
        old_start: 2,
        old_len: 1,
        new_start: 2,
        new_len: 1,
        ops: vec![layertwine::core::types::DiffOp::Replace {
            old_start: 2,
            old_count: 1,
            new_start: 2,
            lines: vec!["xxx".to_string()],
        }],
    };

    let diff = LineDiff::new(vec![hunk]);
    let delta = Delta::new(
        FileNode::new(PathBuf::from("test.txt"), b""),
        diff,
        SourceType::Manual,
    );

    let result = apply_deltas(content, &[delta]).unwrap();
    assert_eq!(result, "aaa\nxxx\nccc\n");
}

#[test]
fn test_apply_deltas_chain() {
    let content = "a\nb\nc\n";

    // First delta: insert 'x' after 'a'
    let hunk1 = layertwine::core::types::Hunk {
        old_start: 2,
        old_len: 1,
        new_start: 2,
        new_len: 2,
        ops: vec![
            layertwine::core::types::DiffOp::Insert {
                new_start: 2,
                lines: vec!["x".to_string()],
            },
            layertwine::core::types::DiffOp::Equal { count: 1 },
        ],
    };

    // Second delta: replace line 3 (now 'b') with 'y'
    let hunk2 = layertwine::core::types::Hunk {
        old_start: 3,
        old_len: 1,
        new_start: 3,
        new_len: 1,
        ops: vec![layertwine::core::types::DiffOp::Replace {
            old_start: 3,
            old_count: 1,
            new_start: 3,
            lines: vec!["y".to_string()],
        }],
    };

    let diff1 = LineDiff::new(vec![hunk1]);
    let diff2 = LineDiff::new(vec![hunk2]);

    let delta1 = Delta::new(
        FileNode::new(PathBuf::from("test.txt"), b""),
        diff1,
        SourceType::Manual,
    );
    let delta2 = Delta::new(
        FileNode::new(PathBuf::from("test.txt"), b""),
        diff2,
        SourceType::Manual,
    );

    let result = apply_deltas(content, &[delta1, delta2]).unwrap();
    assert_eq!(result, "a\nx\ny\nc\n");
}

#[test]
fn test_apply_deltas_empty_content() {
    let content = "";
    let result = apply_deltas(content, &[]).unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_apply_deltas_overlapping_hunks_error() {
    let content = "a\nb\nc\n";

    let hunk1 = layertwine::core::types::Hunk {
        old_start: 1,
        old_len: 1,
        new_start: 1,
        new_len: 1,
        ops: vec![layertwine::core::types::DiffOp::Equal { count: 1 }],
    };

    let hunk2 = layertwine::core::types::Hunk {
        old_start: 1,
        old_len: 1,
        new_start: 1,
        new_len: 1,
        ops: vec![layertwine::core::types::DiffOp::Equal { count: 1 }],
    };

    let diff = LineDiff::new(vec![hunk1, hunk2]);
    let delta = Delta::new(
        FileNode::new(PathBuf::from("test.txt"), b""),
        diff,
        SourceType::Manual,
    );

    let result = apply_deltas(content, &[delta]);
    assert!(result.is_err(), "overlapping hunks should error");
}

#[test]
fn test_apply_deltas_out_of_range_error() {
    let content = "a\nb\n";

    let hunk = layertwine::core::types::Hunk {
        old_start: 10,
        old_len: 1,
        new_start: 10,
        new_len: 1,
        ops: vec![layertwine::core::types::DiffOp::Equal { count: 1 }],
    };

    let diff = LineDiff::new(vec![hunk]);
    let delta = Delta::new(
        FileNode::new(PathBuf::from("test.txt"), b""),
        diff,
        SourceType::Manual,
    );

    let result = apply_deltas(content, &[delta]);
    assert!(result.is_err(), "out-of-range hunk should error");
}

// ---------------------------------------------------------------------------
// Test: Three-way merge (merge_texts)
// ---------------------------------------------------------------------------

#[test]
fn test_merge_identical_texts() {
    let base = "a\nb\nc\n";
    let (merged, conflicts) = merge_texts(base, base, base);

    assert!(conflicts.is_empty());
    assert_eq!(merged, "a\nb\nc\n");
}

#[test]
fn test_merge_non_conflicting_changes() {
    let base = "a\nb\nc\n";
    let ours = "x\nb\nc\n";
    let theirs = "a\nb\ny\n";

    let (merged, conflicts) = merge_texts(base, ours, theirs);

    assert!(conflicts.is_empty());
    assert!(merged.contains('x'));
    assert!(merged.contains('y'));
}

#[test]
fn test_merge_with_conflict() {
    let base = "a\nb\nc\n";
    let ours = "a\nX\nc\n";
    let theirs = "a\nY\nc\n";

    let (_, conflicts) = merge_texts(base, ours, theirs);

    assert!(!conflicts.is_empty());
    assert_eq!(conflicts[0].ours, vec!["X"]);
    assert_eq!(conflicts[0].theirs, vec!["Y"]);
}

#[test]
fn test_merge_same_change_both_sides() {
    let base = "a\nb\nc\n";
    let ours = "a\nX\nc\n";
    let theirs = "a\nX\nc\n";

    let (merged, conflicts) = merge_texts(base, ours, theirs);

    assert!(conflicts.is_empty());
    assert_eq!(merged, "a\nX\nc\n");
}

#[test]
fn test_merge_multiline_conflict() {
    let base = "a\nb\nc\nd\n";
    let ours = "a\nX\nY\nc\nd\n";
    let theirs = "a\nP\nQ\nc\nd\n";

    let (_, conflicts) = merge_texts(base, ours, theirs);

    assert!(!conflicts.is_empty());
    assert_eq!(conflicts[0].ours, vec!["X", "Y"]);
    assert_eq!(conflicts[0].theirs, vec!["P", "Q"]);
}

#[test]
fn test_merge_multiple_conflicts() {
    let base = "a\nb\nc\nd\ne\nf\n";
    let ours = "a\nX\nc\nd\nY\nf\n";
    let theirs = "a\nP\nc\nd\nQ\nf\n";

    let (_, conflicts) = merge_texts(base, ours, theirs);

    assert_eq!(conflicts.len(), 2);
}

#[test]
fn test_merge_empty_base() {
    let base = "";
    let ours = "a\nb\n";
    let theirs = "";

    let (merged, conflicts) = merge_texts(base, ours, theirs);

    assert!(conflicts.is_empty());
    assert_eq!(merged, "a\nb\n");
}

#[test]
fn test_merge_one_side_no_changes() {
    let base = "a\nb\nc\n";
    let ours = "a\nb\nc\n";
    let theirs = "a\nX\nc\n";

    let (merged, conflicts) = merge_texts(base, ours, theirs);

    assert!(conflicts.is_empty());
    assert_eq!(merged, "a\nX\nc\n");
}

#[test]
fn test_merge_insert_vs_delete() {
    let base = "a\nb\nc\n";
    let ours = "a\nc\n";
    let theirs = "a\nX\nb\nc\n";

    let (_, conflicts) = merge_texts(base, ours, theirs);

    assert!(!conflicts.is_empty(), "insert vs delete should conflict");
}

#[test]
fn test_conflict_marker_format() {
    let conflict = MergeConflict {
        start_line: 1,
        base: vec!["b".to_string()],
        ours: vec!["X".to_string()],
        theirs: vec!["Y".to_string()],
    };

    let markers = conflict.to_conflict_marker();

    assert!(markers.contains("<<<<<<< ours"));
    assert!(markers.contains("======="));
    assert!(markers.contains(">>>>>>> theirs"));
    assert!(markers.contains("X"));
    assert!(markers.contains("Y"));
}

#[test]
fn test_merge_empty_theirs() {
    let base = "a\nb\nc\n";
    let ours = "a\nb\nc\n";
    let theirs = "";

    let (merged, conflicts) = merge_texts(base, ours, theirs);

    assert!(conflicts.is_empty());
    // When one side deletes all content and the other makes no changes,
    // the merge result should be empty (deletion wins)
    assert_eq!(merged, "");
}
