use crate::core::delta::Delta;
use crate::core::file_node::FileNode;
use crate::core::types::{ContentId, DiffOp, Hunk, LineDiff, SourceType};

fn create_test_file_node() -> FileNode {
    FileNode::new("test.txt".into(), b"initial content")
}

fn create_simple_diff() -> LineDiff {
    LineDiff::new(vec![Hunk {
        old_start: 1,
        old_len: 1,
        new_start: 1,
        new_len: 2,
        ops: vec![
            DiffOp::Equal { count: 1 },
            DiffOp::Insert {
                new_start: 2,
                lines: vec!["new line".to_string()],
            },
        ],
    }])
}

#[test]
fn test_delta_new() {
    let file = create_test_file_node();
    let diff = create_simple_diff();
    let source = SourceType::Manual;

    let delta = Delta::new(file.clone(), diff, source);

    assert_ne!(delta.id, ContentId([0u8; 32]));
    assert_eq!(delta.file.path_str(), "test.txt");
    assert_eq!(delta.source, SourceType::Manual);
}

#[test]
fn test_delta_compute_id() {
    let file = create_test_file_node();
    let diff = create_simple_diff();
    let source = SourceType::Manual;

    let delta1 = Delta::new(file.clone(), diff.clone(), source.clone());
    std::thread::sleep(std::time::Duration::from_millis(10));
    let delta2 = Delta::new(file, diff, source);

    assert_eq!(delta1.id, delta2.id);
    assert_ne!(delta1.timestamp, delta2.timestamp);
}

#[test]
fn test_delta_summary_insert() {
    let file = create_test_file_node();
    let diff = LineDiff::new(vec![Hunk {
        old_start: 0,
        old_len: 0,
        new_start: 0,
        new_len: 2,
        ops: vec![DiffOp::Insert {
            new_start: 0,
            lines: vec!["line 1".to_string(), "line 2".to_string()],
        }],
    }]);
    let delta = Delta::new(file, diff, SourceType::Manual);

    let summary = delta.summary();
    assert_eq!(summary.inserts, 2);
    assert_eq!(summary.deletes, 0);
    assert_eq!(summary.replaces, 0);
    assert_eq!(summary.total_hunks, 1);
}

#[test]
fn test_delta_summary_delete() {
    let file = create_test_file_node();
    let diff = LineDiff::new(vec![Hunk {
        old_start: 1,
        old_len: 3,
        new_start: 1,
        new_len: 0,
        ops: vec![DiffOp::Delete {
            old_start: 1,
            count: 3,
        }],
    }]);
    let delta = Delta::new(file, diff, SourceType::Manual);

    let summary = delta.summary();
    assert_eq!(summary.inserts, 0);
    assert_eq!(summary.deletes, 3);
    assert_eq!(summary.replaces, 0);
    assert_eq!(summary.total_hunks, 1);
}

#[test]
fn test_delta_summary_replace() {
    let file = create_test_file_node();
    let diff = LineDiff::new(vec![Hunk {
        old_start: 1,
        old_len: 2,
        new_start: 1,
        new_len: 2,
        ops: vec![DiffOp::Replace {
            old_start: 1,
            old_count: 2,
            new_start: 1,
            lines: vec!["new 1".to_string(), "new 2".to_string()],
        }],
    }]);
    let delta = Delta::new(file, diff, SourceType::Manual);

    let summary = delta.summary();
    assert_eq!(summary.inserts, 0);
    assert_eq!(summary.deletes, 0);
    assert_eq!(summary.replaces, 2);
    assert_eq!(summary.total_hunks, 1);
}

#[test]
fn test_delta_summary_mixed() {
    let file = create_test_file_node();
    let diff = LineDiff::new(vec![
        Hunk {
            old_start: 1,
            old_len: 2,
            new_start: 1,
            new_len: 3,
            ops: vec![
                DiffOp::Insert {
                    new_start: 1,
                    lines: vec!["inserted".to_string()],
                },
                DiffOp::Equal { count: 2 },
            ],
        },
        Hunk {
            old_start: 3,
            old_len: 1,
            new_start: 4,
            new_len: 0,
            ops: vec![DiffOp::Delete {
                old_start: 3,
                count: 1,
            }],
        },
    ]);
    let delta = Delta::new(file, diff, SourceType::Manual);

    let summary = delta.summary();
    assert_eq!(summary.inserts, 1);
    assert_eq!(summary.deletes, 1);
    assert_eq!(summary.replaces, 0);
    assert_eq!(summary.total_hunks, 2);
}

#[test]
fn test_delta_serialization() {
    let file = create_test_file_node();
    let diff = create_simple_diff();
    let delta = Delta::new(file, diff, SourceType::Manual);

    let json = serde_json::to_string(&delta).unwrap();
    let delta2: Delta = serde_json::from_str(&json).unwrap();

    assert_eq!(delta.id, delta2.id);
    assert_eq!(delta.source, delta2.source);
}

#[test]
fn test_delta_with_agent_source() {
    let file = create_test_file_node();
    let diff = create_simple_diff();
    let source = SourceType::Agent("agent-001".into());

    let delta = Delta::new(file, diff, source);

    match delta.source {
        SourceType::Agent(id) => assert_eq!(id.to_string(), "agent-001"),
        _ => panic!("Expected Agent source"),
    }
}
