use std::collections::{HashMap, HashSet};

use crate::model::report::{FindingLevel, SectionReport};
use crate::model::trace::Trace;
use crate::model::traverse::walk;

/// Stage order of the `@standard/spec-workflow` pipeline. The analyzer maps
/// visited node ids to these stages and checks that the trace follows the
/// declared order; unknown node ids are ignored so partial or embedded
/// traces stay analyzable.
const STAGE_ORDER: &[&str] = &[
    "specify",
    "spec_gate",
    "plan",
    "plan_gate",
    "tasks",
    "implement",
    "converge",
    "archive",
];

fn stage_of(node_id: &str) -> Option<&'static str> {
    match node_id {
        "spec_writer" => Some("specify"),
        "spec_gate" => Some("spec_gate"),
        "plan_writer" => Some("plan"),
        "plan_gate" => Some("plan_gate"),
        "task_decomposer" | "tasks_route" => Some("tasks"),
        "loop_start" | "implementer" | "goal_delegate" | "spec_reviewer" | "loop_end" => {
            Some("implement")
        }
        "converge_check" | "converge_route" => Some("converge"),
        "archive" => Some("archive"),
        _ => None,
    }
}

fn flag_is_true(
    before: &HashMap<String, serde_json::Value>,
    after: &HashMap<String, serde_json::Value>,
    key: &str,
) -> bool {
    before
        .get(key)
        .or_else(|| after.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Spec pipeline analysis: stage ordering, gate verification, and readiness
/// preconditions. Traces without any spec stage node report an empty section
/// so non-spec workflows stay green.
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("spec");
    let mut first_seen: HashMap<&str, String> = HashMap::new();
    let mut first_order: Vec<&str> = Vec::new();
    let mut visited: HashSet<&str> = HashSet::new();
    let mut tasks_ready_seen = false;
    let mut converged_seen = false;

    for visit in walk(trace) {
        let step = visit.step;
        let Some(stage) = stage_of(&step.node_id) else {
            continue;
        };
        visited.insert(stage);
        if !first_seen.contains_key(stage) {
            first_seen.insert(stage, visit.path.clone());
            first_order.push(stage);
        }
        report.count("stage_visits", 1);
        if flag_is_true(&step.variable_before, &step.variable_after, "tasksReady") {
            tasks_ready_seen = true;
        }
        if flag_is_true(&step.variable_before, &step.variable_after, "converged") {
            converged_seen = true;
        }

        if matches!(stage, "spec_gate" | "plan_gate") {
            report.count("gates", 1);
            match step.interaction.as_ref() {
                None => report.finding(
                    FindingLevel::Warning,
                    &visit.path,
                    format!(
                        "gate '{}' completed without an interaction record; approval is unverified",
                        step.node_id,
                    ),
                    None,
                    None,
                ),
                Some(interaction) if interaction.pending => {
                    report.count("gates_pending", 1);
                    report.finding(
                        FindingLevel::Error,
                        &visit.path,
                        format!(
                            "gate '{}' left pending: '{}'",
                            step.node_id, interaction.interaction_id,
                        ),
                        Some(serde_json::Value::Bool(false)),
                        Some(serde_json::Value::Bool(true)),
                    );
                }
                Some(_) => report.count("gates_verified", 1),
            }
        }
    }

    if visited.is_empty() {
        return report;
    }
    report.count("stages", visited.len() as u64);

    // First-seen order must follow the pipeline. Later revisits of earlier
    // stages are legitimate (gate rejections, loop rounds) and are not
    // checked here.
    let mut last_rank = 0;
    for stage in &first_order {
        let rank = STAGE_ORDER.iter().position(|s| s == stage).unwrap_or(0);
        if rank < last_rank {
            report.finding(
                FindingLevel::Error,
                first_seen.get(stage).map(String::as_str).unwrap_or_default(),
                format!("spec stage '{stage}' ran out of pipeline order"),
                None,
                None,
            );
            break;
        }
        last_rank = rank;
    }

    if visited.contains("implement") && !tasks_ready_seen {
        report.finding(
            FindingLevel::Error,
            first_seen.get("implement").map(String::as_str).unwrap_or_default(),
            "implement loop entered before tasksReady was set".to_string(),
            Some(serde_json::Value::Bool(true)),
            Some(serde_json::Value::Bool(false)),
        );
    }
    if visited.contains("archive") && !converged_seen {
        report.finding(
            FindingLevel::Error,
            first_seen.get("archive").map(String::as_str).unwrap_or_default(),
            "archive reached without convergence".to_string(),
            Some(serde_json::Value::Bool(true)),
            Some(serde_json::Value::Bool(false)),
        );
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{InteractionView, SectionReport, StepRecord, TraceKind, TRACE_SCHEMA_V1};
    use std::collections::HashMap;

    fn warnings(report: &SectionReport) -> usize {
        use crate::model::FindingLevel;
        report
            .findings
            .iter()
            .filter(|finding| finding.level == FindingLevel::Warning)
            .count()
    }

    fn step(node_id: &str) -> StepRecord {
        StepRecord {
            index: 0,
            node_id: node_id.to_string(),
            node_name: String::new(),
            node_type: String::new(),
            input: serde_json::Value::Null,
            result: serde_json::Value::Null,
            success: true,
            error: None,
            error_kind: None,
            retryable: None,
            recovery_hint: None,
            branch_id: None,
            route_target: None,
            start_time: None,
            end_time: None,
            variable_before: HashMap::new(),
            variable_after: HashMap::new(),
            messages_before: HashMap::new(),
            messages_after: HashMap::new(),
            tool_calls: vec![],
            llm_calls: vec![],
            approval: None,
            visibility: None,
            loop_round: None,
            merge: None,
            interruption: None,
            checkpoint: None,
            interaction: None,
            hooks_fired: vec![],
            triggers_seen: vec![],
            compressions: vec![],
            exec_id: None,
            parent_exec_id: None,
            root_exec_id: None,
            depth: None,
            result_var: None,
            wait_for_child: None,
            child_timeout_ms: None,
            dialog_anchor: None,
            writeback: None,
            children: vec![],
        }
    }

    fn trace_with(steps: Vec<StepRecord>) -> Trace {
        Trace {
            schema: TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: "@standard/spec-workflow".to_string(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps,
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        }
    }

    fn interaction(id: &str, pending: bool) -> InteractionView {
        InteractionView {
            interaction_id: id.to_string(),
            prompt: "approve?".to_string(),
            pending,
            response: if pending {
                None
            } else {
                Some(serde_json::json!("approve"))
            },
            timed_out: false,
            wait_ms: None,
            dropped: false,
        }
    }

    #[test]
    fn non_spec_trace_reports_empty_section() {
        let trace = trace_with(vec![step("task_planner")]);
        let report = analyze(&trace);
        assert_eq!(report.errors(), 0);
        assert!(!report.counts.contains_key("stages"));
    }

    #[test]
    fn ordered_gated_trace_is_clean() {
        let mut gate = step("spec_gate");
        gate.interaction = Some(interaction("i1", false));
        let mut decompose = step("task_decomposer");
        decompose.variable_after.insert(
            "tasksReady".to_string(),
            serde_json::Value::Bool(true),
        );
        let trace = trace_with(vec![
            step("spec_writer"),
            gate,
            step("plan_writer"),
            decompose,
            step("implementer"),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.errors(), 0);
        assert_eq!(report.counts.get("stages"), Some(&5));
        assert_eq!(report.counts.get("gates_verified"), Some(&1));
    }

    #[test]
    fn gate_without_interaction_warns() {
        let trace = trace_with(vec![step("spec_writer"), step("spec_gate")]);
        let report = analyze(&trace);
        assert_eq!(report.errors(), 0);
        assert_eq!(report.counts.get("gates"), Some(&1));
        assert_eq!(warnings(&report), 1);
    }

    #[test]
    fn implement_before_tasks_ready_is_an_error() {
        let trace = trace_with(vec![step("spec_writer"), step("implementer")]);
        let report = analyze(&trace);
        assert_eq!(report.errors(), 1);
    }

    #[test]
    fn archive_without_convergence_is_an_error() {
        let trace = trace_with(vec![step("spec_writer"), step("archive")]);
        let report = analyze(&trace);
        assert_eq!(report.errors(), 1);
    }

    #[test]
    fn out_of_order_stage_is_an_error() {
        let trace = trace_with(vec![step("plan_writer"), step("spec_writer")]);
        let report = analyze(&trace);
        assert_eq!(report.errors(), 1);
    }
}
