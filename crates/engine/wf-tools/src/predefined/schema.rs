//! Single-source schema types for predefined tools.
//!
//! A [`ToolDefinition`] is the single definition of a predefined tool. It
//! generates both the registry-facing [`Tool`] (with the parameter schema)
//! and the LLM-facing [`ToolDescriptionData`], guaranteeing the two stay in
//! sync and eliminating duplicated descriptions.

use std::collections::BTreeMap;

use wf_types::tool::{
    CheckpointTiming, Tool, ToolMetadata, ToolParameterSchema, ToolPropertySchema, ToolRiskLevel,
    ToolType,
};
use wf_types::tool_description::{ToolDescriptionData, ToolParameterDescription};

/// Extra schema constraints layered on top of a [`ToolParameter`].
pub struct ToolParameterConstraint {
    /// Allowed literal values (e.g. output formats), enforced by the
    /// validator and rendered as a JSON-Schema `enum`.
    pub enum_values: Option<&'static [&'static str]>,
    /// Regex the string value must match.
    pub pattern: Option<&'static str>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
    pub min_items: Option<u64>,
    pub max_items: Option<u64>,
    /// Element schema for array parameters. The inner parameter's
    /// type/description/constraints describe each element.
    pub items: Option<&'static ToolParameter>,
}

impl ToolParameterConstraint {
    const fn empty() -> Self {
        Self {
            enum_values: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            min_items: None,
            max_items: None,
            items: None,
        }
    }
}

static EMPTY_CONSTRAINT: ToolParameterConstraint = ToolParameterConstraint::empty();

/// A parameter of a tool definition (shared by schema and description).
pub struct ToolParameter {
    pub name: &'static str,
    pub r#type: &'static str,
    pub required: bool,
    pub description: &'static str,
    /// Raw JSON literal of the default value (e.g. `"5"` or `"\"text\""`),
    /// parsed at runtime. Kept as a literal so definitions can be `static`.
    pub default_json: Option<&'static str>,
    /// Optional extra schema constraints (enum, pattern, bounds, items).
    pub constraints: Option<&'static ToolParameterConstraint>,
}

impl ToolParameter {
    fn default_value(&self) -> Option<serde_json::Value> {
        self.default_json.and_then(|s| serde_json::from_str(s).ok())
    }

    fn constraint(&self) -> &'static ToolParameterConstraint {
        self.constraints.unwrap_or(&EMPTY_CONSTRAINT)
    }

    /// Build the strongly-typed property schema for this parameter.
    fn to_property_schema(&self) -> ToolPropertySchema {
        let c = self.constraint();
        ToolPropertySchema {
            r#ref: None,
            property_type: self.r#type.to_string(),
            description: Some(self.description.to_string()),
            r#enum: c.enum_values.map(|vals| {
                vals.iter()
                    .map(|v| serde_json::Value::String(v.to_string()))
                    .collect()
            }),
            items: c.items.map(|item| Box::new(item.to_property_schema())),
            properties: None,
            required: None,
            additional_properties: None,
            default: self.default_value(),
            pattern: c.pattern.map(str::to_string),
            min_length: c.min_length,
            max_length: c.max_length,
            minimum: c.minimum.map(|v| v as f64),
            maximum: c.maximum.map(|v| v as f64),
            exclusive_minimum: None,
            exclusive_maximum: None,
            min_items: c.min_items,
            max_items: c.max_items,
            min_properties: None,
            format: None,
        }
    }
}

/// The single source of truth for a predefined tool.
pub struct ToolDefinition {
    pub id: &'static str,
    pub tool_type: ToolType,
    pub category: &'static str,
    pub tags: &'static [&'static str],
    pub description: &'static str,
    pub parameters: &'static [ToolParameter],
    pub tips: Option<&'static [&'static str]>,
    pub examples: Option<&'static [&'static str]>,
    /// Risk classification used by the approval engine.
    pub risk_level: ToolRiskLevel,
    /// When to create a checkpoint around execution of this tool.
    pub create_checkpoint: Option<CheckpointTiming>,
}

impl ToolDefinition {
    /// The LLM-facing type string for [`ToolDescriptionData`].
    pub fn type_str(&self) -> &'static str {
        match self.tool_type {
            ToolType::Stateless => "STATELESS",
            ToolType::Stateful => "STATEFUL",
            ToolType::Rest => "REST",
            ToolType::BuiltIn => "BUILTIN",
            ToolType::Mcp => "MCP",
        }
    }

    /// Build the registry-facing tool definition (schema + description).
    pub fn tool_def(&self) -> Tool {
        let properties: BTreeMap<String, ToolPropertySchema> = self
            .parameters
            .iter()
            .map(|p| (p.name.to_string(), p.to_property_schema()))
            .collect();
        let required = self
            .parameters
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name.to_string())
            .collect();
        let schema = ToolParameterSchema {
            r#type: "object".into(),
            properties,
            required,
            additional_properties: Some(false),
        };

        Tool {
            id: self.id.into(),
            name: self.id.into(),
            description: self.description.into(),
            tool_type: self.tool_type.clone(),
            parameters: Some(schema),
            metadata: Some(ToolMetadata {
                category: Some(self.category.into()),
                tags: Some(self.tags.iter().map(|t| t.to_string()).collect()),
                documentation_url: None,
                custom_fields: None,
                risk_level: Some(self.risk_level),
                auto_approvable: None,
                create_checkpoint: self.create_checkpoint,
                exposure: None,
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    /// Build the LLM-facing description data (tips + examples).
    pub fn description_data(&self) -> ToolDescriptionData {
        ToolDescriptionData {
            id: self.id.into(),
            r#type: self.type_str().into(),
            category: Some(self.category.into()),
            description: self.description.into(),
            parameters: self
                .parameters
                .iter()
                .map(|p| ToolParameterDescription {
                    name: p.name.into(),
                    r#type: p.r#type.into(),
                    required: p.required,
                    description: p.description.into(),
                    default_value: p.default_value(),
                })
                .collect(),
            tips: self.tips.map(|t| t.iter().map(|s| s.to_string()).collect()),
            examples: self
                .examples
                .map(|e| e.iter().map(|s| s.to_string()).collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::predefined::all_definitions;
    use wf_types::tool::CheckpointTiming;

    /// Every predefined tool parameter uses a supported JSON type.
    #[test]
    fn test_all_definition_parameter_types_are_valid() {
        for def in all_definitions() {
            for p in def.parameters {
                assert!(
                    matches!(
                        p.r#type,
                        "string" | "number" | "integer" | "boolean" | "array" | "object" | "null"
                    ),
                    "{} parameter '{}' has unsupported type '{}'",
                    def.id,
                    p.name,
                    p.r#type
                );
            }
        }
    }

    #[test]
    fn test_edit_file_uses_path_parameter() {
        let tool = crate::predefined::filesystem::EDIT_FILE.tool_def();
        let schema = tool.parameters.unwrap();
        assert!(schema.properties.contains_key("path"));
        assert!(!schema.properties.contains_key("file_path"));
        assert!(schema.required.contains(&"path".to_string()));
    }

    #[test]
    fn test_use_mcp_category_matches_module() {
        assert_eq!(
            crate::predefined::integration::USE_MCP.category,
            "integration"
        );
        assert_eq!(
            crate::predefined::integration::USE_MCP.tool_type,
            wf_types::tool::ToolType::Mcp
        );
    }

    #[test]
    fn test_write_tools_create_checkpoint_before() {
        for id in ["write_file", "edit_file", "apply_diff"] {
            let def = all_definitions()
                .into_iter()
                .find(|d| d.id == id)
                .expect(id);
            assert_eq!(
                def.create_checkpoint,
                Some(CheckpointTiming::Before),
                "{} should checkpoint before execution",
                id
            );
        }
        let apply_patch = all_definitions()
            .into_iter()
            .find(|d| d.id == "apply_patch")
            .unwrap();
        assert_eq!(
            apply_patch.create_checkpoint,
            Some(CheckpointTiming::Both),
            "apply_patch should checkpoint before and after execution"
        );
        let read_only = all_definitions()
            .into_iter()
            .find(|d| d.id == "read_file")
            .unwrap();
        assert_eq!(read_only.create_checkpoint, None);
    }

    #[test]
    fn test_memory_list_is_read_only() {
        assert_eq!(
            crate::predefined::risk::get_tool_risk_level("memory_list"),
            Some(wf_types::tool::ToolRiskLevel::ReadOnly)
        );
    }

    /// Serializing the same definition twice must produce byte-identical
    /// output (BTreeMap-ordered properties, fixed field order).
    #[test]
    fn test_schema_serialization_is_deterministic() {
        for def in all_definitions() {
            let tool = def.tool_def();
            let json1 = serde_json::to_vec(&tool).unwrap();
            let json2 = serde_json::to_vec(&tool).unwrap();
            assert_eq!(
                json1, json2,
                "{} schema serialization must be deterministic",
                def.id
            );
        }
    }

    #[test]
    fn test_all_predefined_tools_force_additional_properties_false() {
        for def in all_definitions() {
            let tool = def.tool_def();
            let schema = tool
                .parameters
                .expect("predefined tools must declare a schema");
            assert_eq!(
                schema.additional_properties,
                Some(false),
                "{} must force additionalProperties:false",
                def.id
            );
        }
    }

    #[test]
    fn test_web_fetch_format_declared_as_enum() {
        let tool = crate::predefined::web::WEB_FETCH.tool_def();
        let schema = tool.parameters.unwrap();
        let format = &schema.properties["format"];
        assert_eq!(format.property_type, "string");
        assert_eq!(
            format.r#enum.as_ref().unwrap(),
            &vec![
                serde_json::json!("text"),
                serde_json::json!("markdown"),
                serde_json::json!("html"),
            ]
        );
        assert_eq!(format.default, Some(serde_json::json!("markdown")));
    }

    #[test]
    fn test_ask_followup_question_options_declares_items() {
        let tool = crate::predefined::interaction::ASK_FOLLOWUP_QUESTION.tool_def();
        let schema = tool.parameters.unwrap();
        let options = &schema.properties["options"];
        assert_eq!(options.property_type, "array");
        let items = options.items.as_ref().expect("options must declare items");
        assert_eq!(items.property_type, "string");
    }

    /// Field order must follow `$ref, type, description, enum, items,
    /// properties, required, additionalProperties, default, ...` so output is
    /// reproducible across runs and versions.
    #[test]
    fn test_property_field_order_is_fixed() {
        let tool = crate::predefined::web::WEB_FETCH.tool_def();
        let schema = tool.parameters.unwrap();
        let format = &schema.properties["format"];
        let full = serde_json::to_string(format).unwrap();
        let expected_order = ["\"type\"", "\"description\"", "\"enum\"", "\"default\""];
        let mut pos = 0;
        for prefix in expected_order {
            let found = full[pos..].find(prefix).map(|i| i + pos);
            assert!(
                found.is_some(),
                "field '{}' missing from serialized property: {}",
                prefix,
                full
            );
            pos = found.unwrap() + prefix.len();
        }
    }
}
