use wf_types::tool::{Tool, ToolParameterSchema};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptionStyle {
    Brief,
    Detailed,
    Markdown,
}

impl DescriptionStyle {
    /// Parse a `description_style` config value ("brief" / "detailed" /
    /// "markdown"); unknown values fall back to `Brief`.
    pub fn from_config_str(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "detailed" => Self::Detailed,
            "markdown" => Self::Markdown,
            _ => Self::Brief,
        }
    }
}

/// Options controlling the discoverable tool metadata verbosity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoverableMetadataOptions {
    /// Whether parameter type/required markers (and, for the Detailed /
    /// Markdown styles, parameter descriptions) are included. When `false`,
    /// the legacy names-only parameter list is emitted.
    pub include_description: bool,
    /// Verbatim style of the parameter list (ignored when
    /// `include_description` is false).
    pub description_style: DescriptionStyle,
}

impl Default for DiscoverableMetadataOptions {
    fn default() -> Self {
        Self {
            include_description: true,
            description_style: DescriptionStyle::Brief,
        }
    }
}

/// Placeholder replaced by [`inject_discoverable_tools_metadata`], mirroring
/// the skill metadata `{SKILLS_METADATA}` mechanism.
pub const DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER: &str = "{DISCOVERABLE_TOOLS_METADATA}";

/// Generate the metadata prompt for discoverable tools: name + one-line
/// description + typed parameter list (`query(string, required)`), so the
/// model knows what exists (and with what signatures) without any schema
/// injection. Uses the default [`DiscoverableMetadataOptions`].
pub fn generate_discoverable_tools_metadata(tools: &[Tool]) -> String {
    generate_discoverable_tools_metadata_with_options(
        tools,
        &DiscoverableMetadataOptions::default(),
    )
}

/// Style-aware variant of [`generate_discoverable_tools_metadata`].
pub fn generate_discoverable_tools_metadata_with_options(
    tools: &[Tool],
    options: &DiscoverableMetadataOptions,
) -> String {
    let entries = generate_discoverable_tool_entries_with_options(tools, options);
    if entries.is_empty() {
        return String::new();
    }
    let mut lines = vec!["Discoverable tools:".to_string()];
    lines.extend(entries);
    lines.push("Invoke them via the general tool.".to_string());
    lines.join("\n")
}

/// Per-tool metadata lines (`- name: description Parameters: a, b`) used as
/// the `{tool_list}` variable of the discoverable metadata template. Uses
/// the default [`DiscoverableMetadataOptions`].
pub fn generate_discoverable_tool_entries(tools: &[Tool]) -> Vec<String> {
    generate_discoverable_tool_entries_with_options(tools, &DiscoverableMetadataOptions::default())
}

/// Style-aware variant of [`generate_discoverable_tool_entries`]: each line
/// carries `name(string, required)` style markers (and, for the
/// Detailed / Markdown styles, parameter descriptions).
pub fn generate_discoverable_tool_entries_with_options(
    tools: &[Tool],
    options: &DiscoverableMetadataOptions,
) -> Vec<String> {
    let mut entries = Vec::new();
    for tool in tools {
        let mut line = format!("  - {}: {}", tool.name, tool.description);
        if let Some(params) = &tool.parameters {
            if !params.properties.is_empty() {
                let rendered = render_parameter_list(params, options);
                line.push_str(&format!(" Parameters: {}", rendered));
            }
        }
        entries.push(line);
    }
    entries
}

/// Render a parameter list for one tool: names only when descriptions are
/// disabled, otherwise `name(type, required)` with optional ` - description`
/// suffixes for the Detailed / Markdown styles.
fn render_parameter_list(
    params: &ToolParameterSchema,
    options: &DiscoverableMetadataOptions,
) -> String {
    if !options.include_description {
        return params
            .properties
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
    }

    let with_descriptions = options.description_style != DescriptionStyle::Brief;
    let mut parts = Vec::new();
    for (name, prop) in &params.properties {
        let marker = if params.required.contains(name) {
            "required"
        } else {
            "optional"
        };
        let mut part = format!("{}({}, {})", name, prop.property_type, marker);
        if with_descriptions {
            if let Some(desc) = prop.description.as_deref() {
                if !desc.is_empty() {
                    part.push_str(&format!(" - {}", desc));
                }
            }
        }
        parts.push(part);
    }
    parts.join(", ")
}

/// Inject a pre-rendered discoverable metadata block into a system prompt:
/// replaces the `{DISCOVERABLE_TOOLS_METADATA}` placeholder when present,
/// otherwise appends the block at the end. Returns the (possibly unchanged)
/// prompt.
pub fn inject_tool_metadata_block(system_prompt: &str, block: &str) -> String {
    if block.is_empty() {
        return system_prompt.replace(DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER, "");
    }
    if system_prompt.contains(DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER) {
        return system_prompt.replace(DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER, block);
    }
    format!("{}\n\n{}", system_prompt, block)
}

/// Inject the discoverable tool metadata into a system prompt: replaces the
/// `{DISCOVERABLE_TOOLS_METADATA}` placeholder when present, otherwise
/// appends the metadata at the end. Returns the (possibly unchanged) prompt.
pub fn inject_discoverable_tools_metadata(system_prompt: &str, tools: &[Tool]) -> String {
    let metadata = generate_discoverable_tools_metadata(tools);

    if metadata.is_empty() {
        return system_prompt.replace(DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER, "");
    }

    if system_prompt.contains(DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER) {
        return system_prompt.replace(DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER, &metadata);
    }

    format!("{}\n\n{}", system_prompt, metadata)
}

/// Derive the discoverable-metadata verbosity options from the effective
/// tool call format config. `description_style` ("brief" / "detailed" /
/// "markdown") selects the shape; `include_description=false` reverts to
/// the legacy names-only parameter list. Single source for both the
/// agent-loop schema assembly and the workflow template rendering.
pub fn discoverable_metadata_options(
    tool_call_format: Option<&wf_types::llm::ToolCallFormatConfig>,
) -> DiscoverableMetadataOptions {
    DiscoverableMetadataOptions {
        include_description: tool_call_format
            .and_then(|c| c.include_description)
            .unwrap_or(true),
        description_style: tool_call_format
            .and_then(|c| c.description_style.as_deref())
            .map(DescriptionStyle::from_config_str)
            .unwrap_or(DescriptionStyle::Brief),
    }
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
                let param_type = &prop.property_type;
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
                    let param_type = &prop.property_type;
                    let param_desc = prop.description.as_deref().unwrap_or("");
                    result.push_str(&format!(
                        "| `{}` | `{}` | {} |\n",
                        name, param_type, param_desc
                    ));
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
        use std::collections::BTreeMap;
        use wf_types::tool::{ToolParameterSchema, ToolPropertySchema};

        let mut properties = BTreeMap::new();
        properties.insert(
            "query".to_string(),
            ToolPropertySchema {
                r#ref: None,
                property_type: "string".to_string(),
                description: Some("Search query".to_string()),
                r#enum: None,
                items: None,
                properties: None,
                required: None,
                additional_properties: None,
                default: None,
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                exclusive_minimum: None,
                exclusive_maximum: None,
                min_items: None,
                max_items: None,
                min_properties: None,
                format: None,
            },
        );

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

    #[test]
    fn discoverable_metadata_lists_names_parameters_and_injects() {
        let tool = make_tool();
        let metadata = generate_discoverable_tools_metadata(std::slice::from_ref(&tool));
        assert!(metadata.contains("search: Search for information"));
        assert!(metadata.contains("Parameters: query(string, required)"));
        assert!(
            !metadata.contains("once activated"),
            "misleading direct-call claim must be gone"
        );

        let injected = inject_discoverable_tools_metadata(
            "You are a coder.\n{DISCOVERABLE_TOOLS_METADATA}",
            std::slice::from_ref(&tool),
        );
        assert!(!injected.contains("{DISCOVERABLE_TOOLS_METADATA}"));
        assert!(injected.contains("Discoverable tools"));

        let appended =
            inject_discoverable_tools_metadata("You are a coder.", std::slice::from_ref(&tool));
        assert!(appended.contains("Discoverable tools"));

        // Empty set removes the placeholder without adding content.
        let empty = inject_discoverable_tools_metadata("Hi {DISCOVERABLE_TOOLS_METADATA}", &[]);
        assert_eq!(empty, "Hi ");
    }

    #[test]
    fn discoverable_entries_include_types_and_required_markers() {
        let tool = make_tool();
        let entries = generate_discoverable_tool_entries(std::slice::from_ref(&tool));
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].contains("query(string, required)"),
            "type + required marker must be present: {}",
            entries[0]
        );
    }

    #[test]
    fn discoverable_entries_respect_include_description_off() {
        let tool = make_tool();
        let options = DiscoverableMetadataOptions {
            include_description: false,
            ..Default::default()
        };
        let entries =
            generate_discoverable_tool_entries_with_options(std::slice::from_ref(&tool), &options);
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].ends_with("Parameters: query"),
            "legacy names-only shape expected: {}",
            entries[0]
        );
        assert!(
            !entries[0].contains("required"),
            "no type markers when descriptions are off"
        );
    }

    #[test]
    fn discoverable_entries_detailed_appends_parameter_descriptions() {
        let tool = make_tool();
        let options = DiscoverableMetadataOptions {
            include_description: true,
            description_style: DescriptionStyle::Detailed,
        };
        let entries =
            generate_discoverable_tool_entries_with_options(std::slice::from_ref(&tool), &options);
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].contains("query(string, required) - Search query"),
            "Detailed style must attach parameter descriptions: {}",
            entries[0]
        );

        // Brief keeps the enhanced shape without descriptions.
        let brief = DiscoverableMetadataOptions {
            include_description: true,
            description_style: DescriptionStyle::Brief,
        };
        let brief_entries =
            generate_discoverable_tool_entries_with_options(std::slice::from_ref(&tool), &brief);
        assert!(!brief_entries[0].contains("- Search query"));
        assert!(brief_entries[0].contains("query(string, required)"));
    }

    #[test]
    fn description_style_parses_config_strings() {
        assert_eq!(
            DescriptionStyle::from_config_str("detailed"),
            DescriptionStyle::Detailed
        );
        assert_eq!(
            DescriptionStyle::from_config_str("markdown"),
            DescriptionStyle::Markdown
        );
        assert_eq!(
            DescriptionStyle::from_config_str("brief"),
            DescriptionStyle::Brief
        );
        assert_eq!(
            DescriptionStyle::from_config_str("unknown"),
            DescriptionStyle::Brief
        );
    }

    #[test]
    fn discoverable_metadata_options_follow_tool_call_format_config() {
        // No config: enhanced default (types on, Brief).
        let defaults = discoverable_metadata_options(None);
        assert!(defaults.include_description);
        assert_eq!(defaults.description_style, DescriptionStyle::Brief);

        // include_description=false reverts to the legacy names-only shape.
        let legacy = discoverable_metadata_options(Some(&wf_types::llm::ToolCallFormatConfig {
            format: wf_types::llm::ToolCallFormat::Xml,
            markers: None,
            xml_tags: None,
            include_description: Some(false),
            description_style: Some("detailed".to_string()),
            include_examples: None,
            include_rules: None,
            additional_config: None,
        }));
        assert!(!legacy.include_description);
        assert_eq!(
            legacy.description_style,
            DescriptionStyle::Detailed,
            "style is derived but unused when descriptions are off"
        );

        // Detailed style is forwarded for parameter descriptions.
        let detailed = discoverable_metadata_options(Some(&wf_types::llm::ToolCallFormatConfig {
            format: wf_types::llm::ToolCallFormat::Xml,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: Some("detailed".to_string()),
            include_examples: None,
            include_rules: None,
            additional_config: None,
        }));
        assert!(detailed.include_description);
        assert_eq!(detailed.description_style, DescriptionStyle::Detailed);
    }
}
