use wf_debugger::agent_dbg::{
    analyze_agent_trace, EXPLORER_AGENT_TEMPLATE_ID, VIOLATION_UNEXPECTED_SUCCESS,
};

fn explorer_trace() -> wf_debugger::model::Trace {
    let text =
        std::fs::read_to_string("examples/sample_agent_explorer.json").expect("explorer trace");
    serde_json::from_str(&text).expect("explorer trace parses")
}

#[test]
fn explorer_example_replays_with_one_denied_call() {
    let trace = explorer_trace();
    assert_eq!(trace.agent_template, EXPLORER_AGENT_TEMPLATE_ID);
    let outcome = wf_debugger::replay_trace(&trace);
    assert_eq!(outcome.summary.steps, 2);
    assert_eq!(outcome.summary.tool_calls, 2);
    assert_eq!(outcome.summary.tool_failures, 1);
}

#[test]
fn explorer_example_assertions_pass() {
    let trace = explorer_trace();
    let outcome = wf_debugger::assert::run_assertions(&trace);
    assert_eq!(outcome.failed, 0);
    assert_eq!(outcome.passed, trace.assertions.len());
}

#[test]
fn wrong_denial_text_reports_actual_value() {
    let mut trace = explorer_trace();
    trace
        .assertions
        .push(wf_debugger::assert::Assertion::ToolDeniedWith {
            tool: "write_file".to_string(),
            contains: "must be invoked through the general tool".to_string(),
        });
    let outcome = wf_debugger::assert::run_assertions(&trace);
    assert_eq!(outcome.failed, 1);
    let last = outcome.results.last().expect("result");
    assert_eq!(
        last.actual,
        Some(serde_json::Value::String(
            "Tool 'write_file' is not in the available tool set".to_string()
        ))
    );
}

#[test]
fn explorer_example_analysis_is_clean() {
    let trace = explorer_trace();
    let analysis = analyze_agent_trace(&trace, None);
    assert!(analysis.clean());
    assert_eq!(analysis.tool_calls, 2);
    assert_eq!(analysis.expected_denials, 1);
}

#[test]
fn successful_write_is_both_assertion_and_analysis_finding() {
    let mut trace = explorer_trace();
    let step = trace
        .steps
        .iter_mut()
        .find(|s| s.index == 1)
        .expect("step 1");
    let call = step
        .tool_calls
        .iter_mut()
        .find(|c| c.name == "write_file")
        .expect("write call");
    call.success = true;
    call.error = None;

    let assertions = wf_debugger::assert::run_assertions(&trace);
    assert!(assertions.failed > 0);

    let analysis = analyze_agent_trace(&trace, None);
    assert!(analysis
        .violations
        .iter()
        .any(|v| v.kind == VIOLATION_UNEXPECTED_SUCCESS && v.tool == "write_file"));
}
