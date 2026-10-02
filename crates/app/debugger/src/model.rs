pub mod assertion;
pub mod observe;
pub mod report;
pub mod step;
pub mod trace;
pub mod traverse;
pub mod views_basic;
pub mod views_cost;
pub mod views_flow;
pub mod views_runtime;

pub use assertion::{AssertOutcome, Assertion, AssertionResult};
pub use observe::{
    all_message_diffs, message_diffs, variable_diffs, ChangeKind, MessageDiff, VariableDiff,
};
pub use report::{unify, Finding, FindingLevel, SectionReport, UnifiedReport};
pub use step::StepRecord;
pub use trace::{Trace, TraceKind, TRACE_SCHEMA_V1};
pub use traverse::{find_step, walk, StepVisit};
pub use views_basic::{ApprovalView, LlmCallView, MessageView, ToolCallView, VisibilityView};
pub use views_cost::{BudgetView, CompressionPhase, CompressionView};
pub use views_flow::{LoopRoundView, MergeBranchView, MergeView, RouteBranch, RouteDecisionPoint};
pub use views_runtime::{
    CheckpointMark, CheckpointSource, CheckpointTiming, HookFireView, InteractionView,
    InterruptionKind, InterruptionView, TriggerEventView, TriggerTemplateView,
};

pub const MAX_PAYLOAD_CHARS: usize = 4000;

pub fn cap_payload_text(text: &str) -> String {
    if text.len() <= MAX_PAYLOAD_CHARS {
        return text.to_string();
    }
    let mut end = MAX_PAYLOAD_CHARS;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated {} chars]", &text[..end], text.len() - end)
}

pub fn cap_json_value(value: &serde_json::Value) -> serde_json::Value {
    let text = serde_json::to_string(value).unwrap_or_default();
    if text.len() <= MAX_PAYLOAD_CHARS {
        return value.clone();
    }
    serde_json::Value::String(cap_payload_text(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_defaults_to_v1() {
        let trace: Trace = serde_json::from_str(r#"{"kind":"workflow","steps":[]}"#)
            .expect("minimal trace parses");
        assert_eq!(trace.schema, TRACE_SCHEMA_V1);
        assert!(trace.steps.is_empty());
    }

    #[test]
    fn payload_cap_truncates_long_text() {
        let long = "x".repeat(MAX_PAYLOAD_CHARS + 10);
        let capped = cap_payload_text(&long);
        assert!(capped.contains("truncated"));
    }
}
