use crate::assert::AssertOutcome;
use crate::model::{cap_payload_text, Trace};
use crate::replay::ReplayOutcome;
use crate::traverse::walk;

pub fn format_text(trace: &Trace, outcome: &ReplayOutcome, no_color: bool) -> String {
    let mut buf = String::new();
    buf.push_str(&format!(
        "trace kind={:?} steps={} failures={} tools={}/{} llm_calls={} tokens={}\n",
        trace.kind,
        outcome.summary.steps,
        outcome.summary.failures,
        outcome.summary.tool_failures,
        outcome.summary.tool_calls,
        outcome.summary.llm_calls,
        outcome.summary.total_tokens,
    ));
    let visits = walk(trace);
    for (step, visit) in outcome.steps.iter().zip(visits.iter()) {
        let record = visit.step;
        let status = if step.success { "ok" } else { "FAIL" };
        let decorated = if step.success || no_color {
            status.to_string()
        } else {
            format!("\u{1b}[31m{status}\u{1b}[0m")
        };
        buf.push_str(&format!(
            "[step {}] {} {} ({}) {decorated}",
            visit.path, step.node_type, record.node_name, step.node_id
        ));
        if let Some(duration) = step.duration_ms {
            buf.push_str(&format!(" {duration}ms"));
        }
        if let Some(target) = step.route_target.as_deref() {
            buf.push_str(&format!(" route->{target}"));
        }
        buf.push('\n');
        if !step.success {
            if let Some(error) = step.error.as_deref() {
                buf.push_str(&format!("  error: {}\n", cap_payload_text(error)));
            }
            if let Some(kind) = record.error_kind.as_deref() {
                buf.push_str(&format!("  error_kind: {kind}"));
                if let Some(retryable) = record.retryable {
                    buf.push_str(&format!(" retryable={retryable}"));
                }
                buf.push('\n');
            }
            if let Some(hint) = record.recovery_hint.as_deref() {
                buf.push_str(&format!("  recovery: {hint}\n"));
            }
        }
        for diff in &step.variable_diffs {
            buf.push_str(&format!("  var {}: {}\n", diff.key, diff.kind_label()));
        }
        for diff in &step.message_diffs {
            buf.push_str(&format!(
                "  msg {}:{} {:?}\n",
                diff.context_id, diff.message_id, diff.kind
            ));
        }
        for tool in &record.tool_calls {
            let tool_status = if tool.success { "ok" } else { "FAIL" };
            buf.push_str(&format!("  tool {} {tool_status}", tool.name));
            if let Some(error) = tool.error.as_deref() {
                buf.push_str(&format!(" error={}", cap_payload_text(error)));
            }
            buf.push('\n');
        }
        for call in &record.llm_calls {
            buf.push_str(&format!(
                "  llm {} tokens={} cost={}\n",
                call.model
                    .clone()
                    .unwrap_or_else(|| call.profile_id.clone()),
                call.effective_total(),
                call.total_cost
                    .map(|cost| format!("{cost:.4}"))
                    .unwrap_or_else(|| "-".to_string()),
            ));
        }
        if let Some(round) = record.loop_round.as_ref() {
            buf.push_str(&format!(
                "  loop '{}' round {}{}\n",
                round.loop_id,
                round.round,
                if round.failed { " FAILED" } else { "" },
            ));
        }
        if let Some(merge) = record.merge.as_ref() {
            buf.push_str(&format!(
                "  merge '{}' outcome={} branches={}\n",
                merge.join_node_id,
                merge.outcome.clone().unwrap_or_else(|| "?".to_string()),
                merge.branches.len(),
            ));
        }
        if step.hook_vetoes > 0 {
            buf.push_str(&format!("  hooks: {} veto(es)\n", step.hook_vetoes));
        }
        if let Some(interruption) = record.interruption.as_ref() {
            buf.push_str(&format!(
                "  interruption: {} recovered={}\n",
                interruption.kind.label(),
                interruption.recovered,
            ));
        }
        if let Some(checkpoint) = record.checkpoint.as_ref() {
            buf.push_str(&format!(
                "  checkpoint: {}{}\n",
                checkpoint.timing.label(),
                checkpoint
                    .checkpoint_id
                    .as_deref()
                    .map(|id| format!(" id={id}"))
                    .unwrap_or_default(),
            ));
        }
        if let Some(interaction) = record.interaction.as_ref() {
            buf.push_str(&format!(
                "  interaction '{}' pending={} timed_out={}\n",
                interaction.interaction_id, interaction.pending, interaction.timed_out,
            ));
        }
    }
    buf
}

pub fn format_json(trace: &Trace, outcome: &ReplayOutcome, assertions: &AssertOutcome) -> String {
    let payload = serde_json::json!({
        "schema": trace.schema,
        "kind": trace.kind,
        "summary": outcome.summary,
        "steps": outcome.steps,
        "assertions": trace.assertions,
        "assertion_results": assertions.results,
        "assertions_passed": assertions.passed,
        "assertions_failed": assertions.failed,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| String::from("{}"))
}

trait DiffKindLabel {
    fn kind_label(&self) -> &'static str;
}

impl DiffKindLabel for crate::observe::VariableDiff {
    fn kind_label(&self) -> &'static str {
        match self.kind {
            crate::observe::ChangeKind::Added => "added",
            crate::observe::ChangeKind::Removed => "removed",
            crate::observe::ChangeKind::Modified => "modified",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_header_mentions_steps() {
        let trace = Trace {
            schema: crate::model::TRACE_SCHEMA_V1.to_string(),
            kind: crate::model::TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: Default::default(),
            steps: vec![],
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        };
        let outcome = ReplayOutcome {
            steps: vec![],
            summary: Default::default(),
        };
        let text = format_text(&trace, &outcome, true);
        assert!(text.contains("steps=0"));
    }

    #[test]
    fn json_carries_assertion_results() {
        let trace = Trace {
            schema: crate::model::TRACE_SCHEMA_V1.to_string(),
            kind: crate::model::TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: Default::default(),
            steps: vec![],
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        };
        let outcome = ReplayOutcome {
            steps: vec![],
            summary: Default::default(),
        };
        let assertions = AssertOutcome::default();
        let payload: serde_json::Value =
            serde_json::from_str(&format_json(&trace, &outcome, &assertions))
                .expect("json renders");
        assert!(payload.get("assertion_results").is_some());
        assert!(payload.get("steps").is_some());
    }
}
