use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::error::ExecutionSharedError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskPriority {
    Default = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

impl TaskPriority {
    pub const fn all() -> [TaskPriority; 5] {
        [
            TaskPriority::Default,
            TaskPriority::Low,
            TaskPriority::Normal,
            TaskPriority::High,
            TaskPriority::Critical,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeoutPolicy {
    Cancel,
    Escalate,
    Manual,
}

#[derive(Debug, Clone)]
pub struct TaskSchedulerConfig {
    pub max_concurrent: usize,
    pub enable_timeout_recovery: bool,
    pub timeout_check_interval: Duration,
    pub fair_scheduling: bool,
}

impl Default for TaskSchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: std::cmp::max(2, num_cpus::get().saturating_sub(1)),
            enable_timeout_recovery: true,
            timeout_check_interval: Duration::from_secs(5),
            fair_scheduling: true,
        }
    }
}

#[derive(Debug)]
struct ScheduledTask {
    #[allow(dead_code)]
    task_id: String,
    source_id: String,
    priority: TaskPriority,
    #[allow(dead_code)]
    submit_time: i64,
    timeout: Option<Duration>,
    deadline: Option<i64>,
    timeout_policy: TimeoutPolicy,
}

#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    pub pending: usize,
    pub executing: usize,
    pub total: usize,
    pub max_concurrent: usize,
}

pub type TaskCallback = Box<dyn Fn() -> futures::future::BoxFuture<'static, ()> + Send + Sync>;

pub struct TaskScheduler {
    config: TaskSchedulerConfig,
    inner: Mutex<SchedulerInner>,
    shutdown_token: CancellationToken,
}

struct SchedulerInner {
    pending_by_priority: HashMap<TaskPriority, Vec<String>>,
    tasks: HashMap<String, ScheduledTask>,
    executing: HashSet<String>,
    source_rr_index: usize,
    source_order: Vec<String>,
}

impl TaskScheduler {
    pub fn new(config: TaskSchedulerConfig) -> Self {
        let mut pending_by_priority = HashMap::new();
        for level in TaskPriority::all() {
            pending_by_priority.insert(level, Vec::new());
        }

        Self {
            config,
            inner: Mutex::new(SchedulerInner {
                pending_by_priority,
                tasks: HashMap::new(),
                executing: HashSet::new(),
                source_rr_index: 0,
                source_order: Vec::new(),
            }),
            shutdown_token: CancellationToken::new(),
        }
    }

    pub fn schedule(
        &self,
        task_id: String,
        source_id: String,
        timeout: Option<Duration>,
        priority: TaskPriority,
        timeout_policy: TimeoutPolicy,
    ) -> Result<(), ExecutionSharedError> {
        let now = wf_common::time::now();
        let deadline = timeout.map(|d| now + d.as_millis() as i64);

        let task = ScheduledTask {
            task_id: task_id.clone(),
            source_id: source_id.clone(),
            priority,
            submit_time: now,
            timeout,
            deadline,
            timeout_policy,
        };

        let mut inner = self.inner.lock().unwrap();

        inner.tasks.insert(task_id.clone(), task);
        inner
            .pending_by_priority
            .entry(priority)
            .or_default()
            .push(task_id.clone());

        if !inner.source_order.contains(&source_id) {
            inner.source_order.push(source_id);
        }

        Ok(())
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();

        if inner.executing.contains(task_id) {
            return false;
        }

        inner.tasks.remove(task_id);
        for bucket in inner.pending_by_priority.values_mut() {
            bucket.retain(|id| id != task_id);
        }

        true
    }

    pub fn next_task(&self) -> Option<(String, ScheduledTaskForExecution)> {
        let mut inner = self.inner.lock().unwrap();

        if inner.executing.len() >= self.config.max_concurrent {
            return None;
        }

        let (task_id, selected_source_idx) = if self.config.fair_scheduling {
            let (id, idx) = Self::next_task_fair(&inner)?;
            (id, Some(idx))
        } else {
            (Self::next_task_by_priority(&inner)?, None)
        };

        if let Some(idx) = selected_source_idx {
            let source_count = inner.source_order.len();
            if source_count > 0 {
                inner.source_rr_index = (idx + 1) % source_count;
            }
        }

        let task = inner.tasks.get(&task_id)?;
            let priority = task.priority;
        let timeout = task.timeout;
        let deadline = task.deadline;

        inner.executing.insert(task_id.clone());
        for bucket in inner.pending_by_priority.values_mut() {
            bucket.retain(|id| id != &task_id);
        }

        Some((
            task_id,
            ScheduledTaskForExecution {
                priority,
                timeout,
                deadline,
            },
        ))
    }

    pub fn complete_task(&self, task_id: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.executing.remove(task_id);
        inner.tasks.remove(task_id);
    }

    pub fn shutdown(&self) {
        self.shutdown_token.cancel();
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown_token.is_cancelled()
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.shutdown_token.child_token()
    }

    pub fn stats(&self) -> SchedulerStats {
        let inner = self.inner.lock().unwrap();
        let pending: usize = inner
            .pending_by_priority
            .values()
            .map(|b| b.len())
            .sum();
        SchedulerStats {
            pending,
            executing: inner.executing.len(),
            total: inner.tasks.len(),
            max_concurrent: self.config.max_concurrent,
        }
    }

    pub fn check_expired(&self) -> Vec<(String, TimeoutPolicy)> {
        let now = wf_common::time::now();
        let mut inner = self.inner.lock().unwrap();
        let mut expired = Vec::new();

        for (task_id, task) in &inner.tasks {
            if inner.executing.contains(task_id) {
                continue;
            }
            if let Some(deadline) = task.deadline {
                if deadline < now {
                    expired.push((task_id.clone(), task.timeout_policy.clone()));
                }
            }
        }

        for (task_id, _) in &expired {
            inner.tasks.remove(task_id);
            for bucket in inner.pending_by_priority.values_mut() {
                bucket.retain(|id| id != task_id);
            }
        }

        expired
    }

    fn next_task_by_priority(inner: &SchedulerInner) -> Option<String> {
        for priority in [
            TaskPriority::Critical,
            TaskPriority::High,
            TaskPriority::Normal,
            TaskPriority::Low,
            TaskPriority::Default,
        ] {
            let bucket = inner.pending_by_priority.get(&priority)?;
            if bucket.is_empty() {
                continue;
            }
            return Some(bucket[0].clone());
        }
        None
    }

    fn next_task_fair(inner: &SchedulerInner) -> Option<(String, usize)> {
        if inner.source_order.is_empty() {
            return None;
        }

        let source_count = inner.source_order.len();
        for i in 0..source_count {
            let idx = (inner.source_rr_index + i) % source_count;
            let source_id = &inner.source_order[idx];

            for priority in [
                TaskPriority::Critical,
                TaskPriority::High,
                TaskPriority::Normal,
                TaskPriority::Low,
                TaskPriority::Default,
            ] {
                let bucket = inner.pending_by_priority.get(&priority)?;
                if let Some(task_id) = bucket.iter().find(|id| {
                    inner
                        .tasks
                        .get(*id)
                        .map(|t| t.source_id == *source_id)
                        .unwrap_or(false)
                }) {
                    return Some((task_id.clone(), idx));
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct ScheduledTaskForExecution {
    pub priority: TaskPriority,
    pub timeout: Option<Duration>,
    pub deadline: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheduler(fair: bool) -> TaskScheduler {
        TaskScheduler::new(TaskSchedulerConfig {
            max_concurrent: 2,
            enable_timeout_recovery: false,
            timeout_check_interval: Duration::from_secs(60),
            fair_scheduling: fair,
        })
    }

    #[test]
    fn test_schedule_and_get_next() {
        let s = scheduler(false);
        s.schedule(
            "t1".to_string(),
            "src1".to_string(),
            None,
            TaskPriority::Normal,
            TimeoutPolicy::Cancel,
        )
        .unwrap();

        let (id, _) = s.next_task().unwrap();
        assert_eq!(id, "t1");
    }

    #[test]
    fn test_priority_ordering() {
        let s = scheduler(false);
        s.schedule(
            "low".to_string(),
            "src".to_string(),
            None,
            TaskPriority::Low,
            TimeoutPolicy::Cancel,
        )
        .unwrap();
        s.schedule(
            "critical".to_string(),
            "src".to_string(),
            None,
            TaskPriority::Critical,
            TimeoutPolicy::Cancel,
        )
        .unwrap();

        let (id, _) = s.next_task().unwrap();
        assert_eq!(id, "critical");
    }

    #[test]
    fn test_max_concurrent() {
        let s = scheduler(false);
        for i in 0..4 {
            s.schedule(
                format!("t{}", i),
                "src".to_string(),
                None,
                TaskPriority::Normal,
                TimeoutPolicy::Cancel,
            )
            .unwrap();
        }

        let _ = s.next_task().unwrap();
        let _ = s.next_task().unwrap();
        assert!(s.next_task().is_none());

        s.complete_task("t0");
        assert!(s.next_task().is_some());
    }

    #[test]
    fn test_cancel() {
        let s = scheduler(false);
        s.schedule(
            "t1".to_string(),
            "src".to_string(),
            None,
            TaskPriority::Normal,
            TimeoutPolicy::Cancel,
        )
        .unwrap();

        assert!(s.cancel("t1"));
        assert!(s.next_task().is_none());
    }

    #[test]
    fn test_cancel_executing_fails() {
        let s = scheduler(false);
        s.schedule(
            "t1".to_string(),
            "src".to_string(),
            None,
            TaskPriority::Normal,
            TimeoutPolicy::Cancel,
        )
        .unwrap();

        let (id, _) = s.next_task().unwrap();
        assert!(!s.cancel(&id));
        assert_eq!(s.stats().executing, 1);
    }

    #[test]
    fn test_stats() {
        let s = scheduler(false);
        s.schedule(
            "t1".to_string(),
            "src".to_string(),
            None,
            TaskPriority::Normal,
            TimeoutPolicy::Cancel,
        )
        .unwrap();
        s.schedule(
            "t2".to_string(),
            "src".to_string(),
            None,
            TaskPriority::Normal,
            TimeoutPolicy::Cancel,
        )
        .unwrap();

        let stats = s.stats();
        assert_eq!(stats.pending, 2);
        assert_eq!(stats.executing, 0);

        let _ = s.next_task().unwrap();
        let stats = s.stats();
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.executing, 1);
    }

    #[test]
    fn test_fair_scheduling() {
        let s = scheduler(true);
        s.schedule(
            "s1_normal".to_string(),
            "source_a".to_string(),
            None,
            TaskPriority::Normal,
            TimeoutPolicy::Cancel,
        )
        .unwrap();
        s.schedule(
            "s2_normal".to_string(),
            "source_b".to_string(),
            None,
            TaskPriority::Normal,
            TimeoutPolicy::Cancel,
        )
        .unwrap();

        let (id, _) = s.next_task().unwrap();
        assert_eq!(id, "s1_normal");

        let (id2, _) = s.next_task().unwrap();
        assert_eq!(id2, "s2_normal");
    }

    #[test]
    fn test_shutdown() {
        let s = scheduler(false);
        assert!(!s.is_shutdown());
        s.shutdown();
        assert!(s.is_shutdown());
    }

    #[test]
    fn test_check_expired() {
        let s = TaskScheduler::new(TaskSchedulerConfig {
            max_concurrent: 2,
            enable_timeout_recovery: false,
            timeout_check_interval: Duration::from_secs(60),
            fair_scheduling: false,
        });

        s.schedule(
            "expired".to_string(),
            "src".to_string(),
            Some(Duration::from_millis(1)),
            TaskPriority::Normal,
            TimeoutPolicy::Cancel,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(10));

        let expired = s.check_expired();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].0, "expired");
    }

    #[test]
    fn test_complete_removes_task() {
        let s = scheduler(false);
        s.schedule(
            "t1".to_string(),
            "src".to_string(),
            None,
            TaskPriority::Normal,
            TimeoutPolicy::Cancel,
        )
        .unwrap();

        let (id, _) = s.next_task().unwrap();
        s.complete_task(&id);

        let stats = s.stats();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.executing, 0);
    }
}
