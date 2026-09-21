use serde::{Deserialize, Serialize};

use crate::model::Trace;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEntry {
    pub at: String,
    pub kind: String,
    pub label: String,
}

pub fn build_timeline(trace: &Trace) -> Vec<TimelineEntry> {
    let mut entries = Vec::new();
    for step in &trace.steps {
        entries.push(TimelineEntry {
            at: format!("step-{}", step.index),
            kind: "node".to_string(),
            label: format!("{} {} ({})", step.node_type, step.node_name, step.node_id),
        });
        for hook in &step.hooks_fired {
            entries.push(TimelineEntry {
                at: format!("step-{}", step.index),
                kind: "hook".to_string(),
                label: format!("{} {}", hook.hook_type, hook.hook_id),
            });
        }
        for trigger in &step.triggers_seen {
            entries.push(TimelineEntry {
                at: format!("step-{}", step.index),
                kind: "trigger".to_string(),
                label: format!("{} matched={}", trigger.template_name, trigger.matched),
            });
        }
        if let Some(checkpoint) = step.checkpoint.as_ref() {
            entries.push(TimelineEntry {
                at: format!("step-{}", step.index),
                kind: "checkpoint".to_string(),
                label: checkpoint.timing.clone(),
            });
        }
        for child in &step.children {
            entries.push(TimelineEntry {
                at: format!("step-{}/child", step.index),
                kind: "nested".to_string(),
                label: format!("{} ({})", child.node_type, child.node_id),
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
        };
        assert!(build_timeline(&trace).is_empty());
    }
}
