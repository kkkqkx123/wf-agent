use crate::report::{FindingLevel, SectionReport};
use crate::trace::Trace;
use crate::traverse::walk;

/// Sub-execution analysis: validate the recorded hierarchy (depth, parent
/// references, root) against the actual nesting, and surface failed or
/// timed-out children to their parents.
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("subexec");
    let mut max_depth = 0;
    for visit in walk(trace) {
        if visit.depth == 0 && visit.step.children.is_empty() {
            continue;
        }
        if visit.depth > 0 {
            report.count("child_steps", 1);
            max_depth = max_depth.max(visit.depth as u64);
        }
        if let Some(declared) = visit.step.depth {
            if declared as usize != visit.depth {
                report.finding(
                    FindingLevel::Error,
                    &visit.path,
                    format!(
                        "step '{}' declares depth {declared} but nests at depth {}",
                        visit.step.node_id, visit.depth,
                    ),
                    Some(serde_json::Value::from(visit.depth)),
                    Some(serde_json::Value::from(declared)),
                );
            }
        }
        if visit.depth > 0 && visit.step.parent_exec_id.is_none() {
            report.count("missing_parent_ref", 1);
            report.finding(
                FindingLevel::Warning,
                &visit.path,
                format!(
                    "nested step '{}' carries no parent execution reference",
                    visit.step.node_id,
                ),
                None,
                None,
            );
        }
        if !visit.step.success && visit.depth > 0 {
            report.count("failed_children", 1);
            report.finding(
                FindingLevel::Warning,
                &visit.path,
                format!(
                    "child step '{}' failed{}",
                    visit.step.node_id,
                    visit
                        .step
                        .error
                        .as_deref()
                        .map(|error| format!(": {error}"))
                        .unwrap_or_default(),
                ),
                None,
                None,
            );
        }
        if visit.step.wait_for_child == Some(true)
            && visit.step.children.iter().any(|child| !child.success)
        {
            report.finding(
                FindingLevel::Warning,
                &visit.path,
                format!(
                    "parent '{}' waited for a child that failed",
                    visit.step.node_id,
                ),
                None,
                None,
            );
        }
    }
    report.count("max_depth", max_depth);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StepRecord, TraceKind, TRACE_SCHEMA_V1};
    use std::collections::HashMap;

    fn step(node_id: &str, depth: Option<u32>, success: bool) -> StepRecord {
        StepRecord {
            index: 0,
            node_id: node_id.to_string(),
            node_name: String::new(),
            node_type: "AGENT".to_string(),
            input: serde_json::Value::Null,
            result: serde_json::Value::Null,
            success,
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
            exec_id: Some(format!("exec-{node_id}")),
            parent_exec_id: None,
            root_exec_id: Some("exec-root".to_string()),
            depth,
            result_var: None,
            wait_for_child: None,
            child_timeout_ms: None,
            dialog_anchor: Some("head".to_string()),
            writeback: None,
            children: vec![],
        }
    }

    #[test]
    fn depth_mismatch_is_an_error() {
        let mut parent = step("root", Some(0), true);
        let mut child = step("child", Some(5), true);
        child.parent_exec_id = Some("exec-root".to_string());
        parent.children = vec![child];
        let trace = Trace {
            schema: TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Agent,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps: vec![parent],
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        };
        let report = analyze(&trace);
        assert_eq!(report.errors(), 1);
        assert_eq!(report.counts.get("max_depth"), Some(&1));
    }
}
