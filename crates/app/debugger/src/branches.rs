use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::{RouteBranch, RouteDecisionPoint};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchVerdict {
    pub target_node_id: String,
    pub expression: Option<String>,
    pub hit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionVerdict {
    pub node_id: String,
    pub branches: Vec<BranchVerdict>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_target: Option<String>,
    pub default_reachable: bool,
    pub hit_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CoverageSummary {
    pub decision_points: usize,
    pub branches: usize,
    pub hit_branches: usize,
    pub dead_branches: Vec<String>,
    pub coverage_ratio: f64,
}

pub fn evaluate_decision(
    point: &RouteDecisionPoint,
    variables: &HashMap<String, serde_json::Value>,
) -> DecisionVerdict {
    let mut branches = Vec::new();
    for branch in &point.branches {
        branches.push(evaluate_branch(branch, variables));
    }
    let hit_count = branches.iter().filter(|b| b.hit).count();
    DecisionVerdict {
        node_id: point.node_id.clone(),
        branches,
        default_target: point.default_target.clone(),
        default_reachable: hit_count == 0,
        hit_count,
    }
}

fn evaluate_branch(
    branch: &RouteBranch,
    variables: &HashMap<String, serde_json::Value>,
) -> BranchVerdict {
    let Some(expression) = branch.expression.clone() else {
        return BranchVerdict {
            target_node_id: branch.target_node_id.clone(),
            expression: None,
            hit: true,
            error: None,
        };
    };
    match wf_core::condition::ConditionEvaluator::evaluate(&expression, variables) {
        Ok(hit) => BranchVerdict {
            target_node_id: branch.target_node_id.clone(),
            expression: Some(expression),
            hit,
            error: None,
        },
        Err(e) => BranchVerdict {
            target_node_id: branch.target_node_id.clone(),
            expression: Some(expression),
            hit: false,
            error: Some(e.to_string()),
        },
    }
}

pub fn summarize_verdicts(verdicts: &[DecisionVerdict]) -> CoverageSummary {
    let mut branches = 0;
    let mut hit = 0;
    let mut dead = Vec::new();
    for verdict in verdicts {
        for branch in &verdict.branches {
            branches += 1;
            if branch.hit {
                hit += 1;
            } else if branch.error.is_none() {
                dead.push(format!("{} -> {}", verdict.node_id, branch.target_node_id));
            }
        }
    }
    CoverageSummary {
        decision_points: verdicts.len(),
        branches,
        hit_branches: hit,
        dead_branches: dead,
        coverage_ratio: if branches == 0 {
            1.0
        } else {
            hit as f64 / branches as f64
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_reachable_when_nothing_hits() {
        let point = RouteDecisionPoint {
            node_id: "route".to_string(),
            branches: vec![RouteBranch {
                target_node_id: "a".to_string(),
                expression: Some("eq(x, 1)".to_string()),
            }],
            default_target: Some("b".to_string()),
        };
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), json!(2));
        let verdict = evaluate_decision(&point, &vars);
        assert!(verdict.default_reachable);
        assert_eq!(verdict.hit_count, 0);
    }
}
