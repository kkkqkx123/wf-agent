use std::collections::{HashMap, HashSet};

use crate::model::report::{FindingLevel, SectionReport};
use crate::model::trace::Trace;
use crate::model::traverse::walk;

/// Stage order of the `@standard/spec-workflow` planning pipeline. The analyzer
/// maps visited node ids to these stages and checks that the trace follows the
/// declared order; unknown node ids are ignored so partial or embedded
/// traces stay analyzable.
const STAGE_ORDER: &[&str] = &[
    "specify",
    "spec_gate",
    "plan",
    "plan_gate",
    "tasks",
];

fn stage_of(node_id: &str) -> Option<&'static str> {
    match node_id {
        "spec_writer" => Some("specify"),
        "spec_gate" => Some("spec_gate"),
        "plan_writer" => Some("plan"),
        "plan_gate" => Some("plan_gate"),
        "task_decomposer" | "tasks_route" => Some("tasks"),
        _ => None,
    }
}

/// Spec planning pipeline analysis: stage ordering and gate verification.
/// Traces without any spec stage node report an empty section so non-spec
/// workflows stay green.
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("spec");
    let mut first_seen: HashMap<&str, String> = HashMap::new();
    let mut first_order: Vec<&str> = Vec::new();
    let mut visited: HashSet<&str> = HashSet::new();

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
        let trace = trace_with(vec![
            step("spec_writer"),
            gate,
            step("plan_writer"),
            step("task_decomposer"),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.errors(), 0);
        assert_eq!(report.counts.get("stages"), Some(&4));
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
    fn out_of_order_stage_is_an_error() {
        let trace = trace_with(vec![step("plan_writer"), step("spec_writer")]);
        let report = analyze(&trace);
        assert_eq!(report.errors(), 1);
    }
}
