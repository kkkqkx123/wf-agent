//! Provenance read-domain tests: change listings, diffs, timelines,
//! conflict enumeration and source-index fallback semantics.

use super::*;
use super::timeline::content_similarity;
use crate::file::FileCheckpointManager;
use crate::git_store::{
    commit_message, edit_ref_for_actor, feat_ref_for_name, merge_file_maps, GitStore, MODE_FILE,
    REF_HUMAN, REF_MAIN, STATE_CONFLICT_RESOLVED, STATE_CONFLICT_UNRESOLVED, SYSTEM_COMMITTER,
    TRAILER_CONFLICT_FILE, TRAILER_STATE,
};
use checkpoint_base::error::CheckpointError;
use std::collections::HashMap;

fn manager() -> FileCheckpointManager {
    FileCheckpointManager::new_in_memory().expect("in-memory manager")
}

fn standalone_store() -> GitStore {
    GitStore::init_temp().expect("temp git store")
}

fn changes(pairs: &[(&str, &[u8])]) -> HashMap<String, Option<(String, Vec<u8>)>> {
    pairs
        .iter()
        .map(|(p, b)| (p.to_string(), Some((MODE_FILE.to_string(), b.to_vec()))))
        .collect()
}

#[test]
fn rename_similarity_threshold_matches_standard_default() {
    assert_eq!(RENAME_SIMILARITY_THRESHOLD, 0.5);
    assert_eq!(content_similarity(b"same", b"same"), 1.0);
    assert_eq!(content_similarity(b"a\0b", b"a\0c"), 0.0);
    let before = "l1\nl2\nl3\nl4\nl5\n";
    let after = "l1\nl2\nCHANGED\nl4\nl5\n";
    let score = content_similarity(before.as_bytes(), after.as_bytes());
    assert!(
        score >= RENAME_SIMILARITY_THRESHOLD,
        "small edit must still count as rename, got {score}"
    );
    let distant = content_similarity(b"aaa\nbbb\n", b"xxx\nyyy\nzzz\n");
    assert!(
        distant < RENAME_SIMILARITY_THRESHOLD,
        "unrelated content must not count as rename, got {distant}"
    );
}

#[test]
fn list_changes_by_actor_reports_kinds_and_filters() {
    let manager = manager();
    let actor = manager.actor_id_for("entity-1");
    let c1 = manager.apply_agent_edit(&actor, "a.txt", b"one\ntwo\n").unwrap();
    let c2 = manager.apply_agent_edit(&actor, "a.txt", b"one\nthree\n").unwrap();
    let c3 = manager.apply_agent_edit(&actor, "b.txt", b"bee\n").unwrap();

    let all = manager
        .list_changes_by_actor(actor.as_str(), None, None)
        .unwrap();
    assert_eq!(all.len(), 3);
    let expected = [
        (c1.clone(), FileDiffKind::Added),
        (c2.clone(), FileDiffKind::Modified),
        (c3.clone(), FileDiffKind::Added),
    ];
    for (id, kind) in expected {
        let entry = all.iter().find(|c| c.snapshot_id == id).expect("commit listed");
        assert_eq!(entry.kind, kind);
        assert_eq!(entry.source, actor.as_str());
        assert_eq!(entry.message.as_deref(), Some("tool edit"));
    }
    let modified = all.iter().find(|c| c.snapshot_id == c2).unwrap();
    assert_eq!((modified.additions, modified.deletions), (1, 1));

    let filtered = manager
        .list_changes_by_actor(actor.as_str(), Some("a.txt"), None)
        .unwrap();
    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|c| c.file == "a.txt"));

    let outside_window = manager
        .list_changes_by_actor(actor.as_str(), None, Some((0, 0)))
        .unwrap();
    assert!(outside_window.is_empty());
}

#[test]
fn list_changes_by_path_includes_deletion_tombstone() {
    let manager = manager();
    let actor = manager.actor_id_for("entity-1");
    let c1 = manager.apply_agent_edit(&actor, "a.txt", b"one\ntwo\n").unwrap();
    let c2 = manager.apply_agent_edit(&actor, "a.txt", b"one\nthree\n").unwrap();
    let c3 = manager.apply_agent_delete(&actor, "a.txt").unwrap();

    let listed = manager.list_changes_by_path("a.txt", None).unwrap();
    assert_eq!(listed.len(), 3);
    let expected = [
        (c1.clone(), FileDiffKind::Added),
        (c2.clone(), FileDiffKind::Modified),
        (c3.clone(), FileDiffKind::Deleted),
    ];
    for (id, kind) in expected {
        let entry = listed.iter().find(|c| c.snapshot_id == id).expect("commit listed");
        assert_eq!(entry.kind, kind);
        assert_eq!(entry.file, "a.txt");
    }
    // Present files carry the resulting hash; a deletion is a tombstone
    // with no bytes and the removed line count.
    assert!(!listed.iter().find(|c| c.snapshot_id == c1).unwrap().hash.is_empty());
    assert!(!listed.iter().find(|c| c.snapshot_id == c2).unwrap().hash.is_empty());
    let tombstone = listed.iter().find(|c| c.snapshot_id == c3).unwrap();
    assert!(tombstone.hash.is_empty());
    assert_eq!((tombstone.additions, tombstone.deletions), (0, 2));

    // A path nobody ever touched stays empty (no similarity folding).
    assert!(manager.list_changes_by_path("zzz.txt", None).unwrap().is_empty());
}

#[test]
fn queries_fall_back_to_graph_when_index_cleared() {
    let manager = manager();
    let actor = manager.actor_id_for("entity-1");
    manager.apply_agent_edit(&actor, "a.txt", b"one\n").unwrap();
    manager.apply_agent_edit(&actor, "b.txt", b"two\n").unwrap();

    let storage = manager.storage_ref().unwrap();
    let indexed_actor = manager
        .list_changes_by_actor(actor.as_str(), None, None)
        .unwrap();
    let indexed_path = manager.list_changes_by_path("a.txt", None).unwrap();

    let cleared = storage.clear_source_index().unwrap();
    assert_eq!(cleared, 2);
    let status = manager.index_status().unwrap();
    assert_eq!(status.total_commits, 2);
    assert_eq!(status.indexed_commits, 0);
    assert!(status.needs_rebuild);

    // With an empty index the same queries must return identical results
    // via the bounded graph-scan fallback.
    let fallback_actor = manager
        .list_changes_by_actor(actor.as_str(), None, None)
        .unwrap();
    let fallback_path = manager.list_changes_by_path("a.txt", None).unwrap();
    assert_eq!(fallback_actor, indexed_actor);
    assert_eq!(fallback_path, indexed_path);
}

#[test]
fn rebuild_source_index_restores_health() {
    let manager = manager();
    let actor = manager.actor_id_for("entity-1");
    let first = manager.apply_agent_edit(&actor, "a.txt", b"one\n").unwrap();
    let second = manager.apply_agent_edit(&actor, "b.txt", b"two\n").unwrap();

    let git = manager.git_ref().unwrap();
    let storage = manager.storage_ref().unwrap();
    storage.clear_source_index().unwrap();
    assert!(manager.index_status().unwrap().needs_rebuild);

    let rebuilt = rebuild_source_index(git, storage).unwrap();
    assert_eq!(rebuilt, 2);
    let status = manager.index_status().unwrap();
    assert_eq!(status.indexed_commits, 2);
    assert!(!status.needs_rebuild);

    // Path lookups hit the fast path again with the right commit rows.
    let by_path = storage.find_commits_by_path("a.txt", 0).unwrap();
    assert_eq!(by_path.len(), 1);
    assert_eq!(by_path[0].commit_id, first);
    let by_actor = storage.find_commits_by_actor(actor.as_str(), 0).unwrap();
    assert_eq!(by_actor.len(), 2);
    assert!(by_actor.iter().any(|e| e.commit_id == second));
}

#[test]
fn file_timeline_links_rename_by_similarity() {
    let manager = manager();
    let actor = manager.actor_id_for("entity-1");
    let original: Vec<u8> = (1..=10)
        .map(|i| format!("line {i}\n"))
        .collect::<String>()
        .into_bytes();
    manager.apply_agent_edit(&actor, "doc.txt", &original).unwrap();
    let renamed: Vec<u8> = (1..=10)
        .map(|i| if i == 9 { "CHANGED\n".to_string() } else { format!("line {i}\n") })
        .collect::<String>()
        .into_bytes();
    manager
        .rename_file(&actor, "doc.txt", "notes.txt", &renamed)
        .unwrap();

    let git = manager.git_ref().unwrap();
    let storage = manager.storage_ref().unwrap();
    let timeline = file_timeline(git, storage, "notes.txt").unwrap();
    assert_eq!(timeline.original_path, "doc.txt");
    assert_eq!(timeline.entries.len(), 1);
    assert_eq!(timeline.entries[0].path, "notes.txt");
    assert_eq!(timeline.entries[0].moved_from.as_deref(), Some("doc.txt"));
    assert_eq!(timeline.entries[0].source, actor.as_str());
    assert!(!timeline.entries[0].content_hash.is_empty());
}

#[test]
fn file_timeline_does_not_link_dissimilar_renames() {
    let manager = manager();
    let actor = manager.actor_id_for("entity-1");
    let original: Vec<u8> = (1..=10)
        .map(|i| format!("line {i}\n"))
        .collect::<String>()
        .into_bytes();
    manager.apply_agent_edit(&actor, "doc.txt", &original).unwrap();
    let other: Vec<u8> = (1..=10)
        .map(|i| format!("other {i}\n"))
        .collect::<String>()
        .into_bytes();
    manager
        .rename_file(&actor, "doc.txt", "fresh.txt", &other)
        .unwrap();

    let git = manager.git_ref().unwrap();
    let storage = manager.storage_ref().unwrap();
    let timeline = file_timeline(git, storage, "fresh.txt").unwrap();
    assert_eq!(timeline.original_path, "fresh.txt");
    assert_eq!(timeline.entries.len(), 1);
    assert!(timeline.entries[0].moved_from.is_none());
}

#[test]
fn partition_views_and_history_paging() {
    let manager = manager();
    let actor = manager.actor_id_for("entity-1");
    let c1 = manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();
    let c2 = manager.apply_agent_edit(&actor, "a.txt", b"v2\n").unwrap();
    let c3 = manager.apply_agent_edit(&actor, "a.txt", b"v3\n").unwrap();
    manager.apply_manual_edit("h.txt", b"human\n").unwrap();

    let partitions = manager.list_partitions().unwrap();
    let edit_ref = edit_ref_for_actor(actor.as_str());
    // The partition's actor field is the sanitized edit-ref leaf, not the
    // raw actor id.
    let leaf = edit_ref
        .strip_prefix("refs/wf/edit/")
        .expect("edit ref leaf")
        .to_string();
    let edit = partitions
        .iter()
        .find(|p| p.name == edit_ref)
        .expect("edit partition listed");
    assert_eq!(edit.kind, "agent");
    assert_eq!(edit.actor.as_deref(), Some(leaf.as_str()));
    assert_eq!(edit.history_len, 3);
    assert_eq!(edit.current_snapshot, c3);
    assert!(partitions.iter().any(|p| p.name == REF_HUMAN && p.kind == "manual"));

    let head = manager.partition_history(&edit_ref, 2, 0).unwrap();
    assert_eq!(head.len(), 2);
    assert_eq!(head[0].snapshot_id, c3);
    assert_eq!(head[1].snapshot_id, c2);
    assert!(!head[0].is_merge);
    assert!(!head[0].has_conflicts);
    assert_eq!(head[0].message.as_deref(), Some("tool edit"));
    // History rows carry the raw actor id from the commit trailer.
    assert_eq!(head[0].actor.as_deref(), Some(actor.as_str()));

    let tail = manager.partition_history(&edit_ref, 1, 2).unwrap();
    assert_eq!(tail.len(), 1);
    assert_eq!(tail[0].snapshot_id, c1);

    assert!(matches!(
        manager.partition_history("refs/wf/feat/none", 5, 0),
        Err(CheckpointError::NotFound { .. })
    ));
}

#[test]
fn diff_actors_classifies_all_kinds() {
    let git = standalone_store();
    git.commit_on_ref(
        &edit_ref_for_actor("agent:x"),
        &changes(&[("a.txt", b"one\ntwo\n"), ("b.txt", b"shared\n")]),
        "agent:x",
        &commit_message("edit x", Some("agent:x"), None, Some("tool"), &[]),
    )
    .unwrap();
    git.commit_on_ref(
        &edit_ref_for_actor("agent:y"),
        &changes(&[("a.txt", b"one\nTWO\n"), ("c.txt", b"new\n")]),
        "agent:y",
        &commit_message("edit y", Some("agent:y"), None, Some("tool"), &[]),
    )
    .unwrap();

    let views = diff_actors(&git, "agent:x", "agent:y").unwrap();
    assert_eq!(views.len(), 3);
    let by_path: HashMap<&str, &FileDiffView> =
        views.iter().map(|v| (v.path.as_str(), v)).collect();

    let a = by_path["a.txt"];
    assert_eq!(a.kind, FileDiffKind::Modified);
    assert_eq!((a.additions, a.deletions), (1, 1));
    assert!(!a.hunks.is_empty());

    let b = by_path["b.txt"];
    assert_eq!(b.kind, FileDiffKind::Deleted);
    assert!(b.hunks.is_empty());
    // Deletion views are empty views: line counts live on the change
    // listing path (`delta_counts`), not on the actor diff.
    assert_eq!((b.additions, b.deletions), (0, 0));

    let c = by_path["c.txt"];
    assert_eq!(c.kind, FileDiffKind::Added);
    assert!(c.hunks.is_empty());

    // Binary content reports Modified without hunks.
    git.commit_on_ref(
        &edit_ref_for_actor("agent:z"),
        &changes(&[("a.txt", &[0xff, 0x00])]),
        "agent:z",
        &commit_message("edit z", Some("agent:z"), None, Some("tool"), &[]),
    )
    .unwrap();
    let bin = diff_actors(&git, "agent:x", "agent:z").unwrap();
    let a = bin.iter().find(|v| v.path == "a.txt").unwrap();
    assert_eq!(a.kind, FileDiffKind::Modified);
    assert!(a.binary);
    assert!(a.hunks.is_empty());
}

#[test]
fn diff_against_main_uses_main_ref() {
    let git = standalone_store();
    // Main holds the shared baseline.
    git.commit_on_ref(
        REF_MAIN,
        &changes(&[("b.txt", b"shared\n")]),
        "wf",
        &commit_message("baseline", Some("wf"), None, Some("tool"), &[]),
    )
    .unwrap();
    // The actor's edit line carries the baseline plus its own file.
    git.commit_on_ref(
        &edit_ref_for_actor("agent:x"),
        &changes(&[("a.txt", b"one\ntwo\n"), ("b.txt", b"shared\n")]),
        "agent:x",
        &commit_message("edit x", Some("agent:x"), None, Some("tool"), &[]),
    )
    .unwrap();

    let views = diff_against_main(&git, "agent:x").unwrap();
    let by_path: HashMap<&str, &FileDiffView> =
        views.iter().map(|v| (v.path.as_str(), v)).collect();
    // The view walks from the edit tree toward main: a file the edit line
    // owns but main lacks reads as Deleted, a shared file as Unchanged.
    assert_eq!(by_path["a.txt"].kind, FileDiffKind::Deleted);
    assert_eq!(by_path["b.txt"].kind, FileDiffKind::Unchanged);

    // Without a main ref this is a clear NotFound, not an empty diff.
    let fresh = standalone_store();
    fresh
        .commit_on_ref(
            &edit_ref_for_actor("agent:y"),
            &changes(&[("f.txt", b"y\n")]),
            "agent:y",
            &commit_message("edit y", Some("agent:y"), None, Some("tool"), &[]),
        )
        .unwrap();
    assert!(matches!(
        diff_against_main(&fresh, "agent:y"),
        Err(CheckpointError::NotFound { .. })
    ));
}

#[test]
fn workspace_views_and_entries() {
    let git = standalone_store();
    let head = git
        .commit_on_ref(
            &edit_ref_for_actor("agent:x"),
            &changes(&[("a.txt", b"one\n"), ("sub/b.txt", b"two\n")]),
            "agent:x",
            &commit_message("edit x", Some("agent:x"), None, Some("tool"), &[]),
        )
        .unwrap()
        .id;

    let files = get_actor_workspace(&git, "agent:x").unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "a.txt");
    assert_eq!(files[1].path, "sub/b.txt");
    assert_eq!(files[0].content, b"one\n".to_vec());
    assert!(!files[0].hash.is_empty());
    let commit = git.read_commit(&head).unwrap();
    assert_eq!(files[0].timestamp, commit.committer_ts);

    assert!(matches!(
        get_actor_workspace(&git, "agent:ghost"),
        Err(CheckpointError::NotFound { .. })
    ));
    assert!(matches!(
        get_main_workspace(&git),
        Err(CheckpointError::NotFound { .. })
    ));

    let feat_ref = feat_ref_for_name("demo");
    git.write_ref(&feat_ref, &head).unwrap();
    assert_eq!(get_feature_workspace(&git, "demo").unwrap(), files);

    let entries = workspace_entries(&files);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, "a.txt");
    assert_eq!(entries[0].content, b"one\n".to_vec());
    assert!(!entries[0].deleted);
}

#[test]
fn diff_workspaces_classifies_file_sets() {
    let w = |path: &str, content: &[u8]| WorkspaceFile {
        path: path.to_string(),
        content: content.to_vec(),
        hash: crate::file::util::sha256_hex(content),
        timestamp: 1_000,
    };
    let a = vec![w("p1.txt", b"one\n"), w("p2.txt", b"two\n"), w("p4.txt", b"same\n")];
    let b = vec![
        w("p1.txt", b"one\nnew\n"),
        w("p3.txt", b"three\n"),
        w("p4.txt", b"same\n"),
    ];

    let views = diff_workspaces(&a, &b);
    let paths: Vec<_> = views.iter().map(|v| v.path.as_str()).collect();
    assert_eq!(paths, vec!["p1.txt", "p2.txt", "p3.txt", "p4.txt"]);
    let by_path: HashMap<&str, &FileDiffView> =
        views.iter().map(|v| (v.path.as_str(), v)).collect();
    assert_eq!(by_path["p1.txt"].kind, FileDiffKind::Modified);
    assert_eq!((by_path["p1.txt"].additions, by_path["p1.txt"].deletions), (1, 0));
    assert!(!by_path["p1.txt"].hunks.is_empty());
    assert_eq!(by_path["p2.txt"].kind, FileDiffKind::Deleted);
    assert_eq!(by_path["p3.txt"].kind, FileDiffKind::Added);
    assert_eq!(by_path["p4.txt"].kind, FileDiffKind::Unchanged);
    assert!(by_path["p4.txt"].hunks.is_empty());
}

#[test]
fn parse_marker_conflicts_handles_all_shapes() {
    let two_way = "<<<<<<< ours\nO1\nO2\n=======\nT1\n>>>>>>> theirs\n";
    let views = parse_marker_conflicts("f.txt", two_way.as_bytes());
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].file, "f.txt");
    assert_eq!(views[0].start_line, 0);
    assert_eq!(views[0].ours, vec!["O1".to_string(), "O2".to_string()]);
    assert_eq!(views[0].theirs, vec!["T1".to_string()]);
    assert!(views[0].base.is_empty());

    let diff3 = "ctx\n<<<<<<< ours\nO1\n||||||| base\nB1\n=======\nT1\n>>>>>>> theirs\ntail\n";
    let views = parse_marker_conflicts("g.txt", diff3.as_bytes());
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].start_line, 1);
    assert_eq!(views[0].base, vec!["B1".to_string()]);
    assert_eq!(views[0].ours, vec!["O1".to_string()]);

    let multiple = "<<<<<<< ours\nA\n=======\nB\n>>>>>>> theirs\nplain\n<<<<<<< ours\nC\n=======\nD\n>>>>>>> theirs\n";
    let views = parse_marker_conflicts("h.txt", multiple.as_bytes());
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].start_line, 0);
    assert_eq!(views[1].start_line, 6);

    let unclosed = "<<<<<<< ours\nO1\n";
    let views = parse_marker_conflicts("i.txt", unclosed.as_bytes());
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].ours, vec!["O1".to_string()]);
    assert!(views[0].theirs.is_empty());

    assert!(parse_marker_conflicts("j.txt", b"clean file\n").is_empty());
}

#[test]
fn list_conflicts_enumerates_unresolved_merges() {
    let git = standalone_store();
    let base: HashMap<String, Vec<u8>> =
        [("f.txt".to_string(), b"base\n".to_vec())].into_iter().collect();
    let mut ours = base.clone();
    ours.insert("f.txt".to_string(), b"ours\n".to_vec());
    let mut theirs = base.clone();
    theirs.insert("f.txt".to_string(), b"theirs\n".to_vec());

    let (merged, conflicted) = merge_file_maps(&base, &ours, &theirs);
    assert_eq!(conflicted, vec!["f.txt".to_string()]);
    let text = String::from_utf8(merged["f.txt"].clone().unwrap()).unwrap();
    assert!(text.contains("<<<<<<< ours"));

    let mut tree_changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
    for (path, bytes) in &merged {
        tree_changes.insert(
            path.clone(),
            bytes
                .clone()
                .map(|b| (MODE_FILE.to_string(), b)),
        );
    }
    let merged_tree = git.build_tree_from_parent(None, &tree_changes).unwrap();

    let ours_tree = git
        .build_tree_from_parent(None, &changes(&[("f.txt", b"ours\n")]))
        .unwrap();
    let theirs_tree = git
        .build_tree_from_parent(None, &changes(&[("f.txt", b"theirs\n")]))
        .unwrap();
    let ours_id = git
        .write_commit(&ours_tree, &[], "agent:x", SYSTEM_COMMITTER, 1_000, "ours")
        .unwrap();
    let theirs_id = git
        .write_commit(&theirs_tree, &[], "agent:y", SYSTEM_COMMITTER, 2_000, "theirs")
        .unwrap();
    let merge_message = format!(
        "merge feature\n{TRAILER_STATE}: {STATE_CONFLICT_UNRESOLVED}\n{TRAILER_CONFLICT_FILE}: f.txt"
    );
    let merge_id = git
        .write_commit(
            &merged_tree,
            &[ours_id, theirs_id],
            "merger",
            SYSTEM_COMMITTER,
            3_000,
            &merge_message,
        )
        .unwrap();
    let feat_ref = feat_ref_for_name("demo");
    git.write_ref(&feat_ref, &merge_id).unwrap();

    let unresolved = unresolved_merges(&git).unwrap();
    assert_eq!(
        unresolved,
        vec![(feat_ref.clone(), merge_id.clone(), vec!["f.txt".to_string()])]
    );

    let conflicts = list_conflicts(&git).unwrap();
    assert_eq!(conflicts.len(), 1);
    let cf = &conflicts[0];
    assert_eq!(cf.path, "f.txt");
    assert_eq!(cf.snapshot_id, merge_id);
    assert_eq!(cf.partition, feat_ref);
    assert_eq!(cf.conflicts.len(), 1);
    assert_eq!(cf.conflicts[0].ours, vec!["ours".to_string()]);
    assert_eq!(cf.conflicts[0].theirs, vec!["theirs".to_string()]);

    // Resolving the merge state removes it from the conflict listing.
    let resolved_id = git
        .write_commit(
            &merged_tree,
            std::slice::from_ref(&merge_id),
            "resolver",
            SYSTEM_COMMITTER,
            4_000,
            &format!("resolve\n{TRAILER_STATE}: {STATE_CONFLICT_RESOLVED}"),
        )
        .unwrap();
    git.write_ref(&feat_ref, &resolved_id).unwrap();
    assert!(list_conflicts(&git).unwrap().is_empty());
    assert!(unresolved_merges(&git).unwrap().is_empty());
}
