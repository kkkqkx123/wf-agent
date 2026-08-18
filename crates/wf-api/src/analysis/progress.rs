//! Execution progress tracking.
//!
//! A polling [`ProgressTracker`] over a workflow execution plus the pure
//! metric computation. It reads the live
//! [`wf_workflow::entity::WorkflowExecutionEntity`] (iteration / completed
//! nodes) combined with the persisted execution record (timestamps /
//! terminal status) and the workflow graph (total iterations).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Metrics snapshot of an execution.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgressMetrics {
    pub execution_id: String,
    /// Completed nodes / current iteration.
    pub iteration: u64,
    /// Total expected nodes (`0` when unknown).
    pub total_iterations: u64,
    /// Completed ratio as a percentage (0.0 ..= 100.0).
    pub progress_percentage: f64,
    /// Elapsed wall-clock time in ms.
    pub elapsed_time: i64,
    /// Estimated remaining time in ms; `-1` when no estimate is available.
    pub estimated_remaining_time: i64,
    /// Estimated total time in ms.
    pub estimated_total_time: i64,
    /// Estimated completion timestamp (ms epoch); `None` when unknown.
    pub estimated_completion_time: Option<i64>,
    /// Confidence of the estimates (0.0 ..= 1.0).
    pub confidence: f64,
    pub iterations_per_second: f64,
    pub tool_calls_per_second: f64,
    /// `running` | `paused` | `completed` | `failed` | `cancelled`.
    pub status: String,
}

/// Progress lifecycle event a listener can subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressEventType {
    /// Fired on every iteration change.
    Progress,
    Complete,
    Failed,
    Paused,
    Resumed,
}

/// Callback invoked with the latest [`ProgressMetrics`] on an event.
pub type ProgressCallback = Arc<dyn Fn(&ProgressMetrics) + Send + Sync>;

/// Subscription handle returned by [`ProgressTracker::on`]; removing the
/// listener on drop.
pub struct Unsubscribe {
    shared: Arc<ProgressShared>,
    id: usize,
}

impl Drop for Unsubscribe {
    fn drop(&mut self) {
        if let Ok(mut listeners) = self.shared.listeners.lock() {
            listeners.retain(|listener| listener.id != self.id);
        }
    }
}

/// One registered listener.
#[derive(Clone)]
struct Listener {
    event: ProgressEventType,
    callback: ProgressCallback,
    id: usize,
}

/// State shared between the tracker and its polling task.
struct ProgressShared {
    next_id: AtomicUsize,
    listeners: Mutex<Vec<Listener>>,
    metrics: Mutex<Option<ProgressMetrics>>,
}

impl ProgressShared {
    fn new() -> Self {
        Self {
            next_id: AtomicUsize::new(1),
            listeners: Mutex::new(Vec::new()),
            metrics: Mutex::new(None),
        }
    }

    /// Store the latest metrics and fire the event callbacks (only
    /// transition/iteration-change events are emitted).
    fn update(&self, metrics: ProgressMetrics) {
        let previous = wf_common::lock::lock_ok(self.metrics.lock()).clone();
        *wf_common::lock::lock_ok(self.metrics.lock()) = Some(metrics.clone());

        let prev_iteration = previous.as_ref().map(|m| m.iteration);
        let prev_status = previous.as_ref().map(|m| m.status.clone());
        let listeners = wf_common::lock::lock_ok(self.listeners.lock()).clone();

        let fire = |event: ProgressEventType, metrics: &ProgressMetrics| {
            for listener in &listeners {
                if listener.event == event {
                    (listener.callback)(metrics);
                }
            }
        };

        if prev_iteration != Some(metrics.iteration) {
            fire(ProgressEventType::Progress, &metrics);
        }
        let status_changed = prev_status.as_deref() != Some(metrics.status.as_str());
        if status_changed {
            match metrics.status.as_str() {
                "completed" => fire(ProgressEventType::Complete, &metrics),
                "failed" => fire(ProgressEventType::Failed, &metrics),
                "paused" => fire(ProgressEventType::Paused, &metrics),
                "running" if prev_status.as_deref() == Some("paused") => {
                    fire(ProgressEventType::Resumed, &metrics);
                }
                _ => {}
            }
        }
    }
}

/// Polling progress tracker for a single workflow execution.
///
/// [`start`](Self::start) spawns a tokio task that polls [`get_progress`] on
/// `poll_interval` and updates the subscribed listeners; [`stop`](Self::stop)
/// stops the polling task.
pub struct ProgressTracker {
    execution_id: String,
    ctx: Arc<ApiContext>,
    poll_interval: Duration,
    shared: Arc<ProgressShared>,
    task: Option<tokio::task::JoinHandle<()>>,
    cancel: CancellationToken,
}

impl ProgressTracker {
    /// Create a tracker for `execution_id`; `poll_interval` defaults to 1s.
    pub fn new(ctx: Arc<ApiContext>, execution_id: impl Into<String>) -> Self {
        Self::with_poll_interval(ctx, execution_id, Duration::from_secs(1))
    }

    /// Create a tracker with a custom polling interval.
    pub fn with_poll_interval(
        ctx: Arc<ApiContext>,
        execution_id: impl Into<String>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            execution_id: execution_id.into(),
            ctx,
            poll_interval,
            shared: Arc::new(ProgressShared::new()),
            task: None,
            cancel: CancellationToken::new(),
        }
    }

    /// Start polling and emitting progress events. No-op when already started.
    pub async fn start(&mut self) -> ApiResult<()> {
        if self.task.is_some() {
            return Ok(());
        }
        let ctx = self.ctx.clone();
        let execution_id = self.execution_id.clone();
        let shared = self.shared.clone();
        let cancel = self.cancel.clone();
        let interval_ms = self.poll_interval.as_millis().max(50) as u64;
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
            // The first tick completes immediately: it performs the initial
            // poll, subsequent ticks space the polls `interval_ms` apart.
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        if let Ok(metrics) = get_progress(&ctx, &execution_id).await {
                            shared.update(metrics);
                        }
                    }
                }
            }
        });
        self.task = Some(task);
        Ok(())
    }

    /// Stop polling. Idempotent.
    pub async fn stop(&mut self) {
        if self.task.is_some() {
            self.cancel.cancel();
            if let Some(task) = self.task.take() {
                let _ = task.await;
            }
        }
        // Fresh token so a later `start` can poll again.
        self.cancel = CancellationToken::new();
    }

    /// Subscribe a listener to a progress event; the returned handle removes
    /// the listener when dropped.
    pub fn on(
        &mut self,
        event: ProgressEventType,
        callback: impl Fn(&ProgressMetrics) + Send + Sync + 'static,
    ) -> Unsubscribe {
        let id = self.shared.next_id.fetch_add(1, Ordering::SeqCst);
        self.shared
            .listeners
            .lock()
            .expect("listeners lock")
            .push(Listener {
                event,
                callback: Arc::new(callback),
                id,
            });
        Unsubscribe {
            shared: self.shared.clone(),
            id,
        }
    }

    /// Latest metrics snapshot, if any poll has completed.
    pub fn get_metrics(&self) -> Option<ProgressMetrics> {
        wf_common::lock::lock_ok(self.shared.metrics.lock()).clone()
    }

    /// Latest progress percentage (0.0 ..= 100.0).
    pub fn get_progress_percentage(&self) -> f64 {
        self.get_metrics().map_or(0.0, |m| m.progress_percentage)
    }

    /// Estimated remaining time in ms (`-1` when unknown).
    pub fn estimate_remaining_time(&self) -> i64 {
        self.get_metrics()
            .map_or(-1, |m| m.estimated_remaining_time)
    }

    /// Whether polling is active.
    pub fn is_tracking(&self) -> bool {
        self.task.is_some()
    }
}

/// Compute the current progress metrics of an execution.
///
/// Reads the live entity (completed nodes / status) combined with the
/// persisted execution record (timestamps / terminal status) and the workflow
/// graph (total expected nodes). Estimates derive per-second rates from
/// elapsed time, remaining time from the average node duration, and a `0.8`
/// confidence once a real rate is measurable.
pub async fn get_progress(ctx: &ApiContext, execution_id: &str) -> ApiResult<ProgressMetrics> {
    let now = wf_common::now();

    let mut current_iteration = 0u64;
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        current_iteration = state.completed_nodes().len() as u64;
        drop(state);
    }

    let record = crate::workflow::get_execution(ctx, execution_id).await.ok();
    let started_at = record.as_ref().map_or(now, |record| record.started_at);
    let status = record
        .as_ref()
        .map(|record| record.status.as_str().to_string())
        .unwrap_or_else(|| "running".to_string());
    let tool_calls = if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        state
            .node_execution_history()
            .iter()
            .filter(|record| record.node_type.to_uppercase().contains("TOOL"))
            .count() as u64
    } else {
        0
    };
    if current_iteration == 0 {
        current_iteration = record
            .as_ref()
            .and_then(|record| record.node_results.as_ref())
            .map_or(0, |results| results.len() as u64);
    }

    let total_iterations = workflow_total_nodes(ctx, record.as_ref()).await;
    Ok(compute_progress_metrics(
        execution_id,
        now,
        started_at,
        current_iteration,
        total_iterations,
        tool_calls,
        &status,
    ))
}

/// Number of nodes in the workflow graph driving `execution`; `None` when the
/// workflow is unknown (progress then equals the completed count).
async fn workflow_total_nodes(
    ctx: &ApiContext,
    execution: Option<&wf_types::WorkflowExecution>,
) -> Option<u64> {
    let workflow_id = execution.map(|record| record.workflow_id.to_string())?;
    let graph = crate::workflow::workflow_execution::resolve_graph(ctx, &workflow_id)
        .await
        .ok()?;
    Some(graph.nodes.len() as u64)
}

/// Pure metric computation over raw execution observations. Extracted for
/// unit testing.
fn compute_progress_metrics(
    execution_id: &str,
    now: i64,
    start_time: i64,
    current_iteration: u64,
    total_iterations: Option<u64>,
    tool_calls: u64,
    status: &str,
) -> ProgressMetrics {
    let elapsed_time = now.saturating_sub(start_time).max(0);
    let total_iterations = total_iterations
        .unwrap_or(current_iteration)
        .max(current_iteration);
    let elapsed_secs = (elapsed_time as f64) / 1000.0;

    let iterations_per_second = if current_iteration > 0 {
        round2(current_iteration as f64 / elapsed_secs.max(f64::EPSILON))
    } else {
        0.0
    };
    let tool_calls_per_second = if tool_calls > 0 {
        round2(tool_calls as f64 / elapsed_secs.max(f64::EPSILON))
    } else {
        0.0
    };

    let progress_percentage = if total_iterations > 0 {
        ((current_iteration as f64) / (total_iterations as f64)) * 100.0
    } else {
        0.0
    };

    let avg_iteration_duration = if current_iteration > 0 {
        (elapsed_time as f64) / (current_iteration as f64)
    } else {
        0.0
    };

    let remaining_iterations = total_iterations.saturating_sub(current_iteration);
    let mut estimated_remaining_time = -1;
    let mut estimated_total_time = elapsed_time;
    let mut confidence = 0.0;
    let mut estimated_completion_time = None;
    if current_iteration > 0 && remaining_iterations > 0 && avg_iteration_duration > 0.0 {
        estimated_remaining_time =
            (remaining_iterations as f64 * avg_iteration_duration).round() as i64;
        estimated_total_time = elapsed_time + estimated_remaining_time;
        estimated_completion_time = Some(now + estimated_remaining_time);
        confidence = 0.8;
    }

    ProgressMetrics {
        execution_id: execution_id.to_string(),
        iteration: current_iteration,
        total_iterations,
        progress_percentage,
        elapsed_time,
        estimated_remaining_time,
        estimated_total_time,
        estimated_completion_time,
        confidence,
        iterations_per_second,
        tool_calls_per_second,
        status: status.to_string(),
    }
}

/// Human-readable progress line.
pub fn format_progress(metrics: &ProgressMetrics) -> String {
    let eta = metrics
        .estimated_completion_time
        .and_then(|ms| {
            chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.format("%H:%M:%S").to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    format!(
        "Progress: {:.1}% | Iteration {}/{} | ETA: {}",
        metrics.progress_percentage, metrics.iteration, metrics.total_iterations, eta
    )
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_compute_rates_and_estimates() {
        let metrics = compute_progress_metrics(
            "exec-1",
            5_000, // now
            1_000, // start
            4,     // current iteration
            Some(8),
            2, // tool calls
            "running",
        );
        assert_eq!(metrics.iteration, 4);
        assert_eq!(metrics.total_iterations, 8);
        assert_eq!(metrics.elapsed_time, 4_000);
        assert_eq!(metrics.progress_percentage, 50.0);
        assert_eq!(metrics.iterations_per_second, 1.0); // 4 / 4s
        assert_eq!(metrics.tool_calls_per_second, 0.5); // 2 / 4s
                                                        // avg node duration 1000ms, 4 remaining => 4000ms ETA
        assert_eq!(metrics.estimated_remaining_time, 4_000);
        assert_eq!(metrics.estimated_total_time, 8_000);
        assert_eq!(metrics.estimated_completion_time, Some(9_000));
        assert_eq!(metrics.confidence, 0.8);
        assert_eq!(metrics.status, "running");
    }

    #[test]
    fn metrics_without_iterations_yield_no_estimates() {
        let metrics = compute_progress_metrics("exec-2", 1_000, 1_000, 0, None, 0, "running");
        assert_eq!(metrics.progress_percentage, 0.0);
        assert_eq!(metrics.estimated_remaining_time, -1);
        assert_eq!(metrics.confidence, 0.0);
        assert_eq!(metrics.estimated_total_time, 0);
        assert!(metrics.estimated_completion_time.is_none());
    }

    #[test]
    fn metrics_when_total_unknown_use_current() {
        let metrics = compute_progress_metrics("exec-3", 2_000, 1_000, 3, None, 0, "completed");
        assert_eq!(metrics.total_iterations, 3);
        assert_eq!(metrics.progress_percentage, 100.0);
        assert_eq!(metrics.status, "completed");
        assert_eq!(metrics.estimated_remaining_time, -1);
    }

    #[test]
    fn format_progress_renders_percentage_iteration_and_eta() {
        let metrics = compute_progress_metrics("exec-4", 5_000, 1_000, 4, Some(8), 2, "running");
        let line = format_progress(&metrics);
        assert!(
            line.starts_with("Progress: 50.0% | Iteration 4/8 | ETA: "),
            "{line}"
        );
    }

    #[test]
    fn tracker_update_fires_transition_events() {
        let shared = Arc::new(ProgressShared::new());
        let progress = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        shared.listeners.lock().unwrap().push(Listener {
            event: ProgressEventType::Progress,
            callback: {
                let count = progress.clone();
                Arc::new(move |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                })
            },
            id: 1,
        });
        shared.listeners.lock().unwrap().push(Listener {
            event: ProgressEventType::Complete,
            callback: {
                let count = completed.clone();
                Arc::new(move |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                })
            },
            id: 2,
        });

        // First poll fires PROGRESS (no previous metrics).
        let running = compute_progress_metrics("e", 1_000, 1_000, 1, Some(2), 0, "running");
        shared.update(running.clone());
        // Same iteration: no PROGRESS.
        shared.update(running);
        // Terminal: PROGRESS (iteration changed) + COMPLETE.
        let done = compute_progress_metrics("e", 2_000, 1_000, 2, Some(2), 0, "completed");
        shared.update(done);

        assert_eq!(progress.load(Ordering::SeqCst), 2);
        assert_eq!(completed.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unsubscribe_removes_listener() {
        let shared = Arc::new(ProgressShared::new());
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let id = shared.next_id.fetch_add(1, Ordering::SeqCst);
        let callback = {
            let count = count.clone();
            Arc::new(move |_: &ProgressMetrics| {
                count.fetch_add(1, Ordering::SeqCst);
            })
        };
        {
            let _handle = Unsubscribe {
                shared: shared.clone(),
                id,
            };
            shared.listeners.lock().unwrap().push(Listener {
                event: ProgressEventType::Progress,
                callback,
                id,
            });
            assert_eq!(shared.listeners.lock().unwrap().len(), 1);
        }
        assert_eq!(shared.listeners.lock().unwrap().len(), 0);
    }
}
