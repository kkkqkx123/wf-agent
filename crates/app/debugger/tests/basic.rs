use std::collections::HashMap;

use wf_debugger::model::{StepRecord, Trace, TraceKind};

fn sample_trace() -> Trace {
    let text = std::fs::read_to_string("examples/sample_trace.json").expect("sample trace");
    serde_json::from_str(&text).expect("sample trace parses")
}

#[test]
fn sample_trace_replays_with_expected_summary() {
    let trace = sample_trace();
    let outcome = wf_debugger::replay_trace(&trace);
    assert_eq!(outcome.summary.steps, 2);
    assert_eq!(outcome.summary.failures, 0);
    assert_eq!(outcome.summary.tool_calls, 1);
    assert_eq!(outcome.summary.trigger_matches, 1);
}

#[test]
fn sample_assertions_pass() {
    let trace = sample_trace();
    let outcome = wf_debugger::assert::run_assertions(&trace);
    assert_eq!(outcome.failed, 0);
    assert_eq!(outcome.passed, trace.assertions.len());
}

#[test]
fn failing_assertion_reports_actual_value() {
    let mut trace = sample_trace();
    trace
        .assertions
        .push(wf_debugger::assert::Assertion::Variable {
            step: 1,
            key: "x".to_string(),
            expected: serde_json::json!(999),
        });
    let outcome = wf_debugger::assert::run_assertions(&trace);
    assert_eq!(outcome.failed, 1);
    let last = outcome.results.last().expect("result");
    assert_eq!(last.actual, Some(serde_json::json!(3)));
}

#[test]
fn empty_steps_replay_cleanly() {
    let trace = Trace {
        schema: wf_debugger::model::TRACE_SCHEMA_V1.to_string(),
        kind: TraceKind::Workflow,
        graph_ref: String::new(),
        initial_variables: HashMap::new(),
        steps: Vec::new(),
        assertions: Vec::new(),
        trigger_templates: Vec::new(),
    };
    let outcome = wf_debugger::replay_trace(&trace);
    assert_eq!(outcome.summary.steps, 0);
    let _ = std::mem::size_of::<StepRecord>();
}
