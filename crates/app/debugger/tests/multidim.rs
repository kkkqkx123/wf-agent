use wf_debugger::model::Trace;

fn multidim_trace() -> Trace {
    let text = std::fs::read_to_string("examples/sample_multidim.json").expect("multidim trace");
    serde_json::from_str(&text).expect("multidim trace parses")
}

fn section<'a>(
    outcome: &'a wf_debugger::CheckOutcome,
    name: &str,
) -> &'a wf_debugger::SectionReport {
    outcome
        .report
        .sections
        .iter()
        .find(|section| section.name == name)
        .expect("section present")
}

#[test]
fn multidim_sample_passes_unified_check() {
    let trace = multidim_trace();
    let outcome = wf_debugger::run_check(&trace, None);
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(outcome.assertions.failed, 0);
    assert_eq!(outcome.assertions.passed, trace.assertions.len());
    assert_eq!(outcome.report.sections.len(), 7);
}

#[test]
fn multidim_sections_carry_expected_counts() {
    let trace = multidim_trace();
    let outcome = wf_debugger::run_check(&trace, None);
    assert_eq!(section(&outcome, "loops").counts.get("rounds"), Some(&2));
    assert_eq!(section(&outcome, "merges").counts.get("branches"), Some(&2));
    assert_eq!(
        section(&outcome, "interruptions").counts.get("pause"),
        Some(&1)
    );
    assert_eq!(
        section(&outcome, "checkpoints").counts.get("after_node"),
        Some(&1)
    );
    assert_eq!(
        section(&outcome, "interactions").counts.get("completed"),
        Some(&1)
    );
    assert_eq!(
        section(&outcome, "subexec").counts.get("max_depth"),
        Some(&1)
    );
    assert_eq!(
        section(&outcome, "cost").counts.get("total_tokens"),
        Some(&150)
    );
}

#[test]
fn walk_addresses_nested_child_by_path() {
    let trace = multidim_trace();
    let visits = wf_debugger::traverse::walk(&trace);
    assert_eq!(visits.len(), 7);
    let child = visits
        .iter()
        .find(|visit| visit.path == "5.0")
        .expect("child path");
    assert_eq!(child.depth, 1);
    assert_eq!(child.step.node_id, "agent-child");
}

#[test]
fn check_json_carries_report_and_assertion_results() {
    let trace = multidim_trace();
    let outcome = wf_debugger::run_check(&trace, None);
    let (_, code) = wf_debugger::runner::render_check(&trace, None, true, true);
    assert_eq!(code, 0);
    let text = wf_debugger::runner::render_check_json(&trace, &outcome);
    let payload: serde_json::Value = serde_json::from_str(&text).expect("check json parses");
    assert!(payload.get("report").is_some());
    assert!(payload.get("assertion_results").is_some());
    assert!(payload.get("steps").is_some());
}

#[test]
fn unrecovered_interruption_fails_check() {
    let mut trace = multidim_trace();
    let step = trace
        .steps
        .iter_mut()
        .find(|step| step.index == 3)
        .expect("step 3");
    let interruption = step.interruption.as_mut().expect("interruption");
    interruption.recovered = false;
    let outcome = wf_debugger::run_check(&trace, None);
    assert_eq!(outcome.exit_code, 1);
    assert!(outcome.report.failed());
}

#[test]
fn tight_budget_fails_check() {
    let mut trace = multidim_trace();
    trace.budget = Some(wf_debugger::model::BudgetView {
        source: Some("test".to_string()),
        limit_tokens: Some(10),
        limit_cost: None,
        warn_at: None,
    });
    let outcome = wf_debugger::run_check(&trace, None);
    assert_eq!(outcome.exit_code, 1);
}

#[test]
fn residual_pending_interaction_fails_check() {
    let mut trace = multidim_trace();
    let step = trace
        .steps
        .iter_mut()
        .find(|step| step.index == 4)
        .expect("step 4");
    let interaction = step.interaction.as_mut().expect("interaction");
    interaction.pending = true;
    let outcome = wf_debugger::run_check(&trace, None);
    assert_eq!(outcome.exit_code, 1);
}
