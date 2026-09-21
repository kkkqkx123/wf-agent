use crate::report::{FindingLevel, SectionReport};
use crate::trace::Trace;
use crate::traverse::walk;

/// Checkpoint analysis: expected-vs-actual coverage by timing and trigger
/// source (policy strategy vs hook request).
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("checkpoints");
    for visit in walk(trace) {
        let Some(checkpoint) = visit.step.checkpoint.as_ref() else {
            continue;
        };
        report.count("total", 1);
        report.count(checkpoint.timing.label(), 1);
        match checkpoint.source {
            Some(crate::views::CheckpointSource::Policy) => report.count("from_policy", 1),
            Some(crate::views::CheckpointSource::Hook) => report.count("from_hook", 1),
            None => report.count("source_unknown", 1),
        }
        if checkpoint.checkpoint_id.is_none() {
            report.finding(
                FindingLevel::Warning,
                &visit.path,
                format!(
                    "checkpoint at {} carries no checkpoint id",
                    checkpoint.timing.label(),
                ),
                None,
                None,
            );
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CheckpointMark, CheckpointSource, CheckpointTiming, StepRecord, TraceKind, TRACE_SCHEMA_V1,
    };
    use std::collections::HashMap;

    #[test]
    fn missing_id_warns() {
        let step = StepRecord {
            index: 0,
            node_id: "n".to_string(),
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
            checkpoint: Some(CheckpointMark {
                timing: CheckpointTiming::AfterNode,
                checkpoint_id: None,
                source: Some(CheckpointSource::Policy),
            }),
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
        let trace = Trace {
            schema: TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps: vec![step],
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        };
        let report = analyze(&trace);
        assert_eq!(report.counts.get("after_node"), Some(&1));
        assert_eq!(report.counts.get("from_policy"), Some(&1));
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.errors(), 0);
    }
}
