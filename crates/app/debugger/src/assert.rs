use serde::{Deserialize, Serialize};

use crate::model::Trace;
use crate::traverse::{find_step, walk};
use crate::views::InterruptionKind;

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
    LoopRounds {
        loop_id: String,
        expected_rounds: usize,
    },
    MergeOutcome {
        step: usize,
        expected: String,
    },
    InterruptionCount {
        #[serde(default)]
        kind: Option<InterruptionKind>,
        expected: usize,
    },
    CheckpointPresent {
        step: usize,
        expected: bool,
    },
    InteractionSettled {
        interaction_id: String,
    },
    TokenBudget {
        max_total_tokens: u64,
    },
    CostBudget {
        max_cost: f64,
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
            let Some(record) = find_step(trace, *step) else {
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
            let Some(record) = find_step(trace, *step) else {
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
            let Some(record) = find_step(trace, *step) else {
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
            let Some(record) = find_step(trace, *step) else {
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
            let vetoes: usize = walk(trace)
                .into_iter()
                .map(|visit| {
                    visit
                        .step
                        .hooks_fired
                        .iter()
                        .filter(|hook| &hook.hook_type == hook_type)
                        .filter(|hook| hook.veto_reason.is_some() || hook.outcome == "veto")
                        .count()
                })
                .sum::<usize>();
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
            let mut actual = None;
            for visit in walk(trace) {
                if let Some(event) = visit
                    .step
                    .triggers_seen
                    .iter()
                    .find(|event| event.template_name == *template)
                {
                    actual = Some(event.matched);
                    break;
                }
            }
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
        Assertion::LoopRounds {
            loop_id,
            expected_rounds,
        } => {
            let actual = walk(trace)
                .into_iter()
                .filter(|visit| {
                    visit
                        .step
                        .loop_round
                        .as_ref()
                        .is_some_and(|round| &round.loop_id == loop_id)
                })
                .count();
            if &actual == expected_rounds {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::from(*expected_rounds)),
                    Some(serde_json::Value::from(actual)),
                    &format!("loop {loop_id} round count mismatch"),
                )
            }
        }
        Assertion::MergeOutcome { step, expected } => {
            let Some(record) = find_step(trace, *step) else {
                return fail(name, None, None, "step not found");
            };
            let Some(merge) = record.merge.as_ref() else {
                return fail(name, None, None, "step has no merge record");
            };
            let actual = merge
                .outcome
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            if &actual == expected {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::String(expected.clone())),
                    Some(serde_json::Value::String(actual)),
                    "merge outcome mismatch",
                )
            }
        }
        Assertion::InterruptionCount { kind, expected } => {
            let actual = walk(trace)
                .into_iter()
                .filter(|visit| {
                    visit
                        .step
                        .interruption
                        .as_ref()
                        .is_some_and(|interruption| {
                            kind.is_none_or(|want| interruption.kind == want)
                        })
                })
                .count();
            if &actual == expected {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::from(*expected)),
                    Some(serde_json::Value::from(actual)),
                    "interruption count mismatch",
                )
            }
        }
        Assertion::CheckpointPresent { step, expected } => {
            let Some(record) = find_step(trace, *step) else {
                return fail(name, None, None, "step not found");
            };
            let actual = record.checkpoint.is_some();
            if &actual == expected {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::Bool(*expected)),
                    Some(serde_json::Value::Bool(actual)),
                    "checkpoint presence mismatch",
                )
            }
        }
        Assertion::InteractionSettled { interaction_id } => {
            let settled = walk(trace).into_iter().any(|visit| {
                visit.step.interaction.as_ref().is_some_and(|interaction| {
                    &interaction.interaction_id == interaction_id && !interaction.pending
                })
            });
            if settled {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::Bool(true)),
                    Some(serde_json::Value::Bool(false)),
                    &format!("interaction {interaction_id} is not settled"),
                )
            }
        }
        Assertion::TokenBudget { max_total_tokens } => {
            let actual: u64 = walk(trace)
                .into_iter()
                .flat_map(|visit| {
                    visit
                        .step
                        .llm_calls
                        .iter()
                        .map(|call| call.effective_total())
                })
                .sum();
            if actual <= *max_total_tokens {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::Value::from(*max_total_tokens)),
                    Some(serde_json::Value::from(actual)),
                    "token budget exceeded",
                )
            }
        }
        Assertion::CostBudget { max_cost } => {
            let actual: f64 = walk(trace)
                .into_iter()
                .flat_map(|visit| {
                    visit
                        .step
                        .llm_calls
                        .iter()
                        .filter(|call| !call.estimated)
                        .filter_map(|call| call.total_cost)
                })
                .sum();
            if actual <= *max_cost {
                pass(name)
            } else {
                fail(
                    name,
                    Some(serde_json::json!(max_cost)),
                    Some(serde_json::json!(actual)),
                    "cost budget exceeded",
                )
            }
        }
    }
}

fn tool_calls<'a>(trace: &'a Trace, tool: &str) -> Vec<&'a crate::model::ToolCallView> {
    let mut out = Vec::new();
    for visit in walk(trace) {
        let step = visit.step;
        for call in step.tool_calls.iter().filter(|call| call.name == tool) {
            out.push(call);
        }
    }
    out
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
            budget: None,
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
            budget: None,
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

    #[test]
    fn loop_rounds_counts_nested_rounds() {
        let mut parent = empty_step(0);
        let mut child = empty_step(1);
        child.loop_round = Some(crate::model::LoopRoundView {
            loop_id: "l1".to_string(),
            round: 0,
            item: None,
            failed: false,
            iteration: Some(0),
            max_iterations: None,
            failures: 0,
            policy: None,
            resumed: false,
            completed_nodes: vec![],
            iterable_kind: None,
        });
        parent.children = vec![child];
        let trace = trace_with_assertions(
            vec![parent],
            vec![Assertion::LoopRounds {
                loop_id: "l1".to_string(),
                expected_rounds: 1,
            }],
        );
        let outcome = run_assertions(&trace);
        assert_eq!(outcome.failed, 0);
    }

    #[test]
    fn token_budget_sums_nested_calls() {
        let mut parent = empty_step(0);
        parent.llm_calls = vec![crate::model::LlmCallView {
            profile_id: "p".to_string(),
            model: None,
            prompt_tokens: 60,
            completion_tokens: 50,
            total_tokens: 0,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_cost: None,
            estimated: false,
            content_preview: None,
            tool_call_count: 0,
            error: None,
        }];
        let trace = trace_with_assertions(
            vec![parent],
            vec![Assertion::TokenBudget {
                max_total_tokens: 100,
            }],
        );
        let outcome = run_assertions(&trace);
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.results[0].actual, Some(serde_json::json!(110)));
    }
}
