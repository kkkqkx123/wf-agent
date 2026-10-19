use super::*;
use std::collections::HashMap;

fn store() -> GitStore {
    GitStore::init_temp().expect("temp git store")
}

fn changes(pairs: &[(&str, &[u8])]) -> HashMap<String, Option<(String, Vec<u8>)>> {
    pairs
        .iter()
        .map(|(p, b)| (p.to_string(), Some((MODE_FILE.to_string(), b.to_vec()))))
        .collect()
}

#[test]
fn refs_roundtrip_atomically() {
    let store = store();
    assert_eq!(store.read_ref(REF_MAIN).unwrap(), None);
    let tree = store.write_tree(&[]).unwrap();
    let id = store
        .write_commit(&tree, &[], "a", SYSTEM_COMMITTER, 1_000, "init")
        .unwrap();
    store.write_ref(REF_MAIN, &id).unwrap();
    assert_eq!(
        store.read_ref(REF_MAIN).unwrap().as_deref(),
        Some(id.as_str())
    );
    assert!(store.compare_and_swap(REF_MAIN, Some("nope"), &id).is_err());
    store.compare_and_swap(REF_MAIN, Some(&id), &id).unwrap();
    assert_eq!(store.copy_ref(REF_MAIN, REF_HUMAN).unwrap(), id);
    // Ref transactions write no reflog sidecar files.
    assert!(!store.git_dir().join("logs").exists());
    // Prefix and exact-name listings agree.
    let refs = store.list_refs("refs/wf/").unwrap();
    assert!(refs.contains(&(REF_MAIN.to_string(), id.clone())));
    assert!(refs.contains(&(REF_HUMAN.to_string(), id.clone())));
    assert_eq!(
        store.list_refs(REF_MAIN).unwrap(),
        vec![(REF_MAIN.to_string(), id.clone())]
    );
    // Deleting a missing ref is a no-op success.
    store.delete_ref("refs/wf/review/missing").unwrap();
}

#[test]
fn concurrent_compare_and_swap_keeps_one_winner() {
    let store = store();
    let tree = store.write_tree(&[]).unwrap();
    let first = store
        .write_commit(&tree, &[], "a", SYSTEM_COMMITTER, 1_000, "one")
        .unwrap();
    let second = store
        .write_commit(&tree, &[], "a", SYSTEM_COMMITTER, 2_000, "two")
        .unwrap();
    store.write_ref(REF_MAIN, &first).unwrap();
    std::thread::scope(|scope| {
        let a = scope.spawn(|| store.compare_and_swap(REF_MAIN, Some(&first), &second));
        let b = scope.spawn(|| store.compare_and_swap(REF_MAIN, Some(&first), &second));
        let results = [a.join().unwrap(), b.join().unwrap()];
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    });
    assert_eq!(
        store.read_ref(REF_MAIN).unwrap().as_deref(),
        Some(second.as_str())
    );
}

#[test]
fn linear_commits_skip_empty() {
    let store = store();
    let first = store
        .commit_on_ref(
            &edit_ref_for_actor("agent:x"),
            &changes(&[("a.txt", b"hi")]),
            "agent:x",
            "edit",
        )
        .unwrap();
    assert!(first.created);
    let second = store
        .commit_on_ref(
            &edit_ref_for_actor("agent:x"),
            &changes(&[("a.txt", b"hi")]),
            "agent:x",
            "edit",
        )
        .unwrap();
    assert!(!second.created);
    assert_eq!(first.id, second.id);
}

#[test]
fn trees_roundtrip_nested_paths() {
    let store = store();
    let tree = store
        .build_tree_from_parent(None, &changes(&[("sub/a.txt", b"a"), ("b.txt", b"b")]))
        .unwrap();
    let files = store.tree_to_files(&tree).unwrap();
    assert_eq!(files.len(), 2);
    let bytes = store.tree_to_bytes(&tree).unwrap();
    assert_eq!(bytes["sub/a.txt"], b"a");
}

#[test]
fn merge_semantics_are_standard() {
    let base: HashMap<String, Vec<u8>> = [("f".to_string(), b"line\n".to_vec())]
        .into_iter()
        .collect();
    let mut ours = base.clone();
    ours.insert("f".to_string(), b"ours\n".to_vec());
    let (merged, conflicts) = merge_file_maps(&base, &ours, &base);
    assert!(conflicts.is_empty());
    assert_eq!(merged["f"].clone().unwrap(), b"ours\n");
    let mut theirs = base.clone();
    theirs.insert("f".to_string(), b"theirs\n".to_vec());
    let (merged, conflicts) = merge_file_maps(&base, &ours, &theirs);
    assert_eq!(conflicts, vec!["f".to_string()]);
    let text = String::from_utf8(merged["f"].clone().unwrap()).unwrap();
    assert!(text.contains("<<<<<<< ours"));
    assert!(text.contains("======="));
    assert!(text.contains(">>>>>>> theirs"));

    // Added on one side only merges cleanly.
    let empty: HashMap<String, Vec<u8>> = HashMap::new();
    let (merged, conflicts) = merge_file_maps(&empty, &empty, &theirs);
    assert!(conflicts.is_empty());
    assert_eq!(merged["f"].clone().unwrap(), b"theirs\n");

    // Deleted on both sides stays deleted without conflict.
    let (merged, conflicts) = merge_file_maps(&base, &empty, &empty);
    assert!(conflicts.is_empty());
    assert_eq!(merged["f"], None);
}

#[test]
fn ancestor_and_merge_base() {
    let store = store();
    let a = store
        .commit_on_ref(
            &edit_ref_for_actor("a"),
            &changes(&[("f", b"1")]),
            "a",
            "one",
        )
        .unwrap()
        .id;
    let b = store
        .commit_on_ref(
            &edit_ref_for_actor("a"),
            &changes(&[("f", b"2")]),
            "a",
            "two",
        )
        .unwrap()
        .id;
    assert!(store.is_ancestor(&a, &b).unwrap());
    assert!(!store.is_ancestor(&b, &a).unwrap());
    assert_eq!(
        store.merge_base(&a, &b).unwrap().as_deref(),
        Some(a.as_str())
    );
}

#[test]
fn deep_linear_history_traverses_beyond_one_hundred_commits() {
    let store = store();
    let depth = 150usize;

    let mut last = String::new();
    for i in 0..depth {
        let outcome = store
            .commit_on_ref(
                REF_MAIN,
                &changes(&[("f", format!("v{i}").as_bytes())]),
                "a",
                &format!("commit {i}"),
            )
            .unwrap();
        assert!(outcome.created, "commit {i} must land");
        last = outcome.id;
    }

    let head = store.read_ref(REF_MAIN).unwrap().unwrap();
    assert_eq!(head, last);

    // Full traversal sees every commit exactly once.
    let all = store.all_commits().unwrap();
    assert_eq!(all.len(), depth);
    let mut ids: Vec<_> = all.iter().map(|c| c.id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), depth, "no duplicate ids in traversal");

    // Bounded log keeps topological order from the tip.
    let log = store.log(&head, 200).unwrap();
    assert_eq!(log.len(), depth);
    assert_eq!(log[0].id, head);

    // The very first commit is an ancestor of the tip.
    let first = log.last().unwrap().id.clone();
    assert!(store.is_ancestor(&first, &head).unwrap());
}

#[test]
fn large_blob_roundtrips_intact() {
    let store = store();
    // 2 MiB pseudo-random payload: well above any inline threshold, and
    // incompressible enough that zlib cannot shrink it away.
    let mut payload = Vec::with_capacity(2 * 1024 * 1024);
    let mut x: u64 = 0x9E3779B97F4A7C15;
    for _ in 0..payload.capacity() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        payload.push(x as u8);
    }

    let id = store.write_blob(&payload).unwrap();
    assert!(is_hex_id(&id));
    let round = store.read_blob(&id).unwrap();
    assert_eq!(round.len(), payload.len());
    assert_eq!(round, payload);
}

#[test]
fn truncated_loose_object_reports_corruption_instead_of_panic() {
    let store = store();
    let id = store.write_blob(b"payload that will be half written").unwrap();

    // Simulate a crash mid-write: the loose object file exists but is
    // truncated (the store writes object before ref, so a torn object is
    // never referenced, but the file itself must still read as an error).
    let loose = store
        .git_dir()
        .join("objects")
        .join(&id[..2])
        .join(&id[2..]);
    assert!(loose.exists(), "loose object file exists before the tear");
    // Loose objects are stored read-only: reopen for the simulated tear.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&loose).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&loose, perms).unwrap();
    }
    let full = std::fs::read(&loose).unwrap();
    std::fs::write(&loose, &full[..full.len() / 2]).unwrap();

    let err = store.read_blob(&id).unwrap_err();
    assert!(
        matches!(err, GitStoreError::Corrupt { .. } | GitStoreError::ObjectNotFound(_)),
        "torn object must surface as an explicit error, got {err:?}"
    );
}

#[test]
fn dangling_ref_reports_missing_object_explicitly() {
    let store = store();
    // A ref that points at a commit id with no object behind it: the state
    // a crash between ref write and object write would leave behind.
    let ghost = "1234567890123456789012345678901234567890";
    store.write_ref(REF_MAIN, ghost).unwrap();

    let err = store.read_commit(ghost).unwrap_err();
    assert!(matches!(err, GitStoreError::ObjectNotFound(_)));

    // Writes on other refs are untouched: the corruption is scoped to the
    // torn row, not the whole store.
    store
        .commit_on_ref(REF_HUMAN, &changes(&[("f", b"1")]), "a", "ok")
        .unwrap()
        .id;
    assert!(store.read_ref(REF_HUMAN).unwrap().is_some());
}
