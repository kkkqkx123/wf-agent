//! Background task registry for shell sessions.
//!
//! A [`BackgroundTaskRegistry`] tracks shell commands that outlive a single
//! tool call: what is running, which execution owns it, whether the caller
//! moved it to the background, and how it settled. The session store owns
//! processes and bytes; this registry owns task state and the background
//! handoff. Entries are keyed by session id so task state joins directly
//! with [`crate::store::BackgroundShellStore`] snapshots and output reads.

use dashmap::DashMap;
use std::sync::OnceLock;

/// Lifecycle state of one background task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl BackgroundTaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackgroundTaskStatus::Running => "running",
            BackgroundTaskStatus::Completed => "completed",
            BackgroundTaskStatus::Failed => "failed",
            BackgroundTaskStatus::Cancelled => "cancelled",
        }
    }

    pub fn is_settled(&self) -> bool {
        *self != BackgroundTaskStatus::Running
    }
}

/// One tracked shell command.
#[derive(Debug, Clone)]
pub struct BackgroundTask {
    pub session_id: String,
    pub command: String,
    pub execution_id: String,
    pub started_at: i64,
    pub backgrounded: bool,
    pub status: BackgroundTaskStatus,
    pub exit_code: Option<i32>,
    pub completed_at: Option<i64>,
}

impl BackgroundTask {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "session_id": self.session_id,
            "command": self.command,
            "execution_id": self.execution_id,
            "started_at": self.started_at,
            "backgrounded": self.backgrounded,
            "status": self.status.as_str(),
            "exit_code": self.exit_code,
            "completed_at": self.completed_at,
        })
    }
}

/// Bounded count of settled entries kept for late handoff queries.
const SETTLED_LIMIT: usize = 25;

/// Registry of background shell tasks keyed by session id.
pub struct BackgroundTaskRegistry {
    tasks: DashMap<String, BackgroundTask>,
}

impl BackgroundTaskRegistry {
    pub fn new() -> Self {
        Self {
            tasks: DashMap::new(),
        }
    }

    /// Track a newly spawned command. A still-running entry for the same
    /// session is left untouched; a settled one is replaced.
    pub fn register(&self, session_id: &str, command: &str, execution_id: &str) {
        let fresh = BackgroundTask {
            session_id: session_id.to_string(),
            command: command.to_string(),
            execution_id: execution_id.to_string(),
            started_at: wf_common::time::now(),
            backgrounded: false,
            status: BackgroundTaskStatus::Running,
            exit_code: None,
            completed_at: None,
        };
        match self.tasks.entry(session_id.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(mut occupied) => {
                if occupied.get().status.is_settled() {
                    occupied.insert(fresh);
                }
            }
            dashmap::mapref::entry::Entry::Vacant(vacant) => {
                vacant.insert(fresh);
            }
        }
    }

    /// Mark a running task as backgrounded: the caller was told not to wait
    /// and completion is reported through events. Returns false when the
    /// task is unknown or already settled.
    pub fn background(&self, session_id: &str) -> bool {
        match self.tasks.get_mut(session_id) {
            Some(mut task) if task.status == BackgroundTaskStatus::Running => {
                task.backgrounded = true;
                true
            }
            _ => false,
        }
    }

    /// Record settlement of a running task. Later calls on a settled task
    /// are ignored so the first settlement wins.
    pub fn settle(&self, session_id: &str, exit_code: Option<i32>, success: bool) {
        let Some(mut task) = self.tasks.get_mut(session_id) else {
            return;
        };
        if task.status.is_settled() {
            return;
        }
        task.status = if success {
            BackgroundTaskStatus::Completed
        } else {
            BackgroundTaskStatus::Failed
        };
        task.exit_code = exit_code;
        task.completed_at = Some(wf_common::time::now());
        drop(task);
        self.evict_settled();
    }

    /// Mark a running task cancelled (kill path). Settled tasks keep their
    /// settlement.
    pub fn cancel(&self, session_id: &str) {
        let Some(mut task) = self.tasks.get_mut(session_id) else {
            return;
        };
        if task.status.is_settled() {
            return;
        }
        task.status = BackgroundTaskStatus::Cancelled;
        task.completed_at = Some(wf_common::time::now());
        drop(task);
        self.evict_settled();
    }

    pub fn get(&self, session_id: &str) -> Option<BackgroundTask> {
        self.tasks.get(session_id).map(|entry| entry.clone())
    }

    pub fn is_backgrounded(&self, session_id: &str) -> bool {
        self.tasks
            .get(session_id)
            .is_some_and(|entry| entry.backgrounded)
    }

    /// Running tasks across all executions ordered by start time.
    /// Settled entries are appended after running ones when requested so a
    /// global backlog view needs a single call.
    pub fn list_all(&self, include_settled: bool) -> Vec<BackgroundTask> {
        let mut tasks: Vec<BackgroundTask> = self
            .tasks
            .iter()
            .filter(|entry| include_settled || !entry.status.is_settled())
            .map(|entry| entry.clone())
            .collect();
        tasks.sort_by_key(|task| task.started_at);
        tasks
    }

    /// Recently settled tasks ordered by completion time.
    pub fn list_settled(&self) -> Vec<BackgroundTask> {
        let mut tasks: Vec<BackgroundTask> = self
            .tasks
            .iter()
            .filter(|entry| entry.status.is_settled())
            .map(|entry| entry.clone())
            .collect();
        tasks.sort_by_key(|task| task.completed_at.unwrap_or(task.started_at));
        tasks
    }

    /// Mark every running task bound to `execution_id` as backgrounded.
    /// Returns the tasks moved to the background in start-time order.
    pub fn background_all_for_execution(&self, execution_id: &str) -> Vec<BackgroundTask> {
        let mut moved = Vec::new();
        for mut entry in self.tasks.iter_mut() {
            if entry.execution_id == execution_id
                && entry.status == BackgroundTaskStatus::Running
                && !entry.backgrounded
            {
                entry.backgrounded = true;
                moved.push(entry.clone());
            }
        }
        moved.sort_by_key(|task: &BackgroundTask| task.started_at);
        moved
    }

    /// Confirm delivery of a settled task notification and drop the entry.
    /// Returns true when a settled entry was removed.
    pub fn acknowledge(&self, session_id: &str) -> bool {
        match self.tasks.get(session_id) {
            Some(entry) if entry.status.is_settled() => {}
            _ => return false,
        }
        self.tasks.remove(session_id).is_some()
    }

    /// Running tasks ordered by start time.
    pub fn list_running(&self) -> Vec<BackgroundTask> {
        let mut tasks: Vec<BackgroundTask> = self
            .tasks
            .iter()
            .filter(|entry| entry.status == BackgroundTaskStatus::Running)
            .map(|entry| entry.clone())
            .collect();
        tasks.sort_by_key(|task| task.started_at);
        tasks
    }

    /// Tasks bound to one execution. Settled tasks are included only when
    /// `include_settled` is set.
    pub fn list_for_execution(
        &self,
        execution_id: &str,
        include_settled: bool,
    ) -> Vec<BackgroundTask> {
        let mut tasks: Vec<BackgroundTask> = self
            .tasks
            .iter()
            .filter(|entry| {
                entry.execution_id == execution_id
                    && (include_settled || !entry.status.is_settled())
            })
            .map(|entry| entry.clone())
            .collect();
        tasks.sort_by_key(|task| task.started_at);
        tasks
    }

    pub fn running_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|entry| entry.status == BackgroundTaskStatus::Running)
            .count()
    }

    fn evict_settled(&self) {
        let mut settled: Vec<(String, i64)> = self
            .tasks
            .iter()
            .filter(|entry| entry.status.is_settled())
            .map(|entry| {
                (
                    entry.session_id.clone(),
                    entry.completed_at.unwrap_or(entry.started_at),
                )
            })
            .collect();
        if settled.len() <= SETTLED_LIMIT {
            return;
        }
        settled.sort_by_key(|(_, at)| *at);
        for (session_id, _) in settled.iter().take(settled.len() - SETTLED_LIMIT) {
            self.tasks.remove(session_id);
        }
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn reset_for_tests(&self) {
        self.tasks.clear();
    }
}

impl Default for BackgroundTaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

static REGISTRY: OnceLock<BackgroundTaskRegistry> = OnceLock::new();

/// Process-wide background task registry shared by every shell store.
pub fn background_task_registry() -> &'static BackgroundTaskRegistry {
    REGISTRY.get_or_init(BackgroundTaskRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_background_settle_flow() {
        let registry = BackgroundTaskRegistry::new();
        registry.register("s-1", "sleep 30", "exec-1");
        assert_eq!(registry.running_count(), 1);
        assert!(registry.background("s-1"));
        let task = registry.get("s-1").expect("task tracked");
        assert!(task.backgrounded);
        assert_eq!(task.status, BackgroundTaskStatus::Running);
        registry.settle("s-1", Some(0), true);
        let task = registry.get("s-1").expect("task kept");
        assert_eq!(task.status, BackgroundTaskStatus::Completed);
        assert_eq!(task.exit_code, Some(0));
        assert!(registry.list_running().is_empty());
    }

    #[test]
    fn first_settlement_wins() {
        let registry = BackgroundTaskRegistry::new();
        registry.register("s-1", "cmd", "exec-1");
        registry.settle("s-1", Some(1), false);
        registry.settle("s-1", Some(0), true);
        assert_eq!(
            registry.get("s-1").expect("task kept").status,
            BackgroundTaskStatus::Failed
        );
    }

    #[test]
    fn background_rejects_settled_tasks() {
        let registry = BackgroundTaskRegistry::new();
        assert!(!registry.background("missing"));
        registry.register("s-1", "cmd", "exec-1");
        registry.cancel("s-1");
        assert!(!registry.background("s-1"));
        assert_eq!(
            registry.get("s-1").expect("task kept").status,
            BackgroundTaskStatus::Cancelled
        );
    }

    #[test]
    fn execution_listing_filters_settled() {
        let registry = BackgroundTaskRegistry::new();
        registry.register("s-1", "a", "exec-1");
        registry.register("s-2", "b", "exec-1");
        registry.register("s-3", "c", "exec-2");
        registry.settle("s-2", Some(0), true);
        assert_eq!(registry.list_for_execution("exec-1", false).len(), 1);
        assert_eq!(registry.list_for_execution("exec-1", true).len(), 2);
        assert_eq!(registry.list_for_execution("exec-2", false).len(), 1);
    }

    #[test]
    fn settled_entries_are_bounded() {
        let registry = BackgroundTaskRegistry::new();
        for i in 0..SETTLED_LIMIT + 5 {
            let id = format!("s-{i}");
            registry.register(&id, "cmd", "exec-1");
            registry.settle(&id, Some(0), true);
        }
        let settled = registry.list_for_execution("exec-1", true).len();
        assert_eq!(settled, SETTLED_LIMIT);
    }

    #[test]
    fn background_all_moves_only_running_tasks_of_execution() {
        let registry = BackgroundTaskRegistry::new();
        registry.register("s-1", "a", "exec-1");
        registry.register("s-2", "b", "exec-1");
        registry.register("s-3", "c", "exec-2");
        registry.settle("s-2", Some(0), true);
        let moved = registry.background_all_for_execution("exec-1");
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].session_id, "s-1");
        assert!(registry.is_backgrounded("s-1"));
        assert!(!registry.is_backgrounded("s-3"));
    }

    #[test]
    fn acknowledge_removes_only_settled_entries() {
        let registry = BackgroundTaskRegistry::new();
        registry.register("s-1", "a", "exec-1");
        assert!(!registry.acknowledge("s-1"));
        assert!(!registry.acknowledge("missing"));
        registry.settle("s-1", Some(0), true);
        assert!(registry.acknowledge("s-1"));
        assert!(registry.get("s-1").is_none());
    }

    #[test]
    fn global_listing_covers_settled_when_requested() {
        let registry = BackgroundTaskRegistry::new();
        registry.register("s-1", "a", "exec-1");
        registry.register("s-2", "b", "exec-2");
        registry.settle("s-2", Some(0), true);
        assert_eq!(registry.list_all(false).len(), 1);
        assert_eq!(registry.list_all(true).len(), 2);
        assert_eq!(registry.list_settled().len(), 1);
    }
}
