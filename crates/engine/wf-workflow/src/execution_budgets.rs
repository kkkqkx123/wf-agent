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

/// Accumulated active time before this segment, carried across resumes.
pub fn accumulated_ms(variables: &VariableStore) -> u64 {
    variables
        .get(ELAPSED_VAR)
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// Build the `execution_config` snapshot payload: original totals plus the
/// total active time spent up to now (prior segments plus this one).
pub fn snapshot_config(
    variables: &VariableStore,
    segment_start_ms: i64,
) -> Option<serde_json::Value> {
    let budgets = variables.get(BUDGETS_VAR).map(|v| v.clone())?;
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
        return Ok(unbudgeted());
    };
    // Snapshots captured through the application restore path nest the full
    // options beside the spent time instead of the flattened totals.
    if let (Some(nested), Some(spent)) = (
        config.get("options"),
        config.get("executed_ms").and_then(|v| v.as_u64()),
    ) {
        let mut options: wf_types::workflow_execution::WorkflowExecutionOptions =
            serde_json::from_value(nested.clone()).unwrap_or_else(|_| unbudgeted());
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
    options.max_pause_duration = budgets
        .get("max_pause_duration")
        .and_then(|v| v.as_u64());
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
    execution_config
        .and_then(|c| {
            c.get("elapsed_ms")
                .or_else(|| c.get("executed_ms"))
                .and_then(|v| v.as_u64())
        })
        .unwrap_or(0)
}

fn unbudgeted() -> wf_types::workflow_execution::WorkflowExecutionOptions {
    wf_types::workflow_execution::WorkflowExecutionOptions {
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
}
