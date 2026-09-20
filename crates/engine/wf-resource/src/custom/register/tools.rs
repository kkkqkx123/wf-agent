use std::collections::{BTreeMap, HashMap};

use wf_core::registry::Registry;
use wf_tools::registry::ToolRegistry;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool::{ToolMetadata, ToolParameterSchema, ToolPropertySchema, ToolType};

use crate::custom::types::{CustomParamDef, CustomToolDefinition, CustomToolType};
use crate::result::Summary;

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
    params: &[CustomParamDef],
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
