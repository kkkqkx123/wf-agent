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
<function_calls>
<invoke name="tool_name">
<parameter name="parameter_name">parameter_value</parameter>
</invoke>
</function_calls>
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

const TOOL_RAW_COMPACT_TEMPLATE: &str = "{name}: {description}";

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
            let param_type = schema.r#type.as_deref().unwrap_or("any");
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
