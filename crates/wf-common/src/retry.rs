use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Time budget mode.
///
/// - `DelayOnly`: only retry delays count against the time budget (default,
///   backward compatible).
/// - `TotalTime`: retry delays plus execution time count against the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeBudgetMode {
    #[default]
    DelayOnly,
    TotalTime,
}

/// Kind of a [`RetryBudgetEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryBudgetEventType {
    RetryConsumed,
    BudgetExhausted,
    RetryDenied,
    BudgetReset,
}

/// Event emitted by a [`RetryBudget`] for observability. Consumers (e.g. the
/// retry-budget metrics collector) subscribe through the `on_event` callback.
#[derive(Debug, Clone)]
pub struct RetryBudgetEvent {
    pub event_type: RetryBudgetEventType,
    pub retries_consumed: u32,
    pub max_retries: Option<u32>,
    pub time_budget_consumed_ms: u64,
    pub time_budget_ms: Option<u64>,
    pub branch_id: Option<String>,
    pub reason: Option<String>,
    pub delay_ms: Option<u64>,
}

/// Retry budget configuration. Both `max_retries` and `time_budget_ms`
/// follow the same rule: `None` = unlimited, `0` = no capacity.
/// Callback invoked on budget events (retry consumed, exhausted, denied,
/// reset). See `RetryBudgetEventType`.
pub type RetryBudgetEventHandler = Box<dyn Fn(&RetryBudgetEvent) + Send + Sync>;

/// Configuration for a shared retry budget.
pub struct RetryBudgetConfig {
    /// Maximum retry count. `None` = unlimited (time-only mode).
    pub max_retries: Option<u32>,
    /// Optional time budget in milliseconds. `None` = unlimited.
    pub time_budget_ms: Option<u64>,
    /// Time budget mode: `DelayOnly` (default) or `TotalTime`.
    pub time_budget_mode: TimeBudgetMode,
    /// Name for identification and logging.
    pub name: String,
    /// Optional callback for budget events (retry consumed, budget
    /// exhausted, retry denied, budget reset).
    pub on_event: Option<RetryBudgetEventHandler>,
}

/// Result of a time-budget check, used by both `can_retry` and
/// `consume_retry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetCheckResult {
    pub allowed: bool,
    /// Remaining time budget in ms; `u64::MAX` means unlimited.
    pub remaining: u64,
    /// When denied, explains which constraint was hit.
    pub reason: Option<String>,
}

fn allowed_result(remaining: u64) -> BudgetCheckResult {
    BudgetCheckResult {
        allowed: true,
        remaining,
        reason: None,
    }
}

fn denied_result(remaining: u64, reason: &str) -> BudgetCheckResult {
    BudgetCheckResult {
        allowed: false,
        remaining,
        reason: Some(reason.to_string()),
    }
}

/// Retry budget state snapshot (single source of truth for snapshot data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryBudgetState {
    /// Total retries available; `-1` means unlimited.
    pub total_retries: i64,
    pub retries_consumed: u32,
    /// Retries remaining; `u32::MAX` means unlimited.
    pub retries_remaining: u32,
    /// Time budget in ms; `-1` means unlimited.
    pub time_budget_ms: i64,
    pub time_budget_mode: TimeBudgetMode,
    pub time_budget_consumed_ms: u64,
    /// Execution time consumed (only tracked in `TotalTime` mode).
    pub execution_time_consumed_ms: u64,
    pub elapsed_time_ms: u64,
    pub is_exhausted: bool,
    /// Delay-only portion of `time_budget_consumed_ms`.
    pub total_delay_consumed_ms: u64,
    /// Remaining time budget; `u64::MAX` means unlimited.
    pub remaining_ms: u64,
}

/// Per-branch budget state (per-branch allocation, Problem #4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchBudgetState {
    pub branch_id: String,
    pub allocated_retries: u32,
    pub retries_consumed: u32,
    pub retries_remaining: u32,
}

/// Sentinel meaning "unlimited" for remaining-time values.
pub const UNLIMITED_MS: u64 = u64::MAX;
/// Sentinel meaning "unlimited" for remaining-retries values.
pub const UNLIMITED_RETRIES: u32 = u32::MAX;

#[derive(Debug)]
struct BranchBudget {
    allocated: u32,
    consumed: u32,
}

struct RetryBudgetInner {
    retries_consumed: u32,
    time_budget_consumed_ms: u64,
    execution_time_consumed_ms: u64,
    start_time: Instant,
    /// Per-branch budgets (per-branch allocation).
    branch_budgets: HashMap<String, BranchBudget>,
    /// Total retries allocated to all branches (used for pool calculation).
    total_branch_allocated: u32,
}

/// Global retry budget shared across a workflow/agent execution (fork
/// branches, agent-loop iterations and other retryable operations) to
/// prevent unbounded retry spending.
///
/// Two dimensions: retry count (`max_retries`) and time (`time_budget_ms`).
/// Both follow the same semantic: `None` = unlimited, `0` = no capacity.
///
/// Supports per-branch allocation (each branch gets `total / N` retries and
/// may borrow from the unallocated pool) and two time-budget modes.
/// Interior mutability makes the budget shareable through `Arc` across
/// parallel fork branches.
pub struct RetryBudget {
    config: RetryBudgetConfig,
    inner: Mutex<RetryBudgetInner>,
}

impl RetryBudget {
    pub fn new(config: RetryBudgetConfig) -> Self {
        Self {
            config,
            inner: Mutex::new(RetryBudgetInner {
                retries_consumed: 0,
                time_budget_consumed_ms: 0,
                execution_time_consumed_ms: 0,
                start_time: Instant::now(),
                branch_budgets: HashMap::new(),
                total_branch_allocated: 0,
            }),
        }
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Elapsed time since the budget was created.
    pub fn get_elapsed_time(&self) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .start_time
            .elapsed()
            .as_millis() as u64
    }

    fn emit_event(
        &self,
        event_type: RetryBudgetEventType,
        retries_consumed: u32,
        time_budget_consumed_ms: u64,
        branch_id: Option<String>,
        reason: Option<String>,
        delay_ms: Option<u64>,
    ) {
        let Some(callback) = &self.config.on_event else {
            return;
        };
        let event = RetryBudgetEvent {
            event_type,
            retries_consumed,
            max_retries: self.config.max_retries,
            time_budget_consumed_ms,
            time_budget_ms: self.config.time_budget_ms,
            branch_id,
            reason,
            delay_ms,
        };
        callback(&event);
    }

    /// Allocate per-branch budget (must be called before any branch starts
    /// retrying). Returns the number of retries allocated per branch
    /// (`u32::MAX` in unlimited-count mode).
    pub fn allocate_branch_budgets(&self, branch_ids: &[String]) -> u32 {
        if branch_ids.is_empty() {
            return 0;
        }
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // In unlimited count mode, skip per-branch budget allocation.
        if self.config.max_retries.is_none() {
            return UNLIMITED_RETRIES;
        }

        // Distribute total retries equally among branches (floor division).
        let max_retries = self.config.max_retries.unwrap_or(0);
        let allocated_per_branch = max_retries / branch_ids.len() as u32;
        let existing_count = inner.branch_budgets.len();

        for branch_id in branch_ids {
            // Skip branches that already have a budget allocated to prevent
            // re-allocation reset.
            if inner.branch_budgets.contains_key(branch_id) {
                continue;
            }
            inner.branch_budgets.insert(
                branch_id.clone(),
                BranchBudget {
                    allocated: allocated_per_branch,
                    consumed: 0,
                },
            );
        }

        let new_branch_count = inner.branch_budgets.len() - existing_count;
        inner.total_branch_allocated += new_branch_count as u32 * allocated_per_branch;

        allocated_per_branch
    }

    /// Check whether a retry is allowed within the budget (global count,
    /// per-branch count with pool borrowing, then time budget).
    ///
    /// - `delay_ms`: proposed delay for this retry in ms.
    /// - `branch_id`: optional branch id (per-branch budget check).
    /// - `execution_time_ms`: optional execution time added to the budget
    ///   (`TotalTime` mode only).
    pub fn can_retry(
        &self,
        delay_ms: u64,
        branch_id: Option<&str>,
        execution_time_ms: u64,
    ) -> BudgetCheckResult {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // 1. Global retry count budget (skip when unlimited).
        if let Some(max_retries) = self.config.max_retries {
            if inner.retries_consumed >= max_retries {
                return denied_result(
                    0,
                    &format!(
                        "Retry count budget exhausted ({}/{})",
                        inner.retries_consumed, max_retries
                    ),
                );
            }
        }

        // 2. Per-branch budget with pool borrowing.
        if let Some(branch_id) = branch_id {
            if let Some(branch) = inner.branch_budgets.get(branch_id) {
                if branch.consumed >= branch.allocated {
                    // Branch allocation exhausted: check whether pool
                    // retries are available. Pool = max_retries minus the
                    // total allocated (unallocated remainder from floor
                    // division). A branch can only borrow from the
                    // unallocated pool, not from other branches.
                    let pool_size = self
                        .config
                        .max_retries
                        .map(|max| max.saturating_sub(inner.total_branch_allocated))
                        .unwrap_or(0);

                    if pool_size == 0 {
                        return denied_result(
                            0,
                            &format!(
                                "Branch {} retry budget exhausted ({}/{}, no pool available)",
                                branch_id, branch.consumed, branch.allocated
                            ),
                        );
                    }

                    // Pool consumed = total retries consumed by branches
                    // beyond their allocation.
                    let pool_consumed: u32 = inner
                        .branch_budgets
                        .values()
                        .map(|bb| bb.consumed.saturating_sub(bb.allocated))
                        .sum();
                    let pool_remaining = pool_size.saturating_sub(pool_consumed);

                    if pool_remaining == 0 {
                        return denied_result(
                            0,
                            &format!(
                                "Branch {} retry budget exhausted (pool fully consumed)",
                                branch_id
                            ),
                        );
                    }
                    // Pool has remaining — allow borrowing.
                }
            }
        }

        // 3. Time budget.
        if let Some(time_budget_ms) = self.config.time_budget_ms {
            if time_budget_ms == 0 {
                return denied_result(0, "Time budget is 0 — no time allowed for retries");
            }

            let projected_budget_ms = match self.config.time_budget_mode {
                TimeBudgetMode::DelayOnly => inner.time_budget_consumed_ms + delay_ms,
                TimeBudgetMode::TotalTime => {
                    inner.time_budget_consumed_ms + delay_ms + execution_time_ms
                }
            };

            if projected_budget_ms > time_budget_ms {
                let remaining = time_budget_ms.saturating_sub(inner.time_budget_consumed_ms);
                return denied_result(
                    remaining,
                    &format!(
                        "Retry delay would exceed time budget (remaining: {}ms)",
                        remaining
                    ),
                );
            }

            return allowed_result(time_budget_ms.saturating_sub(projected_budget_ms));
        }

        // No time limit — unlimited remaining.
        allowed_result(UNLIMITED_MS)
    }

    /// Consume a retry from the budget (global and per-branch). Returns the
    /// check result; `allowed == false` means the budget rejected the retry
    /// and it must not be attempted.
    pub fn consume_retry(
        &self,
        delay_ms: u64,
        branch_id: Option<&str>,
        execution_time_ms: u64,
    ) -> BudgetCheckResult {
        let check = self.can_retry(delay_ms, branch_id, execution_time_ms);
        if !check.allowed {
            // Read data under lock, then release before calling callback.
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            let retries_consumed = inner.retries_consumed;
            let time_budget_consumed_ms = inner.time_budget_consumed_ms;
            drop(inner); // Release lock before calling callback

            self.emit_event(
                RetryBudgetEventType::RetryDenied,
                retries_consumed,
                time_budget_consumed_ms,
                branch_id.map(String::from),
                check.reason.clone(),
                Some(delay_ms),
            );
            return check;
        }

        // Mutate under lock, then read data and release before calling callback.
        let (retries_consumed, time_budget_consumed_ms) = {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

            // Consume from the global budget.
            inner.retries_consumed += 1;

            // Consume from the per-branch budget.
            if let Some(branch_id) = branch_id {
                if let Some(branch) = inner.branch_budgets.get_mut(branch_id) {
                    branch.consumed += 1;
                }
            }

            // Update time budget consumption.
            inner.time_budget_consumed_ms += delay_ms;
            if self.config.time_budget_mode == TimeBudgetMode::TotalTime {
                inner.execution_time_consumed_ms += execution_time_ms;
                inner.time_budget_consumed_ms += execution_time_ms;
            }

            (inner.retries_consumed, inner.time_budget_consumed_ms)
        }; // Lock released here

        self.emit_event(
            RetryBudgetEventType::RetryConsumed,
            retries_consumed,
            time_budget_consumed_ms,
            branch_id.map(String::from),
            None,
            Some(delay_ms),
        );

        let remaining = self
            .config
            .time_budget_ms
            .map(|budget| budget.saturating_sub(time_budget_consumed_ms))
            .unwrap_or(UNLIMITED_MS);
        allowed_result(remaining)
    }

    /// Record execution time against the time budget (`TotalTime` mode only).
    /// Unlike `consume_retry`, this does NOT consume a retry-count slot; used
    /// by agent-style execution where execution time is tracked separately.
    /// In `DelayOnly` mode this is a no-op.
    pub fn record_execution_time(&self, execution_time_ms: u64) {
        if self.config.time_budget_mode != TimeBudgetMode::TotalTime {
            return;
        }
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.execution_time_consumed_ms += execution_time_ms;
        inner.time_budget_consumed_ms += execution_time_ms;
    }

    /// Whether the budget is exhausted (count or time dimension).
    pub fn is_exhausted(&self) -> bool {
        let state = self.get_state();
        state.is_exhausted
    }

    /// Get the current budget state snapshot.
    pub fn get_state(&self) -> RetryBudgetState {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        let count_exhausted = self
            .config
            .max_retries
            .is_some_and(|max| inner.retries_consumed >= max);
        let time_exhausted = self
            .config
            .time_budget_ms
            .is_some_and(|budget| budget == 0 || inner.time_budget_consumed_ms > budget);
        // Derived from the held guard instead of `get_retries_remaining` to
        // avoid re-entrant locking (std Mutex is not re-entrant).
        let retries_remaining = match self.config.max_retries {
            Some(max) => max.saturating_sub(inner.retries_consumed),
            None => UNLIMITED_RETRIES,
        };

        RetryBudgetState {
            total_retries: self.config.max_retries.map(|v| v as i64).unwrap_or(-1),
            retries_consumed: inner.retries_consumed,
            retries_remaining,
            time_budget_ms: self.config.time_budget_ms.map(|v| v as i64).unwrap_or(-1),
            time_budget_mode: self.config.time_budget_mode,
            time_budget_consumed_ms: inner.time_budget_consumed_ms,
            execution_time_consumed_ms: inner.execution_time_consumed_ms,
            elapsed_time_ms: inner.start_time.elapsed().as_millis() as u64,
            is_exhausted: count_exhausted || time_exhausted,
            total_delay_consumed_ms: inner
                .time_budget_consumed_ms
                .saturating_sub(inner.execution_time_consumed_ms),
            remaining_ms: self
                .config
                .time_budget_ms
                .map(|budget| budget.saturating_sub(inner.time_budget_consumed_ms))
                .unwrap_or(UNLIMITED_MS),
        }
    }

    /// Get the per-branch budget state.
    pub fn get_branch_budget_state(&self, branch_id: &str) -> Option<BranchBudgetState> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let branch = inner.branch_budgets.get(branch_id)?;
        Some(BranchBudgetState {
            branch_id: branch_id.to_string(),
            allocated_retries: branch.allocated,
            retries_consumed: branch.consumed,
            retries_remaining: branch.allocated.saturating_sub(branch.consumed),
        })
    }

    /// Retries remaining; `u32::MAX` means unlimited.
    pub fn get_retries_remaining(&self) -> u32 {
        match self.config.max_retries {
            Some(max) => max.saturating_sub(
                self.inner
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .retries_consumed,
            ),
            None => UNLIMITED_RETRIES,
        }
    }

    /// Reset budget consumption.
    pub fn reset(&self, reset_start_time: bool) {
        {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.retries_consumed = 0;
            inner.time_budget_consumed_ms = 0;
            inner.execution_time_consumed_ms = 0;
            inner.branch_budgets.clear();
            inner.total_branch_allocated = 0;
            if reset_start_time {
                inner.start_time = Instant::now();
            }
        } // Lock released here

        self.emit_event(RetryBudgetEventType::BudgetReset, 0, 0, None, None, None);
    }
}

/// Execute `operation` with retries governed by an optional shared budget.
/// When `budget` is `None`, the operation runs exactly once (fail fast).
pub async fn execute_with_retry<F, Fut, T, E>(
    budget: Option<&RetryBudget>,
    operation: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let Some(budget) = budget else {
                    return Err(e);
                };
                let delay = Duration::from_millis(
                    1000 * 2_u64.pow(budget.get_state().retries_consumed.min(10)),
                );
                let check = budget.consume_retry(delay.as_millis() as u64, None, 0);
                if !check.allowed {
                    return Err(e);
                }
                tokio::time::sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn config(
        max_retries: Option<u32>,
        time_budget_ms: Option<u64>,
        mode: TimeBudgetMode,
    ) -> RetryBudgetConfig {
        RetryBudgetConfig {
            max_retries,
            time_budget_ms,
            time_budget_mode: mode,
            name: "test".to_string(),
            on_event: None,
        }
    }

    #[test]
    fn unlimited_budget_never_exhausts() {
        let budget = RetryBudget::new(config(None, None, TimeBudgetMode::DelayOnly));
        assert!(budget.can_retry(0, None, 0).allowed);
        assert_eq!(budget.get_retries_remaining(), UNLIMITED_RETRIES);
        assert_eq!(budget.get_state().remaining_ms, UNLIMITED_MS);
        assert!(!budget.is_exhausted());
    }

    #[test]
    fn zero_retries_immediately_exhausted() {
        let budget = RetryBudget::new(config(Some(0), None, TimeBudgetMode::DelayOnly));
        let check = budget.can_retry(0, None, 0);
        assert!(!check.allowed);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn count_budget_exhausted_after_max_retries() {
        let budget = RetryBudget::new(config(Some(2), None, TimeBudgetMode::DelayOnly));
        assert!(budget.consume_retry(0, None, 0).allowed);
        assert!(budget.consume_retry(0, None, 0).allowed);
        assert!(!budget.consume_retry(0, None, 0).allowed);
        assert!(budget.is_exhausted());
        assert_eq!(budget.get_retries_remaining(), 0);
    }

    #[test]
    fn delay_only_mode_ignores_execution_time() {
        let budget = RetryBudget::new(config(None, Some(100), TimeBudgetMode::DelayOnly));
        // Execution time does not count in delay-only mode.
        assert!(budget.can_retry(90, None, 10_000).allowed);
        assert!(budget.consume_retry(90, None, 10_000).allowed);
        // Remaining budget covers only delays: 10ms left.
        assert!(budget.can_retry(9, None, 10_000).allowed);
        assert!(!budget.can_retry(11, None, 0).allowed);
    }

    #[test]
    fn total_time_mode_counts_execution_time() {
        let budget = RetryBudget::new(config(None, Some(100), TimeBudgetMode::TotalTime));
        // 90ms delay + 10ms execution time exactly consumes the budget.
        assert!(budget.consume_retry(90, None, 10).allowed);
        assert!(!budget.can_retry(1, None, 0).allowed);
    }

    #[test]
    fn total_time_mode_record_execution_time() {
        let budget = RetryBudget::new(config(None, Some(100), TimeBudgetMode::TotalTime));
        budget.record_execution_time(60);
        budget.record_execution_time(30);
        assert!(budget.can_retry(9, None, 0).allowed);
        assert!(!budget.can_retry(11, None, 0).allowed);
    }

    #[test]
    fn delay_only_record_execution_time_is_noop() {
        let budget = RetryBudget::new(config(None, Some(100), TimeBudgetMode::DelayOnly));
        budget.record_execution_time(10_000);
        assert!(budget.can_retry(100, None, 0).allowed);
        assert_eq!(budget.get_state().time_budget_consumed_ms, 0);
    }

    #[test]
    fn branch_allocation_and_pool_borrowing() {
        let budget = Arc::new(RetryBudget::new(config(
            Some(5),
            None,
            TimeBudgetMode::DelayOnly,
        )));
        let branches: Vec<String> = vec!["b1".into(), "b2".into()];
        // 5 / 2 = 2 allocated per branch; 1 remains in the pool.
        let per_branch = budget.allocate_branch_budgets(&branches);
        assert_eq!(per_branch, 2);

        // Each branch can consume its allocation.
        assert!(budget.consume_retry(0, Some("b1"), 0).allowed);
        assert!(budget.consume_retry(0, Some("b1"), 0).allowed);
        assert!(budget.consume_retry(0, Some("b2"), 0).allowed);
        assert!(budget.consume_retry(0, Some("b2"), 0).allowed);

        let b1 = budget.get_branch_budget_state("b1").unwrap();
        assert_eq!(b1.retries_remaining, 0);

        // Both branches exhausted their allocations; only 1 pool retry is
        // available between them.
        assert!(
            budget.consume_retry(0, Some("b1"), 0).allowed,
            "borrow from pool"
        );
        assert!(
            !budget.consume_retry(0, Some("b2"), 0).allowed,
            "pool fully consumed"
        );
        assert!(!budget.consume_retry(0, Some("b1"), 0).allowed);
    }

    #[test]
    fn reallocation_does_not_reset_existing_branches() {
        let budget = Arc::new(RetryBudget::new(config(
            Some(10),
            None,
            TimeBudgetMode::DelayOnly,
        )));
        budget.allocate_branch_budgets(&["b1".to_string()]);
        assert!(budget.consume_retry(0, Some("b1"), 0).allowed);
        // Re-allocate with the same branch id plus a new one: b1 keeps its
        // consumed state, b2 gets a fresh allocation.
        budget.allocate_branch_budgets(&["b1".to_string(), "b2".to_string()]);
        let b1 = budget.get_branch_budget_state("b1").unwrap();
        assert_eq!(b1.retries_consumed, 1);
        let b2 = budget.get_branch_budget_state("b2").unwrap();
        assert_eq!(b2.retries_consumed, 0);
        assert_eq!(b2.allocated_retries, 5);
    }

    #[test]
    fn concurrent_branch_consumption_never_exceeds_budget() {
        let budget = Arc::new(RetryBudget::new(config(
            Some(20),
            None,
            TimeBudgetMode::DelayOnly,
        )));
        let branches: Vec<String> = (0..4).map(|i| format!("b{i}")).collect();
        budget.allocate_branch_budgets(&branches);

        let handles: Vec<_> = branches
            .iter()
            .map(|branch| {
                let budget = budget.clone();
                let branch = branch.clone();
                std::thread::spawn(move || {
                    let mut consumed = 0u32;
                    while budget.consume_retry(0, Some(&branch), 0).allowed {
                        consumed += 1;
                    }
                    consumed
                })
            })
            .collect();

        let total: u32 = handles.into_iter().map(|h| h.join().unwrap()).sum();
        // 4 branches × 5 allocated = 20; the pool is empty (20 / 4 = 5).
        assert_eq!(total, 20);
        assert_eq!(budget.get_state().retries_consumed, 20);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn reset_clears_consumption() {
        let budget = RetryBudget::new(config(Some(1), Some(100), TimeBudgetMode::DelayOnly));
        assert!(budget.consume_retry(50, None, 0).allowed);
        assert!(budget.is_exhausted());
        budget.reset(false);
        assert!(!budget.is_exhausted());
        assert_eq!(budget.get_state().retries_consumed, 0);
        assert!(budget.can_retry(50, None, 0).allowed);
    }

    #[test]
    fn on_event_callback_receives_events() {
        let events: Arc<Mutex<Vec<RetryBudgetEventType>>> = Arc::new(Mutex::new(Vec::new()));
        let events_cb = events.clone();
        let budget = RetryBudget::new(RetryBudgetConfig {
            max_retries: Some(1),
            time_budget_ms: None,
            time_budget_mode: TimeBudgetMode::DelayOnly,
            name: "event-test".to_string(),
            on_event: Some(Box::new(move |event| {
                events_cb.lock().unwrap().push(event.event_type);
            })),
        });

        budget.consume_retry(0, None, 0);
        budget.consume_retry(0, None, 0);
        budget.reset(false);

        let seen = events.lock().unwrap();
        assert!(seen.contains(&RetryBudgetEventType::RetryConsumed));
        assert!(seen.contains(&RetryBudgetEventType::RetryDenied));
        assert!(seen.contains(&RetryBudgetEventType::BudgetReset));
    }

    #[test]
    fn branch_budget_skipped_in_unlimited_mode() {
        let budget = RetryBudget::new(config(None, None, TimeBudgetMode::DelayOnly));
        assert_eq!(
            budget.allocate_branch_budgets(&["b1".to_string()]),
            UNLIMITED_RETRIES
        );
        assert!(budget.get_branch_budget_state("b1").is_none());
        for _ in 0..100 {
            assert!(budget.consume_retry(0, Some("b1"), 0).allowed);
        }
    }

    #[tokio::test]
    async fn execute_with_retry_stops_when_budget_exhausted() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let budget = RetryBudget::new(config(Some(2), None, TimeBudgetMode::DelayOnly));
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_cb = attempts.clone();
        let result: Result<(), &str> = execute_with_retry(Some(&budget), move || {
            let attempts = attempts_cb.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err("boom")
            }
        })
        .await;
        assert_eq!(result, Err("boom"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn execute_with_retry_without_budget_fails_fast() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_cb = attempts.clone();
        let result: Result<(), &str> = execute_with_retry(None, move || {
            let attempts = attempts_cb.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err("boom")
            }
        })
        .await;
        assert_eq!(result, Err("boom"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
