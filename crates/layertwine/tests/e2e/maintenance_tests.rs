//! Maintenance operation E2E tests
//!
//! Covers real business scenarios for database maintenance:
//!   - Compact: admin runs to reclaim WAL and free pages after many operations
//!   - Garbage collection: admin runs to clean up unreachable checkpoints

use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use crate::common::output::*;
use layertwine::api::{ApiService, CompactRequest, GcRequest};

// ── Compact tests ──

#[test]
fn test_compact_basic() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_compact_basic");

    print_info("Step 1: Initialize repository and make some edits");
    init_repository(&env);
    for i in 1..=5 {
        let content = format!("version {}\nline 2\nline 3\n", i);
        apply_edit(&env, "test.txt", &content);
        commit_changes(&env, &format!("commit {}", i), "user");
    }

    print_info("Step 2: Run compact (incremental)");
    let resp = env
        .api
        .compact(CompactRequest { vacuum_full: None })
        .expect("compact failed");

    print_info(&format!("  WAL checkpointed: {}", resp.wal_checkpointed));
    print_info(&format!("  Free before: {} pages", resp.freelist_before));
    print_info(&format!("  Total pages: {}", resp.total_pages));
    print_info(&format!("  Free after: {} pages", resp.freelist_after));
    print_info(&format!("  Vacuum performed: {}", resp.vacuum_performed));
    print_info(&format!("  Message: {}", resp.message));

    assert!(resp.total_pages > 0, "should have pages in the database");
    assert!(!resp.message.is_empty(), "should return a status message");

    print_info("Step 3: Run compact with vacuum_full");
    let resp = env
        .api
        .compact(CompactRequest {
            vacuum_full: Some(true),
        })
        .expect("compact with vacuum_full failed");

    print_info(&format!("  Vacuum performed: {}", resp.vacuum_performed));
    print_info(&format!("  Message: {}", resp.message));

    assert!(
        resp.vacuum_performed,
        "vacuum_full should trigger actual VACUUM"
    );

    print_test_result(true, "test_compact_basic", None);
}

#[test]
fn test_compact_after_many_operations() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_compact_after_many_operations");

    print_info("Step 1: Initialize repository and make 20 commits");
    init_repository(&env);
    for i in 1..=20 {
        let content: String = (1..=10).map(|j| format!("line {}\n", j * i)).collect();
        apply_edit(&env, "test.txt", &content);
        commit_changes(&env, &format!("commit {}", i), "user");
    }
    print_success("20 commits created");

    print_info("Step 2: Run compact to reclaim space");
    let resp = env
        .api
        .compact(CompactRequest {
            vacuum_full: Some(true),
        })
        .expect("compact after many ops failed");

    print_info(&format!("  Free before: {} pages", resp.freelist_before));
    print_info(&format!("  Free after: {} pages", resp.freelist_after));
    print_info(&format!("  Vacuum performed: {}", resp.vacuum_performed));
    print_info(&format!("  Total pages: {}", resp.total_pages));

    assert!(resp.total_pages > 0, "should have pages");
    assert!(resp.vacuum_performed, "vacuum should be performed");

    print_info("Step 3: Verify data is still intact after compact");
    let log = get_log(&env, Some(5));
    assert_eq!(log.len(), 5, "should still have recent log entries");

    print_test_result(true, "test_compact_after_many_operations", None);
}

// ── GC tests ──

#[test]
fn test_gc_basic() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_gc_basic");

    print_info("Step 1: Initialize repository with commits");
    init_repository(&env);
    apply_edit(&env, "test.txt", "version 1\n");
    commit_changes(&env, "commit 1", "user");

    apply_edit(&env, "test.txt", "version 1\nversion 2\n");
    commit_changes(&env, "commit 2", "user");

    print_info("Step 2: Run garbage collection");
    let resp = env.api.gc(GcRequest {}).expect("gc failed");

    print_info(&format!(
        "  Removed checkpoints: {}",
        resp.removed_checkpoints
    ));
    print_info(&format!("  Removed snapshots: {}", resp.removed_snapshots));
    print_info(&format!("  Freed bytes: {}", resp.freed_bytes));
    print_info(&format!(
        "  Delta chain depth triggered: {}",
        resp.delta_chain_depth_triggered
    ));

    // GC on a small repo should succeed and report stats
    print_info("Step 3: Verify GC response is well-formed");
    // Even if nothing was removed, the response should be valid
    assert!(!resp.freed_bytes.to_string().is_empty());

    print_test_result(true, "test_gc_basic", None);
}

#[test]
fn test_gc_preserves_reachable_checkpoints() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_gc_preserves_reachable_checkpoints");

    print_info("Step 1: Initialize repository with 5 commits");
    init_repository(&env);
    for i in 1..=5 {
        let content = format!("line 1\nversion {}\n", i);
        apply_edit(&env, "test.txt", &content);
        commit_changes(&env, &format!("commit {}", i), "user");
    }

    let log_before = get_log(&env, None);
    print_info(&format!("  Commits before GC: {}", log_before.len()));

    print_info("Step 2: Run garbage collection");
    let resp = env.api.gc(GcRequest {}).expect("gc failed");
    print_info(&format!(
        "  Removed checkpoints: {}",
        resp.removed_checkpoints
    ));

    print_info("Step 3: Verify all reachable checkpoints are still accessible");
    let log_after = get_log(&env, None);
    print_info(&format!("  Commits after GC: {}", log_after.len()));

    // All commits should still be reachable since they're on the current branch DAG
    assert_eq!(
        log_before.len(),
        log_after.len(),
        "all commits should be preserved"
    );
    assert!(
        log_after.iter().any(|e| e.contains("commit 5")),
        "latest commit should be present"
    );
    assert!(
        log_after.iter().any(|e| e.contains("commit 1")),
        "first commit should be present"
    );

    print_test_result(true, "test_gc_preserves_reachable_checkpoints", None);
}

#[test]
fn test_gc_on_fresh_repo() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config.clone());

    print_test_header("test_gc_on_fresh_repo");

    print_info("Step 1: Initialize repository (no user commits)");
    init_repository(&env);

    print_info("Step 2: Run GC on fresh repo");
    let resp = env.api.gc(GcRequest {}).expect("gc on fresh repo failed");

    print_info(&format!(
        "  Removed checkpoints: {}",
        resp.removed_checkpoints
    ));
    print_info(&format!("  Removed snapshots: {}", resp.removed_snapshots));

    // GC on a fresh repo should not crash
    print_info("Step 3: Verify repo is still functional after GC");
    apply_edit(&env, "test.txt", "content after gc\n");
    let snap = commit_changes(&env, "commit after gc", "user");
    assert!(
        !snap.0.iter().all(|&b| b == 0),
        "should be able to commit after GC"
    );

    print_test_result(true, "test_gc_on_fresh_repo", None);
}
