use std::sync::Arc;

use super::ExecutionContextRegistry;

/// Re-arm the persisted pre-request budget warning after a workflow
/// compression write-back lands. The execution-scoped tracker state is
/// checkpointed in the variable map; flipping the guard there lets the next
/// over-budget request warn again. Best-effort: missing or unparsable state
/// is left untouched.
pub(crate) fn reset_persisted_preflight_warning(
    contexts: &Arc<ExecutionContextRegistry>,
    execution_id: &str,
) {
    use wf_workflow::handler::llm::token_budget::TRACKER_STATE_KEY;
    let Some(variables) = contexts.variables_for(execution_id) else {
        return;
    };
    let Some(value) = variables.get(TRACKER_STATE_KEY).map(|entry| entry.clone()) else {
        return;
    };
    let mut state: wf_execution_shared::TokenTrackerState = match serde_json::from_value(value) {
        Ok(state) => state,
        Err(_) => return,
    };
    if !state.preflight_warning_emitted {
        return;
    }
    state.preflight_warning_emitted = false;
    if let Ok(value) = serde_json::to_value(state) {
        variables.insert(TRACKER_STATE_KEY.to_string(), value);
    }
}

/// Record a still-over-budget completion in the persisted tracker streak so
/// consecutive loops escalate to an error instead of compressing forever.
/// Best-effort: missing state is left untouched.
pub(crate) fn record_still_over_budget_streak(
    variables: &Arc<dashmap::DashMap<String, serde_json::Value>>,
    target: &str,
) {
    use wf_workflow::handler::llm::token_budget::TRACKER_STATE_KEY;
    let Some(value) = variables.get(TRACKER_STATE_KEY).map(|entry| entry.clone()) else {
        return;
    };
    let mut state: wf_execution_shared::TokenTrackerState = match serde_json::from_value(value) {
        Ok(state) => state,
        Err(_) => return,
    };
    let count = state
        .still_over_budget_counts
        .get(target)
        .copied()
        .unwrap_or(0)
        + 1;
    state
        .still_over_budget_counts
        .insert(target.to_string(), count);
    if count >= wf_execution_shared::TokenUsageTracker::STILL_OVER_BUDGET_ESCALATION_THRESHOLD {
        tracing::error!(
            target = %target,
            streak = count,
            "compressed result still exceeds budget repeatedly; check budget or content compressibility"
        );
    }
    if let Ok(value) = serde_json::to_value(state) {
        variables.insert(TRACKER_STATE_KEY.to_string(), value);
    }
}

/// Reset the still-over-budget streak after a fitting result.
pub(crate) fn reset_still_over_budget_streak(
    variables: &Arc<dashmap::DashMap<String, serde_json::Value>>,
    target: &str,
) {
    use wf_workflow::handler::llm::token_budget::TRACKER_STATE_KEY;
    let Some(value) = variables.get(TRACKER_STATE_KEY).map(|entry| entry.clone()) else {
        return;
    };
    let mut state: wf_execution_shared::TokenTrackerState = match serde_json::from_value(value) {
        Ok(state) => state,
        Err(_) => return,
    };
    if !state.still_over_budget_counts.contains_key(target) {
        return;
    }
    state.still_over_budget_counts.remove(target);
    if let Ok(value) = serde_json::to_value(state) {
        variables.insert(TRACKER_STATE_KEY.to_string(), value);
    }
}
