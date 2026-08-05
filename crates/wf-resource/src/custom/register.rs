use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use wf_core::registry::ConcurrentRegistry;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool::{ToolMetadata, ToolParameterSchema, ToolProperty, ToolType};
use wf_types::trigger::{TriggerCondition, TriggerTemplate};
use wf_types::PromptTemplate;

use crate::custom::types::{
    CustomPromptDefinition, CustomPromptType, CustomResources, CustomToolDefinition,
    CustomToolType, CustomTriggerCondition, CustomTriggerDefinition, CustomValidationLevel,
};
use crate::registrar::{register_item, Registries};
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

fn convert_tool_type(tt: &CustomToolType) -> ToolType {
    match tt {
        CustomToolType::Stateless => ToolType::Stateless,
        CustomToolType::Stateful => ToolType::Stateful,
    }
}

fn build_properties(
    params: &[crate::custom::types::CustomParamDef],
) -> (HashMap<String, ToolProperty>, Vec<String>) {
    let mut properties = HashMap::new();
    let mut required = Vec::new();
    for p in params {
        if p.required {
            required.push(p.name.clone());
        }
        properties.insert(
            p.name.clone(),
            ToolProperty {
                name: p.name.clone(),
                value: serde_json::Value::Null,
                r#type: Some(p.param_type.clone()),
                required: Some(p.required),
                description: Some(p.description.clone()),
            },
        );
    }
    (properties, required)
}

pub fn register_custom_tools(
    registry: &ConcurrentRegistry<ToolDef>,
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
                }
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        };

        total.merge(register_item(registry, t.id, tool, skip_if_exists));
    }
    total
}

pub fn register_custom_triggers(
    registry: &ConcurrentRegistry<TriggerTemplate>,
    triggers: Vec<CustomTriggerDefinition>,
    skip_if_exists: bool,
) -> Summary {
    let mut total = Summary::new();
    let ts = now_ts();

    for t in triggers {
        let condition = match &t.condition {
            CustomTriggerCondition::Event { value } => TriggerCondition {
                event_type: "custom".into(),
                event_name: Some(value.clone()),
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            },
            CustomTriggerCondition::Schedule { value } => TriggerCondition {
                event_type: "schedule".into(),
                event_name: None,
                condition: Some(value.clone()),
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            },
            CustomTriggerCondition::Webhook { value } => TriggerCondition {
                event_type: "webhook".into(),
                event_name: Some(value.clone()),
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            },
        };

        let template = TriggerTemplate {
            name: t.name.clone(),
            description: Some(t.description.clone()),
            condition: Some(condition),
            action: None,
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

        total.merge(register_item(registry, t.name, template, skip_if_exists));
    }
    total
}

pub fn register_custom_prompts(
    registry: &ConcurrentRegistry<PromptTemplate>,
    prompts: Vec<CustomPromptDefinition>,
    skip_if_exists: bool,
) -> Summary {
    let mut total = Summary::new();

    for p in prompts {
        let category = match p.prompt_type {
            CustomPromptType::System => "system",
            CustomPromptType::User => "user",
            CustomPromptType::Assistant => "assistant",
        };

        let variables = p.variables.map(|vars| {
            vars.into_iter()
                .map(|v| wf_types::PromptVariableDefinition {
                    name: v.name,
                    r#type: v.var_type,
                    required: v.required.unwrap_or(false),
                    description: v.description,
                    default_value: None,
                })
                .collect()
        });

        let template = PromptTemplate {
            id: p.id.clone(),
            name: p.name.clone(),
            description: p.name,
            category: category.into(),
            content: p.content,
            variables,
            fragments: None,
        };

        if let Err(e) = wf_config::processor::prompt::validate_prompt_template(&template) {
            total.merge(Summary::err(&p.id, e.to_string()));
            continue;
        }

        total.merge(register_item(registry, p.id, template, skip_if_exists));
    }
    total
}

pub fn register_custom_resources(
    regs: &Registries,
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

    let r = register_custom_tools(&regs.tools, resources.tools, skip_if_exists);
    total.merge(r);

    let r = register_custom_triggers(&regs.trigger_templates, resources.triggers, skip_if_exists);
    total.merge(r);

    let r = register_custom_prompts(&regs.prompt_templates, resources.prompts, skip_if_exists);
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
        let regs = Registries::new();
        let summary = register_custom_resources(
            &regs,
            make_resources_with_error(),
            false,
            CustomValidationLevel::Lenient,
        );
        assert!(summary.failed.iter().any(|f| f.id == "custom_load"));
        assert!(regs.tools.has("custom-tool"));
    }

    #[test]
    fn test_strict_aborts_pipeline() {
        let regs = Registries::new();
        let summary = register_custom_resources(
            &regs,
            make_resources_with_error(),
            false,
            CustomValidationLevel::Strict,
        );
        assert!(summary.failed.iter().any(|f| f.id == "custom_load.strict"));
        assert!(!regs.tools.has("custom-tool"));
    }
}
