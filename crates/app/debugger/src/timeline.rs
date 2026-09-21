use serde::{Deserialize, Serialize};

use crate::model::Trace;
use crate::traverse::walk;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEntry {
    pub at: String,
    pub kind: String,
    pub label: String,
}

/// Fixed event order per step so timeline and replay agree on one trace:
/// node, loop round, hooks, triggers, model calls, interruption, checkpoint,
/// interaction, merge.
pub fn build_timeline(trace: &Trace) -> Vec<TimelineEntry> {
    let mut entries = Vec::new();
    for visit in walk(trace) {
        let step = visit.step;
        let at = format!("step-{}", visit.path);
        entries.push(TimelineEntry {
            at: at.clone(),
            kind: "node".to_string(),
            label: format!("{} {} ({})", step.node_type, step.node_name, step.node_id),
        });
        if let Some(round) = step.loop_round.as_ref() {
            entries.push(TimelineEntry {
                at: at.clone(),
                kind: "loop".to_string(),
                label: format!(
                    "{} round {} failed={}",
                    round.loop_id, round.round, round.failed
                ),
            });
        }
        for hook in &step.hooks_fired {
            entries.push(TimelineEntry {
                at: at.clone(),
                kind: "hook".to_string(),
                label: format!("{} {}", hook.hook_type, hook.hook_id),
            });
        }
        for trigger in &step.triggers_seen {
            entries.push(TimelineEntry {
                at: at.clone(),
                kind: "trigger".to_string(),
                label: format!("{} matched={}", trigger.template_name, trigger.matched),
            });
        }
        for call in &step.llm_calls {
            entries.push(TimelineEntry {
                at: at.clone(),
                kind: "llm".to_string(),
                label: format!(
                    "{} tokens={}",
                    call.model
                        .clone()
                        .unwrap_or_else(|| call.profile_id.clone()),
                    call.effective_total(),
                ),
            });
        }
        if let Some(interruption) = step.interruption.as_ref() {
            entries.push(TimelineEntry {
                at: at.clone(),
                kind: "interruption".to_string(),
                label: format!(
                    "{} recovered={}",
                    interruption.kind.label(),
                    interruption.recovered
                ),
            });
        }
        if let Some(checkpoint) = step.checkpoint.as_ref() {
            entries.push(TimelineEntry {
                at: at.clone(),
                kind: "checkpoint".to_string(),
                label: checkpoint.timing.label().to_string(),
            });
        }
        if let Some(interaction) = step.interaction.as_ref() {
            entries.push(TimelineEntry {
                at: at.clone(),
                kind: "interaction".to_string(),
                label: format!(
                    "{} pending={} timed_out={}",
                    interaction.interaction_id, interaction.pending, interaction.timed_out,
                ),
            });
        }
        if let Some(merge) = step.merge.as_ref() {
            entries.push(TimelineEntry {
                at,
                kind: "merge".to_string(),
                label: format!("{} branches={}", merge.join_node_id, merge.branches.len(),),
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_trace_has_empty_timeline() {
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
        assert!(build_timeline(&trace).is_empty());
    }
}
