use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::HookFireView;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookPointReport {
    pub hook_type: String,
    pub matched: Vec<HookFireView>,
    pub skipped: Vec<HookSkip>,
    pub veto_reason: Option<String>,
    pub gate_blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookSkip {
    pub hook_id: String,
    #[serde(default)]
    pub reason: String,
}

pub fn is_gate_point(hook_type: &str) -> bool {
    matches!(
        hook_type,
        "BEFORE_EXECUTE" | "BEFORE_TOOL_CALL" | "BEFORE_ITERATION"
    )
}

pub fn explain_hook_point(
    hook_type: &str,
    fires: &[HookFireView],
    context: &HashMap<String, serde_json::Value>,
) -> HookPointReport {
    let mut matched = Vec::new();
    let mut skipped = Vec::new();
    for fire in fires {
        if !fire.enabled {
            skipped.push(HookSkip {
                hook_id: fire.hook_id.clone(),
                reason: "disabled".to_string(),
            });
            continue;
        }
        if let Some(condition) = fire.condition.clone() {
            let verdict =
                match wf_core::condition::ConditionEvaluator::evaluate(&condition, context) {
                    Ok(true) => true,
                    Ok(false) => false,
                    Err(e) => {
                        skipped.push(HookSkip {
                            hook_id: fire.hook_id.clone(),
                            reason: format!("condition error: {e}"),
                        });
                        continue;
                    }
                };
            if !verdict {
                skipped.push(HookSkip {
                    hook_id: fire.hook_id.clone(),
                    reason: "condition false".to_string(),
                });
                continue;
            }
        }
        matched.push(fire.clone());
    }
    let veto_reason = matched.iter().find_map(|m| m.veto_reason.clone());
    let gate_blocked = veto_reason.is_some() && is_gate_point(hook_type);
    HookPointReport {
        hook_type: hook_type.to_string(),
        matched,
        skipped,
        veto_reason,
        gate_blocked,
    }
}

pub fn collect_hook_points(trace: &crate::model::Trace) -> Vec<HookPointReport> {
    let mut by_type: HashMap<String, Vec<HookFireView>> = HashMap::new();
    for visit in crate::traverse::walk(trace) {
        for fire in &visit.step.hooks_fired {
            by_type
                .entry(fire.hook_type.clone())
                .or_default()
                .push(fire.clone());
        }
    }
    let context: HashMap<String, serde_json::Value> = trace.initial_variables.clone();
    let mut out: Vec<HookPointReport> = by_type
        .iter()
        .map(|(hook_type, fires)| explain_hook_point(hook_type, fires, &context))
        .collect();
    out.sort_by(|a, b| a.hook_type.cmp(&b.hook_type));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_hooks_are_skipped() {
        let fires = vec![HookFireView {
            hook_type: "BEFORE_EXECUTE".to_string(),
            hook_id: "h1".to_string(),
            enabled: false,
            condition: None,
            condition_matched: None,
            payload: serde_json::Value::Null,
            handler: None,
            outcome: "continue".to_string(),
            veto_reason: None,
            error: None,
            duration_ms: None,
            gate: true,
        }];
        let report = explain_hook_point("BEFORE_EXECUTE", &fires, &HashMap::new());
        assert!(report.matched.is_empty());
        assert_eq!(report.skipped.len(), 1);
    }
}
