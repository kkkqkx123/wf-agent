use wf_types::llm::{ToolCallFormat, ToolCallMarkers};
use wf_types::message::Message;
use wf_types::tool::Tool;

use crate::tool_call_parser::{ParseFormat, ToolCallParseOptions};

/// Tool format template set.
pub struct ToolFormatTemplateSet {
    pub list_template: &'static str,
    pub single_template: &'static str,
    pub parameter_template: &'static str,
}

const TOOLS_XML_LIST_TEMPLATE: &str = r#"## Available Tools

The following tools are available for use:

{tools}

### Tool Call Format

Use the following format to call tools:

```xml
<tool_use>
  <tool_name>tool_name</tool_name>
  <parameters>
    <parameter_name>parameter_value</parameter_name>
  </parameters>
</tool_use>
```"#;

const TOOL_XML_FORMAT_TEMPLATE: &str = r#"<tool name="{name}">
<description>{description}</description>
<parameters>
{parameters}
</parameters>
</tool>"#;

const TOOL_XML_PARAMETER_LINE_TEMPLATE: &str = "- {name} ({type}){required}: {description}";

const TOOLS_JSON_LIST_TEMPLATE: &str = r#"## Available Tools

The following tools are available for use:

{tools}

### Tool Call Format

When calling tools, respond with a JSON object:

```json
{
  "function": "tool_name",
  "parameters": {
    "parameter_name": "parameter_value"
  }
}
```"#;

const TOOL_JSON_FORMAT_TEMPLATE: &str = r#"Tool Name: {name}
Description: {description}
Parameters (JSON Schema):
```json
{parameters}
```"#;

const TOOL_JSON_PARAMETER_LINE_TEMPLATE: &str = "- {name}: {description} ({type}){required}";

const TOOLS_RAW_LIST_TEMPLATE: &str = r#"Available Tools:
{tools}"#;

const TOOL_RAW_FORMAT_TEMPLATE: &str = r#"Tool: {name}
Description: {description}
Parameters: {parameters}"#;

const TOOL_RAW_PARAMETER_LINE_TEMPLATE: &str = "- {name}: {description} ({type}){required}";

const TOOLS_RAW_COMPACT_LIST_TEMPLATE: &str = r#"Available tools: {tools}"#;

const TOOL_RAW_COMPACT_TEMPLATE: &str =
    "{name}: {description}\nParameters (JSON Schema):\n{parameters}";

/// Get the appropriate template set for a tool call format.
pub fn get_tool_format_templates(format: ToolCallFormat, compact: bool) -> ToolFormatTemplateSet {
    match format {
        ToolCallFormat::Native => ToolFormatTemplateSet {
            list_template: TOOLS_RAW_LIST_TEMPLATE,
            single_template: TOOL_RAW_FORMAT_TEMPLATE,
            parameter_template: TOOL_RAW_PARAMETER_LINE_TEMPLATE,
        },
        ToolCallFormat::Xml => ToolFormatTemplateSet {
            list_template: TOOLS_XML_LIST_TEMPLATE,
            single_template: TOOL_XML_FORMAT_TEMPLATE,
            parameter_template: TOOL_XML_PARAMETER_LINE_TEMPLATE,
        },
        ToolCallFormat::JsonWrapped => ToolFormatTemplateSet {
            list_template: TOOLS_JSON_LIST_TEMPLATE,
            single_template: TOOL_JSON_FORMAT_TEMPLATE,
            parameter_template: TOOL_JSON_PARAMETER_LINE_TEMPLATE,
        },
        ToolCallFormat::JsonRaw => {
            if compact {
                ToolFormatTemplateSet {
                    list_template: TOOLS_RAW_COMPACT_LIST_TEMPLATE,
                    single_template: TOOL_RAW_COMPACT_TEMPLATE,
                    parameter_template: TOOL_RAW_PARAMETER_LINE_TEMPLATE,
                }
            } else {
                ToolFormatTemplateSet {
                    list_template: TOOLS_RAW_LIST_TEMPLATE,
                    single_template: TOOL_RAW_FORMAT_TEMPLATE,
                    parameter_template: TOOL_RAW_PARAMETER_LINE_TEMPLATE,
                }
            }
        }
    }
}

/// Get tool call parser options for a specific format.
pub fn get_tool_call_parser_options(
    format: ToolCallFormat,
    custom_markers: Option<&ToolCallMarkers>,
) -> ToolCallParseOptions {
    match format {
        ToolCallFormat::Native => ToolCallParseOptions {
            preferred_formats: vec![ParseFormat::Raw],
            ..Default::default()
        },
        ToolCallFormat::Xml => ToolCallParseOptions {
            preferred_formats: vec![ParseFormat::Xml],
            ..Default::default()
        },
        ToolCallFormat::JsonWrapped => ToolCallParseOptions {
            preferred_formats: vec![ParseFormat::Json],
            markers: custom_markers.cloned(),
            ..Default::default()
        },
        ToolCallFormat::JsonRaw => ToolCallParseOptions {
            preferred_formats: vec![ParseFormat::Raw],
            ..Default::default()
        },
    }
}

/// Check if a format requires tool descriptions in the prompt.
pub fn requires_prompt_tool_descriptions(format: ToolCallFormat) -> bool {
    !matches!(format, ToolCallFormat::Native)
}

/// Check if the given format is text-based (non-native).
pub fn is_text_based_tool_mode(format: Option<ToolCallFormat>) -> bool {
    matches!(
        format,
        Some(ToolCallFormat::Xml)
            | Some(ToolCallFormat::JsonWrapped)
            | Some(ToolCallFormat::JsonRaw)
    )
}

/// Get tool usage instructions for a text format.
pub fn get_tool_usage_instructions(format: ToolCallFormat) -> &'static str {
    match format {
        ToolCallFormat::Xml => {
            r#"## Tool Usage Instructions

When you need to use a tool, format your response as follows:

<tool_use>
  <tool_name>tool_name_here</tool_name>
  <parameters>
    <param1>value1</param1>
    <param2>value2</param2>
  </parameters>
</tool_use>

You can use multiple tools in one response by including multiple <tool_use> blocks."#
        }
        ToolCallFormat::JsonWrapped => {
            r#"## Tool Usage Instructions

When you need to use a tool, format your response as follows:

<<<TOOL_CALL>>>
{
  "tool": "tool_name_here",
  "parameters": {
    "param1": "value1",
    "param2": "value2"
  }
}
<<<END_TOOL_CALL>>>

You can use multiple tools in one response by including multiple blocks."#
        }
        _ => "",
    }
}

/// Render a single tool declaration.
pub fn render_tool_declaration(tool: &Tool, format: ToolCallFormat, compact: bool) -> String {
    let templates = get_tool_format_templates(format.clone(), compact);

    let parameters = render_parameters(tool, format.clone(), templates.parameter_template, compact);

    templates
        .single_template
        .replace("{name}", &tool.name)
        .replace("{description}", &tool.description)
        .replace("{parameters}", &parameters)
}

/// Render tool parameters.
fn render_parameters(
    tool: &Tool,
    _format: ToolCallFormat,
    parameter_template: &str,
    compact: bool,
) -> String {
    if compact {
        return tool
            .parameters
            .as_ref()
            .map(|p| serde_json::to_string_pretty(p).unwrap_or_default())
            .unwrap_or_default();
    }

    if let Some(params) = &tool.parameters {
        let required = &params.required;
        let properties = &params.properties;

        let mut lines = Vec::new();
        for (name, schema) in properties {
            let param_type = &schema.property_type;
            let desc = schema.description.as_deref().unwrap_or("");
            let req = if required.contains(name) {
                " [required]"
            } else {
                ""
            };
            lines.push(
                parameter_template
                    .replace("{name}", name)
                    .replace("{type}", param_type)
                    .replace("{required}", req)
                    .replace("{description}", desc),
            );
        }
        return lines.join("\n");
    }

    String::new()
}

/// Build the full tool list description to inject into system prompt.
pub fn render_tool_list_description(
    tools: &[Tool],
    format: ToolCallFormat,
    compact: bool,
) -> String {
    let templates = get_tool_format_templates(format.clone(), compact);
    let tool_str = tools
        .iter()
        .map(|t| render_tool_declaration(t, format.clone(), compact))
        .collect::<Vec<_>>()
        .join("\n\n");

    templates.list_template.replace("{tools}", &tool_str)
}

/// Build the complete system content for text-based tool mode by injecting
/// tool declarations + usage instructions into the existing system message.
pub fn build_text_mode_system_content(
    existing_system: &str,
    tools: &[Tool],
    format: ToolCallFormat,
    compact: bool,
) -> String {
    let instructions = get_tool_usage_instructions(format.clone());
    let tool_declarations = render_tool_list_description(tools, format.clone(), compact);
    let mut parts = Vec::new();

    if !existing_system.is_empty() {
        parts.push(existing_system.to_string());
    }
    if !instructions.is_empty() {
        parts.push(instructions.to_string());
    }
    if !tool_declarations.is_empty() {
        parts.push(tool_declarations);
    }

    parts.join("\n\n").trim().to_string()
}

/// Extract the system message content from messages and filter out system messages.
pub fn extract_system_message(messages: &[Message]) -> (Option<String>, Vec<Message>) {
    let mut system_content = None;
    let mut filtered = Vec::new();

    for msg in messages {
        match msg.role {
            wf_types::message::MessageRole::System => {
                if let Some(text) = Some(crate::message_helper::extract_text_content(msg)) {
                    system_content = Some(text);
                } else {
                    system_content = Some(String::new());
                }
            }
            _ => filtered.push(msg.clone()),
        }
    }

    (system_content, filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::ToolCallMarkers;
    use wf_types::message::{Message, MessageContentValue, MessageRole};
    use wf_types::tool::{Tool, ToolParameterSchema, ToolPropertySchema};
    fn tool(name: &str, desc: &str) -> Tool {
        fn property(property_type: &str, description: &str) -> ToolPropertySchema {
            ToolPropertySchema {
                property_type: property_type.to_string(),
                description: Some(description.to_string()),
                ..ToolPropertySchema::typed("")
            }
        }
        Tool {
            id: wf_types::Id::new(),
            name: name.to_string(),
            description: desc.to_string(),
            tool_type: wf_types::tool::ToolType::BuiltIn,
            parameters: Some(ToolParameterSchema {
                r#type: "object".to_string(),
                properties: [
                    ("query".to_string(), property("string", "search query")),
                    ("limit".to_string(), property("integer", "max results")),
                ]
                .into_iter()
                .collect(),
                required: vec!["query".to_string()],
                additional_properties: Some(false),
            }),
            metadata: None,
            config: None,
            enabled: None,
            strict: None,
            default_timeout_ms: None,
        }
    }

    #[test]
    fn template_sets_per_format() {
        for (format, expected_list) in [
            (ToolCallFormat::Native, "Available Tools:"),
            (ToolCallFormat::Xml, "## Available Tools"),
            (ToolCallFormat::JsonWrapped, "## Available Tools"),
            (ToolCallFormat::JsonRaw, "Available Tools:"),
        ] {
            let templates = get_tool_format_templates(format.clone(), false);
            assert!(
                templates.list_template.contains(expected_list),
                "format {format:?}"
            );
        }
        let compact = get_tool_format_templates(ToolCallFormat::JsonRaw, true);
        assert!(compact.list_template.starts_with("Available tools:"));
        assert!(compact.single_template.starts_with("{name}:"));
        // Compact only affects JsonRaw; the others keep the full templates.
        assert!(get_tool_format_templates(ToolCallFormat::Xml, true)
            .list_template
            .contains("## Available Tools"));
    }

    #[test]
    fn parser_options_match_format() {
        let markers = ToolCallMarkers {
            start: Some("<<<START>>>".to_string()),
            end: Some("<<<END>>>".to_string()),
        };
        let opts = get_tool_call_parser_options(ToolCallFormat::JsonWrapped, Some(&markers));
        assert!(matches!(opts.preferred_formats[0], ParseFormat::Json));
        assert_eq!(opts.markers.unwrap().start.as_deref(), Some("<<<START>>>"));
        assert!(matches!(
            get_tool_call_parser_options(ToolCallFormat::Native, None).preferred_formats[0],
            ParseFormat::Raw
        ));
        assert!(matches!(
            get_tool_call_parser_options(ToolCallFormat::Xml, None).preferred_formats[0],
            ParseFormat::Xml
        ));
        assert!(matches!(
            get_tool_call_parser_options(ToolCallFormat::JsonRaw, None).preferred_formats[0],
            ParseFormat::Raw
        ));
        // Native format keeps default (no custom markers).
        assert!(
            get_tool_call_parser_options(ToolCallFormat::Native, Some(&markers))
                .markers
                .is_none()
        );
    }

    #[test]
    fn format_classification_helpers() {
        assert!(requires_prompt_tool_descriptions(ToolCallFormat::Xml));
        assert!(requires_prompt_tool_descriptions(
            ToolCallFormat::JsonWrapped
        ));
        assert!(requires_prompt_tool_descriptions(ToolCallFormat::JsonRaw));
        assert!(!requires_prompt_tool_descriptions(ToolCallFormat::Native));

        assert!(!is_text_based_tool_mode(None));
        assert!(!is_text_based_tool_mode(Some(ToolCallFormat::Native)));
        for f in [
            ToolCallFormat::Xml,
            ToolCallFormat::JsonWrapped,
            ToolCallFormat::JsonRaw,
        ] {
            assert!(is_text_based_tool_mode(Some(f)));
        }
    }

    #[test]
    fn usage_instructions_only_for_text_formats() {
        assert!(get_tool_usage_instructions(ToolCallFormat::Xml).contains("<tool_use>"));
        assert!(get_tool_usage_instructions(ToolCallFormat::JsonWrapped).contains("TOOL_CALL"));
        assert_eq!(get_tool_usage_instructions(ToolCallFormat::Native), "");
        assert_eq!(get_tool_usage_instructions(ToolCallFormat::JsonRaw), "");
    }

    #[test]
    fn xml_list_template_matches_usage_instructions() {
        let list = get_tool_format_templates(ToolCallFormat::Xml, false)
            .list_template
            .to_string();
        let instructions = get_tool_usage_instructions(ToolCallFormat::Xml);

        for text in [list, instructions.to_string()] {
            assert!(
                text.contains("<tool_use>"),
                "must teach the <tool_use> form"
            );
            assert!(
                text.contains("<tool_name>"),
                "must name the tool via <tool_name>"
            );
            assert!(
                !text.contains("<function_calls>"),
                "legacy <function_calls> form must be gone"
            );
            assert!(
                !text.contains("<invoke name="),
                "legacy <invoke name=...> form must be gone"
            );
        }
    }

    #[test]
    fn renders_declaration_and_list() {
        let t = tool("search", "Search the web");
        let decl = render_tool_declaration(&t, ToolCallFormat::Xml, false);
        assert!(decl.contains("<tool name=\"search\">"));
        assert!(decl.contains("Search the web"));
        assert!(decl.contains("query (string) [required]: search query"));
        assert!(decl.contains("limit (integer): max results"));

        let list = render_tool_list_description(&[t], ToolCallFormat::Xml, false);
        assert!(list.contains("## Available Tools"));
        assert!(list.contains("<tool name=\"search\">"));
    }

    #[test]
    fn renders_compact_json_raw_with_json_schema() {
        let t = tool("search", "Search the web");
        let decl = render_tool_declaration(&t, ToolCallFormat::JsonRaw, true);
        assert!(decl.contains("search: Search the web"));
        let list = render_tool_list_description(&[t], ToolCallFormat::JsonRaw, true);
        assert!(list.contains("Available tools:"));
    }

    #[test]
    fn compact_json_schema_includes_additional_properties_constraint() {
        let t = tool("search", "Search the web");
        let decl = render_tool_declaration(&t, ToolCallFormat::JsonRaw, true);
        assert!(
            decl.contains("additionalProperties"),
            "compact mode dumps the full schema which must carry additionalProperties"
        );
    }

    #[test]
    fn renders_tool_without_parameters() {
        let t = Tool {
            id: wf_types::Id::new(),
            name: "noop".to_string(),
            description: "does nothing".to_string(),
            tool_type: wf_types::tool::ToolType::BuiltIn,
            parameters: None,
            metadata: None,
            config: None,
            enabled: None,
            strict: None,
            default_timeout_ms: None,
        };
        let decl = render_tool_declaration(&t, ToolCallFormat::Xml, false);
        assert!(decl.contains("<tool name=\"noop\">"));
        assert!(!decl.contains("[required]"));
    }

    #[test]
    fn build_text_mode_system_content_composes_parts() {
        let tools = vec![tool("search", "Search the web")];
        let content =
            build_text_mode_system_content("You are helpful", &tools, ToolCallFormat::Xml, false);
        assert!(content.contains("You are helpful"));
        assert!(content.contains("## Tool Usage Instructions"));
        assert!(content.contains("<tool name=\"search\">"));
        assert!(
            content.starts_with("You are helpful"),
            "existing system content must come first"
        );

        // Empty system message: no leading empty part.
        let no_system = build_text_mode_system_content("", &tools, ToolCallFormat::Xml, false);
        assert!(!no_system.contains("You are helpful"));

        // Native format: no usage instructions; the tools section header is
        // still appended even for an empty tool list.
        let native = build_text_mode_system_content("hello", &[], ToolCallFormat::Native, false);
        assert!(native.contains("hello"));
        assert!(native.contains("Available Tools:"));
        assert!(!native.contains("## Tool Usage Instructions"));
    }

    #[test]
    fn extract_system_message_filters_and_returns_last_system() {
        let sys1 = Message {
            id: wf_types::Id::new(),
            role: MessageRole::System,
            content: MessageContentValue::Text("sys one".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let user = Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text("hi".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let sys2 = Message {
            id: wf_types::Id::new(),
            role: MessageRole::System,
            content: MessageContentValue::Text("sys two".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let (system, filtered) = extract_system_message(&[sys1.clone(), user.clone(), sys2]);
        assert_eq!(system.as_deref(), Some("sys two"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].role, MessageRole::User);

        // No system message at all: None, and the user message is kept.
        let (system, filtered) = extract_system_message(std::slice::from_ref(&user));
        assert_eq!(system, None);
        assert_eq!(filtered.len(), 1);

        let (system, filtered) = extract_system_message(&[]);
        assert_eq!(system, None);
        assert!(filtered.is_empty());
    }
}
