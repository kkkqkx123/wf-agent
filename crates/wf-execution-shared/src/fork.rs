use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::Notify;
use wf_types::Id;

/// Runtime status of one fork branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
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
