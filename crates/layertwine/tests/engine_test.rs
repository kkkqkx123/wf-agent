//! Test for apply_deltas function to verify newline handling

use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::types::SourceType;
use layertwine::core::types::{ContentId, DiffOp, Hunk, LineDiff};
use layertwine::engine::merge::apply_deltas;
use std::path::PathBuf;

fn create_test_delta(diff: LineDiff) -> Delta {
    Delta {
        id: ContentId([0u8; 32]),
        file: FileNode::new(PathBuf::from("test.txt"), &[]),
        diff,
        source: SourceType::Manual,
        timestamp: 0,
    }
}

#[test]
fn test_apply_deltas_preserves_trailing_newline() {
    // Test that trailing newline IS preserved
    let content = "Hello, World!\nThis is a test file.\n";
    let delta = create_test_delta(LineDiff { hunks: vec![] });

    let result = apply_deltas(content, &[delta]).unwrap();
    assert_eq!(
        result, "Hello, World!\nThis is a test file.\n",
        "Trailing newline should be preserved"
    );
}

#[test]
fn test_apply_deltas_without_trailing_newline() {
    // Test that content without trailing newline stays that way
    let content = "Hello, World!\nThis is a test file.";
    let delta = create_test_delta(LineDiff { hunks: vec![] });

    let result = apply_deltas(content, &[delta]).unwrap();
    assert_eq!(
        result, content,
        "Content without trailing newline should stay unchanged"
    );
}

#[test]
fn test_apply_deltas_with_insert_and_trailing_newline() {
    // Test that insert operations preserve trailing newline
    let content = "Line 1\nLine 2\n";
    let delta = create_test_delta(LineDiff {
        hunks: vec![Hunk {
            old_start: 2,
            old_len: 0,
            new_start: 2,
            new_len: 1,
            ops: vec![DiffOp::Insert {
                new_start: 2,
                lines: vec!["Inserted Line".to_string()],
            }],
        }],
    });

    let result = apply_deltas(content, &[delta]).unwrap();
    assert_eq!(
        result, "Line 1\nInserted Line\nLine 2\n",
        "Trailing newline should be preserved after insert"
    );
}

#[test]
fn test_apply_deltas_with_delete_and_trailing_newline() {
    // Test that delete operations preserve trailing newline
    let content = "Line 1\nLine 2\nLine 3\n";
    let delta = create_test_delta(LineDiff {
        hunks: vec![Hunk {
            old_start: 2,
            old_len: 1,
            new_start: 2,
            new_len: 0,
            ops: vec![DiffOp::Delete {
                old_start: 2,
                count: 1,
            }],
        }],
    });

    let result = apply_deltas(content, &[delta]).unwrap();
    assert_eq!(
        result, "Line 1\nLine 3\n",
        "Trailing newline should be preserved after delete"
    );
}
