use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use wf_execution_shared::{BranchStatus, ForkRegistry};

fn settled_registry() -> Arc<ForkRegistry> {
    let registry = Arc::new(ForkRegistry::new());
    registry.register("a", "exec-a".to_string());
    registry.register("b", "exec-b".to_string());
    registry
}

#[test]
fn register_and_get_running_record() {
    let registry = ForkRegistry::new();
    assert!(registry.get("missing").is_none());
    registry.register("a", "exec-a".to_string());
    let record = registry.get("a").expect("registered branch visible");
    assert_eq!(record.status, BranchStatus::Running);
    assert!(!record.is_settled());
    assert_eq!(record.execution_id.as_deref(), Some("exec-a"));
}

#[test]
fn register_updates_execution_id_of_existing_record() {
    let registry = ForkRegistry::new();
    registry.register("a", "exec-1".to_string());
    registry.register("a", "exec-2".to_string());
    assert_eq!(
        registry.get("a").expect("record").execution_id.as_deref(),
        Some("exec-2")
    );
}

#[test]
fn update_variables_replaces_snapshot() {
    let registry = settled_registry();
    let mut vars = HashMap::new();
    vars.insert("step".to_string(), json!(1));
    registry.update_variables("a", vars);
    assert_eq!(
        registry.get("a").expect("record").variables["step"],
        json!(1)
    );
    registry.update_variables("missing", HashMap::new());
}

#[test]
fn settle_success_marks_completed_and_first_wins() {
    let registry = settled_registry();
    let mut vars = HashMap::new();
    vars.insert("done".to_string(), json!(true));
    registry.settle("a", true, json!({"ok": true}), None, Some(vars));
    let record = registry.get("a").expect("record");
    assert_eq!(record.status, BranchStatus::Completed);
    assert!(record.is_settled());
    assert_eq!(record.output, Some(json!({"ok": true})));
    assert_eq!(record.variables["done"], json!(true));

    registry.settle("a", false, json!(null), Some("late".to_string()), None);
    let record = registry.get("a").expect("record");
    assert_eq!(record.status, BranchStatus::Completed);
    assert!(record.error.is_none());
}

#[test]
fn settle_failure_marks_failed_with_error() {
    let registry = settled_registry();
    registry.settle("b", false, json!(null), Some("boom".to_string()), None);
    let record = registry.get("b").expect("record");
    assert_eq!(record.status, BranchStatus::Failed);
    assert_eq!(record.error.as_deref(), Some("boom"));
}

#[test]
fn cancel_running_marks_cancelled_and_ignores_settled() {
    let registry = settled_registry();
    registry.cancel("a");
    assert_eq!(
        registry.get("a").expect("record").status,
        BranchStatus::Cancelled
    );

    registry.settle("b", true, json!(1), None, None);
    registry.cancel("b");
    assert_eq!(
        registry.get("b").expect("record").status,
        BranchStatus::Completed
    );
}

#[test]
fn settled_count_records_and_path_ids() {
    let registry = settled_registry();
    let paths = vec!["a".to_string(), "b".to_string()];
    assert_eq!(registry.settled_count(&paths), 0);
    registry.settle("a", true, json!(1), None, None);
    assert_eq!(registry.settled_count(&paths), 1);

    let records = registry.records(&paths);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].0, "a");

    let mut ids = registry.path_ids();
    ids.sort();
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}

#[tokio::test]
async fn wait_for_settled_branch_returns_true() {
    let registry = settled_registry();
    registry.settle("a", true, json!(1), None, None);
    assert!(registry.wait_for("a", None).await);
    assert!(registry.wait_for("a", Some(10)).await);
}

#[tokio::test]
async fn wait_for_running_branch_times_out() {
    let registry = settled_registry();
    assert!(!registry.wait_for("a", Some(20)).await);
}

#[tokio::test]
async fn wait_for_all_and_count_observe_settlement() {
    let registry = settled_registry();
    let paths = vec!["a".to_string(), "b".to_string()];
    assert!(!registry.wait_for_all(&paths, Some(20)).await);
    assert!(!registry.wait_for_count(&paths, 2, Some(20)).await);

    registry.settle("a", true, json!(1), None, None);
    assert!(registry.wait_for_count(&paths, 1, Some(100)).await);
    assert!(registry.wait_for_count(&[], 0, Some(10)).await);

    registry.settle("b", true, json!(2), None, None);
    assert!(registry.wait_for_all(&paths, Some(100)).await);
    assert!(registry.wait_for_count(&paths, 2, Some(100)).await);
}

#[tokio::test]
async fn wait_for_wakes_on_settlement() {
    let registry = settled_registry();
    let handle = tokio::spawn({
        let registry = Arc::clone(&registry);
        async move { registry.wait_for("a", None).await }
    });
    tokio::task::yield_now().await;
    registry.settle("a", true, json!(1), None, None);
    assert!(handle.await.expect("wait task joins"));
}

#[test]
fn abort_all_cancels_only_running_branches() {
    let registry = settled_registry();
    registry.settle("a", true, json!(1), None, None);
    registry.abort_all();
    assert_eq!(
        registry.get("a").expect("record").status,
        BranchStatus::Completed
    );
    assert_eq!(
        registry.get("b").expect("record").status,
        BranchStatus::Cancelled
    );
}
