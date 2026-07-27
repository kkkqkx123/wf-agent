use std::sync::Arc;

use tokio::sync::Notify;

use crate::error::WorkflowResult;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BranchResult {
    pub branch_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

impl BranchResult {
    pub fn success(branch_id: impl Into<String>, output: serde_json::Value) -> Self {
        Self {
            branch_id: branch_id.into(),
            success: true,
            output,
            error: None,
        }
    }

    pub fn failure(branch_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            branch_id: branch_id.into(),
            success: false,
            output: serde_json::Value::Null,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FailureStrategy {
    FailFast,
    ContinueOnError,
    FailOnThreshold { threshold: f64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkOutcome {
    Succeeded,
    Failed,
    Partial,
}

impl FailureStrategy {
    pub fn evaluate(&self, results: &[BranchResult]) -> ForkOutcome {
        let total = results.len();
        let failures = results.iter().filter(|r| !r.success).count();

        match self {
            FailureStrategy::FailFast => {
                if failures > 0 { ForkOutcome::Failed } else { ForkOutcome::Succeeded }
            }
            FailureStrategy::ContinueOnError => ForkOutcome::Succeeded,
            FailureStrategy::FailOnThreshold { threshold } => {
                if total == 0 { return ForkOutcome::Succeeded; }
                let failure_rate = failures as f64 / total as f64;
                if failure_rate > *threshold {
                    ForkOutcome::Failed
                } else if failures > 0 {
                    ForkOutcome::Partial
                } else {
                    ForkOutcome::Succeeded
                }
            }
        }
    }
}

pub struct SyncBarrier {
    expected_count: usize,
    completed: Arc<tokio::sync::Mutex<Vec<String>>>,
    notify: Arc<Notify>,
}

impl SyncBarrier {
    pub fn new(expected_count: usize) -> Self {
        Self {
            expected_count,
            completed: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            notify: Arc::new(Notify::new()),
        }
    }

    pub async fn notify_branch_completed(&self, branch_id: &str) -> bool {
        let mut completed = self.completed.lock().await;
        completed.push(branch_id.to_string());
        let is_complete = completed.len() >= self.expected_count;
        if is_complete {
            self.notify.notify_waiters();
        }
        is_complete
    }

    pub async fn wait_for_all(&self) {
        while self.completed.lock().await.len() < self.expected_count {
            self.notify.notified().await;
        }
    }

    pub async fn remaining(&self) -> usize {
        let completed = self.completed.lock().await;
        self.expected_count.saturating_sub(completed.len())
    }

    pub async fn is_complete(&self) -> bool {
        self.completed.lock().await.len() >= self.expected_count
    }
}
