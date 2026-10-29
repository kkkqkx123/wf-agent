//! Wall-clock budget carry-over across suspend/resume.
//!
//! A checkpoint stores the run's total budgets plus the active time already
//! spent (`elapsed_ms`), never a "remaining milliseconds" value: time parked
//! in storage must not consume the budget, so the continuation recomputes
//! its remainder from the frozen totals. Non-duration limits travel verbatim.

use dashmap::DashMap;
use std::sync::Arc;

use crate::error::{WorkflowError, WorkflowResult};

/// Reserved variable holding the run's original budget totals. Written once
/// when the execution starts; resumes must not overwrite it with remainders.
pub const BUDGETS_VAR: &str = "__execution_budgets";
/// Reserved variable holding the cumulative active time spent before the
/// current segment. Seeded from the snapshot on resume so the next
/// checkpoint keeps accumulating instead of restarting the clock.
pub const ELAPSED_VAR: &str = "__execution_elapsed_ms";

type VariableStore = Arc<DashMap<String, serde_json::Value>>;

/// Freeze the duration-relevant totals of `options` into the shared variable
/// map so later checkpoints (built from the entity alone) can persist them.
pub fn persist_totals(
    variables: &VariableStore,
    options: &wf_types::workflow_execution::WorkflowExecutionOptions,
) {
    variables.insert(
        BUDGETS_VAR.to_string(),
        serde_json::json!({
            "max_execution_time": options.max_execution_time,
            "node_timeout": options.node_timeout,
            "max_pause_duration": options.max_pause_duration,
            "max_navigation_multiplier": options.max_navigation_multiplier,
            "loop_max_iterations_cap": options.loop_max_iterations_cap,
        }),
    );
}

/// Reserved variable holding the cumulative human-approval wait time of
/// this execution. Approval waits never consume wall-clock budgets, so both
/// the node timeout and the total `max_execution_time` check subtract it.
pub const APPROVAL_WAIT_VAR: &str = "__approval_wait_ms";
/// Reserved variable marking an in-progress approval wait with its start
/// timestamp. Tool calls run sequentially within one node execution, so a
/// single marker is sufficient; its presence freezes the active-time clock
/// until the wait lands and is credited.
pub const APPROVAL_WAITING_VAR: &str = "__approval_wait_started_ms";

/// Completed approval waits so far, in milliseconds. A present but
/// non-numeric value is treated as zero with a warning so a corrupted slot
/// never silently extends the budget.
pub fn approval_wait_ms(variables: &VariableStore) -> u64 {
    match variables.get(APPROVAL_WAIT_VAR) {
        None => 0,
        Some(v) => match v.value().as_u64() {
            Some(ms) => ms,
            None => {
                tracing::warn!(
                    key = APPROVAL_WAIT_VAR,
                    value = ?v.value(),
                    "approval wait accumulator is corrupted, treating as zero"
                );
                0
            }
        },
    }
}

/// Start timestamp of the in-progress approval wait, if any. A present but
/// non-integer value is ignored with a warning instead of a silent pass.
pub fn approval_wait_started_ms(variables: &VariableStore) -> Option<i64> {
    match variables.get(APPROVAL_WAITING_VAR) {
        None => None,
        Some(v) => match v.value().as_i64() {
            Some(ms) => Some(ms),
            None => {
                tracing::warn!(
                    key = APPROVAL_WAITING_VAR,
                    value = ?v.value(),
                    "approval wait marker is corrupted, ignoring the in-progress wait"
                );
                None
            }
        },
    }
}

/// Active time between two timestamps: wall-clock minus completed approval
/// waits minus the still-running wait, so budget guards never charge human
/// response time to the execution.
pub fn active_elapsed_ms(variables: &VariableStore, start_ms: i64, now_ms: i64) -> u64 {
    let elapsed = (now_ms - start_ms).max(0) as u64;
    let in_progress = approval_wait_started_ms(variables)
        .map(|t0| (now_ms - t0).max(0) as u64)
        .unwrap_or(0);
    elapsed
        .saturating_sub(approval_wait_ms(variables))
        .saturating_sub(in_progress)
}

/// Mark the start of a human-approval wait.
pub fn begin_approval_wait(variables: &VariableStore) {
    variables.insert(
        APPROVAL_WAITING_VAR.to_string(),
        serde_json::Value::from(wf_common::now()),
    );
}

/// Land a finished approval wait: remove the in-progress marker and credit
/// its duration into the accumulator.
pub fn end_approval_wait(variables: &VariableStore) {
    let now = wf_common::now();
    if let Some(started) = variables.get(APPROVAL_WAITING_VAR).and_then(|v| v.as_i64()) {
        variables.remove(APPROVAL_WAITING_VAR);
        let waited = (now - started).max(0) as u64;
        let total = approval_wait_ms(variables).saturating_add(waited);
        variables.insert(
            APPROVAL_WAIT_VAR.to_string(),
            serde_json::Value::from(total),
        );
    }
}
/// Accumulated active time before this segment, carried across resumes.
/// A present but non-numeric value is treated as zero with a warning so a
/// corrupted slot restarts the clock loudly instead of silently.
pub fn accumulated_ms(variables: &VariableStore) -> u64 {
    match variables.get(ELAPSED_VAR) {
        None => 0,
        Some(v) => match v.value().as_u64() {
            Some(ms) => ms,
            None => {
                tracing::warn!(
                    key = ELAPSED_VAR,
                    value = ?v.value(),
                    "accumulated active time is corrupted, restarting the clock from zero"
                );
                0
            }
        },
    }
}

/// Build the `execution_config` snapshot payload: original totals plus the
/// total active time spent up to now (prior segments plus this one).
pub fn snapshot_config(
    variables: &VariableStore,
    segment_start_ms: i64,
) -> Option<serde_json::Value> {
    let Some(budgets) = variables.get(BUDGETS_VAR).map(|v| v.clone()) else {
        tracing::warn!(
            key = BUDGETS_VAR,
            "budget totals are missing, snapshotting without an execution config"
        );
        return None;
    };
    let segment_ms = (wf_common::now() - segment_start_ms).max(0) as u64;
    let elapsed_ms = accumulated_ms(variables).saturating_add(segment_ms);
    Some(serde_json::json!({
        "budgets": budgets,
        "elapsed_ms": elapsed_ms,
    }))
}

/// Rebuild continuation options from a snapshot's `execution_config`. The
/// wall-clock budget is reduced to its remainder; the other limits are
/// restored verbatim. A fully spent budget refuses the resume instead of
/// handing the continuation an unbudgeted run.
pub fn continuation_options(
    execution_id: &str,
    execution_config: Option<&serde_json::Value>,
) -> WorkflowResult<wf_types::workflow_execution::WorkflowExecutionOptions> {
    let Some(config) = execution_config else {
        tracing::warn!(
            execution_id = %execution_id,
            "checkpoint carries no execution config, resuming without budgets"
        );
        return Ok(unbudgeted());
    };
    // Snapshots captured through the application restore path nest the full
    // options beside the spent time instead of the flattened totals.
    if let (Some(nested), Some(spent)) = (
        config.get("options"),
        config.get("executed_ms").and_then(|v| v.as_u64()),
    ) {
        let mut options: wf_types::workflow_execution::WorkflowExecutionOptions =
            match serde_json::from_value(nested.clone()) {
                Ok(options) => options,
                Err(e) => {
                    tracing::warn!(
                        execution_id = %execution_id,
                        error = %e,
                        "nested execution options are corrupted, resuming without budgets"
                    );
                    unbudgeted()
                }
            };
        options.max_steps = None;
        options.max_execution_time = match options.max_execution_time {
            Some(total) if total > 0 => {
                let remaining = total.saturating_sub(spent);
                if remaining == 0 {
                    return Err(WorkflowError::StateTransitionError(format!(
                        "execution {execution_id} already spent its {total}ms wall-clock \
                         budget before the checkpoint; refusing to resume with no remaining budget"
                    )));
                }
                Some(remaining)
            }
            other => other,
        };
        return Ok(options);
    }
    let budgets = config.get("budgets").unwrap_or(config);
    let elapsed_ms = config
        .get("elapsed_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let mut options = unbudgeted();
    options.node_timeout = budgets.get("node_timeout").and_then(|v| v.as_u64());
    options.max_pause_duration = budgets.get("max_pause_duration").and_then(|v| v.as_u64());
    options.max_navigation_multiplier = budgets
        .get("max_navigation_multiplier")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    options.loop_max_iterations_cap = budgets
        .get("loop_max_iterations_cap")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    options.max_execution_time = match budgets.get("max_execution_time").and_then(|v| v.as_u64()) {
        Some(total) if total > 0 => {
            let remaining = total.saturating_sub(elapsed_ms);
            if remaining == 0 {
                return Err(WorkflowError::StateTransitionError(format!(
                    "execution {execution_id} already spent its {total}ms wall-clock \
                     budget before the checkpoint; refusing to resume with no remaining budget"
                )));
            }
            Some(remaining)
        }
        other => other,
    };
    Ok(options)
}

/// Total active time recorded in a snapshot's `execution_config`, reseeded
/// into the resumed entity so the following checkpoint keeps accumulating.
pub fn snapshot_elapsed_ms(execution_config: Option<&serde_json::Value>) -> u64 {
    let Some(config) = execution_config else {
        return 0;
    };
    let raw = config
        .get("elapsed_ms")
        .or_else(|| config.get("executed_ms"));
    match raw {
        None => 0,
        Some(v) => match v.as_u64() {
            Some(ms) => ms,
            None => {
                tracing::warn!(
                    value = %v,
                    "checkpoint elapsed time is corrupted, reseeding from zero"
                );
                0
            }
        },
    }
}

fn unbudgeted() -> wf_types::workflow_execution::WorkflowExecutionOptions {
    wf_types::workflow_execution::WorkflowExecutionOptions {
        file_scope: None,
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(true),
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> VariableStore {
        Arc::new(DashMap::new())
    }

    fn options() -> wf_types::workflow_execution::WorkflowExecutionOptions {
        wf_types::workflow_execution::WorkflowExecutionOptions {
            file_scope: None,
            input: None,
            max_steps: Some(10),
            timeout: None,
            max_execution_time: Some(1000),
            enable_checkpoints: Some(true),
            node_timeout: Some(50),
            max_pause_duration: Some(500),
            max_navigation_multiplier: Some(7),
            loop_max_iterations_cap: Some(100),
        }
    }

    #[test]
    fn resume_keeps_remaining_wall_clock_and_verbatim_limits() {
        let vars = store();
        persist_totals(&vars, &options());
        let start = wf_common::now() - 400;
        let config = snapshot_config(&vars, start).expect("budgets persisted");
        let next = continuation_options("exec-1", Some(&config)).unwrap();
        let remaining = next.max_execution_time.unwrap();
        assert!(remaining <= 600 && remaining > 0, "{remaining}");
        assert_eq!(next.node_timeout, Some(50));
        assert_eq!(next.max_pause_duration, Some(500));
        assert_eq!(next.max_navigation_multiplier, Some(7));
        assert_eq!(next.loop_max_iterations_cap, Some(100));
        assert_eq!(next.max_steps, None);
    }

    #[test]
    fn exhausted_budget_refuses_resume() {
        let vars = store();
        persist_totals(&vars, &options());
        let start = wf_common::now() - 5000;
        let config = snapshot_config(&vars, start).expect("budgets persisted");
        let err = continuation_options("exec-1", Some(&config)).unwrap_err();
        assert!(err.to_string().contains("no remaining budget"), "{err}");
    }

    #[test]
    fn missing_config_stays_unbudgeted() {
        let next = continuation_options("exec-1", None).unwrap();
        assert_eq!(next.max_execution_time, None);
    }

    #[test]
    fn approval_waits_are_credited_and_subtracted() {
        let vars = store();
        assert_eq!(approval_wait_ms(&vars), 0);
        assert_eq!(approval_wait_started_ms(&vars), None);
        assert_eq!(active_elapsed_ms(&vars, 1000, 1500), 500);

        vars.insert(APPROVAL_WAIT_VAR.to_string(), serde_json::json!(200u64));
        vars.insert(APPROVAL_WAITING_VAR.to_string(), serde_json::json!(1200i64));
        assert_eq!(approval_wait_ms(&vars), 200);
        assert_eq!(approval_wait_started_ms(&vars), Some(1200));
        assert_eq!(active_elapsed_ms(&vars, 1000, 1500), 0);

        vars.remove(APPROVAL_WAITING_VAR);
        begin_approval_wait(&vars);
        assert!(approval_wait_started_ms(&vars).is_some());
        end_approval_wait(&vars);
        assert_eq!(approval_wait_started_ms(&vars), None);
        let credited = approval_wait_ms(&vars);
        assert!(credited >= 200);
        let now = wf_common::now();
        assert_eq!(
            active_elapsed_ms(&vars, 1000, now),
            (now - 1000).max(0) as u64 - credited
        );
    }

    #[test]
    fn end_without_begin_credits_nothing() {
        let vars = store();
        end_approval_wait(&vars);
        assert_eq!(approval_wait_ms(&vars), 0);
    }

    #[test]
    fn corrupted_budget_slots_degrade_to_zero() {
        let vars = store();
        vars.insert(APPROVAL_WAIT_VAR.to_string(), serde_json::json!("bad"));
        vars.insert(APPROVAL_WAITING_VAR.to_string(), serde_json::json!("bad"));
        vars.insert(ELAPSED_VAR.to_string(), serde_json::json!("bad"));
        assert_eq!(approval_wait_ms(&vars), 0);
        assert_eq!(approval_wait_started_ms(&vars), None);
        assert_eq!(accumulated_ms(&vars), 0);
    }

    #[test]
    fn corrupted_snapshot_elapsed_reseeds_from_zero() {
        let bad = serde_json::json!({"elapsed_ms": "bad"});
        assert_eq!(snapshot_elapsed_ms(Some(&bad)), 0);
        assert_eq!(snapshot_elapsed_ms(None), 0);
    }

    #[test]
    fn missing_budgets_snapshot_returns_none() {
        let vars = store();
        assert!(snapshot_config(&vars, wf_common::now()).is_none());
    }

    #[test]
    fn corrupted_elapsed_falls_back_to_segment_only() {
        let vars = store();
        persist_totals(&vars, &options());
        vars.insert(ELAPSED_VAR.to_string(), serde_json::json!(100_000u64));
        let full = snapshot_config(&vars, wf_common::now()).expect("budgets persisted");
        assert!(full.get("elapsed_ms").and_then(|v| v.as_u64()).unwrap() >= 100_000);

        vars.insert(ELAPSED_VAR.to_string(), serde_json::json!("bad"));
        let degraded = snapshot_config(&vars, wf_common::now()).expect("budgets persisted");
        assert!(
            degraded.get("elapsed_ms").and_then(|v| v.as_u64()).unwrap() < 5_000,
            "corrupted accumulation restarts from zero: {degraded}"
        );
    }

    #[test]
    fn corrupted_config_elapsed_keeps_full_budget() {
        let vars = store();
        persist_totals(&vars, &options());
        let mut config = snapshot_config(&vars, wf_common::now()).expect("budgets persisted");
        config["elapsed_ms"] = serde_json::json!("bad");
        let next = continuation_options("exec-1", Some(&config)).unwrap();
        assert_eq!(
            next.max_execution_time,
            Some(1000),
            "unparseable elapsed defaults to zero, keeping the full budget"
        );
    }
}
