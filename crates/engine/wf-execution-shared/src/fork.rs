use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::Notify;
use wf_types::execution::ChildExecutionReference;
use wf_types::Id;

/// Runtime status of one fork branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Snapshot-facing status of one fork branch. Uses the same vocabulary as
/// the restore-side `forkJoinAggregationState.pathStatuses` record
/// (`PENDING` / `COMPLETED` / `FAILED`) so snapshots and inference agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkChildStatus {
    Pending,
    Completed,
    Failed,
}

impl From<BranchStatus> for ForkChildStatus {
    fn from(status: BranchStatus) -> Self {
        match status {
            BranchStatus::Running => ForkChildStatus::Pending,
            BranchStatus::Completed => ForkChildStatus::Completed,
            BranchStatus::Failed | BranchStatus::Cancelled => ForkChildStatus::Failed,
        }
    }
}

/// Build the `forkJoinAggregationState` snapshot record from the parent's
/// fork child references. `status_of` resolves each child's live status
/// (fork registry record, live entity status, or `Pending` when unknown).
/// Paths are sorted by branch path id for deterministic snapshots.
/// Returns `None` when the execution launched no fork branches.
pub fn fork_aggregation_state(
    children: &[ChildExecutionReference],
    status_of: impl Fn(&Id) -> ForkChildStatus,
) -> Option<Value> {
    let mut fork_children: Vec<&ChildExecutionReference> =
        children.iter().filter(|c| c.fork_path.is_some()).collect();
    if fork_children.is_empty() {
        return None;
    }
    fork_children.sort_by(|a, b| {
        a.fork_path
            .as_ref()
            .map(|p| p.branch_path_id.as_str())
            .cmp(&b.fork_path.as_ref().map(|p| p.branch_path_id.as_str()))
    });
    let mut path_statuses = serde_json::Map::new();
    let mut branch_children = serde_json::Map::new();
    let mut complete = true;
    for child in &fork_children {
        let Some(fork) = child.fork_path.as_ref() else {
            continue;
        };
        let status = status_of(&child.child_id);
        if status == ForkChildStatus::Pending {
            complete = false;
        }
        path_statuses.insert(
            fork.branch_path_id.clone(),
            Value::String(
                match status {
                    ForkChildStatus::Pending => "PENDING",
                    ForkChildStatus::Completed => "COMPLETED",
                    ForkChildStatus::Failed => "FAILED",
                }
                .to_string(),
            ),
        );
        branch_children.insert(
            fork.branch_path_id.clone(),
            Value::String(child.child_id.to_string()),
        );
    }
    let fork_node_id = fork_children
        .first()
        .and_then(|c| c.fork_path.as_ref())
        .map(|f| f.fork_node_id.clone())
        .unwrap_or_default();
    Some(serde_json::json!({
        "forkNodeId": fork_node_id,
        "pathStatuses": Value::Object(path_statuses),
        "branchChildren": Value::Object(branch_children),
        "isAggregationComplete": complete,
    }))
}

/// Live record of one fork branch, shared between the fork handler, the
/// branch execution and the SYNC/JOIN nodes that consume it.
#[derive(Debug, Clone)]
pub struct BranchRecord {
    /// Execution id of the branch sub-execution.
    pub execution_id: Option<Id>,
    pub status: BranchStatus,
    /// Final output of the branch; `None` while running or on failure.
    pub output: Option<Value>,
    /// Failure message of a failed/cancelled branch.
    pub error: Option<String>,
    /// Public variables (non-`__`-prefixed) of the branch. Updated after
    /// every completed node while the branch runs and frozen at settlement,
    /// so SYNC nodes can read the source branch's intermediate state.
    pub variables: HashMap<String, Value>,
}

impl BranchRecord {
    pub fn is_settled(&self) -> bool {
        self.status != BranchStatus::Running
    }
}

/// Registry of all branches of one fork, keyed by `path_id`. Live variables
/// are written by the branch executor (after every node) so SYNC nodes can
/// read the source branch's intermediate state; settlement is recorded by
/// the branch task so JOIN can aggregate the final results. Waiters (SYNC
/// with `wait_for_completion`, JOIN in non-blocking forks) block on
/// per-branch [`Notify`] channels until the branch settles or a timeout
/// elapses.
///
/// All record operations are synchronous short critical sections (no
/// `await` while holding the lock); only the wait primitives are async.
pub struct ForkRegistry {
    inner: Mutex<HashMap<String, BranchRecord>>,
    notifies: Mutex<HashMap<String, Arc<Notify>>>,
    handles: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    /// Registry-wide "something changed" signal for waiters that wait on a
    /// subset/count of branches (JOIN `wait_for_any`/`wait_for_n`).
    changed: Notify,
}

impl Default for ForkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ForkRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            notifies: Mutex::new(HashMap::new()),
            handles: Mutex::new(HashMap::new()),
            changed: Notify::new(),
        }
    }

    /// Register a branch before its task starts. Re-registration updates the
    /// execution id of an existing record.
    pub fn register(&self, path_id: impl Into<String>, execution_id: Id) {
        let path_id = path_id.into();
        let mut guard = self.inner.lock().expect("fork registry poisoned");
        let record = guard.entry(path_id).or_insert_with(|| BranchRecord {
            execution_id: Some(execution_id.clone()),
            status: BranchStatus::Running,
            output: None,
            error: None,
            variables: HashMap::new(),
        });
        record.execution_id = Some(execution_id);
    }

    /// Replace the branch's public-variable snapshot (written after every
    /// completed node while the branch runs).
    pub fn update_variables(&self, path_id: &str, variables: HashMap<String, Value>) {
        let mut guard = self.inner.lock().expect("fork registry poisoned");
        if let Some(record) = guard.get_mut(path_id) {
            record.variables = variables;
        }
    }

    /// Record the branch settlement and wake all waiters. Idempotent: later
    /// calls on a settled branch are ignored (the first settlement wins).
    pub fn settle(
        &self,
        path_id: &str,
        success: bool,
        output: Value,
        error: Option<String>,
        variables: Option<HashMap<String, Value>>,
    ) {
        let mut guard = self.inner.lock().expect("fork registry poisoned");
        let record = guard
            .entry(path_id.to_string())
            .or_insert_with(|| BranchRecord {
                execution_id: None,
                status: BranchStatus::Running,
                output: None,
                error: None,
                variables: HashMap::new(),
            });
        if record.status != BranchStatus::Running {
            return;
        }
        record.status = if success {
            BranchStatus::Completed
        } else {
            BranchStatus::Failed
        };
        record.output = Some(output);
        record.error = error;
        if let Some(vars) = variables {
            record.variables = vars;
        }
        drop(guard);
        self.notify_for(path_id).notify_waiters();
        self.changed.notify_waiters();
    }

    /// Mark a still-running branch as cancelled (e.g. the fork was aborted)
    /// and wake its waiters.
    pub fn cancel(&self, path_id: &str) {
        let mut guard = self.inner.lock().expect("fork registry poisoned");
        let record = guard
            .entry(path_id.to_string())
            .or_insert_with(|| BranchRecord {
                execution_id: None,
                status: BranchStatus::Running,
                output: None,
                error: None,
                variables: HashMap::new(),
            });
        if record.status != BranchStatus::Running {
            return;
        }
        record.status = BranchStatus::Cancelled;
        drop(guard);
        self.notify_for(path_id).notify_waiters();
        self.changed.notify_waiters();
    }

    /// Snapshot of one branch record; `None` when the branch was never
    /// registered.
    pub fn get(&self, path_id: &str) -> Option<BranchRecord> {
        self.inner
            .lock()
            .expect("fork registry poisoned")
            .get(path_id)
            .cloned()
    }

    /// Snapshot of the records for the given paths, in path order.
    pub fn records(&self, path_ids: &[String]) -> Vec<(String, BranchRecord)> {
        let guard = self.inner.lock().expect("fork registry poisoned");
        path_ids
            .iter()
            .filter_map(|id| guard.get(id).cloned().map(|r| (id.clone(), r)))
            .collect()
    }

    /// All registered path ids (in registration order).
    pub fn path_ids(&self) -> Vec<String> {
        self.inner
            .lock()
            .expect("fork registry poisoned")
            .keys()
            .cloned()
            .collect()
    }

    /// Store the spawned branch task handle so the fork can abort in-flight
    /// branches on cancellation.
    pub fn register_handle(&self, path_id: &str, handle: tokio::task::JoinHandle<()>) {
        self.handles
            .lock()
            .expect("fork registry poisoned")
            .insert(path_id.to_string(), handle);
    }

    /// Abort all in-flight branch tasks and mark running branches
    /// cancelled, waking their waiters.
    pub fn abort_all(&self) {
        {
            let mut handles = self.handles.lock().expect("fork registry poisoned");
            for handle in handles.values() {
                handle.abort();
            }
            handles.clear();
        }
        let paths: Vec<String> = {
            let mut guard = self.inner.lock().expect("fork registry poisoned");
            for record in guard.values_mut() {
                if record.status == BranchStatus::Running {
                    record.status = BranchStatus::Cancelled;
                }
            }
            guard.keys().cloned().collect()
        };
        for path in paths {
            self.notify_for(&path).notify_waiters();
        }
        self.changed.notify_waiters();
    }

    /// How many of the given branches have settled.
    pub fn settled_count(&self, path_ids: &[String]) -> usize {
        let guard = self.inner.lock().expect("fork registry poisoned");
        path_ids
            .iter()
            .filter(|id| guard.get(*id).is_some_and(|r| r.is_settled()))
            .count()
    }

    /// Wait until the branch settles (any non-`Running` status). `timeout_ms`
    /// of `0`/`None` waits indefinitely; a positive value returns `false`
    /// when the branch is still running after the timeout.
    pub async fn wait_for(&self, path_id: &str, timeout_ms: Option<u64>) -> bool {
        let notify = self.notify_for(path_id);
        let wait = async {
            loop {
                let notified = notify.notified();
                tokio::pin!(notified);
                {
                    // Register the waiter *before* re-checking the record:
                    // poll the notification future once (noop waker). A
                    // settlement before this poll is caught by the check
                    // below; a settlement after this poll wakes the
                    // registered waiter. Without this step a settle racing
                    // between the check and the first poll would be lost
                    // (`notify_waiters` stores no permit).
                    let waker = std::task::Waker::noop();
                    let mut cx = std::task::Context::from_waker(waker);
                    if std::future::Future::poll(notified.as_mut(), &mut cx).is_ready() {
                        // A notification was already latched; fall through
                        // and re-check the record.
                        return self
                            .inner
                            .lock()
                            .expect("fork registry poisoned")
                            .get(path_id)
                            .is_some_and(|r| r.is_settled());
                    }
                }
                {
                    let guard = self.inner.lock().expect("fork registry poisoned");
                    if let Some(record) = guard.get(path_id) {
                        if record.is_settled() {
                            return true;
                        }
                    }
                }
                notified.await;
            }
        };
        match timeout_ms {
            Some(ms) if ms > 0 => tokio::time::timeout(std::time::Duration::from_millis(ms), wait)
                .await
                .is_ok(),
            _ => {
                wait.await;
                true
            }
        }
    }

    /// Wait until every given branch settles. The overall wait is bounded by
    /// `timeout_ms` (0/None = indefinite).
    pub async fn wait_for_all(&self, path_ids: &[String], timeout_ms: Option<u64>) -> bool {
        let wait = async {
            for path_id in path_ids {
                if !self.wait_for(path_id, None).await {
                    return false;
                }
            }
            true
        };
        match timeout_ms {
            Some(ms) if ms > 0 => tokio::time::timeout(std::time::Duration::from_millis(ms), wait)
                .await
                .is_ok(),
            _ => wait.await,
        }
    }

    /// Wait until at least `count` of the given branches settle, listening on
    /// the registry-wide change signal (JOIN `wait_for_any`/`wait_for_n` on
    /// non-blocking forks). Bounded by `timeout_ms` (0/None = indefinite).
    pub async fn wait_for_count(
        &self,
        path_ids: &[String],
        count: usize,
        timeout_ms: Option<u64>,
    ) -> bool {
        if count == 0 || path_ids.is_empty() {
            return true;
        }
        let wait = async {
            loop {
                if self.settled_count(path_ids) >= count {
                    return true;
                }
                let notified = self.changed.notified();
                tokio::pin!(notified);
                {
                    // Register before re-checking (lost-wakeup-safe).
                    let waker = std::task::Waker::noop();
                    let mut cx = std::task::Context::from_waker(waker);
                    let _ = std::future::Future::poll(notified.as_mut(), &mut cx);
                }
                if self.settled_count(path_ids) >= count {
                    return true;
                }
                notified.await;
            }
        };
        match timeout_ms {
            Some(ms) if ms > 0 => tokio::time::timeout(std::time::Duration::from_millis(ms), wait)
                .await
                .is_ok(),
            _ => {
                wait.await;
                true
            }
        }
    }

    fn notify_for(&self, path_id: &str) -> Arc<Notify> {
        let mut map = self.notifies.lock().expect("fork registry poisoned");
        map.entry(path_id.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }
}

#[cfg(test)]
mod aggregation_tests {
    use super::*;
    use wf_types::execution::{ChildExecutionReference, ExecutionType, ForkPath};

    fn fork_child(id: &str, path: &str) -> ChildExecutionReference {
        ChildExecutionReference {
            child_type: ExecutionType::Workflow,
            child_id: id.to_string(),
            created_at: 0,
            fork_path: Some(ForkPath::new("fork-1", path.to_string())),
        }
    }

    #[test]
    fn no_fork_children_yields_no_record() {
        let children = vec![ChildExecutionReference {
            child_type: ExecutionType::Workflow,
            child_id: "plain".to_string(),
            created_at: 0,
            fork_path: None,
        }];
        assert!(fork_aggregation_state(&children, |_| ForkChildStatus::Pending).is_none());
    }

    #[test]
    fn live_statuses_drive_path_statuses_and_completion() {
        let children = vec![fork_child("b-slow", "slow"), fork_child("a-fast", "fast")];
        let record = fork_aggregation_state(&children, |id| {
            if id.as_str() == "a-fast" {
                ForkChildStatus::Completed
            } else {
                ForkChildStatus::Pending
            }
        })
        .expect("record built");
        assert_eq!(record["forkNodeId"], serde_json::json!("fork-1"));
        assert_eq!(
            record["pathStatuses"]["fast"],
            serde_json::json!("COMPLETED")
        );
        assert_eq!(record["pathStatuses"]["slow"], serde_json::json!("PENDING"));
        assert_eq!(
            record["branchChildren"]["fast"],
            serde_json::json!("a-fast")
        );
        assert_eq!(record["isAggregationComplete"], serde_json::json!(false));
    }

    #[test]
    fn settled_branches_mark_aggregation_complete() {
        let children = vec![fork_child("b-2", "p2"), fork_child("b-1", "p1")];
        let record = fork_aggregation_state(&children, |id| {
            if id.as_str() == "b-1" {
                ForkChildStatus::Completed
            } else {
                ForkChildStatus::Failed
            }
        })
        .expect("record built");
        assert_eq!(record["isAggregationComplete"], serde_json::json!(true));
        assert_eq!(record["pathStatuses"]["p2"], serde_json::json!("FAILED"));
    }

    #[test]
    fn registry_status_maps_onto_snapshot_vocabulary() {
        assert_eq!(
            ForkChildStatus::from(BranchStatus::Running),
            ForkChildStatus::Pending
        );
        assert_eq!(
            ForkChildStatus::from(BranchStatus::Completed),
            ForkChildStatus::Completed
        );
        assert_eq!(
            ForkChildStatus::from(BranchStatus::Failed),
            ForkChildStatus::Failed
        );
        assert_eq!(
            ForkChildStatus::from(BranchStatus::Cancelled),
            ForkChildStatus::Failed
        );
    }
}
