pub use crate::step::StepRecord;
pub use crate::trace::{Trace, TraceKind, TRACE_SCHEMA_V1};
pub use crate::views::{
    ApprovalView, BudgetView, CheckpointMark, CheckpointSource, CheckpointTiming, HookFireView,
    InteractionView, InterruptionKind, InterruptionView, LlmCallView, LoopRoundView,
    MergeBranchView, MergeView, MessageView, RouteBranch, RouteDecisionPoint, ToolCallView,
    TriggerEventView, TriggerTemplateView, VisibilityView,
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
