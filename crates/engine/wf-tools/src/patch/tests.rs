//! Unit tests for both patch formats: the V4A parser/applier and the
//! SEARCH/REPLACE parser/applier.

use super::*;

// ── apply_patch parser ───────────────────────────────

#[test]
fn test_parse_add_file() {
    let patch = "*** Begin Patch\n*** Add File: new.txt\n+hello\n+world\n*** End Patch";
    let hunks = parse_patch(patch).unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].op, PatchOp::AddFile);
    assert_eq!(hunks[0].path, "new.txt");
    assert_eq!(hunks[0].contents, "hello\nworld\n");
}

#[test]
fn test_parse_delete_file() {
    let patch = "*** Begin Patch\n*** Delete File: old.txt\n*** End Patch";
    let hunks = parse_patch(patch).unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].op, PatchOp::DeleteFile);
    assert_eq!(hunks[0].path, "old.txt");
}

#[test]
fn test_parse_update_file() {
    let patch = "*** Begin Patch\n*** Update File: a.txt\n@@ context\n-old\n+new\n*** End Patch";
    let hunks = parse_patch(patch).unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].op, PatchOp::UpdateFile);
    assert_eq!(hunks[0].path, "a.txt");
    let chunk = &hunks[0].chunks[0];
    assert_eq!(chunk.change_context.as_deref(), Some("context"));
    assert_eq!(chunk.old_lines, vec!["old"]);
    assert_eq!(chunk.new_lines, vec!["new"]);
}

#[test]
fn test_parse_update_file_move() {
    let patch = "*** Begin Patch\n*** Update File: a.txt\n*** Move to: b.txt\n@@\n-old\n+new\n*** End Patch";
    let hunks = parse_patch(patch).unwrap();
    assert_eq!(hunks[0].move_path.as_deref(), Some("b.txt"));
}

#[test]
fn test_parse_multiple_hunks_and_missing_context() {
    let patch = "*** Begin Patch\n*** Add File: x\n+a\n*** Update File: y.txt\n+added-line\n*** Delete File: z\n*** End Patch";
    let hunks = parse_patch(patch).unwrap();
    assert_eq!(hunks.len(), 3);
    assert_eq!(hunks[0].op, PatchOp::AddFile);
    assert_eq!(hunks[1].op, PatchOp::UpdateFile);
    assert_eq!(hunks[2].op, PatchOp::DeleteFile);
    // First chunk of an update may omit the @@ context marker.
    assert_eq!(hunks[1].chunks[0].old_lines.len(), 0);
    assert_eq!(hunks[1].chunks[0].new_lines, vec!["added-line"]);
}

#[test]
fn test_parse_end_of_file_marker() {
    let patch =
        "*** Begin Patch\n*** Update File: a.txt\n@@\n+tail\n*** End of File\n*** End Patch";
    let hunks = parse_patch(patch).unwrap();
    assert!(hunks[0].chunks[0].is_end_of_file);
}

#[test]
fn test_parse_rejects_bad_boundaries() {
    assert!(parse_patch("*** Begin Patch\n*** End Patch\n").is_ok());
    assert!(parse_patch("foo\n*** End Patch").is_err());
    assert!(parse_patch("*** Begin Patch\nfoo").is_err());
}

#[test]
fn test_parse_rejects_absolute_or_traversal_path() {
    let patch = "*** Begin Patch\n*** Add File: /etc/passwd\n+x\n*** End Patch";
    assert!(parse_patch(patch).is_err());
    let patch = "*** Begin Patch\n*** Add File: ../evil.txt\n+x\n*** End Patch";
    assert!(parse_patch(patch).is_err());
}

#[test]
fn test_parse_heredoc_lenient() {
    let patch = "<<EOF\n*** Begin Patch\n*** Add File: x.txt\n+line\n*** End Patch\nEOF";
    let hunks = parse_patch(patch).unwrap();
    assert_eq!(hunks.len(), 1);
}

// ── apply_patch applier ──────────────────────────────

#[test]
fn test_apply_pure_insert_at_end() {
    let chunks = vec![UpdateFileChunk {
        change_context: None,
        old_lines: vec![],
        new_lines: vec!["tail".into()],
        is_end_of_file: false,
    }];
    let result = apply_chunks_to_content("a\nb\n", "f.txt", &chunks).unwrap();
    assert_eq!(result, "a\nb\ntail\n");
}

#[test]
fn test_apply_update_replaces_old_lines() {
    let chunks = vec![UpdateFileChunk {
        change_context: None,
        old_lines: vec!["old".into()],
        new_lines: vec!["new".into()],
        is_end_of_file: false,
    }];
    let result = apply_chunks_to_content("before\nold\nafter\n", "f.txt", &chunks).unwrap();
    assert_eq!(result, "before\nnew\nafter\n");
}

#[test]
fn test_apply_old_lines_not_found() {
    let chunks = vec![UpdateFileChunk {
        change_context: None,
        old_lines: vec!["missing".into()],
        new_lines: vec!["x".into()],
        is_end_of_file: false,
    }];
    assert!(apply_chunks_to_content("hello\n", "f.txt", &chunks).is_err());
}

#[test]
fn test_apply_trim_end_match() {
    let chunks = vec![UpdateFileChunk {
        change_context: None,
        old_lines: vec!["foo".into()],
        new_lines: vec!["bar".into()],
        is_end_of_file: false,
    }];
    let result = apply_chunks_to_content("foo  \n", "f.txt", &chunks).unwrap();
    assert_eq!(result, "bar\n");
}

#[test]
fn test_apply_trailing_newline_pattern_retry() {
    let chunks = vec![UpdateFileChunk {
        change_context: None,
        old_lines: vec!["line".into(), String::new()],
        new_lines: vec!["replaced".into(), String::new()],
        is_end_of_file: false,
    }];
    let result = apply_chunks_to_content("a\nline", "f.txt", &chunks).unwrap();
    assert_eq!(result, "a\nreplaced\n");
}

#[test]
fn test_apply_context_anchors() {
    let chunks = vec![UpdateFileChunk {
        change_context: Some("anchor".into()),
        old_lines: vec!["hit".into()],
        new_lines: vec!["changed".into()],
        is_end_of_file: false,
    }];
    let result = apply_chunks_to_content("anchor\nhit\n", "f.txt", &chunks).unwrap();
    assert_eq!(result, "anchor\nchanged\n");
}

#[test]
fn test_apply_multiple_chunks_sequentially() {
    let chunks = vec![
        UpdateFileChunk {
            change_context: None,
            old_lines: vec!["one".into()],
            new_lines: vec!["ONE".into()],
            is_end_of_file: false,
        },
        UpdateFileChunk {
            change_context: None,
            old_lines: vec!["two".into()],
            new_lines: vec!["TWO".into()],
            is_end_of_file: false,
        },
    ];
    let result = apply_chunks_to_content("one\ntwo\n", "f.txt", &chunks).unwrap();
    assert_eq!(result, "ONE\nTWO\n");
}

// ── apply_diff parser ────────────────────────────────

#[test]
fn test_parse_basic_search_replace() {
    let diff = "<<<<<<< SEARCH\nfoo\n=======\nbar\n>>>>>>> REPLACE";
    let blocks = parse_search_replace_blocks(diff).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].search_content, "foo");
    assert_eq!(blocks[0].replace_content, "bar");
}

#[test]
fn test_parse_multiple_blocks() {
    let diff = "<<<<<<< SEARCH\na\n=======\nb\n>>>>>>> REPLACE\n<<<<<<< SEARCH\nc\n=======\nd\n>>>>>>> REPLACE";
    let blocks = parse_search_replace_blocks(diff).unwrap();
    assert_eq!(blocks.len(), 2);
}

#[test]
fn test_parse_legacy_start_line_and_separator() {
    let diff = "<<<<<<< SEARCH\n:start_line:10\n-------\nfoo\n=======\nbar\n>>>>>>> REPLACE";
    let blocks = parse_search_replace_blocks(diff).unwrap();
    assert_eq!(blocks[0].start_line, Some(10));
    assert_eq!(blocks[0].search_content, "foo");
}

#[test]
fn test_parse_hints() {
    let diff = "<<<<<<< SEARCH\n# line: 5\n# context: fn main\nfoo\n=======\nbar\n>>>>>>> REPLACE";
    let blocks = parse_search_replace_blocks(diff).unwrap();
    assert_eq!(blocks[0].start_line, Some(5));
    assert_eq!(blocks[0].context_hint.as_deref(), Some("fn main"));
    assert_eq!(blocks[0].search_content, "foo");
}

#[test]
fn test_parse_escaped_markers() {
    let diff = "<<<<<<< SEARCH\n\\=======\nliteral\n=======\n\\\\>>>>>>>\n>>>>>>> REPLACE";
    let blocks = parse_search_replace_blocks(diff).unwrap();
    assert!(blocks[0].search_content.contains("======="));
    assert!(blocks[0].replace_content.contains("\\>>>>>>>"));
}

#[test]
fn test_parse_invalid_missing_separator() {
    let diff = "<<<<<<< SEARCH\nfoo\n>>>>>>> REPLACE";
    assert!(parse_search_replace_blocks(diff).is_err());
}

#[test]
fn test_validate_marker_sequencing() {
    assert!(validate_marker_sequencing("<<<<<<< SEARCH\na\n=======\nb\n>>>>>>> REPLACE").is_ok());
    assert!(validate_marker_sequencing("=======\nb\n>>>>>>> REPLACE").is_err());
    assert!(validate_marker_sequencing("<<<<<<< SEARCH\na\n=======\nb").is_err());
}

// ── apply_diff applier ───────────────────────────────

#[test]
fn test_apply_block_basic() {
    let lines: Vec<String> = vec!["a".into(), "foo".into(), "b".into()];
    let block = SearchReplaceBlock {
        search_content: "foo".into(),
        replace_content: "bar".into(),
        start_line: None,
        context_hint: None,
    };
    let (new_lines, delta) = apply_block(&lines, &block, 0).unwrap();
    assert_eq!(new_lines, vec!["a", "bar", "b"]);
    assert_eq!(delta, 0);
}

#[test]
fn test_apply_block_no_match_error() {
    let lines: Vec<String> = vec!["a".into(), "b".into()];
    let block = SearchReplaceBlock {
        search_content: "zzz".into(),
        replace_content: "x".into(),
        start_line: None,
        context_hint: None,
    };
    assert!(apply_block(&lines, &block, 0).is_err());
}

#[test]
fn test_apply_block_empty_search_error() {
    let lines: Vec<String> = vec!["a".into()];
    let block = SearchReplaceBlock {
        search_content: String::new(),
        replace_content: "x".into(),
        start_line: None,
        context_hint: None,
    };
    assert!(apply_block(&lines, &block, 0).is_err());
}

#[test]
fn test_apply_block_indentation_preserved() {
    let lines: Vec<String> = vec![
        "    fn foo() {".into(),
        "        old".into(),
        "    }".into(),
    ];
    let block = SearchReplaceBlock {
        search_content: "    old".into(),
        replace_content: "    new1\n    new2".into(),
        start_line: None,
        context_hint: None,
    };
    let (new_lines, delta) = apply_block(&lines, &block, 0).unwrap();
    assert_eq!(
        new_lines,
        vec!["    fn foo() {", "        new1", "        new2", "    }"]
    );
    assert_eq!(delta, 1);
}

#[test]
fn test_apply_block_context_hint() {
    let lines: Vec<String> = vec![
        "fn helper() {}".into(),
        "x".into(),
        "fn main() {}".into(),
        "target".into(),
    ];
    let block = SearchReplaceBlock {
        search_content: "target".into(),
        replace_content: "changed".into(),
        start_line: None,
        context_hint: Some("fn main".into()),
    };
    let (new_lines, _) = apply_block(&lines, &block, 0).unwrap();
    assert_eq!(new_lines[3], "changed");
}

#[test]
fn test_apply_block_start_line_anchored() {
    let lines: Vec<String> = vec!["a".into(), "b".into(), "target".into()];
    let block = SearchReplaceBlock {
        search_content: "target".into(),
        replace_content: "changed".into(),
        start_line: Some(3),
        context_hint: None,
    };
    let (new_lines, _) = apply_block(&lines, &block, 0).unwrap();
    assert_eq!(new_lines[2], "changed");
}
