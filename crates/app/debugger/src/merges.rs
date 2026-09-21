use crate::report::{FindingLevel, SectionReport};
use crate::trace::Trace;
use crate::traverse::walk;
use crate::views::MergeView;

/// Merge analysis: replay the join per branch so the outcome (success,
/// partial, failed) and the absorbed failures stay attributable.
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("merges");
    for visit in walk(trace) {
        let Some(merge) = visit.step.merge.as_ref() else {
            continue;
        };
        report.count("merges", 1);
        let failed_branches: Vec<&crate::views::MergeBranchView> = merge
            .branches
            .iter()
            .filter(|branch| !branch.success)
            .collect();
        report.count("branches", merge.branches.len() as u64);
        report.count("failed_branches", failed_branches.len() as u64);
        report.count("absorbed", merge.failures_absorbed as u64);
        let derived = derived_outcome(merge, failed_branches.len());
        if let Some(declared) = merge.outcome.as_deref() {
            if declared != derived {
                report.finding(
                    FindingLevel::Warning,
                    &visit.path,
                    format!(
                        "join '{}' declares outcome '{declared}' but branch records derive '{derived}'",
                        merge.join_node_id,
                    ),
                    Some(serde_json::Value::String(declared.to_string())),
                    Some(serde_json::Value::String(derived.to_string())),
                );
            }
        }
        for branch in failed_branches {
            report.finding(
                FindingLevel::Error,
                &visit.path,
                format!(
                    "join '{}' branch '{}' failed",
                    merge.join_node_id, branch.branch_id,
                ),
                None,
                branch.error.clone().map(serde_json::Value::String),
            );
        }
        if merge.branches.is_empty() && merge.branch_count > 0 {
            report.finding(
                FindingLevel::Warning,
                &visit.path,
                format!(
                    "join '{}' reports {} branches but carries no branch records",
                    merge.join_node_id, merge.branch_count,
                ),
                Some(serde_json::Value::from(merge.branch_count)),
                Some(serde_json::Value::from(0)),
            );
        }
    }
    report
}

fn derived_outcome(merge: &MergeView, failed_branches: usize) -> &'static str {
    if failed_branches == 0 {
        return "success";
    }
    if merge.failures_absorbed >= failed_branches {
        "partial"
    } else {
        "failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MergeBranchView, MergeView, StepRecord, TraceKind, TRACE_SCHEMA_V1};
    use std::collections::HashMap;

    fn trace_with_merge(merge: MergeView) -> Trace {
        let step = StepRecord {
            index: 0,
            node_id: "join".to_string(),
            node_name: String::new(),
            node_type: "JOIN".to_string(),
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
            merge: Some(merge),
            interruption: None,
            checkpoint: None,
            interaction: None,
            hooks_fired: vec![],
            triggers_seen: vec![],
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
        };
        Trace {
            schema: TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps: vec![step],
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        }
    }

    fn branch(id: &str, success: bool) -> MergeBranchView {
        MergeBranchView {
            branch_id: id.to_string(),
            success,
            output: serde_json::Value::Null,
            error: if success {
                None
            } else {
                Some("branch exploded".to_string())
            },
            variables: HashMap::new(),
        }
    }

    #[test]
    fn failed_branch_is_an_error_with_actual_text() {
        let trace = trace_with_merge(MergeView {
            join_node_id: "join".to_string(),
            branch_count: 2,
            failures_absorbed: 0,
            summary: serde_json::Value::Null,
            outcome: Some("failed".to_string()),
            policy: Some("fail_fast".to_string()),
            branches: vec![branch("a", true), branch("b", false)],
        });
        let report = analyze(&trace);
        assert_eq!(report.counts.get("failed_branches"), Some(&1));
        assert_eq!(report.errors(), 1);
        assert!(report.findings[0]
            .actual
            .as_ref()
            .is_some_and(|actual| actual.as_str() == Some("branch exploded")));
    }

    #[test]
    fn declared_outcome_mismatch_warns() {
        let trace = trace_with_merge(MergeView {
            join_node_id: "join".to_string(),
            branch_count: 1,
            failures_absorbed: 0,
            summary: serde_json::Value::Null,
            outcome: Some("success".to_string()),
            policy: None,
            branches: vec![branch("a", false)],
        });
        let report = analyze(&trace);
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Warning
                && finding.message.contains("declares outcome")));
    }
}
