use crate::step::StepRecord;
use crate::trace::Trace;

/// One step visited by the single depth-first walk. Every analyzer, the
/// replay pass, the timeline builder and the assertion engine consume this
/// walk so nested steps are never counted, flattened or ignored ad hoc.
pub struct StepVisit<'a> {
    /// Dot-joined child positions, e.g. `1` for a top-level step and `1.0`
    /// for its first nested child. Unique within a trace.
    pub path: String,
    pub depth: usize,
    pub step: &'a StepRecord,
}

pub fn walk(trace: &Trace) -> Vec<StepVisit<'_>> {
    let mut out = Vec::new();
    for (position, step) in trace.steps.iter().enumerate() {
        walk_step(step, &position.to_string(), 0, &mut out);
    }
    out
}

fn walk_step<'a>(step: &'a StepRecord, path: &str, depth: usize, out: &mut Vec<StepVisit<'a>>) {
    out.push(StepVisit {
        path: path.to_string(),
        depth,
        step,
    });
    for (position, child) in step.children.iter().enumerate() {
        walk_step(child, &format!("{path}.{position}"), depth + 1, out);
    }
}

/// Find a top-level step by its recorded index. Nested steps are addressed
/// through [`walk`] paths instead.
pub fn find_step(trace: &Trace, index: usize) -> Option<&StepRecord> {
    trace.steps.iter().find(|step| step.index == index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TRACE_SCHEMA_V1;
    use crate::model::TraceKind;
    use std::collections::HashMap;

    fn step(index: usize, children: Vec<StepRecord>) -> StepRecord {
        StepRecord {
            index,
            node_id: format!("n{index}"),
            node_name: String::new(),
            node_type: "SCRIPT".to_string(),
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
            exec_id: None,
            parent_exec_id: None,
            root_exec_id: None,
            depth: None,
            result_var: None,
            wait_for_child: None,
            child_timeout_ms: None,
            dialog_anchor: None,
            writeback: None,
            children,
        }
    }

    #[test]
    fn walk_visits_nested_steps_with_unique_paths() {
        let trace = Trace {
            schema: TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps: vec![step(0, vec![step(0, vec![])]), step(1, vec![])],
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        };
        let visits = walk(&trace);
        let paths: Vec<&str> = visits.iter().map(|visit| visit.path.as_str()).collect();
        assert_eq!(paths, vec!["0", "0.0", "1"]);
        assert_eq!(visits[1].depth, 1);
    }
}
