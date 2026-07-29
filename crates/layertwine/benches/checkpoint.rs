use std::collections::HashMap;
use std::path::PathBuf;

use layertwine::checkpoint::dag::CheckpointDag;
use layertwine::checkpoint::repo::CheckpointRepo;
use layertwine::checkpoint::time_index::TimeIndex;
use layertwine::checkpoint::types::{Checkpoint, CheckpointBuilder, CheckpointMetadata};
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::{Snapshot, SnapshotContent};
use layertwine::core::types::{CheckpointId, ContentId, SnapshotId};

// ============================================================================
// Shared helpers
// ============================================================================

fn cid(data: &[u8]) -> CheckpointId {
    ContentId::from_content(data)
}

fn sid(data: &[u8]) -> SnapshotId {
    ContentId::from_content(data)
}

fn make_metadata() -> CheckpointMetadata {
    CheckpointMetadata::new("bench-user", "benchmark commit")
}

fn make_snapshot(seed: u8, source: &str) -> Snapshot {
    let file = FileNode::new(PathBuf::from("dummy"), &[seed]);
    let mut snap = Snapshot::new_with_content(
        file,
        SnapshotContent::FileContent(vec![seed]),
        source.to_string(),
        String::new(),
        vec![],
        vec![],
    );
    snap.id = ContentId::from_content(&[seed; 16]);
    snap
}

fn make_checkpoint_with_snapshots(count: usize) -> Checkpoint {
    let snapshots: Vec<SnapshotId> = (0..count)
        .map(|i| ContentId::from_content(&[i as u8; 16]))
        .collect();
    Checkpoint::new(snapshots, vec![], make_metadata())
}

// ============================================================================
// Group 1: Checkpoint creation & compute_id
// ============================================================================

fn bench_checkpoint_creation(c: &mut criterion::Criterion) {
    for num_snapshots in &[1, 5, 10, 50, 100] {
        let snapshots: Vec<SnapshotId> = (0..*num_snapshots)
            .map(|i| ContentId::from_content(&[i as u8; 16]))
            .collect();
        let metadata = make_metadata();

        c.bench_function(
            &format!("checkpoint_create_with_{}_snapshots", num_snapshots),
            |b| b.iter(|| Checkpoint::new(snapshots.clone(), vec![], metadata.clone())),
        );
    }
}

fn bench_checkpoint_builder(c: &mut criterion::Criterion) {
    let snap_id = ContentId::from_content(b"test-snapshot");
    let parent_id = ContentId::from_content(b"test-parent");

    c.bench_function("checkpoint_builder_minimal", |b| {
        b.iter(|| {
            CheckpointBuilder::new()
                .baseline_snapshot(snap_id)
                .author("bench")
                .message("msg")
                .parent(parent_id)
                .build()
                .unwrap()
        })
    });
}

// ============================================================================
// Group 2: DAG operations
// ============================================================================

fn build_dag_linear(size: usize) -> (CheckpointDag, Vec<CheckpointId>) {
    let mut dag = CheckpointDag::new();
    let mut ids = Vec::with_capacity(size);
    for i in 0..size {
        let id = cid(&(i as u64).to_le_bytes());
        dag.add_node(id);
        ids.push(id);
    }
    for i in 1..size {
        dag.add_edge(ids[i - 1], ids[i]);
    }
    (dag, ids)
}

fn build_dag_branching(
    branch_count: usize,
    depth: usize,
) -> (CheckpointDag, CheckpointId, Vec<CheckpointId>) {
    let mut dag = CheckpointDag::new();
    let root = cid(b"root");
    dag.add_node(root);
    let mut branch_tips = Vec::new();

    for b in 0..branch_count {
        let mut parent = root;
        for d in 0..depth {
            let node = cid(format!("b{}-d{}", b, d).as_bytes());
            dag.add_node(node);
            dag.add_edge(parent, node);
            parent = node;
        }
        branch_tips.push(parent);
    }
    (dag, root, branch_tips)
}

fn bench_dag_add_edge_linear(c: &mut criterion::Criterion) {
    for &size in &[10, 100, 500, 1000] {
        c.bench_function(&format!("dag_add_edge_linear_{}_nodes", size), |b| {
            b.iter(|| {
                let mut dag = CheckpointDag::new();
                let mut prev = cid(&(0_u64).to_le_bytes());
                dag.add_node(prev);
                for i in 1u64..size as u64 {
                    let cur = cid(&i.to_le_bytes());
                    dag.add_node(cur);
                    dag.add_edge(prev, cur);
                    prev = cur;
                }
            })
        });
    }
}

fn bench_dag_add_node(c: &mut criterion::Criterion) {
    for &size in &[10, 100, 500, 1000] {
        let ids: Vec<CheckpointId> = (0..size).map(|i| cid(&(i as u64).to_le_bytes())).collect();

        c.bench_function(&format!("dag_add_node_{}_nodes", size), |b| {
            b.iter(|| {
                let mut dag = CheckpointDag::new();
                for id in &ids {
                    dag.add_node(*id);
                }
            })
        });
    }
}

fn bench_dag_is_ancestor(c: &mut criterion::Criterion) {
    for &size in &[10, 100, 500, 1000] {
        let (dag, ids) = build_dag_linear(size);
        let root = ids[0];
        let _mid = ids[size / 2];
        let tip = ids[size - 1];

        c.bench_function(&format!("dag_is_ancestor_shallow_{}_nodes", size), |b| {
            b.iter(|| dag.is_ancestor(&root, &ids[1]))
        });
        c.bench_function(&format!("dag_is_ancestor_deep_{}_nodes", size), |b| {
            b.iter(|| dag.is_ancestor(&root, &tip))
        });
        c.bench_function(&format!("dag_is_ancestor_reverse_{}_nodes", size), |b| {
            b.iter(|| dag.is_ancestor(&tip, &root))
        });

        let _mid = ids[size / 2];
        let leaf = ids[size - 1];
        c.bench_function(
            &format!("dag_is_ancestor_mid_to_leaf_{}_nodes", size),
            |b| b.iter(|| dag.is_ancestor(&ids[size / 2], &leaf)),
        );
    }
}

fn bench_dag_ancestors(c: &mut criterion::Criterion) {
    for &size in &[10, 50, 200] {
        let (dag, ids) = build_dag_linear(size);
        let parents: HashMap<CheckpointId, Vec<CheckpointId>> = ids
            .windows(2)
            .map(|w| (w[1], vec![w[0]]))
            .collect();
        let get_parents = |id: &CheckpointId| -> Vec<CheckpointId> {
            parents.get(id).cloned().unwrap_or_default()
        };

        c.bench_function(&format!("dag_ancestors_{}_nodes", size), |b| {
            b.iter(|| dag.ancestors(ids.last().unwrap(), get_parents))
        });
    }

    // branching DAG
    for &branch_count in &[5, 20] {
        let depth = 10;
        let (dag, root, branch_tips) = build_dag_branching(branch_count, depth);
        let parents: HashMap<CheckpointId, Vec<CheckpointId>> = {
            let mut m = HashMap::new();
            for b in 0..branch_count {
                let mut prev = root;
                for d in 0..depth {
                    let cur = cid(format!("b{}-d{}", b, d).as_bytes());
                    m.entry(cur).or_insert_with(Vec::new).push(prev);
                    prev = cur;
                }
            }
            m
        };
        let get_parents = |id: &CheckpointId| -> Vec<CheckpointId> {
            parents.get(id).cloned().unwrap_or_default()
        };

        c.bench_function(
            &format!(
                "dag_ancestors_branching_{}_branches_{}_depth",
                branch_count, depth
            ),
            |b| {
                let tip = branch_tips[0];
                b.iter(|| dag.ancestors(&tip, get_parents))
            },
        );
    }
}

fn bench_dag_merge_base(c: &mut criterion::Criterion) {
    for &(branch_count, depth) in &[(2, 10), (5, 10), (10, 20), (20, 20)] {
        let (dag, root, branch_tips) = build_dag_branching(branch_count, depth);
        let parents: HashMap<CheckpointId, Vec<CheckpointId>> = {
            let mut m = HashMap::new();
            for b in 0..branch_count {
                let mut prev = root;
                for d in 0..depth {
                    let cur = cid(format!("b{}-d{}", b, d).as_bytes());
                    m.entry(cur).or_insert_with(Vec::new).push(prev);
                    prev = cur;
                }
            }
            m
        };
        let get_parents = |id: &CheckpointId| -> Vec<CheckpointId> {
            parents.get(id).cloned().unwrap_or_default()
        };

        let tip_a = branch_tips[0];
        let tip_b = branch_tips[branch_count - 1];

        c.bench_function(
            &format!("dag_merge_base_{}_branches_{}_depth", branch_count, depth),
            |b| b.iter(|| dag.merge_base(&tip_a, &tip_b, get_parents)),
        );
    }
}

// ============================================================================
// Group 3: TimeIndex operations
// ============================================================================

fn bench_time_index_insert(c: &mut criterion::Criterion) {
    for &size in &[10, 100, 500, 1000] {
        let checkpoints: Vec<Checkpoint> = (0..size)
            .map(|i| {
                let mut cp = make_checkpoint_with_snapshots(1);
                cp.created_at = (i as i64) * 1000;
                cp.id = cid(&(i as u64).to_le_bytes());
                cp
            })
            .collect();

        c.bench_function(&format!("time_index_insert_{}_entries", size), |b| {
            b.iter(|| {
                let mut index = TimeIndex::new();
                for cp in &checkpoints {
                    index.insert(cp);
                }
            })
        });
    }
}

fn bench_time_index_query_range(c: &mut criterion::Criterion) {
    for &size in &[10, 100, 500, 1000] {
        let mut index = TimeIndex::new();
        for i in 0..size {
            let mut cp = make_checkpoint_with_snapshots(1);
            cp.created_at = (i as i64) * 1000;
            cp.id = cid(&(i as u64).to_le_bytes());
            index.insert(&cp);
        }

        let mid = (size / 2) as i64 * 1000;
        c.bench_function(&format!("time_index_query_range_{}_entries", size), |b| {
            b.iter(|| index.query_range(mid, mid + size as i64 * 500))
        });
    }
}

fn bench_time_index_find_nearest(c: &mut criterion::Criterion) {
    for &size in &[10, 100, 500, 1000] {
        let mut index = TimeIndex::new();
        for i in 0..size {
            let mut cp = make_checkpoint_with_snapshots(1);
            cp.created_at = (i as i64) * 1000;
            cp.id = cid(&(i as u64).to_le_bytes());
            index.insert(&cp);
        }

        let target = (size / 2) as i64 * 1000 + 500;
        c.bench_function(&format!("time_index_find_nearest_{}_entries", size), |b| {
            b.iter(|| index.find_nearest(target))
        });
    }
}

fn bench_time_index_before_after(c: &mut criterion::Criterion) {
    for &size in &[10, 100, 500, 1000] {
        let mut index = TimeIndex::new();
        for i in 0..size {
            let mut cp = make_checkpoint_with_snapshots(1);
            cp.created_at = (i as i64) * 1000;
            cp.id = cid(&(i as u64).to_le_bytes());
            index.insert(&cp);
        }

        let mid = (size / 2) as i64 * 1000;
        c.bench_function(&format!("time_index_before_{}_entries", size), |b| {
            b.iter(|| index.before(mid))
        });
        c.bench_function(&format!("time_index_after_{}_entries", size), |b| {
            b.iter(|| index.after(mid))
        });
    }
}

// ============================================================================
// Group 4: CheckpointRepo commit (linear chain)
// ============================================================================

fn bench_repo_commit_linear(c: &mut criterion::Criterion) {
    for &commit_count in &[10, 50, 100, 200, 500] {
        c.bench_function(
            &format!("repo_commit_linear_{}_commits", commit_count),
            |b| {
                b.iter(|| {
                    let mut repo = CheckpointRepo::new_single(sid(b"initial"));
                    for i in 1u64..=commit_count as u64 {
                        let snap = ContentId::from_content(&i.to_le_bytes());
                        repo.commit_single(snap, &format!("commit {}", i), "bench")
                            .unwrap();
                    }
                })
            },
        );
    }
}

fn bench_repo_commit_multi_snapshot(c: &mut criterion::Criterion) {
    for &snapshot_count in &[1, 5, 10, 50] {
        let commit_count = if snapshot_count <= 5 { 100 } else { 20 };

        c.bench_function(
            &format!(
                "repo_commit_multi_{}_snapshots_{}_commits",
                snapshot_count, commit_count
            ),
            |b| {
                b.iter(|| {
                    let mut repo = CheckpointRepo::new_single(sid(b"initial"));
                    for round in 0u64..commit_count as u64 {
                        let snapshots: Vec<SnapshotId> = (0u64..snapshot_count as u64)
                            .map(|i| ContentId::from_content(&(round * 1000 + i).to_le_bytes()))
                            .collect();
                        repo.commit(snapshots, "multi", "bench").unwrap();
                    }
                })
            },
        );
    }
}

// ============================================================================
// Group 5: CheckpointRepo log (history traversal)
// ============================================================================

fn bench_repo_log(c: &mut criterion::Criterion) {
    for &commit_count in &[10, 50, 100, 200, 500] {
        let mut repo = CheckpointRepo::new_single(sid(b"initial"));
        for i in 1u64..=commit_count as u64 {
            let snap = ContentId::from_content(&i.to_le_bytes());
            repo.commit_single(snap, &format!("commit {}", i), "bench")
                .unwrap();
        }

        c.bench_function(
            &format!("repo_log_{}_commits_count_10", commit_count),
            |b| b.iter(|| repo.log(10)),
        );
        c.bench_function(
            &format!("repo_log_{}_commits_count_all", commit_count),
            |b| b.iter(|| repo.log(commit_count)),
        );
    }
}

// ============================================================================
// Group 6: Branch and merge operations
// ============================================================================

fn bench_repo_create_branch(c: &mut criterion::Criterion) {
    let mut repo = CheckpointRepo::new_single(sid(b"initial"));
    for i in 1u64..=10 {
        let snap = ContentId::from_content(&i.to_le_bytes());
        repo.commit_single(snap, &format!("c{}", i), "bench")
            .unwrap();
    }

    c.bench_function("repo_create_branch", |b| {
        b.iter(|| {
            repo.create_branch("bench-feature").ok();
            // remove duplicate check: delete the branch we just added
            // (we skip actual removal since the API doesn't support it - bench just for creation speed)
        })
    });
}

fn bench_repo_switch_branch(c: &mut criterion::Criterion) {
    let mut repo = CheckpointRepo::new_single(sid(b"initial"));
    for i in 1u64..=10 {
        repo.commit_single(
            ContentId::from_content(&i.to_le_bytes()),
            &format!("c{}", i),
            "bench",
        )
        .unwrap();
    }
    repo.create_branch("feature-a").unwrap();
    repo.create_branch("feature-b").unwrap();
    repo.create_branch("feature-c").unwrap();

    c.bench_function("repo_switch_branch", |b| {
        b.iter(|| {
            repo.switch_branch("feature-a").ok();
            repo.switch_branch("feature-b").ok();
            repo.switch_branch("feature-c").ok();
            repo.switch_branch("main").ok();
        })
    });
}

fn bench_repo_merge_branches(c: &mut criterion::Criterion) {
    for &(main_commits, feature_commits) in &[(5, 5), (10, 10), (50, 50), (100, 50)] {
        c.bench_function(
            &format!(
                "repo_merge_branches_main_{}_feature_{}",
                main_commits, feature_commits
            ),
            |b| {
                b.iter(|| {
                    let mut repo = CheckpointRepo::new_single(sid(b"initial"));
                    for i in 1..=main_commits {
                        repo.commit_single(
                            ContentId::from_content(&(i as u64).to_le_bytes()),
                            &format!("main-c{}", i),
                            "bench",
                        )
                        .unwrap();
                    }

                    repo.create_branch("feature").unwrap();

                    for i in 1..=feature_commits {
                        repo.commit_single(
                            ContentId::from_content(&((i + 1000) as u64).to_le_bytes()),
                            &format!("feat-c{}", i),
                            "bench",
                        )
                        .unwrap();
                    }

                    repo.switch_branch("main").unwrap();
                    let merge_snap = ContentId::from_content(&[255; 16]);
                    repo.merge_branches("feature", vec![merge_snap], "merge", "bench")
                        .unwrap();
                })
            },
        );
    }
}

// ============================================================================
// Group 7: Restore operations
// ============================================================================

fn bench_restore_full(c: &mut criterion::Criterion) {
    for &commit_count in &[10, 50, 100, 200] {
        let mut repo = CheckpointRepo::new_single(sid(b"initial"));

        // Cache the initial snapshot
        let initial = make_snapshot(0, "file://initial");
        repo.cache_snapshot(initial);

        for i in 1..=commit_count {
            let snap = make_snapshot(i as u8, &format!("file://file_{}", i));
            let snap_id = snap.id;
            repo.cache_snapshot(snap);
            repo.commit_single(snap_id, &format!("c{}", i), "bench")
                .unwrap();
        }

        let head = repo.current_branch_head();
        c.bench_function(&format!("restore_full_{}_commits", commit_count), |b| {
            b.iter(|| repo.restore_full(&head).unwrap())
        });
    }
}

fn bench_restore_selective(c: &mut criterion::Criterion) {
    for &snapshot_count in &[5, 20, 50, 100] {
        let mut snap_ids = Vec::new();
        let mut repo = CheckpointRepo::new_single(sid(b"initial"));
        let init_snap = make_snapshot(0, "agent://state");
        snap_ids.push(init_snap.id);
        repo.cache_snapshot(init_snap);

        // Add the remaining snapshots in one commit
        for i in 1..snapshot_count {
            let source = if i % 2 == 0 {
                format!("agent://state/{}", i)
            } else {
                format!("file://src/file_{}", i)
            };
            let snap = make_snapshot(i as u8, &source);
            snap_ids.push(snap.id);
            repo.cache_snapshot(snap);
        }

        let cp_id = repo.commit(snap_ids, "multi-snapshot", "bench").unwrap();
        for i in 0..snapshot_count {
            let source = if i % 2 == 0 {
                format!("agent://state/{}", i)
            } else {
                format!("file://src/file_{}", i)
            };
            let snap_id = ContentId::from_content(&[i as u8; 16]);
            repo.set_snapshot_source(&cp_id, snap_id, source).unwrap();
        }

        let head = repo.current_branch_head();

        c.bench_function(
            &format!(
                "restore_selective_{}_snapshots_agent_filter",
                snapshot_count
            ),
            |b| b.iter(|| repo.restore_selective(&head, vec!["agent://"]).unwrap()),
        );
        c.bench_function(
            &format!("restore_selective_{}_snapshots_file_filter", snapshot_count),
            |b| b.iter(|| repo.restore_selective(&head, vec!["file://"]).unwrap()),
        );
    }
}

fn bench_restore_by_time(c: &mut criterion::Criterion) {
    for &size in &[10, 50, 100, 200] {
        let mut repo = CheckpointRepo::new_single(sid(b"initial"));
        let init_snap = make_snapshot(0, "file://initial");
        repo.cache_snapshot(init_snap);

        for i in 1..=size {
            let snap = make_snapshot(i as u8, &format!("file://file_{}", i));
            let snap_id = snap.id;
            repo.cache_snapshot(snap);
            repo.commit_single(snap_id, &format!("c{}", i), "bench")
                .unwrap();
        }

        let target_time = (size / 2) as i64 * 1000;
        c.bench_function(&format!("restore_by_time_{}_entries", size), |b| {
            b.iter(|| repo.restore_by_time(target_time, None).unwrap())
        });
    }
}

fn bench_ancestry_chain(c: &mut criterion::Criterion) {
    for &commit_count in &[10, 50, 100, 200, 500] {
        let mut repo = CheckpointRepo::new_single(sid(b"initial"));
        for i in 1u64..=commit_count as u64 {
            let snap = ContentId::from_content(&i.to_le_bytes());
            repo.commit_single(snap, &format!("c{}", i), "bench")
                .unwrap();
        }

        let head = repo.current_branch_head();
        c.bench_function(&format!("ancestry_chain_{}_commits", commit_count), |b| {
            b.iter(|| repo.get_ancestry_chain(&head).unwrap())
        });
    }
}

// ============================================================================
// Group 8: CheckpointDiff computation
// ============================================================================

fn bench_diff_checkpoints(c: &mut criterion::Criterion) {
    for &snapshot_count in &[5, 20, 50, 100] {
        let mut r = CheckpointRepo::new_single(sid(b"initial"));
        let init_snap = make_snapshot(0, "file://initial");
        let init_snap_id = init_snap.id;
        r.cache_snapshot(init_snap);

        let cp_from = r.commit_single(init_snap_id, "first", "bench").unwrap();
        // Create a second commit with different snapshot set
        let mut snap_ids_b = vec![init_snap_id];
        let snap_b = make_snapshot(1, "file://new");
        let snap_b_id = snap_b.id;
        r.cache_snapshot(snap_b);
        snap_ids_b.push(snap_b_id);
        let cp_to = r.commit(snap_ids_b, "second", "bench").unwrap();

        c.bench_function(
            &format!("diff_checkpoints_{}_snapshots", snapshot_count),
            |b| b.iter(|| r.diff_checkpoints(&cp_from, &cp_to).unwrap()),
        );
    }
}

// ============================================================================
// Group 9: Validate integrity
// ============================================================================

fn bench_validate_integrity(c: &mut criterion::Criterion) {
    for &snapshot_count in &[5, 20, 50, 100] {
        let mut repo = CheckpointRepo::new_single(sid(b"initial"));
        let mut snap_ids = Vec::new();

        for i in 0..snapshot_count {
            let snap = make_snapshot(i as u8, &format!("file://file_{}", i));
            snap_ids.push(snap.id);
            repo.cache_snapshot(snap);
        }

        let cp_id = repo.commit(snap_ids, "multi-snapshot", "bench").unwrap();
        for i in 0..snapshot_count {
            repo.set_snapshot_source(
                &cp_id,
                ContentId::from_content(&[i as u8; 16]),
                format!("file://file_{}", i),
            )
            .unwrap();
        }

        let head = repo.current_branch_head();
        c.bench_function(
            &format!("validate_integrity_{}_snapshots", snapshot_count),
            |b| b.iter(|| repo.validate_integrity(&head).unwrap()),
        );
    }
}

// ============================================================================
// Group 10: CheckpointRepo init/load
// ============================================================================

fn bench_repo_init(c: &mut criterion::Criterion) {
    c.bench_function("repo_init_single_snapshot", |b| {
        b.iter(|| CheckpointRepo::new_single(sid(b"test")))
    });

    for &snapshot_count in &[1, 5, 10, 50] {
        let snap_ids: Vec<SnapshotId> = (0..snapshot_count)
            .map(|i| ContentId::from_content(&[i as u8; 16]))
            .collect();

        c.bench_function(&format!("repo_init_{}_snapshots", snapshot_count), |b| {
            b.iter(|| CheckpointRepo::new(snap_ids.clone()))
        });
    }
}

// ============================================================================
// Criterion group definitions
// ============================================================================

criterion::criterion_group!(
    checkpoint_creation,
    bench_checkpoint_creation,
    bench_checkpoint_builder
);

criterion::criterion_group!(
    dag_operations,
    bench_dag_add_node,
    bench_dag_add_edge_linear,
    bench_dag_is_ancestor,
    bench_dag_ancestors,
    bench_dag_merge_base
);

criterion::criterion_group!(
    time_index,
    bench_time_index_insert,
    bench_time_index_query_range,
    bench_time_index_find_nearest,
    bench_time_index_before_after
);

criterion::criterion_group!(
    repo_commit,
    bench_repo_commit_linear,
    bench_repo_commit_multi_snapshot
);

criterion::criterion_group!(repo_log_group, bench_repo_log);

criterion::criterion_group!(
    repo_branch,
    bench_repo_create_branch,
    bench_repo_switch_branch,
    bench_repo_merge_branches
);

criterion::criterion_group!(
    restore,
    bench_restore_full,
    bench_restore_selective,
    bench_restore_by_time,
    bench_ancestry_chain
);

criterion::criterion_group!(
    checkpoint_diff_group,
    bench_diff_checkpoints,
    bench_validate_integrity
);

criterion::criterion_group!(repo_init_group, bench_repo_init);

criterion::criterion_main!(
    checkpoint_creation,
    dag_operations,
    time_index,
    repo_commit,
    repo_log_group,
    repo_branch,
    restore,
    checkpoint_diff_group,
    repo_init_group
);
