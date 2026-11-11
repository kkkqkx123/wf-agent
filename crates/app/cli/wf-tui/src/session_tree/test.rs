use super::*;
use wf_types::message::MessageRole;

fn user_message(id: &str, text: &str, timestamp: i64) -> Message {
    Message {
        id: id.to_string(),
        role: MessageRole::User,
        content: MessageContentValue::Text(text.to_string()),
        timestamp,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    }
}

fn assistant_message(id: &str, text: &str, timestamp: i64) -> Message {
    Message {
        id: id.to_string(),
        role: MessageRole::Assistant,
        content: MessageContentValue::Text(text.to_string()),
        timestamp,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    }
}

fn open_tree() -> SessionTree {
    let mut tree = SessionTree::new();
    tree.open("exec-1".to_string());
    tree
}

fn land_sample(tree: &mut SessionTree) {
    let messages = vec![
        user_message("u1", "read the file", 1000),
        assistant_message("a1", "reading now", 2000),
        user_message("u2", "run the tests", 3000),
    ];
    let checkpoints = vec![CheckpointPoint {
        id: "cp-1".to_string(),
        timestamp: 1500,
    }];
    let (nodes, truncated) = assemble_tree(&messages, &checkpoints, &[]);
    tree.land(nodes, truncated, messages.len());
}

#[test]
fn open_replaces_the_session_and_close_forgets_it() {
    let mut tree = SessionTree::new();
    assert_eq!(tree.session_id(), None);
    tree.open("exec-1".to_string());
    assert_eq!(tree.session_id(), Some("exec-1"));
    tree.open("exec-2".to_string());
    assert_eq!(tree.session_id(), Some("exec-2"));
    tree.close();
    assert_eq!(tree.session_id(), None);
}

#[test]
fn assemble_interleaves_checkpoints_and_records_nearest() {
    let messages = vec![
        user_message("u1", "hello", 1000),
        assistant_message("a1", "hi", 2000),
    ];
    let checkpoints = vec![CheckpointPoint {
        id: "cp-1".to_string(),
        timestamp: 1500,
    }];
    let (nodes, truncated) = assemble_tree(&messages, &checkpoints, &[]);
    assert!(!truncated);
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0].id, "u1");
    assert_eq!(nodes[1].id, "checkpoint:cp-1");
    assert_eq!(nodes[2].id, "a1");
    assert_eq!(nodes[0].checkpoint_id, None);
    assert_eq!(nodes[2].checkpoint_id, Some("cp-1".to_string()));
}

#[test]
fn iteration_details_back_an_empty_trunk() {
    let iterations = vec![
        serde_json::json!({"iteration": 1, "response_content": "done", "timestamp": 100}),
        serde_json::json!({"iteration": 1, "tool_calls": [{"name": "read", "success": true}], "timestamp": 200}),
    ];
    let (nodes, _) = assemble_tree(&[], &[], &iterations);
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].kind, TreeNodeKind::Assistant);
    assert_eq!(nodes[1].kind, TreeNodeKind::Tool);
    assert_eq!(nodes[1].tool_name.as_deref(), Some("read"));
}

#[test]
fn filters_hide_system_and_tool_rows() {
    let mut tree = open_tree();
    land_sample(&mut tree);
    assert_eq!(tree.len(), 4);
    tree.set_filter(TreeFilter::NoTools);
    // No tool rows in the sample; count stays.
    assert_eq!(tree.len(), 4);
    tree.set_filter(TreeFilter::All);
    assert_eq!(tree.len(), 4);
}

#[test]
fn search_matches_all_tokens_case_insensitively() {
    let mut tree = open_tree();
    land_sample(&mut tree);
    tree.search_push('R');
    tree.search_push('E');
    tree.search_push('A');
    tree.search_push('D');
    // Substring match: "read the file" and "reading now"; the
    // hidden tail falls back to the nearest visible row.
    assert_eq!(tree.len(), 2);
    assert_eq!(tree.selected_node().map(|n| n.id.as_str()), Some("a1"));
    tree.search_backspace();
    tree.search_backspace();
    tree.search_backspace();
    tree.search_backspace();
    assert_eq!(tree.len(), 4);
    // Multi-token AND: both tokens must match; only one row qualifies.
    for c in "read file".chars() {
        tree.search_push(c);
    }
    assert_eq!(tree.len(), 1);
    assert_eq!(tree.selected_node().map(|n| n.id.as_str()), Some("u1"));
}

#[test]
fn fold_hides_section_rows_and_keeps_selection() {
    let mut tree = open_tree();
    land_sample(&mut tree);
    // Rows: u1, checkpoint, a1, u2; land starts at the tail (u2).
    // Walk back twice onto the checkpoint header.
    tree.move_prev();
    tree.move_prev();
    assert_eq!(
        tree.selected_node().map(|n| n.kind),
        Some(TreeNodeKind::Checkpoint)
    );
    tree.toggle_fold();
    // Section rows (a1, u2) hide; header stays.
    assert_eq!(tree.len(), 2);
    // Selection stays on the still-visible header.
    assert_eq!(
        tree.selected_node().map(|n| n.kind),
        Some(TreeNodeKind::Checkpoint)
    );
    tree.toggle_fold();
    assert_eq!(tree.len(), 4);
}

#[test]
fn selection_falls_to_nearest_following_row() {
    let mut tree = open_tree();
    land_sample(&mut tree);
    assert_eq!(tree.selected_node().map(|n| n.id.as_str()), Some("u2"));
    for c in "file".chars() {
        tree.search_push(c);
    }
    // u2 hidden and only u1 matches; selection falls back to row 0.
    assert_eq!(tree.len(), 1);
    assert_eq!(tree.selected(), 0);
}

#[test]
fn movement_wraps_and_pages_clamp() {
    let mut tree = open_tree();
    land_sample(&mut tree);
    // Land starts at the tail (index 3).
    assert_eq!(tree.selected(), 3);
    tree.move_next();
    assert_eq!(tree.selected(), 0);
    tree.move_prev();
    assert_eq!(tree.selected(), 3);
    tree.jump_to_first();
    tree.page_by(100);
    assert_eq!(tree.selected(), 3);
    tree.page_by(-100);
    assert_eq!(tree.selected(), 0);
}

#[test]
fn rows_mark_fold_state_and_live_tail() {
    let mut tree = open_tree();
    land_sample(&mut tree);
    let rows = tree.rows();
    assert!(rows[1].starts_with("▾ ⬢"), "expanded header: {}", rows[1]);
    assert!(rows[3].ends_with(" ●"), "live tail: {}", rows[3]);
    tree.move_prev();
    tree.move_prev();
    tree.toggle_fold();
    let rows = tree.rows();
    assert!(rows[1].starts_with("▸ ⬢"), "folded header: {}", rows[1]);
    tree.toggle_fold();
    let rows = tree.rows();
    assert!(rows[1].starts_with("▾ ⬢"), "expanded header: {}", rows[1]);
}

#[test]
fn status_line_reports_position_filter_and_search() {
    let mut tree = open_tree();
    land_sample(&mut tree);
    // Land starts at the tail.
    assert!(tree.status_line().starts_with("(4/4) · [default]"));
    tree.search_push('e');
    assert!(tree.status_line().contains("search: e"));
}

#[test]
fn node_cap_keeps_the_newest_messages() {
    let messages: Vec<Message> = (0..SESSION_TREE_NODE_CAP + 10)
        .map(|i| user_message(&format!("u{i}"), "hi", i as i64))
        .collect();
    let (nodes, truncated) = assemble_tree(&messages, &[], &[]);
    assert!(truncated);
    assert_eq!(nodes.len(), SESSION_TREE_NODE_CAP);
    assert_eq!(nodes.first().map(|n| n.id.as_str()), Some("u10"));
}

#[test]
fn checkpoint_points_flatten_both_response_shapes() {
    let loop_view = serde_json::json!({"items": [
        {"id": "cp-1", "timestamp": 100},
        {"id": "cp-2", "timestamp": 200},
    ]});
    assert_eq!(checkpoint_points(&loop_view).len(), 2);
    let chain_view = serde_json::json!({"items": [
        {"checkpoints": [{"id": "cp-3", "timestamp": 300}]},
    ]});
    let points = checkpoint_points(&chain_view);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].id, "cp-3");
}

#[test]
fn bookmark_toggle_filter_and_rows() {
    let mut tree = open_tree();
    land_sample(&mut tree);
    assert_eq!(tree.len(), 4);
    // Mark the second row (checkpoint header).
    tree.move_prev();
    tree.move_prev();
    let toggled = tree.toggle_bookmark_selected();
    assert!(toggled.is_some_and(|(_, _, marked)| marked));
    assert!(tree.rows()[1].contains("★"));
    assert!(tree.status_line().contains("marked: 1"));
    // The marked filter shows only the bookmarked row.
    tree.set_filter(TreeFilter::Bookmarked);
    assert_eq!(tree.len(), 1);
    // Unmarking while filtered empties the view.
    let toggled = tree.toggle_bookmark_selected();
    assert!(toggled.is_some_and(|(_, _, marked)| !marked));
    assert!(tree.is_empty());
    // Restoring the default filter shows the trunk again.
    tree.set_filter(TreeFilter::Default);
    assert_eq!(tree.len(), 4);
}

#[test]
fn bookmark_filter_cycle_includes_marked() {
    assert_eq!(TreeFilter::All.cycle(), TreeFilter::Bookmarked);
    assert_eq!(TreeFilter::Bookmarked.cycle(), TreeFilter::Default);
    assert_eq!(TreeFilter::Bookmarked.label(), "marked");
}

#[test]
fn tool_role_messages_become_tool_rows() {
    let mut message = user_message("t1", "result body", 1000);
    message.role = MessageRole::Tool;
    message.tool_name = Some("read".to_string());
    let (nodes, _) = assemble_tree(&[message], &[], &[]);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].kind, TreeNodeKind::Tool);
    assert_eq!(nodes[0].tool_name.as_deref(), Some("read"));
}
