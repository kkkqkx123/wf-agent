use wf_types::tool::Tool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptionStyle {
    Brief,
    Detailed,
    Markdown,
}

pub struct ToolDescriptionGenerator;

impl ToolDescriptionGenerator {
    pub fn generate(tool: &Tool, style: &DescriptionStyle) -> String {
        match style {
            DescriptionStyle::Brief => Self::generate_brief(tool),
            DescriptionStyle::Detailed => Self::generate_detailed(tool),
            DescriptionStyle::Markdown => Self::generate_markdown(tool),
        }
    }

    fn generate_brief(tool: &Tool) -> String {
        format!("{}: {}", tool.name, tool.description)
    }

    fn generate_detailed(tool: &Tool) -> String {
        let mut result = format!("Tool: {}\n", tool.name);
        result.push_str(&format!("Description: {}\n", tool.description));
        result.push_str(&format!("Type: {:?}\n", tool.tool_type));

        if let Some(ref params) = tool.parameters {
            result.push_str("Parameters:\n");
            for (name, prop) in &params.properties {
                let param_type = prop.r#type.as_deref().unwrap_or("any");
                let param_desc = prop.description.as_deref().unwrap_or("");
                result.push_str(&format!("  - {} ({}): {}\n", name, param_type, param_desc));
            }

            if !params.required.is_empty() {
                result.push_str(&format!("Required: {}\n", params.required.join(", ")));
            }
        }

        result
    }

    fn generate_markdown(tool: &Tool) -> String {
        let mut result = format!("## {}\n\n", tool.name);
        result.push_str(&format!("{}\n\n", tool.description));
        result.push_str(&format!("**Type:** `{:?}`\n\n", tool.tool_type));

        if let Some(ref params) = tool.parameters {
            if !params.properties.is_empty() {
                result.push_str("### Parameters\n\n");
                result.push_str("| Name | Type | Description |\n");
                result.push_str("|------|------|-------------|\n");

                for (name, prop) in &params.properties {
                    let param_type = prop.r#type.as_deref().unwrap_or("any");
                    let param_desc = prop.description.as_deref().unwrap_or("");
                    result.push_str(&format!("| `{}` | `{}` | {} |\n", name, param_type, param_desc));
                }
                result.push('\n');
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool() -> Tool {
        use std::collections::HashMap;
        use wf_types::tool::{ToolParameterSchema, ToolProperty};

        let mut properties = HashMap::new();
        properties.insert("query".to_string(), ToolProperty {
            name: "query".to_string(),
            value: serde_json::json!(""),
            r#type: Some("string".to_string()),
            required: Some(true),
            description: Some("Search query".to_string()),
        });

        Tool {
            id: wf_types::Id::new(),
            name: "search".to_string(),
            description: "Search for information".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: Some(ToolParameterSchema {
                r#type: "object".to_string(),
                properties,
                required: vec!["query".to_string()],
                additional_properties: None,
            }),
            metadata: None,
            config: None,
            enabled: None,
            strict: None,
            default_timeout_ms: None,
        }
    }

    #[test]
    fn test_brief_description() {
        let tool = make_tool();
        let desc = ToolDescriptionGenerator::generate(&tool, &DescriptionStyle::Brief);
        assert!(desc.contains("search"));
        assert!(desc.contains("Search for information"));
    }

    #[test]
    fn test_detailed_description() {
        let tool = make_tool();
        let desc = ToolDescriptionGenerator::generate(&tool, &DescriptionStyle::Detailed);
        assert!(desc.contains("Tool: search"));
        assert!(desc.contains("query"));
    }

    #[test]
    fn test_markdown_description() {
        let tool = make_tool();
        let desc = ToolDescriptionGenerator::generate(&tool, &DescriptionStyle::Markdown);
        assert!(desc.contains("## search"));
        assert!(desc.contains("| Name |"));
    }
}
