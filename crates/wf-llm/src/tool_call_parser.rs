use wf_types::llm::ToolCallMarkers;
use wf_types::message::{LlmFunctionCall, LlmToolCall};

/// Options for parsing tool calls from text.
#[derive(Debug, Clone)]
pub struct ToolCallParseOptions {
    pub preferred_formats: Vec<ParseFormat>,
    pub markers: Option<ToolCallMarkers>,
    pub allow_partial: bool,
}

impl Default for ToolCallParseOptions {
    fn default() -> Self {
        Self {
            preferred_formats: vec![ParseFormat::Xml, ParseFormat::Json, ParseFormat::Raw],
            markers: None,
            allow_partial: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParseFormat {
    Xml,
    Json,
    Raw,
}

/// Parse tool calls from XML format text.
///
/// Supported format:
/// ```xml
/// <tool_use>
///   <tool_name>tool_name</tool_name>
///   <parameters>
///     <param1>value1</param1>
///   </parameters>
/// </tool_use>
/// ```
pub fn parse_xml_tool_calls(xml_text: &str) -> Vec<LlmToolCall> {
    let mut results = Vec::new();
    let mut remaining = xml_text;

    while let Some(start_pos) = find_tag_start(remaining, "tool_use") {
        let after_start = &remaining[start_pos + 11..]; // "<tool_use>".len() = 11
        if let Some(end_pos) = find_tag_end(after_start, "tool_use") {
            let content = &after_start[..end_pos];
            if let Some(call) = parse_xml_tool_call_block(content) {
                results.push(call);
            }
            remaining = &after_start[end_pos + 12..]; // "</tool_use>".len() = 12
        } else {
            break;
        }
    }

    results
}

fn find_tag_start(text: &str, tag_name: &str) -> Option<usize> {
    let pattern = format!("<{}>", tag_name);
    text.find(&pattern)
}

fn find_tag_end(text: &str, tag_name: &str) -> Option<usize> {
    let pattern = format!("</{}>", tag_name);
    text.find(&pattern)
}

fn parse_xml_tool_call_block(content: &str) -> Option<LlmToolCall> {
    let tool_name = extract_xml_element(content, "tool_name")?.trim().to_string();
    let params_content = extract_xml_element(content, "parameters")?;
    let args = parse_xml_parameters(&params_content);

    Some(LlmToolCall {
        id: generate_tool_call_id(),
        r#type: "function".to_string(),
        function: LlmFunctionCall {
            name: tool_name,
            arguments: serde_json::to_string(&args).unwrap_or_default(),
        },
    })
}

fn extract_xml_element(content: &str, tag_name: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag_name);
    let close_tag = format!("</{}>", tag_name);

    let start = content.find(&open_tag)? + open_tag.len();
    let end = content[start..].find(&close_tag)? + start;

    Some(content[start..end].to_string())
}

fn parse_xml_parameters(params_content: &str) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    let mut remaining = params_content.trim();

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() || !remaining.starts_with('<') {
            break;
        }

        if let Some((tag_name, inner, rest)) = extract_next_xml_element(remaining) {
            let value = if contains_nested_element(&inner) {
                if tag_name == "item" || inner.contains("<item>") {
                    serde_json::Value::Array(parse_xml_array(&inner))
                } else {
                    parse_xml_parameters(&inner)
                }
            } else {
                parse_xml_value(&inner)
            };
            result.insert(tag_name, value);
            remaining = rest;
        } else {
            break;
        }
    }

    serde_json::Value::Object(result)
}

fn extract_next_xml_element(text: &str) -> Option<(String, String, &str)> {
    if !text.starts_with('<') {
        return None;
    }

    let tag_end = text.find('>')?;
    let tag_name = text[1..tag_end].trim().to_string();
    let close_tag = format!("</{}>", tag_name);

    let after_open = &text[tag_end + 1..];
    let close_pos = after_open.find(&close_tag)?;
    let inner = after_open[..close_pos].to_string();
    let rest = &after_open[close_pos + close_tag.len()..];

    Some((tag_name, inner, rest))
}

fn contains_nested_element(text: &str) -> bool {
    text.trim().starts_with('<') && text.contains('>')
}

fn parse_xml_array(array_content: &str) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    let mut remaining = array_content.trim();

    while let Some(start) = find_tag_start(remaining, "item") {
        let after_start = &remaining[start + 6..];
        if let Some(end) = find_tag_end(after_start, "item") {
            let inner = &after_start[..end];
            let value = if contains_nested_element(inner) {
                parse_xml_parameters(inner)
            } else {
                parse_xml_value(inner)
            };
            items.push(value);
            remaining = &after_start[end + 7..];
        } else {
            break;
        }
    }

    items
}

fn parse_xml_value(value: &str) -> serde_json::Value {
    let trimmed = value.trim();

    if trimmed == "true" {
        serde_json::Value::Bool(true)
    } else if trimmed == "false" {
        serde_json::Value::Bool(false)
    } else if trimmed == "null" {
        serde_json::Value::Null
    } else if let Ok(int_val) = trimmed.parse::<i64>() {
        serde_json::Value::Number(int_val.into())
    } else if let Ok(float_val) = trimmed.parse::<f64>() {
        serde_json::Number::from_f64(float_val)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::String(trimmed.to_string()))
    } else {
        serde_json::Value::String(trimmed.to_string())
    }
}

/// Parse tool calls from JSON wrapped with custom markers.
pub fn parse_json_tool_calls(text: &str, markers: &ToolCallMarkers) -> Vec<LlmToolCall> {
    let mut results = Vec::new();
    let start_marker = markers.start.as_deref().unwrap_or("<<<TOOL_CALL>>>");
    let end_marker = markers.end.as_deref().unwrap_or("<<<END_TOOL_CALL>>>");

    let mut remaining = text;

    while let Some(start_pos) = remaining.find(start_marker) {
        let after_start = &remaining[start_pos + start_marker.len()..];
        if let Some(end_pos) = after_start.find(end_marker) {
            let json_str = after_start[..end_pos].trim();
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(call) = convert_to_standard_tool_call(&value) {
                    results.push(call);
                }
            }
            remaining = &after_start[end_pos + end_marker.len()..];
        } else {
            break;
        }
    }

    results
}

/// Parse raw JSON tool calls (no markers).
pub fn parse_raw_json_tool_calls(text: &str) -> Vec<LlmToolCall> {
    let cleaned = text
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&cleaned) {
        let array = if value.is_array() {
            value.as_array().unwrap().clone()
        } else {
            vec![value]
        };

        array
            .into_iter()
            .filter_map(|v| convert_to_standard_tool_call(&v))
            .collect()
    } else {
        Vec::new()
    }
}

/// Convert any known tool call format to standard LlmToolCall.
fn convert_to_standard_tool_call(value: &serde_json::Value) -> Option<LlmToolCall> {
    let obj = value.as_object()?;

    // Format 1: { tool: "name", parameters: {} }
    if let Some(tool) = obj.get("tool").and_then(|v| v.as_str()) {
        let params = obj
            .get("parameters")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        return Some(LlmToolCall {
            id: generate_tool_call_id(),
            r#type: "function".to_string(),
            function: LlmFunctionCall {
                name: tool.to_string(),
                arguments: serde_json::to_string(&params).unwrap_or_default(),
            },
        });
    }

    // Format 2: { name: "name", arguments: "{}" | {} }
    if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
        let args = match obj.get("arguments") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => serde_json::to_string(v).unwrap_or_default(),
            None => "{}".to_string(),
        };
        return Some(LlmToolCall {
            id: generate_tool_call_id(),
            r#type: "function".to_string(),
            function: LlmFunctionCall {
                name: name.to_string(),
                arguments: args,
            },
        });
    }

    // Format 3: OpenAI native { id, function: { name, arguments } }
    if let Some(function) = obj.get("function").and_then(|v| v.as_object()) {
        if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
            let args = match function.get("arguments") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(v) => serde_json::to_string(v).unwrap_or_default(),
                None => "{}".to_string(),
            };
            return Some(LlmToolCall {
                id: obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(generate_tool_call_id),
                r#type: "function".to_string(),
                function: LlmFunctionCall {
                    name: name.to_string(),
                    arguments: args,
                },
            });
        }
    }

    None
}

/// Auto-detect and parse tool calls from text.
pub fn parse_from_text(text: &str, options: &ToolCallParseOptions) -> Vec<LlmToolCall> {
    if text.is_empty() {
        return Vec::new();
    }

    for format in &options.preferred_formats {
        let calls = match format {
            ParseFormat::Xml => parse_xml_tool_calls(text),
            ParseFormat::Json => {
                let markers = options.markers.clone().unwrap_or_else(ToolCallMarkers::default_json);
                parse_json_tool_calls(text, &markers)
            }
            ParseFormat::Raw => parse_raw_json_tool_calls(text),
        };

        if !calls.is_empty() {
            return calls;
        }
    }

    // Fallback for partial streaming content
    if options.allow_partial {
        parse_partial(text, options)
    } else {
        Vec::new()
    }
}

/// Parse partial tool calls (for streaming chunks).
pub fn parse_partial(text: &str, options: &ToolCallParseOptions) -> Vec<LlmToolCall> {
    if text.is_empty() {
        return Vec::new();
    }

    // Try XML first
    if text.contains("<tool_use>") {
        return parse_xml_tool_calls(text);
    }

    // Try wrapped JSON
    let markers = options.markers.clone().unwrap_or_else(ToolCallMarkers::default_json);
    let start_marker = markers.start.as_deref().unwrap_or("<<<TOOL_CALL>>>");
    if text.contains(start_marker) {
        return parse_json_tool_calls(text, &markers);
    }

    // Try raw JSON best-effort
    parse_raw_json_tool_calls(text)
}

/// Check if text contains XML tool calls.
pub fn has_xml_tool_calls(text: &str) -> bool {
    text.contains("<tool_use>")
}

/// Check if text contains wrapped JSON tool calls.
pub fn has_json_tool_calls(text: &str, markers: &ToolCallMarkers) -> bool {
    text.contains(markers.start.as_deref().unwrap_or("<<<TOOL_CALL>>>"))
}

/// Check if text contains raw JSON tool calls.
pub fn has_raw_json_tool_calls(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn generate_tool_call_id() -> String {
    format!(
        "call_{}_{}",
        wf_common::time::now(),
        wf_common::generate_id()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_xml_tool_calls() {
        let xml = r#"
<tool_use>
  <tool_name>search</tool_name>
  <parameters>
    <query>test</query>
  </parameters>
</tool_use>
"#;
        let calls = parse_xml_tool_calls(xml);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search");
    }

    #[test]
    fn test_parse_json_tool_calls() {
        let markers = ToolCallMarkers::default_json();
        let text = r#"
<<<TOOL_CALL>>>
{"tool": "search", "parameters": {"query": "test"}}
<<<END_TOOL_CALL>>>
"#;
        let calls = parse_json_tool_calls(text, &markers);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search");
    }

    #[test]
    fn test_parse_raw_json_tool_calls() {
        let json = r#"[{"name": "search", "arguments": {"query": "test"}}]"#;
        let calls = parse_raw_json_tool_calls(json);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search");
    }
}
