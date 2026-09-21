use serde::{Deserialize, Serialize};

use crate::model::{ToolCallView, Trace};
use crate::policy::{
    NOT_ACTIVATED, NOT_CALLABLE, NOT_IN_AVAILABLE_SET, VIA_GENERAL, BuiltinAgentPolicy,
    builtin_policy,
};

pub use crate::policy::{snapshot_meta, POLICY_SNAPSHOT_VERSION};

pub const VIOLATION_UNEXPECTED_SUCCESS: &str = "unexpected_success";
pub const VIOLATION_UNEXPECTED_DENIAL_TEXT: &str = "unexpected_denial_text";
pub const VIOLATION_MISSING_APPROVAL: &str = "missing_approval";
pub const VIOLATION_VISIBILITY_MISMATCH: &str = "visibility_mismatch";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentViolation {
    /// Walk path of the offending step, unique within the trace.
    #[serde(default)]
    pub path: String,
    pub tool: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentAnalysis {
    pub template_id: String,
    pub known_template: bool,
    pub tool_calls: usize,
    pub expected_denials: usize,
    pub violations: Vec<AgentViolation>,
}

impl AgentAnalysis {
    pub fn clean(&self) -> bool {
        self.known_template && self.violations.is_empty()
    }
}

/// Check an agent trace against the builtin policy for its template.
/// The template comes from the explicit override first, then the trace
/// identity. Expected policy denials are counted, never reported; only
/// deviations become violations.
pub fn analyze_agent_trace(trace: &Trace, template_override: Option<&str>) -> AgentAnalysis {
    let template_id = template_override
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| trace.agent_template.clone());
    let Some(policy) = builtin_policy(&template_id) else {
        return AgentAnalysis {
            template_id,
            known_template: false,
            ..Default::default()
        };
    };
    let mut analysis = AgentAnalysis {
        template_id,
        known_template: true,
        ..Default::default()
    };
    for visit in crate::traverse::walk(trace) {
        let step = visit.step;
        for call in &step.tool_calls {
            analysis.tool_calls += 1;
            check_call(policy, &visit.path, call, &mut analysis);
        }
        check_approvals(policy, &visit.path, step, &mut analysis);
    }
    analysis
}

fn check_call(
    policy: &BuiltinAgentPolicy,
    path: &str,
    call: &ToolCallView,
    analysis: &mut AgentAnalysis,
) {
    let tool = call.name.as_str();
    if !policy.available.contains(&tool) {
        if call.success {
            violate(
                analysis,
                path,
                tool,
                VIOLATION_UNEXPECTED_SUCCESS,
                &format!(
                    "tool '{tool}' succeeded but is outside the available set of {}",
                    policy.template_id
                ),
            );
        } else if is_expected_outside_denial(call) {
            analysis.expected_denials += 1;
        } else {
            violate(
                analysis,
                path,
                tool,
                VIOLATION_UNEXPECTED_DENIAL_TEXT,
                &format!(
                    "tool '{tool}' was denied without the exposure gate message: {}",
                    call.error.clone().unwrap_or_default()
                ),
            );
        }
        return;
    }
    if policy.discoverable.contains(&tool) {
        if call.success {
            violate(
                analysis,
                path,
                tool,
                VIOLATION_UNEXPECTED_SUCCESS,
                &format!(
                    "discoverable tool '{tool}' succeeded on the direct path; it must go through the general tool (or the run overrode the template tool lists)"
                ),
            );
        } else if is_expected_discoverable_denial(call) {
            analysis.expected_denials += 1;
        } else {
            violate(
                analysis,
                path,
                tool,
                VIOLATION_UNEXPECTED_DENIAL_TEXT,
                &format!(
                    "tool '{tool}' was denied without the discoverable gate message: {}",
                    call.error.clone().unwrap_or_default()
                ),
            );
        }
    }
}

fn check_approvals(
    policy: &BuiltinAgentPolicy,
    path: &str,
    step: &crate::model::StepRecord,
    analysis: &mut AgentAnalysis,
) {
    for call in &step.tool_calls {
        if !call.success || !policy.require_approval.contains(&call.name.as_str()) {
            continue;
        }
        let approved = step.approval.as_ref().is_some_and(|approval| {
            approval.tool_name == call.name && is_approval_decision(&approval.decision)
        });
        if !approved {
            violate(
                analysis,
                path,
                &call.name,
                VIOLATION_MISSING_APPROVAL,
                &format!(
                    "tool '{}' succeeded without an approving approval record",
                    call.name
                ),
            );
        }
    }
    check_visibility(policy, path, step, analysis);
}

fn check_visibility(
    policy: &BuiltinAgentPolicy,
    path: &str,
    step: &crate::model::StepRecord,
    analysis: &mut AgentAnalysis,
) {
    let Some(visibility) = step.visibility.as_ref() else {
        return;
    };
    for call in &step.tool_calls {
        if !call.success {
            continue;
        }
        // Outside-pool and discoverable tools already carry their own
        // findings; the visibility cross-check only covers tools the
        // policy expects on the direct path.
        if !policy.available.contains(&call.name.as_str())
            || policy.discoverable.contains(&call.name.as_str())
        {
            continue;
        }
        if !visibility.visible.contains(&call.name) {
            violate(
                analysis,
                path,
                &call.name,
                VIOLATION_VISIBILITY_MISMATCH,
                &format!(
                    "tool '{}' succeeded but is absent from the step visibility list",
                    call.name
                ),
            );
        }
    }
}

fn is_expected_outside_denial(call: &ToolCallView) -> bool {
    call.error
        .as_deref()
        .is_some_and(|error| error.contains(NOT_IN_AVAILABLE_SET) || error.contains(NOT_CALLABLE))
}

fn is_expected_discoverable_denial(call: &ToolCallView) -> bool {
    call.error
        .as_deref()
        .is_some_and(|error| error.contains(VIA_GENERAL) || error.contains(NOT_ACTIVATED))
}

fn is_approval_decision(decision: &str) -> bool {
    let normalized = decision.trim().to_lowercase();
    normalized.starts_with("approv") || normalized == "allow" || normalized.starts_with("auto")
}

fn violate(analysis: &mut AgentAnalysis, path: &str, tool: &str, kind: &str, detail: &str) {
    analysis.violations.push(AgentViolation {
        path: path.to_string(),
        tool: tool.to_string(),
        kind: kind.to_string(),
        detail: detail.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ApprovalView, StepRecord, TraceKind, VisibilityView};
    use crate::policy::{
        EXPLORER_AGENT_TEMPLATE_ID, MAIN_AGENT_TEMPLATE_ID, WORKER_AGENT_TEMPLATE_ID,
    };
    use std::collections::HashMap;

    fn step_with(index: usize, tool_calls: Vec<ToolCallView>) -> StepRecord {
        StepRecord {
            index,
            node_id: "agent-1".to_string(),
            node_name: String::new(),
            node_type: "AGENT".to_string(),
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
            tool_calls,
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
            children: vec![],
        }
    }

    fn denied_call(name: &str, error: &str) -> ToolCallView {
        ToolCallView {
            name: name.to_string(),
            call_id: "c1".to_string(),
            arguments: serde_json::Value::Null,
            result: None,
            error: Some(error.to_string()),
            duration_ms: None,
            success: false,
        }
    }

    fn trace_with(template: &str, steps: Vec<StepRecord>) -> Trace {
        Trace {
            schema: crate::model::TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Agent,
            graph_ref: String::new(),
            agent_template: template.to_string(),
            initial_variables: HashMap::new(),
            steps,
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        }
    }

    #[test]
    fn explorer_expected_denial_is_clean() {
        let trace = trace_with(
            EXPLORER_AGENT_TEMPLATE_ID,
            vec![step_with(
                0,
                vec![denied_call(
                    "write_file",
                    "Tool 'write_file' is not in the available tool set",
                )],
            )],
        );
        let analysis = analyze_agent_trace(&trace, None);
        assert!(analysis.clean());
        assert_eq!(analysis.expected_denials, 1);
    }

    #[test]
    fn explorer_successful_write_is_violation() {
        let trace = trace_with(
            EXPLORER_AGENT_TEMPLATE_ID,
            vec![step_with(
                0,
                vec![ToolCallView {
                    name: "write_file".to_string(),
                    call_id: "c1".to_string(),
                    arguments: serde_json::Value::Null,
                    result: None,
                    error: None,
                    duration_ms: None,
                    success: true,
                }],
            )],
        );
        let analysis = analyze_agent_trace(&trace, None);
        assert_eq!(analysis.violations.len(), 1);
        assert_eq!(analysis.violations[0].kind, VIOLATION_UNEXPECTED_SUCCESS);
    }

    #[test]
    fn denial_with_foreign_text_is_flagged() {
        let trace = trace_with(
            EXPLORER_AGENT_TEMPLATE_ID,
            vec![step_with(
                0,
                vec![denied_call("write_file", "connection reset by peer")],
            )],
        );
        let analysis = analyze_agent_trace(&trace, None);
        assert_eq!(analysis.violations.len(), 1);
        assert_eq!(
            analysis.violations[0].kind,
            VIOLATION_UNEXPECTED_DENIAL_TEXT
        );
    }

    #[test]
    fn main_direct_discoverable_call_must_use_general() {
        let trace = trace_with(
            MAIN_AGENT_TEMPLATE_ID,
            vec![step_with(
                0,
                vec![denied_call(
                    "write_file",
                    "Tool 'write_file' is discoverable and must be invoked through the general tool",
                )],
            )],
        );
        let analysis = analyze_agent_trace(&trace, None);
        assert!(analysis.clean());
        assert_eq!(analysis.expected_denials, 1);
    }

    #[test]
    fn shell_success_without_approval_reports_both_findings() {
        let mut step = step_with(
            0,
            vec![ToolCallView {
                name: "execute_command".to_string(),
                call_id: "c1".to_string(),
                arguments: serde_json::Value::Null,
                result: None,
                error: None,
                duration_ms: None,
                success: true,
            }],
        );
        step.visibility = Some(VisibilityView {
            visible: vec!["execute_command".to_string()],
            gated: vec![],
            hidden: vec![],
            discoverable: vec![],
        });
        let trace = trace_with(WORKER_AGENT_TEMPLATE_ID, vec![step]);
        let analysis = analyze_agent_trace(&trace, None);
        // Direct success of a discoverable tool is one finding; the missing
        // approval record is a second, independent finding.
        assert!(analysis
            .violations
            .iter()
            .any(|v| v.kind == VIOLATION_MISSING_APPROVAL));
        assert!(analysis
            .violations
            .iter()
            .any(|v| v.kind == VIOLATION_UNEXPECTED_SUCCESS));
    }

    #[test]
    fn approval_record_clears_only_the_approval_finding() {
        let mut step = step_with(
            0,
            vec![ToolCallView {
                name: "execute_command".to_string(),
                call_id: "c1".to_string(),
                arguments: serde_json::Value::Null,
                result: None,
                error: None,
                duration_ms: None,
                success: true,
            }],
        );
        step.approval = Some(ApprovalView {
            tool_name: "execute_command".to_string(),
            decision: "approve".to_string(),
            reason: None,
        });
        step.visibility = Some(VisibilityView {
            visible: vec!["read_file".to_string()],
            gated: vec![],
            hidden: vec![],
            discoverable: vec![],
        });
        let trace = trace_with(MAIN_AGENT_TEMPLATE_ID, vec![step]);
        let analysis = analyze_agent_trace(&trace, None);
        assert!(!analysis
            .violations
            .iter()
            .any(|v| v.kind == VIOLATION_MISSING_APPROVAL));
        assert!(analysis
            .violations
            .iter()
            .any(|v| v.kind == VIOLATION_UNEXPECTED_SUCCESS));
    }

    #[test]
    fn visible_tool_missing_from_visibility_list_is_flagged() {
        let mut step = step_with(
            0,
            vec![ToolCallView {
                name: "read_file".to_string(),
                call_id: "c1".to_string(),
                arguments: serde_json::Value::Null,
                result: None,
                error: None,
                duration_ms: None,
                success: true,
            }],
        );
        step.visibility = Some(VisibilityView {
            visible: vec!["glob_search".to_string()],
            gated: vec![],
            hidden: vec![],
            discoverable: vec![],
        });
        let trace = trace_with(EXPLORER_AGENT_TEMPLATE_ID, vec![step]);
        let analysis = analyze_agent_trace(&trace, None);
        assert_eq!(analysis.violations.len(), 1);
        assert_eq!(analysis.violations[0].kind, VIOLATION_VISIBILITY_MISMATCH);
    }

    #[test]
    fn unknown_template_reports_without_violations() {
        let trace = trace_with("custom-agent", vec![]);
        let analysis = analyze_agent_trace(&trace, None);
        assert!(!analysis.known_template);
        assert!(analysis.violations.is_empty());
    }

    #[test]
    fn override_wins_over_trace_identity() {
        let trace = trace_with("custom-agent", vec![]);
        let analysis = analyze_agent_trace(&trace, Some(EXPLORER_AGENT_TEMPLATE_ID));
        assert!(analysis.known_template);
        assert_eq!(analysis.template_id, EXPLORER_AGENT_TEMPLATE_ID);
    }
}
