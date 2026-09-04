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
        let condition = match &t.condition {
            // Custom event triggers match `NODE_CUSTOM_EVENT` events by their
            // concrete `event_name`.
            CustomTriggerCondition::Event { value } => TriggerCondition {
                event_type: "NODE_CUSTOM_EVENT".into(),
                event_name: Some(value.clone()),
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            },
            // Schedulers (cron) and webhook servers are not implemented:
            // rejected at load time instead of being registered and never
            // firing.
            CustomTriggerCondition::Schedule { .. } => {
                total.merge(Summary::err(
                    &t.name,
                    "schedule triggers are not implemented yet; use an event trigger instead"
                        .to_string(),
                ));
                continue;
            }
            CustomTriggerCondition::Webhook { .. } => {
                total.merge(Summary::err(
                    &t.name,
                    "webhook triggers are not implemented yet; use an event trigger instead"
                        .to_string(),
                ));
                continue;
            }
        };

        let template = TriggerTemplate {
            name: t.name.clone(),
            description: Some(t.description.clone()),
            condition: Some(condition),
            action: t.action,
            enabled: Some(true),
            max_triggers: None,
            priority: None,
            metadata: t.metadata.and_then(|m| match m {
                serde_json::Value::Object(obj) => {
                    let map: HashMap<String, serde_json::Value> = obj.into_iter().collect();
                    Some(map)
                }
                _ => None,
            }),
            created_at: ts,
            updated_at: ts,
            create_checkpoint: None,
            checkpoint_description_template: None,
        };

        if let Err(e) = wf_config::processor::trigger::validate_trigger_template(&template) {
            total.merge(Summary::err(&t.name, e.to_string()));
            continue;
        }

        total.merge(if skip_if_exists {
            register_item_skip(registry, t.name, template)
        } else {
            register_item_strict(registry, t.name, template)
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
    fn schedule_and_webhook_triggers_are_rejected() {
        use wf_core::registry::Registry;
        let regs = ResourceRegistries::new();
        let schedule = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Schedule {
                value: "* * * * *".to_string(),
            },
            ..event_trigger("cron-trigger")
        };
        let webhook = CustomTriggerDefinition {
            condition: CustomTriggerCondition::Webhook {
                value: "/hooks/x".to_string(),
            },
            ..event_trigger("hook-trigger")
        };
        let summary =
            register_custom_triggers(&regs.trigger_templates, vec![schedule, webhook], false);
        assert!(summary.failed.iter().any(|f| f.id == "cron-trigger"));
        assert!(summary.failed.iter().any(|f| f.id == "hook-trigger"));
        assert!(!regs.trigger_templates.has("cron-trigger"));
        assert!(!regs.trigger_templates.has("hook-trigger"));
    }
}
