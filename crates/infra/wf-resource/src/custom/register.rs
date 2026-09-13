use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

use wf_core::registry::{ConcurrentRegistry, Registry};
use wf_tools::registry::ToolRegistry;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool::{ToolMetadata, ToolParameterSchema, ToolPropertySchema, ToolType};
use wf_types::trigger::{TriggerCondition, TriggerTemplate};
use wf_types::Template;

use crate::custom::types::{
    CustomPromptDefinition, CustomPromptType, CustomResources, CustomToolDefinition,
    CustomToolType, CustomTriggerCondition, CustomTriggerDefinition, CustomValidationLevel,
};
use crate::registry::{register_item_skip, register_item_strict, ResourceRegistries};
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

fn validate_tool(tool: &CustomToolDefinition) -> Result<(), String> {
    wf_config::validator::validate_required(&tool.id, "tool.id").map_err(|e| e.to_string())?;
    wf_config::validator::validate_required(&tool.description, "tool.description")
        .map_err(|e| e.to_string())
}

fn validate_parameters_schema(schema: &ToolParameterSchema) -> Result<(), String> {
    for required_field in &schema.required {
        if !schema.properties.contains_key(required_field) {
            return Err(format!(
                "Required field '{}' is not defined in properties",
                required_field
            ));
        }
    }
    for (key, prop) in &schema.properties {
        match prop.property_type.as_str() {
            "string" | "number" | "integer" | "boolean" | "array" | "object" | "null" => {}
            other => {
                return Err(format!("Property '{}' has invalid type '{}'", key, other));
            }
        }
    }
    Ok(())
}

fn convert_tool_type(tt: &CustomToolType) -> ToolType {
    match tt {
        CustomToolType::Stateless => ToolType::Stateless,
        CustomToolType::Stateful => ToolType::Stateful,
    }
}

fn build_properties(
    params: &[crate::custom::types::CustomParamDef],
) -> (BTreeMap<String, ToolPropertySchema>, Vec<String>) {
    let mut properties = BTreeMap::new();
    let mut required = Vec::new();
    for p in params {
        if p.required {
            required.push(p.name.clone());
        }
        properties.insert(
            p.name.clone(),
            ToolPropertySchema {
                property_type: p.param_type.clone(),
                description: Some(p.description.clone()),
                ..ToolPropertySchema::typed(&p.param_type)
            },
        );
    }
    (properties, required)
}

pub fn register_custom_tools(
    tool_registry: &ToolRegistry,
    tools: Vec<CustomToolDefinition>,
    skip_if_exists: bool,
) -> Summary {
    let mut total = Summary::new();
    for t in tools {
        if let Err(e) = validate_tool(&t) {
            total.merge(Summary::err(&t.id, e));
            continue;
        }
        let (properties, required) = build_properties(&t.schema.parameters);
        let schema = ToolParameterSchema {
            r#type: "object".into(),
            properties,
            required,
            additional_properties: None,
        };
        if let Err(e) = validate_parameters_schema(&schema) {
            total.merge(Summary::err(
                &t.id,
                format!("Invalid parameters schema: {}", e),
            ));
            continue;
        }

        let tool = ToolDef {
            id: t.id.clone(),
            name: t.id.clone(),
            description: t.description.clone(),
            tool_type: convert_tool_type(&t.tool_type),
            parameters: Some(schema),
            metadata: t.metadata.map(|m| {
                let map: HashMap<String, serde_json::Value> = match m {
                    serde_json::Value::Object(obj) => obj.into_iter().collect(),
                    other => {
                        let mut h = HashMap::new();
                        h.insert("value".into(), other);
                        h
                    }
                };
                ToolMetadata {
                    category: None,
                    tags: None,
                    documentation_url: None,
                    custom_fields: Some(map),
                    risk_level: None,
                    auto_approvable: None,
                    create_checkpoint: None,
                    exposure: None,
                }
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        };

        total.merge(register_item_tool(
            tool_registry,
            t.id,
            tool,
            skip_if_exists,
        ));
    }
    total
}

fn register_item_tool(
    tool_registry: &ToolRegistry,
    key: String,
    tool: ToolDef,
    skip_if_exists: bool,
) -> Summary {
    if skip_if_exists && tool_registry.has(&key) {
        return Summary::ok(&key);
    }
    tool_registry.register_tool(tool);
    Summary::ok(&key)
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

pub fn register_custom_prompts(
    regs: &ResourceRegistries,
    prompts: Vec<CustomPromptDefinition>,
    skip_if_exists: bool,
) -> Summary {
    let mut total = Summary::new();

    for p in prompts {
        let category = match p.prompt_type {
            CustomPromptType::System => "system",
            CustomPromptType::User => "user",
            CustomPromptType::Assistant => "assistant",
            CustomPromptType::Fragments => "fragments",
        };

        let variables = p.variables.map(|vars| {
            vars.into_iter()
                .map(|v| wf_types::TemplateVariableDefinition {
                    name: v.name,
                    r#type: v.var_type,
                    required: v.required.unwrap_or(false),
                    description: v.description,
                    default_value: v.default_value,
                })
                .collect()
        });

        // Declared fragments must exist so composition cannot silently
        // drop sections at render time.
        if let Some(ref fragment_ids) = p.fragments {
            let missing: Vec<&str> = fragment_ids
                .iter()
                .filter(|id| !regs.fragments.has(id))
                .map(String::as_str)
                .collect();
            if !missing.is_empty() {
                total.merge(Summary::err(
                    &p.id,
                    format!("references unregistered fragments: {}", missing.join(", ")),
                ));
                continue;
            }
        }

        let template = Template {
            id: p.id.clone(),
            name: p.name.clone(),
            description: Some(p.name),
            category: category.into(),
            content: p.content,
            variables,
            fragments: p.fragments,
        };

        if let Err(e) = wf_config::processor::prompt::validate_prompt_template(&template) {
            total.merge(Summary::err(&p.id, e.to_string()));
            continue;
        }

        total.merge(if skip_if_exists {
            register_item_skip(&regs.templates, p.id, template)
        } else {
            register_item_strict(&regs.templates, p.id, template)
        });
    }
    total
}

pub fn register_custom_resources(
    regs: &ResourceRegistries,
    tool_registry: &ToolRegistry,
    resources: CustomResources,
    skip_if_exists: bool,
    validation_level: CustomValidationLevel,
) -> Summary {
    let mut total = Summary::new();

    if !resources.errors.is_empty() {
        if validation_level == CustomValidationLevel::Strict {
            // Strict mode: any load/parse failure aborts the whole custom
            // resource pipeline; nothing is registered partially.
            for err in &resources.errors {
                total.merge(Summary::err("custom_load.strict", err));
            }
            return total;
        }
        for err in &resources.errors {
            total.merge(Summary::err("custom_load", err));
        }
    }

    let r = register_custom_tools(tool_registry, resources.tools, skip_if_exists);
    total.merge(r);

    let r = register_custom_triggers(&regs.trigger_templates, resources.triggers, skip_if_exists);
    total.merge(r);

    let r = register_custom_prompts(regs, resources.prompts, skip_if_exists);
    total.merge(r);

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::types::{CustomHandlerConfig, CustomParamSchema};
    use wf_core::registry::Registry;

    fn make_resources_with_error() -> CustomResources {
        CustomResources {
            tools: vec![CustomToolDefinition {
                id: "custom-tool".into(),
                tool_type: CustomToolType::Stateless,
                description: "A custom tool".into(),
                schema: CustomParamSchema { parameters: vec![] },
                handler: CustomHandlerConfig::Inline { code: "x".into() },
                metadata: None,
            }],
            triggers: vec![],
            prompts: vec![],
            errors: vec!["cannot read tools.json: parse error".into()],
        }
    }

    #[test]
    fn test_lenient_registers_partial() {
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let summary = register_custom_resources(
            &regs,
            &tool_registry,
            make_resources_with_error(),
            false,
            CustomValidationLevel::Lenient,
        );
        assert!(summary.failed.iter().any(|f| f.id == "custom_load"));
        assert!(tool_registry.has("custom-tool"));
    }

    #[test]
    fn test_strict_aborts_pipeline() {
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let summary = register_custom_resources(
            &regs,
            &tool_registry,
            make_resources_with_error(),
            false,
            CustomValidationLevel::Strict,
        );
        assert!(summary.failed.iter().any(|f| f.id == "custom_load.strict"));
        assert!(!tool_registry.has("custom-tool"));
    }

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
        use wf_core::registry::Registry;
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
        use wf_core::registry::Registry;
        let regs = ResourceRegistries::new();
        let mut trigger = event_trigger("noop-trigger");
        trigger.action = None;
        let summary = register_custom_triggers(&regs.trigger_templates, vec![trigger], false);
        assert!(summary.failed.iter().any(|f| f.id == "noop-trigger"));
        assert!(!regs.trigger_templates.has("noop-trigger"));
    }

    #[test]
    fn trigger_priority_and_dispatch_mode_pass_through() {
        use wf_core::registry::Registry;
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
        use wf_core::registry::Registry;
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
        use wf_core::registry::Registry;
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
        use wf_core::registry::Registry;
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
        use wf_core::registry::Registry;
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
