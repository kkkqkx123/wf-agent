use std::time::{Duration, Instant};

use crate::error::ExecutionSharedResult;

#[derive(Debug, Clone)]
pub struct RetryBudgetConfig {
    pub max_retries: u32,
    pub time_budget_ms: u64,
    pub time_budget_mode: TimeBudgetMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeBudgetMode {
    DelayOnly,
    TotalTime,
}

#[derive(Debug)]
pub struct RetryBudget {
    config: RetryBudgetConfig,
    attempts: u32,
    start_time: Instant,
    total_delay: Duration,
}

impl RetryBudget {
    pub fn new(config: RetryBudgetConfig) -> Self {
        Self {
            config,
            attempts: 0,
            start_time: Instant::now(),
            total_delay: Duration::ZERO,
        }
    }

    pub fn can_retry(&self) -> bool {
        if self.attempts >= self.config.max_retries {
            return false;
        }

        let elapsed = self.start_time.elapsed();
        match self.config.time_budget_mode {
            TimeBudgetMode::DelayOnly => self.total_delay < Duration::from_millis(self.config.time_budget_ms),
            TimeBudgetMode::TotalTime => elapsed < Duration::from_millis(self.config.time_budget_ms),
        }
    }

    pub fn record_attempt(&mut self, delay: Duration) {
        self.attempts += 1;
        self.total_delay += delay;
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }
}

pub async fn execute_with_retry<F, Fut, T>(
    budget: &mut RetryBudget,
    operation: F,
) -> ExecutionSharedResult<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = ExecutionSharedResult<T>>,
{
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !budget.can_retry() {
                    return Err(e);
                }
                let delay = Duration::from_millis(1000 * 2_u64.pow(budget.attempts()));
                budget.record_attempt(delay);
                tokio::time::sleep(delay).await;
            }
        }
    }
}
