pub mod assertion;
pub mod observe;
pub mod report;
pub mod step;
pub mod trace;
pub mod traverse;
pub mod util;
pub mod views_basic;
pub mod views_cost;
pub mod views_flow;
pub mod views_runtime;

pub use assertion::{AssertOutcome, Assertion, AssertionResult};
pub use observe::{
    all_message_diffs, message_diffs, variable_diffs, ChangeKind, MessageDiff, VariableDiff,
};
pub use report::{format_step_path, unify, Finding, FindingLevel, SectionReport, UnifiedReport};
pub use step::StepRecord;
pub use trace::{Trace, TraceKind, TRACE_SCHEMA_V1};
pub use traverse::{find_step, walk, StepVisit};
pub use util::{cap_json_value, cap_payload_text, MAX_PAYLOAD_CHARS};
pub use views_basic::{ApprovalView, LlmCallView, MessageView, ToolCallView, VisibilityView};
pub use views_cost::{BudgetView, CompressionPhase, CompressionView};
pub use views_flow::{LoopRoundView, MergeBranchView, MergeView, RouteBranch, RouteDecisionPoint};
pub use views_runtime::{
    CheckpointMark, CheckpointSource, CheckpointTiming, HookFireView, InteractionView,
    InterruptionKind, InterruptionView, TriggerEventView, TriggerTemplateView,
};

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
}
