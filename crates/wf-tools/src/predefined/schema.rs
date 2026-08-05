//! Single-source schema types for predefined tools.
//!
//! A [`ToolDefinition`] is the single definition of a predefined tool. It
//! generates both the registry-facing [`Tool`] (with the parameter schema)
//! and the LLM-facing [`ToolDescriptionData`], guaranteeing the two stay in
//! sync and eliminating duplicated descriptions.

use wf_types::tool::{
    CheckpointTiming, Tool, ToolMetadata, ToolParameterSchema, ToolProperty, ToolRiskLevel,
    ToolType,
};
use wf_types::tool_description::{ToolDescriptionData, ToolParameterDescription};

/// A parameter of a tool definition (shared by schema and description).
pub struct ToolParameter {
    pub name: &'static str,
    pub r#type: &'static str,
    pub required: bool,
    pub description: &'static str,
    /// Raw JSON literal of the default value (e.g. `"5"` or `"\"text\""`),
    /// parsed at runtime. Kept as a literal so definitions can be `static`.
    pub default_json: Option<&'static str>,
}

impl ToolParameter {
    fn default_value(&self) -> Option<serde_json::Value> {
        self.default_json.and_then(|s| serde_json::from_str(s).ok())
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
        let properties = self
            .parameters
            .iter()
            .map(|p| {
                (
                    p.name.to_string(),
                    ToolProperty {
                        name: p.name.to_string(),
                        value: p.default_value().unwrap_or(serde_json::Value::Null),
                        r#type: Some(p.r#type.to_string()),
                        required: Some(p.required),
                        description: Some(p.description.to_string()),
                    },
                )
            })
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
            additional_properties: None,
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
