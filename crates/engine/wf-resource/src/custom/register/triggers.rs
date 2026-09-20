use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use wf_core::registry::{ConcurrentRegistry, Registry};
use wf_types::trigger::{TriggerCondition, TriggerTemplate};

use crate::custom::types::{CustomTriggerCondition, CustomTriggerDefinition};
use crate::registry::{register_item_skip, register_item_strict};
use crate::result::Summary;

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The producer target and the trigger action must agree: a creation target
/// publishes events without an `execution_id`, which only execution-creating
/// actions can run; an execution-scoped target binds a live execution, where
/// a cold-start action has nothing to anchor to.
fn check_target_action_match(
    trigger_name: &str,
    target: &wf_types::trigger::ScheduleTarget,
    action: Option<&wf_types::trigger::TriggerAction>,
) -> Result<(), String> {
    let Some(action) = action else {
        return Ok(());
    };
    match (target.is_creating(), action.is_execution_creating()) {
        (true, false) => Err(format!(
            "trigger '{}' targets creation but its action '{}' needs an emitting execution; use an execution-creating action (execute_workflow / execute_agent)",
            trigger_name,
            action.action_name()
        )),
        (false, true) => Err(format!(
            "trigger '{}' targets a live execution but its action '{}' cold-starts a fresh run; use a creation target or an execution-scoped action",
            trigger_name,
            action.action_name()
        )),
        _ => Ok(()),
    }
}

/// Webhook path declared by a registered template, if any.
fn template_webhook_path(template: &TriggerTemplate) -> Option<&str> {
    template
        .metadata
        .as_ref()?
        .get(wf_types::trigger::WEBHOOK_SPEC_METADATA_KEY)?
        .get("path")?
        .as_str()
}

pub fn register_custom_triggers(
    registry: &ConcurrentRegistry<TriggerTemplate>,
    triggers: Vec<CustomTriggerDefinition>,
    skip_if_exists: bool,
) -> Summary {
    let mut total = Summary::new();
    let ts = now_ts();

    // Phase one: build every candidate and run the single-template
    // validation, so malformed definitions fail individually first.
    let mut candidates: Vec<TriggerTemplate> = Vec::new();
    for t in triggers {
        // No action = the template can match forever but never do anything:
        // rejected explicitly instead of registered as a silent no-op.
        if t.action.is_none() {
            total.merge(Summary::err(
                &t.name,
                "custom trigger has no action; nothing would execute on match".to_string(),
            ));
            continue;
        }
        let (condition, producer_spec, producer_enabled) = match &t.condition {
            // Custom event triggers match `NODE_CUSTOM_EVENT` events by their
            // concrete `event_name`.
            CustomTriggerCondition::Event { value } => (
                TriggerCondition {
                    event_type: "NODE_CUSTOM_EVENT".into(),
                    event_name: Some(value.clone()),
                    condition: None,
                    metadata: None,
                    metadata_exists: None,
                    execution_prefix: None,
                },
                None,
                None,
            ),
            // Schedules are fed by the runtime scheduler: validate the cron
            // expression, timezone and target here; the condition reuses the
            // event competition scope keys through
            // `translate_schedule_to_condition`, and the validated spec
            // travels in the template metadata for the scheduler to read.
            CustomTriggerCondition::Schedule {
                cron,
                tz,
                target,
                misfire,
                enabled,
            } => {
                let spec = wf_types::trigger::ScheduleSpec {
                    cron: cron.clone(),
                    tz: tz.clone(),
                    target: target.clone(),
                    misfire: *misfire,
                    enabled: *enabled,
                };
                if let Err(e) = spec.validate(&t.name) {
                    total.merge(Summary::err(&t.name, e));
                    continue;
                }
                if let Err(e) = check_target_action_match(&t.name, target, t.action.as_ref()) {
                    total.merge(Summary::err(&t.name, e));
                    continue;
                }
                let condition =
                    wf_types::trigger::TriggerSource::translate_schedule_to_condition(&t.name);
                let spec_json = serde_json::to_value(&spec).unwrap_or(serde_json::Value::Null);
                (
                    condition,
                    Some((
                        wf_types::trigger::SCHEDULE_SPEC_METADATA_KEY.to_string(),
                        spec_json,
                    )),
                    Some(*enabled),
                )
            }
            // Webhooks are fed by the server ingress gateway: validate path
            // shape, auth completeness and target; path uniqueness is checked
            // against already-registered templates below.
            CustomTriggerCondition::Webhook {
                path,
                auth,
                input_mapping,
                target,
            } => {
                let spec = wf_types::trigger::WebhookSpec {
                    path: path.clone(),
                    auth: auth.clone(),
                    input_mapping: input_mapping.clone(),
                    target: target.clone(),
                };
                if let Err(e) = spec.validate(&t.name) {
                    total.merge(Summary::err(&t.name, e));
                    continue;
                }
                if let Err(e) = check_target_action_match(&t.name, target, t.action.as_ref()) {
                    total.merge(Summary::err(&t.name, e));
                    continue;
                }
                let condition =
                    wf_types::trigger::TriggerSource::translate_webhook_to_condition(&t.name);
                let spec_json = serde_json::to_value(&spec).unwrap_or(serde_json::Value::Null);
                (
                    condition,
                    Some((
                        wf_types::trigger::WEBHOOK_SPEC_METADATA_KEY.to_string(),
                        spec_json,
                    )),
                    None,
                )
            }
        };

        let mut metadata: HashMap<String, serde_json::Value> = t
            .metadata
            .and_then(|m| match m {
                serde_json::Value::Object(obj) => {
                    let map: HashMap<String, serde_json::Value> = obj.into_iter().collect();
                    Some(map)
                }
                _ => None,
            })
            .unwrap_or_default();
        if let Some((key, value)) = producer_spec {
            metadata.insert(key, value);
        }
        let metadata = if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        };

        let template = TriggerTemplate {
            name: t.name.clone(),
            description: Some(t.description.clone()),
            condition: Some(condition),
            action: t.action,
            enabled: Some(producer_enabled.unwrap_or(true)),
            max_triggers: None,
            priority: t.priority,
            dispatch_mode: t.dispatch_mode,
            allow_multi_effect: t.allow_multi_effect,
            effect_order: t.effect_order.clone(),
            metadata,
            created_at: ts,
            updated_at: ts,
            create_checkpoint: None,
            checkpoint_description_template: None,
        };

        if let Err(e) = wf_config::processor::trigger::validate_trigger_template(&template) {
            total.merge(Summary::err(&t.name, e.to_string()));
            continue;
        }
        candidates.push(template);
    }

    // Phase two: check the competition scopes of the merged set (already
    // registered templates plus this batch). Incoming members of a violated
    // scope are rejected with the scope message; the rest registers.
    // Violated scopes with no incoming member are pre-existing conflicts and
    // only warn here.
    let existing: Vec<TriggerTemplate> = registry
        .list()
        .iter()
        .filter_map(|key| registry.get(key).map(|t| t.as_ref().clone()))
        .collect();
    // Webhook paths are globally unique: a second template claiming an
    // already-mounted path would shadow ingress routing.
    let mut webhook_paths: HashMap<String, String> = HashMap::new();
    for template in &existing {
        if let Some(path) = template_webhook_path(template) {
            webhook_paths.insert(path.to_string(), template.name.clone());
        }
    }
    let mut candidates_without_path_conflict: Vec<TriggerTemplate> = Vec::new();
    for template in candidates {
        if let Some(path) = template_webhook_path(&template) {
            if let Some(owner) = webhook_paths.get(path) {
                total.merge(Summary::err(
                    &template.name,
                    format!(
                        "webhook path '{}' is already claimed by trigger '{}'; paths must be globally unique",
                        path, owner
                    ),
                ));
                continue;
            }
            webhook_paths.insert(path.to_string(), template.name.clone());
        }
        candidates_without_path_conflict.push(template);
    }
    let candidates = candidates_without_path_conflict;
    // Unified registration entry: single-template shape (already applied
    // per candidate above, re-checked idempotently) plus the merged-set
    // scope check.
    let (validated, reports) =
        wf_config::processor::trigger::validate_trigger_registration(&existing, &candidates);
    let candidates = validated.unwrap_or_default();
    let mut rejected: HashMap<String, String> = HashMap::new();
    for report in reports {
        if report.incoming_names.is_empty() {
            tracing::warn!(
                "pre-existing trigger scope violation without incoming subscriber: {}",
                report.message,
            );
            continue;
        }
        for name in report.incoming_names {
            rejected
                .entry(name)
                .or_insert_with(|| report.message.clone());
        }
    }
    for template in candidates {
        if let Some(message) = rejected.remove(&template.name) {
            total.merge(Summary::err(&template.name, message));
            continue;
        }
        total.merge(if skip_if_exists {
            register_item_skip(registry, template.name.clone(), template)
        } else {
            register_item_strict(registry, template.name.clone(), template)
        });
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::types::CustomTriggerCondition;
    use crate::registry::ResourceRegistries;

    fn event_trigger(name: &str) -> CustomTriggerDefinition {
        CustomTriggerDefinition {
            name: name.to_string(),
            description: "t".to_string(),
            condition: CustomTriggerCondition::Event {
                value: "on_issue_created".to_string(),
            },
            action: Some(wf_types::trigger::TriggerAction::SetVariable {
                variable_name: "x".to_string(),
                value: serde_json::json!(1),
            }),
            priority: None,
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
            config: None,
            metadata: None,
        }
    }

    #[test]
    fn event_trigger_registers_with_real_event_type_and_action() {
        let regs = ResourceRegistries::new();
        let summary = register_custom_triggers(
            &regs.trigger_templates,
            vec![event_trigger("on-issue")],
            false,
        );
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        let template = regs
            .trigger_templates
            .get("on-issue")
            .expect("trigger registered");
        let condition = template.condition.as_ref().unwrap();
        assert_eq!(condition.event_type, "NODE_CUSTOM_EVENT");
        assert_eq!(condition.event_name.as_deref(), Some("on_issue_created"));
        assert!(template.action.is_some());
    }

    #[test]
    fn trigger_without_action_is_rejected() {
        let regs = ResourceRegistries::new();
        let mut trigger = event_trigger("noop-trigger");
        trigger.action = None;
        let summary = register_custom_triggers(&regs.trigger_templates, vec![trigger], false);
        assert!(summary.failed.iter().any(|f| f.id == "noop-trigger"));
        assert!(!regs.trigger_templates.has("noop-trigger"));
    }

    #[test]
    fn trigger_priority_and_dispatch_mode_pass_through() {
        let regs = ResourceRegistries::new();
        let mut trigger = event_trigger("ranked-trigger");
        trigger.priority = Some(5);
        trigger.dispatch_mode = Some(wf_types::trigger::TriggerDispatchMode::BestWin);
        let summary = register_custom_triggers(&regs.trigger_templates, vec![trigger], false);
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        let template = regs
            .trigger_templates
            .get("ranked-trigger")
            .expect("trigger registered");
        assert_eq!(template.priority, Some(5));
        assert_eq!(
            template.dispatch_mode,
            Some(wf_types::trigger::TriggerDispatchMode::BestWin)
        );
    }

    #[test]
    fn second_subscriber_of_unique_scope_is_rejected() {
        let regs = ResourceRegistries::new();
        let summary = register_custom_triggers(
            &regs.trigger_templates,
            vec![event_trigger("first-trigger")],
            false,
        );
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        let summary = register_custom_triggers(
            &regs.trigger_templates,
            vec![event_trigger("second-trigger")],
            false,
        );
        assert!(summary.failed.iter().any(|f| f.id == "second-trigger"));
        assert!(!regs.trigger_templates.has("second-trigger"));
        assert!(regs.trigger_templates.has("first-trigger"));
    }

    #[test]
    fn schedule_and_webhook_triggers_register_with_specs() {
        let regs = ResourceRegistries::new();
        let schedule = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Schedule {
                cron: "0 2 * * *".to_string(),
                tz: Some("UTC".to_string()),
                target: wf_types::trigger::ScheduleTarget::Create {
                    workflow_id: Some("nightly_flow".to_string()),
                    agent_id: None,
                    input: None,
                },
                misfire: wf_types::trigger::ScheduleMisfirePolicy::FireOnce,
                enabled: true,
            },
            action: Some(wf_types::trigger::TriggerAction::ExecuteWorkflow {
                workflow_id: "nightly_flow".to_string(),
                input: None,
                timeout: None,
            }),
            ..event_trigger("cron-trigger")
        };
        let webhook = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Webhook {
                path: "/hooks/x".to_string(),
                auth: wf_types::trigger::WebhookAuth::None,
                input_mapping: None,
                target: wf_types::trigger::ScheduleTarget::Create {
                    workflow_id: None,
                    agent_id: Some("child".to_string()),
                    input: None,
                },
            },
            action: Some(wf_types::trigger::TriggerAction::ExecuteAgent {
                agent_id: "child".to_string(),
                prompt: None,
                model: None,
                input: None,
                timeout: None,
                checkpoint_message_interval: None,
            }),
            ..event_trigger("hook-trigger")
        };
        let summary =
            register_custom_triggers(&regs.trigger_templates, vec![schedule, webhook], false);
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        let cron_template = regs
            .trigger_templates
            .get("cron-trigger")
            .expect("schedule registered");
        let condition = cron_template.condition.as_ref().unwrap();
        assert_eq!(condition.event_type, "NODE_CUSTOM_EVENT");
        assert_eq!(condition.event_name.as_deref(), Some("cron-trigger"));
        let spec = cron_template
            .metadata
            .as_ref()
            .and_then(|m| m.get(wf_types::trigger::SCHEDULE_SPEC_METADATA_KEY))
            .expect("schedule spec stored");
        assert_eq!(spec["cron"], serde_json::json!("0 2 * * *"));
        let hook_template = regs
            .trigger_templates
            .get("hook-trigger")
            .expect("webhook registered");
        let hook_spec = hook_template
            .metadata
            .as_ref()
            .and_then(|m| m.get(wf_types::trigger::WEBHOOK_SPEC_METADATA_KEY))
            .expect("webhook spec stored");
        assert_eq!(hook_spec["path"], serde_json::json!("/hooks/x"));
    }

    #[test]
    fn invalid_schedule_and_webhook_are_rejected() {
        let regs = ResourceRegistries::new();
        let bad_cron = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Schedule {
                cron: "not a cron".to_string(),
                tz: None,
                target: wf_types::trigger::ScheduleTarget::ExecutionScoped,
                misfire: wf_types::trigger::ScheduleMisfirePolicy::Skip,
                enabled: true,
            },
            ..event_trigger("bad-cron")
        };
        let bad_tz = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Schedule {
                cron: "0 2 * * *".to_string(),
                tz: Some("Asia/Shanghai".to_string()),
                target: wf_types::trigger::ScheduleTarget::ExecutionScoped,
                misfire: wf_types::trigger::ScheduleMisfirePolicy::Skip,
                enabled: true,
            },
            ..event_trigger("bad-tz")
        };
        let bad_path = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Webhook {
                path: "hooks/x".to_string(),
                auth: wf_types::trigger::WebhookAuth::None,
                input_mapping: None,
                target: wf_types::trigger::ScheduleTarget::ExecutionScoped,
            },
            ..event_trigger("bad-path")
        };
        // Creation target with an execution-scoped action never fires.
        let mismatched = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Schedule {
                cron: "0 2 * * *".to_string(),
                tz: None,
                target: wf_types::trigger::ScheduleTarget::Create {
                    workflow_id: Some("w".to_string()),
                    agent_id: None,
                    input: None,
                },
                misfire: wf_types::trigger::ScheduleMisfirePolicy::Skip,
                enabled: true,
            },
            ..event_trigger("mismatched")
        };
        let summary = register_custom_triggers(
            &regs.trigger_templates,
            vec![bad_cron, bad_tz, bad_path, mismatched],
            false,
        );
        for id in ["bad-cron", "bad-tz", "bad-path", "mismatched"] {
            assert!(
                summary.failed.iter().any(|f| f.id == id),
                "{:?}",
                summary.failed
            );
            assert!(!regs.trigger_templates.has(id));
        }
    }

    #[test]
    fn duplicate_webhook_path_is_rejected() {
        let regs = ResourceRegistries::new();
        let first = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Webhook {
                path: "/hooks/shared".to_string(),
                auth: wf_types::trigger::WebhookAuth::None,
                input_mapping: None,
                target: wf_types::trigger::ScheduleTarget::ExecutionScoped,
            },
            ..event_trigger("first-hook")
        };
        let summary = register_custom_triggers(&regs.trigger_templates, vec![first], false);
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        let second = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Webhook {
                path: "/hooks/shared".to_string(),
                auth: wf_types::trigger::WebhookAuth::None,
                input_mapping: None,
                target: wf_types::trigger::ScheduleTarget::ExecutionScoped,
            },
            ..event_trigger("second-hook")
        };
        let summary = register_custom_triggers(&regs.trigger_templates, vec![second], false);
        assert!(summary.failed.iter().any(|f| f.id == "second-hook"));
        assert!(!regs.trigger_templates.has("second-hook"));
    }
}
