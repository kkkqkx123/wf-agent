use serde::{Deserialize, Serialize};

use crate::model::Trace;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assertion {
    StepSuccess {
        step: usize,
    },
    StepResult {
        step: usize,
        expected: serde_json::Value,
    },
    Variable {
        step: usize,
        key: String,
        expected: serde_json::Value,
    },
    Route {
        step: usize,
        expected_target: String,
    },
    HookOutcome {
        hook_type: String,
        expected_veto: bool,
    },
    TriggerMatched {
        template: String,
        expected: bool,
    },
    ToolNeverCalled {
        tool: String,
    },
    ToolNeverSucceeded {
        tool: String,
    },
    ToolDeniedWith {
        tool: String,
        contains: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssertionResult {
    pub name: String,
    pub pass: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<serde_json::Value>,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AssertOutcome {
    pub results: Vec<AssertionResult>,
    pub passed: usize,
    pub failed: usize,
}

impl AssertOutcome {
    pub fn failed(&self) -> bool {
        self.failed > 0
    }
}

pub fn run_assertions(trace: &Trace) -> AssertOutcome {
    let mut results = Vec::new();
    for (position, assertion) in trace.assertions.iter().enumerate() {
        results.push(evaluate_one(trace, position, assertion));
    }
    let passed = results.iter().filter(|r| r.pass).count();
    let failed = results.len() - passed;
    AssertOutcome {
        results,
        passed,
        failed,
    }
}

fn evaluate_one(trace: &Trace, position: usize, assertion: &Assertion) -> AssertionResult {
    let name = format!("assert-{position}");
    match assertion {
        Assertion::StepSuccess { step } => {
            let Some(record) = trace.steps.iter().find(|s| s.index == *step) else {
                return fail(name, None, None, "step not found");
            };
            if record.success {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::Bool(true)),
                    Some(serde_json::Value::Bool(false)),
                    &format!(
                        "step {} failed: {}",
                        step,
                        record.error.clone().unwrap_or_default()
                    ),
                )
            }
        }
        Assertion::StepResult { step, expected } => {
            let Some(record) = trace.steps.iter().find(|s| s.index == *step) else {
                return fail(name, None, None, "step not found");
            };
            if &record.result == expected {
                pass(name)
            } else {
                fail(
                    name,
                    Some(expected.clone()),
                    Some(record.result.clone()),
                    "step result mismatch",
                )
            }
        }
        Assertion::Variable {
            step,
            key,
            expected,
        } => {
            let Some(record) = trace.steps.iter().find(|s| s.index == *step) else {
                return fail(name, None, None, "step not found");
            };
            let actual = record.variable_after.get(key);
            if actual == Some(expected) {
                pass(name)
            } else {
                fail(
                    name,
                    Some(expected.clone()),
                    actual.cloned(),
                    &format!("variable {key} mismatch"),
                )
            }
        }
        Assertion::Route {
            step,
            expected_target,
        } => {
            let Some(record) = trace.steps.iter().find(|s| s.index == *step) else {
                return fail(name, None, None, "step not found");
            };
            if record.route_target.as_deref() == Some(expected_target.as_str()) {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::String(expected_target.clone())),
                    record
                        .route_target
                        .clone()
                        .map(serde_json::Value::String)
                        .or(Some(serde_json::Value::Null)),
                    "route target mismatch",
                )
            }
        }
        Assertion::HookOutcome {
            hook_type,
            expected_veto,
        } => {
            let vetoes = trace
                .steps
                .iter()
                .flat_map(|s| s.hooks_fired.iter())
                .filter(|h| &h.hook_type == hook_type)
                .filter(|h| h.veto_reason.is_some() || h.outcome == "veto")
                .count();
            let actual_veto = vetoes > 0;
            if &actual_veto == expected_veto {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::Bool(*expected_veto)),
                    Some(serde_json::Value::Bool(actual_veto)),
                    "hook veto mismatch",
                )
            }
        }
        Assertion::TriggerMatched { template, expected } => {
            let actual = trace
                .steps
                .iter()
                .flat_map(|s| s.triggers_seen.iter())
                .find(|t| &t.template_name == template)
                .map(|t| t.matched);
            match actual {
                Some(matched) if &matched == expected => pass(name),
                Some(matched) => fail(
                    name,
                    Some(serde_json::Value::Bool(*expected)),
                    Some(serde_json::Value::Bool(matched)),
                    "trigger match mismatch",
                ),
                None => fail(name, None, None, "trigger template not seen"),
            }
        }
        Assertion::ToolNeverCalled { tool } => {
            let calls = tool_calls(trace, tool);
            if calls.is_empty() {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::Bool(false)),
                    Some(serde_json::Value::Bool(true)),
                    &format!("tool {tool} was called {} time(s)", calls.len()),
                )
            }
        }
        Assertion::ToolNeverSucceeded { tool } => {
            let succeeded = tool_calls(trace, tool).iter().filter(|c| c.success).count();
            if succeeded == 0 {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::from(0)),
                    Some(serde_json::Value::from(succeeded)),
                    &format!("tool {tool} succeeded unexpectedly"),
                )
            }
        }
        Assertion::ToolDeniedWith { tool, contains } => {
            let calls = tool_calls(trace, tool);
            if calls.is_empty() {
                return fail(name, None, None, &format!("tool {tool} was never called"));
            }
            let denied: Vec<&crate::model::ToolCallView> =
                calls.iter().filter(|c| !c.success).copied().collect();
            if denied.is_empty() {
                return fail(
                    name,
                    Some(serde_json::Value::Bool(false)),
                    Some(serde_json::Value::Bool(true)),
                    &format!("tool {tool} was called but never denied"),
                );
            }
            let matched = denied.iter().any(|c| {
                c.error
                    .as_deref()
                    .is_some_and(|error| error.contains(contains.as_str()))
            });
            if matched {
                pass(name)
            } else {
                let actual = denied
                    .iter()
                    .filter_map(|c| c.error.clone())
                    .next()
                    .map(serde_json::Value::String)
                    .or(Some(serde_json::Value::Null));
                fail(
                    name,
                    Some(serde_json::Value::String(contains.clone())),
                    actual,
                    &format!("tool {tool} denial text mismatch"),
                )
            }
        }
    }
}

fn tool_calls<'a>(trace: &'a Trace, tool: &str) -> Vec<&'a crate::model::ToolCallView> {
    trace
        .steps
        .iter()
        .flat_map(|s| s.tool_calls.iter())
        .filter(|c| c.name == tool)
        .collect()
}

fn pass(name: String) -> AssertionResult {
    AssertionResult {
        name,
        pass: true,
        expected: None,
        actual: None,
        message: String::new(),
    }
}

fn fail(
    name: String,
    expected: Option<serde_json::Value>,
    actual: Option<serde_json::Value>,
    message: &str,
) -> AssertionResult {
    AssertionResult {
        name,
        pass: false,
        expected,
        actual,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StepRecord, TraceKind};
    use std::collections::HashMap;

    fn empty_step(index: usize) -> StepRecord {
        StepRecord {
            index,
            node_id: "n".to_string(),
            node_name: String::new(),
            node_type: "SCRIPT".to_string(),
            input: serde_json::Value::Null,
            result: serde_json::json!(1),
            success: true,
            error: None,
            error_kind: None,
            retryable: None,
            recovery_hint: None,
            branch_id: None,
            route_target: Some("b".to_string()),
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
            children: vec![],
        }
    }

    #[test]
    fn reports_expected_and_actual_on_mismatch() {
        let trace = Trace {
            schema: crate::model::TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps: vec![empty_step(0)],
            assertions: vec![Assertion::StepResult {
                step: 0,
                expected: serde_json::json!(2),
            }],
            trigger_templates: vec![],
        };
        let outcome = run_assertions(&trace);
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.results[0].actual, Some(serde_json::json!(1)));
    }

    fn step_with_tool_call(call: crate::model::ToolCallView) -> StepRecord {
        let mut step = empty_step(0);
        step.tool_calls = vec![call];
        step
    }

    fn denied_call(name: &str, error: &str) -> crate::model::ToolCallView {
        crate::model::ToolCallView {
            name: name.to_string(),
            call_id: "c1".to_string(),
            arguments: serde_json::Value::Null,
            result: None,
            error: Some(error.to_string()),
            duration_ms: None,
            success: false,
        }
    }

    fn trace_with_assertions(steps: Vec<StepRecord>, assertions: Vec<Assertion>) -> Trace {
        Trace {
            schema: crate::model::TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps,
            assertions,
            trigger_templates: vec![],
        }
    }

    #[test]
    fn tool_never_called_fails_when_called() {
        let trace = trace_with_assertions(
            vec![step_with_tool_call(denied_call("write_file", "denied"))],
            vec![Assertion::ToolNeverCalled {
                tool: "write_file".to_string(),
            }],
        );
        let outcome = run_assertions(&trace);
        assert_eq!(outcome.failed, 1);
    }

    #[test]
    fn tool_denied_with_fails_when_never_called() {
        let trace = trace_with_assertions(
            vec![empty_step(0)],
            vec![Assertion::ToolDeniedWith {
                tool: "write_file".to_string(),
                contains: "denied".to_string(),
            }],
        );
        let outcome = run_assertions(&trace);
        assert_eq!(outcome.failed, 1);
        assert!(outcome.results[0].message.contains("never called"));
    }

    #[test]
    fn tool_denied_with_passes_on_matching_denial() {
        let trace = trace_with_assertions(
            vec![step_with_tool_call(denied_call(
                "write_file",
                "Tool 'write_file' is not in the available tool set",
            ))],
            vec![Assertion::ToolDeniedWith {
                tool: "write_file".to_string(),
                contains: "not in the available tool set".to_string(),
            }],
        );
        let outcome = run_assertions(&trace);
        assert_eq!(outcome.failed, 0);
    }
}
