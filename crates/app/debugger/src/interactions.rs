use crate::report::{FindingLevel, SectionReport};
use crate::trace::Trace;
use crate::traverse::walk;

/// Interaction analysis: lifecycle terminal states. Residual pending
/// interactions are errors; timeouts and discarded waiters are warnings.
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("interactions");
    for visit in walk(trace) {
        let Some(interaction) = visit.step.interaction.as_ref() else {
            continue;
        };
        report.count("total", 1);
        if interaction.pending {
            report.count("pending", 1);
            report.finding(
                FindingLevel::Error,
                &visit.path,
                format!(
                    "residual pending interaction '{}': {}",
                    interaction.interaction_id, interaction.prompt,
                ),
                Some(serde_json::Value::Bool(false)),
                Some(serde_json::Value::Bool(true)),
            );
        } else {
            report.count("completed", 1);
        }
        if interaction.timed_out {
            report.count("timed_out", 1);
            report.finding(
                FindingLevel::Warning,
                &visit.path,
                format!(
                    "interaction '{}' timed out{}",
                    interaction.interaction_id,
                    interaction
                        .wait_ms
                        .map(|wait| format!(" after {wait}ms"))
                        .unwrap_or_default(),
                ),
                None,
                interaction.response.clone(),
            );
        }
        if interaction.dropped {
            report.count("dropped", 1);
            report.finding(
                FindingLevel::Warning,
                &visit.path,
                format!(
                    "interaction '{}' was discarded while pending",
                    interaction.interaction_id,
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
    use crate::model::{InteractionView, StepRecord, TraceKind, TRACE_SCHEMA_V1};
    use std::collections::HashMap;

    fn trace_with(interaction: InteractionView) -> Trace {
        let step = StepRecord {
            index: 0,
            node_id: "n".to_string(),
            node_name: String::new(),
            node_type: "INTERACT".to_string(),
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
            interaction: Some(interaction),
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

    #[test]
    fn pending_interaction_is_an_error() {
        let trace = trace_with(InteractionView {
            interaction_id: "i1".to_string(),
            prompt: "approve?".to_string(),
            pending: true,
            response: None,
            timed_out: false,
            wait_ms: None,
            dropped: false,
        });
        let report = analyze(&trace);
        assert_eq!(report.errors(), 1);
    }

    #[test]
    fn completed_interaction_is_clean() {
        let trace = trace_with(InteractionView {
            interaction_id: "i1".to_string(),
            prompt: "approve?".to_string(),
            pending: false,
            response: Some(serde_json::json!("yes")),
            timed_out: false,
            wait_ms: Some(120),
            dropped: false,
        });
        let report = analyze(&trace);
        assert_eq!(report.errors(), 0);
        assert_eq!(report.counts.get("completed"), Some(&1));
    }
}
