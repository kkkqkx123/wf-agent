use serde::{Deserialize, Serialize};

use crate::model::{TriggerEventView, TriggerTemplateView};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerDryRun {
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    pub candidates: Vec<String>,
    pub dropped: Vec<TriggerDrop>,
    pub permits: Vec<TriggerPermitView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerDrop {
    pub template_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerPermitView {
    pub template_name: String,
    pub permit: String,
}

pub fn is_compression_target(event_type: &str) -> bool {
    event_type == "CONTEXT_COMPRESSION_REQUESTED"
}

pub fn is_before_hook_target(event_type: &str, event_name: Option<&str>) -> bool {
    event_type == "HOOK_TRIGGERED" && event_name.is_some_and(|name| name.starts_with("BEFORE_"))
}

pub fn dry_run(
    templates: &[TriggerTemplateView],
    event_type: &str,
    event_name: Option<&str>,
    variables: &std::collections::HashMap<String, serde_json::Value>,
) -> TriggerDryRun {
    let mut candidates = Vec::new();
    let mut dropped = Vec::new();
    for template in templates {
        if !template.enabled {
            dropped.push(TriggerDrop {
                template_name: template.name.clone(),
                reason: "disabled".to_string(),
            });
            continue;
        }
        if template.event_type != event_type {
            dropped.push(TriggerDrop {
                template_name: template.name.clone(),
                reason: format!(
                    "event type mismatch: want {}, got {}",
                    template.event_type, event_type
                ),
            });
            continue;
        }
        if let Some(want) = template.event_name.as_deref() {
            if event_name != Some(want) {
                dropped.push(TriggerDrop {
                    template_name: template.name.clone(),
                    reason: format!("event name mismatch: want {want}"),
                });
                continue;
            }
        }
        if is_compression_target(event_type) {
            dropped.push(TriggerDrop {
                template_name: template.name.clone(),
                reason: "guarded: compression signal needs a hook handler".to_string(),
            });
            continue;
        }
        if is_before_hook_target(event_type, event_name) {
            dropped.push(TriggerDrop {
                template_name: template.name.clone(),
                reason: "guarded: BEFORE hook point needs a veto handler".to_string(),
            });
            continue;
        }
        if let Some(expression) = template.expression.as_deref() {
            match wf_core::condition::ConditionEvaluator::evaluate(expression, variables) {
                Ok(true) => {}
                Ok(false) => {
                    dropped.push(TriggerDrop {
                        template_name: template.name.clone(),
                        reason: "expression false".to_string(),
                    });
                    continue;
                }
                Err(e) => {
                    dropped.push(TriggerDrop {
                        template_name: template.name.clone(),
                        reason: format!("expression error: {e}"),
                    });
                    continue;
                }
            }
        }
        candidates.push(template.name.clone());
    }
    candidates.sort();
    let permits = candidates
        .iter()
        .map(|name| TriggerPermitView {
            template_name: name.clone(),
            permit: "granted (dry run, no budget spent)".to_string(),
        })
        .collect();
    TriggerDryRun {
        event_type: event_type.to_string(),
        event_name: event_name.map(str::to_string),
        candidates,
        dropped,
        permits,
    }
}

pub fn summarize_seen(events: &[TriggerEventView]) -> (usize, Vec<TriggerDrop>) {
    let matched = events.iter().filter(|e| e.matched).count();
    let drops = events
        .iter()
        .filter(|e| !e.matched)
        .map(|e| TriggerDrop {
            template_name: e.template_name.clone(),
            reason: e
                .drop_reason
                .clone()
                .unwrap_or_else(|| "unmatched".to_string()),
        })
        .collect();
    (matched, drops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_template_is_dropped() {
        let templates = vec![TriggerTemplateView {
            name: "t1".to_string(),
            enabled: false,
            event_type: "NODE_COMPLETED".to_string(),
            event_name: None,
            expression: None,
            priority: None,
            scope: None,
            max_triggers: None,
        }];
        let run = dry_run(&templates, "NODE_COMPLETED", None, &Default::default());
        assert!(run.candidates.is_empty());
        assert_eq!(run.dropped.len(), 1);
    }
}
