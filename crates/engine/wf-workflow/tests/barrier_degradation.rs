//! Integration tests for the fork barrier types (`barrier::BranchResult`,
//! `FailureStrategy`) and fail-open degradation event emission
//! (`degradation::emit_data_degradation`).

use std::collections::HashMap;

use wf_core::EventBus;
use wf_types::events::EventType;
use wf_types::workflow::error_branch::NodeErrorCategory;
use wf_workflow::{emit_data_degradation, BranchResult, FailureStrategy, ForkOutcome};

fn success(branch_id: &str) -> BranchResult {
    BranchResult::success(branch_id, serde_json::json!({"ok": true}))
}

fn failure(branch_id: &str, detail: &str) -> BranchResult {
    BranchResult::failure(branch_id, detail)
}

// ── BranchResult ─────────────────────────────────────────────────────────

#[test]
fn success_result_has_no_failure() {
    let r = success("b1");
    assert!(r.success);
    assert_eq!(r.branch_id, "b1");
    assert!(r.failure.is_none());
    assert!(r.error_message().is_none());
    assert!(r.error_category().is_none());
    assert_eq!(r.output, serde_json::json!({"ok": true}));
}

#[test]
fn failure_result_carries_detail_and_default_category() {
    let r = failure("b2", "boom");
    assert!(!r.success);
    assert_eq!(r.error_message(), Some("boom"));
    assert_eq!(r.error_category(), Some(NodeErrorCategory::BusinessFailure));
    assert_eq!(r.output, serde_json::Value::Null);
}

#[test]
fn with_category_overrides_failure_category_only() {
    let r = failure("b3", "cancelled").with_category(NodeErrorCategory::CancelledInterrupted);
    assert_eq!(
        r.error_category(),
        Some(NodeErrorCategory::CancelledInterrupted)
    );
    // On a success result the override must be a no-op.
    let ok = success("b4").with_category(NodeErrorCategory::BusinessFailure);
    assert!(ok.failure.is_none());
}

#[test]
fn success_with_variables_carries_branch_variables() {
    let mut vars = HashMap::new();
    vars.insert("k".to_string(), serde_json::json!(1));
    let r = BranchResult::success_with_variables("b5", serde_json::json!(null), vars);
    assert!(r.success);
    assert_eq!(
        r.variables.as_ref().unwrap().get("k"),
        Some(&serde_json::json!(1))
    );
}

#[test]
fn branch_result_serializes_failure_without_null_fields() {
    let ok = serde_json::to_value(success("b6")).expect("serialize success");
    assert!(
        ok.get("failure").is_none(),
        "skip_serializing_if must drop it"
    );
    let bad = serde_json::to_value(failure("b7", "x")).expect("serialize failure");
    assert_eq!(bad["failure"]["detail"], "x");
}

// ── FailureStrategy::evaluate ────────────────────────────────────────────

#[test]
fn fail_fast_fails_on_any_branch_failure() {
    let s = FailureStrategy::FailFast;
    assert_eq!(s.evaluate(&[]), ForkOutcome::Succeeded);
    assert_eq!(s.evaluate(&[success("a")]), ForkOutcome::Succeeded);
    assert_eq!(
        s.evaluate(&[success("a"), failure("b", "x")]),
        ForkOutcome::Failed
    );
}

#[test]
fn continue_on_error_reports_partial_instead_of_failed() {
    let s = FailureStrategy::ContinueOnError;
    assert_eq!(s.evaluate(&[success("a")]), ForkOutcome::Succeeded);
    assert_eq!(
        s.evaluate(&[success("a"), failure("b", "x")]),
        ForkOutcome::Partial
    );
    assert_eq!(
        s.evaluate(&[failure("a", "x"), failure("b", "y")]),
        ForkOutcome::Partial
    );
}

#[test]
fn threshold_strategy_compares_failure_rate() {
    let s = FailureStrategy::FailOnThreshold { threshold: 0.5 };
    // 0/2 failures -> success.
    assert_eq!(
        s.evaluate(&[success("a"), success("b")]),
        ForkOutcome::Succeeded
    );
    // 1/2 = 0.5 is NOT strictly above the threshold -> partial.
    assert_eq!(
        s.evaluate(&[success("a"), failure("b", "x")]),
        ForkOutcome::Partial
    );
    // 2/2 = 1.0 > 0.5 -> failed.
    assert_eq!(
        s.evaluate(&[failure("a", "x"), failure("b", "y")]),
        ForkOutcome::Failed
    );
    // Empty input -> trivially succeeded.
    assert_eq!(s.evaluate(&[]), ForkOutcome::Succeeded);
}

#[test]
fn threshold_with_failures_below_rate_stays_partial() {
    let s = FailureStrategy::FailOnThreshold { threshold: 0.9 };
    let results = vec![failure("a", "x"), success("b"), success("c"), success("d")];
    assert_eq!(s.evaluate(&results), ForkOutcome::Partial);
}

// ── emit_data_degradation ────────────────────────────────────────────────

#[tokio::test]
async fn degradation_event_is_published_on_the_bus() {
    let bus = EventBus::new(16);
    let mut sub = bus.subscribe();
    let execution_id = wf_types::Id::new();

    emit_data_degradation(
        Some(&bus),
        None,
        &execution_id,
        "parse_node_config",
        "bad json",
    );

    let event = sub.try_recv().expect("degradation event must be published");
    assert_eq!(event.r#type, EventType::NodeCustomEvent);
    assert_eq!(event.execution_id, Some(execution_id));
    let metadata = event.metadata.expect("metadata must carry the marker");
    assert_eq!(
        metadata.get("event").and_then(|v| v.as_str()),
        Some("data_degraded")
    );
    assert_eq!(
        metadata.get("site").and_then(|v| v.as_str()),
        Some("parse_node_config")
    );
    assert_eq!(
        metadata.get("detail").and_then(|v| v.as_str()),
        Some("bad json")
    );
}

#[tokio::test]
async fn degradation_without_a_bus_is_a_silent_no_op() {
    let execution_id = wf_types::Id::new();
    // Must not panic and must not require a bus.
    emit_data_degradation(None, None, &execution_id, "site", "detail");
}

#[tokio::test]
async fn degradation_carries_optional_workflow_id() {
    let bus = EventBus::new(16);
    let mut sub = bus.subscribe();
    let workflow_id = wf_types::Id::new();
    let execution_id = wf_types::Id::new();

    emit_data_degradation(
        Some(&bus),
        Some(workflow_id.clone()),
        &execution_id,
        "variable_backfill",
        "missing reference",
    );

    let event = sub.try_recv().expect("event must be published");
    assert_eq!(event.workflow_id, Some(workflow_id));
}
