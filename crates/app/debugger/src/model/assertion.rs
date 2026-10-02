use serde::{Deserialize, Serialize};

use super::views_runtime::InterruptionKind;

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
