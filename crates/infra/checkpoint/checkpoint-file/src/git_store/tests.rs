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
