//! ROUTE node rules: condition/default target resolution, expression
//! syntax, and default-target duplication checks.

use std::collections::HashSet;

use wf_types::workflow_execution::WorkflowGraphStructure;
use wf_types::ValidationError;

/// ROUTE targets must resolve to real graph nodes: every condition
/// target and the default target are instance-level references.
/// Condition expressions are checked for static syntax (empty, unknown
/// function, arity, unbalanced delimiters); value semantics stay runtime.
/// A ROUTE must declare at least one condition or a default target, and
/// no condition target may duplicate the default target.
pub(super) fn validate_route_targets(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    for node in &graph.nodes {
        if node.node_type != "ROUTE" {
            continue;
        }
        let conditions = node.inner.get("conditions").and_then(|v| v.as_array());
        let has_conditions = conditions.is_some_and(|c| !c.is_empty());
        let default_target = node
            .inner
            .get("default_target_node_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        if !has_conditions && default_target.is_none() {
            errors.push(ValidationError::new(
                format!("nodes.{}.config", node.id),
                format!(
                    "ROUTE node '{}' must define at least one condition or a default_target_node_id",
                    node.id
                ),
            ));
        }
        if let Some(conditions) = conditions {
            for (idx, condition) in conditions.iter().enumerate() {
                if let Some(expression) = condition.get("expression").and_then(|v| v.as_str()) {
                    if expression.trim().is_empty() {
                        errors.push(ValidationError::new(
                            format!("nodes.{}.config.conditions[{}].expression", node.id, idx),
                            format!("ROUTE node '{}' has an empty condition expression", node.id),
                        ));
                    } else if let Err(reason) =
                        wf_core::condition::ConditionEvaluator::validate_syntax(expression)
                    {
                        errors.push(ValidationError::new(
                            format!("nodes.{}.config.conditions[{}].expression", node.id, idx),
                            format!(
                                "ROUTE node '{}' has an invalid condition expression: {}",
                                node.id, reason
                            ),
                        ));
                    }
                }
                if let Some(target) = condition
                    .get("target_node_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    if !node_ids.contains(target) {
                        errors.push(ValidationError::new(
                            format!("nodes.{}.config.conditions[{}]", node.id, idx),
                            format!("ROUTE node '{}' targets unknown node '{}'", node.id, target),
                        ));
                    }
                    if Some(target) == default_target {
                        errors.push(ValidationError::new(
                            format!("nodes.{}.config.conditions[{}]", node.id, idx),
                            format!(
                                "ROUTE node '{}' condition target '{}' duplicates the default target; use distinct targets",
                                node.id, target
                            ),
                        ));
                    }
                }
            }
        }
        if let Some(target) = node
            .inner
            .get("default_target_node_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            if !node_ids.contains(target) {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config.default_target_node_id", node.id),
                    format!(
                        "ROUTE node '{}' default target '{}' does not exist in the graph",
                        node.id, target
                    ),
                ));
            }
        }
    }
    errors
}
